use std::marker::PhantomData;

use bevy::{
    ecs::{
        change_detection::Tick,
        system::{SystemChangeTick, SystemParam},
    },
    prelude::*,
};

use crate::{
    player::Player,
    states::{IntoSetConfigs, SetConfigs},
};

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlayerFocusedSet;

impl IntoSetConfigs for PlayerFocusedSet {
    fn cfg(&self) -> SetConfigs {
        Self.run_if(has_focus::<Player>)
    }
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlayerNotFocusedSet;

impl IntoSetConfigs for PlayerNotFocusedSet {
    fn cfg(&self) -> SetConfigs {
        Self.run_if(not(has_focus::<Player>))
    }
}

pub fn has_focus<C: Component>(q: Query<(), (With<C>, With<Focused>)>) -> bool {
    q.single().is_ok()
}

// Transfer Focus to C, if only one entity with C exists, and is not
// currently focused.
// pub fn transfer_focus<C: Component>(
//     mut reqs: MessageWriter<FocusRequested>,
//     q_target: Query<Entity, (With<C>, Without<Focused>)>,
// ) {
//     if let Ok(entity) = q_target.single() {
//         reqs.write(entity.into());
//     }
// }

/// The entity that receives the input.
#[derive(Copy, Clone, Component)]
pub struct Focused;

// /// Focus was requested by this entity.
// #[derive(Message, Copy, Clone)]
// pub struct FocusRequested(pub Option<Entity>);

/// Fired when a request occurs to change the currently focused entity.
#[derive(Message, Copy, Clone, Debug)]
pub enum FocusRequested {
    /// Transfer or assign focus to a specific entity.
    ToEntity(Entity),

    /// Transfer or assign focus to the player.
    ToPlayer,

    /// Make it so no entity has focus.
    None,
}

impl From<Entity> for FocusRequested {
    fn from(value: Entity) -> Self {
        Self::ToEntity(value)
    }
}

/// Focus was transferred between one entity and another.
#[derive(Message, Copy, Clone)]
pub struct FocusChanged {
    /// The entity focus was removed from, if any.
    pub from: Option<Entity>,

    /// The entity focus was added to, if any.
    pub to: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct FocusManager {
    /// The currently focused entity.
    pub curr: Option<Entity>,

    /// The entity that was previously focused.
    pub prev: Option<Entity>,

    /// The tick of the last time focus changed.
    pub last_change: Tick,

    /// Whether the player has focus.
    pub is_player: bool,
}

impl FocusManager {
    fn update(&mut self, new_curr: Option<Entity>, tick: SystemChangeTick, is_player: bool) {
        self.prev = self.curr;
        self.curr = new_curr;
        self.is_player = is_player;
        self.last_change = tick.this_run();
        info!("FOCUS CHANGED TO: '{new_curr:?}'");
    }
}

pub fn update_focus_manager(
    tick: SystemChangeTick,
    mut manager: ResMut<FocusManager>,
    mut changed_msgs: MessageWriter<FocusChanged>,
    mut focus_reqs: MessageReader<FocusRequested>,
    mut commands: Commands,
    q_player: Query<Entity, With<Player>>,
    q: Query<Entity, With<Focused>>,
) {
    if let Some(curr) = manager.curr {
        // Check for manual removal of focused entity.
        if q.get(curr).is_err() {
            changed_msgs.write(FocusChanged {
                from: Some(curr),
                to: None,
            });
            manager.update(None, tick, false);
            return;
        }
    } else {
        if let Ok(curr) = q.single() {
            // Check for manual insertions of Focus
            manager.update(Some(curr), tick, q_player.get(curr).is_ok());
            changed_msgs.write(FocusChanged {
                from: None,
                to: Some(curr),
            });
            return;
        }
    }

    // process one at a time.
    if let Some(req) = focus_reqs.read().next() {
        match req {
            FocusRequested::ToPlayer => {
                // change focus to the player

                if let Ok(player) = q_player.single() {
                    if let Some(curr) = manager.curr {
                        if curr == player {
                            return;
                        } else {
                            commands.entity(curr).remove::<Focused>();
                        }
                    }

                    commands.entity(player).insert(Focused);
                    manager.update(Some(player), tick, true);
                    changed_msgs.write(FocusChanged {
                        from: manager.prev,
                        to: manager.curr,
                    });
                }
            }
            FocusRequested::ToEntity(ent) => {
                // change focus to this entity.

                if let Some(curr) = manager.curr {
                    if curr == *ent {
                        return;
                    } else {
                        commands.entity(curr).remove::<Focused>();
                    }
                }

                commands.entity(*ent).insert(Focused);
                manager.update(Some(*ent), tick, q_player.get(*ent).is_ok());
                changed_msgs.write(FocusChanged {
                    from: manager.prev,
                    to: manager.curr,
                });
            }
            FocusRequested::None => {
                // change focus to none

                if let Some(curr) = manager.curr {
                    commands.entity(curr).remove::<Focused>();
                    manager.update(None, tick, false);
                    changed_msgs.write(FocusChanged {
                        from: Some(curr),
                        to: None,
                    });
                }
            }
        }
    }
}

#[derive(SystemParam)]
pub struct Focus<'w, 's> {
    tick: SystemChangeTick,
    manager: Res<'w, FocusManager>,
    requests: MessageWriter<'w, FocusRequested>,
    _marker: PhantomData<&'s ()>,
}

impl<'w, 's> Focus<'w, 's> {
    /// Request transfer of focus to another entity.
    pub fn to_entity(&mut self, to: Entity) {
        self.requests.write(to.into());
    }

    /// Request transfer of focus to the Player.
    pub fn to_player(&mut self) {
        self.requests.write(FocusRequested::ToPlayer);
    }

    /// Set the current focus to None.
    pub fn none(&mut self) {
        self.requests.write(FocusRequested::None);
    }

    /// Check whether this entity has focus.
    pub fn has_focus(&self, tar: Entity) -> bool {
        self.manager.curr.is_some_and(|ent| tar == ent)
    }

    /// Check whether the currently focused entity is the player.
    pub fn player_has_focus(&self) -> bool {
        self.manager.is_player
    }

    /// Get the currently focused entity.
    pub fn curr(&self) -> Option<Entity> {
        self.manager.curr
    }

    /// Get the previously focused entity.
    pub fn prev(&self) -> Option<Entity> {
        self.manager.prev
    }

    /// check if the focus just changed.
    pub fn just_changed(&self) -> bool {
        self.manager
            .last_change
            .is_newer_than(self.tick.last_run(), self.tick.this_run())
    }

    /// check if focus was just lost, and returns the entity if so.
    pub fn just_lost(&self) -> Option<Entity> {
        self.manager
            .prev
            .filter(|_| self.just_changed() && self.manager.curr.is_none())
    }

    /// Returns the entity if focus was just gained.
    pub fn just_gained(&self) -> Option<Entity> {
        self.manager.curr.filter(|_| self.just_changed())
    }
}
