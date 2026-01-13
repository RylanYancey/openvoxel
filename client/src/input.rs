use bevy::{ecs::system::SystemId, prelude::*};
use fxhash::FxHashMap;

use crate::{focus::Focus, states::AppState, ui::menus::Menu};

#[derive(Resource, Default)]
pub struct Actions {
    bindings: FxHashMap<String, Binding>,
}

impl Actions {
    fn get(&self, label: &str) -> Option<&Binding> {
        if let Some(binding) = self.bindings.get(label) {
            Some(binding)
        } else {
            warn!("[C313] Attempted to get action '{label}', but it does not exist");
            None
        }
    }

    fn get_mut(&mut self, label: &str) -> Option<&mut Binding> {
        if let Some(binding) = self.bindings.get_mut(label) {
            Some(binding)
        } else {
            warn!("[C313] Attempted to get action '{label}', but it does not exist");
            None
        }
    }

    /// Add a new binding with the given buttons.
    pub fn add(&mut self, label: impl Into<String>, buttons: impl IntoIterator<Item = Button>) {
        let buttons = buttons.into_iter().collect::<Vec<_>>();
        self.bindings.insert(
            label.into(),
            Binding {
                default: buttons.clone(),
                custom: buttons,
                triggers: Vec::new(),
                time_active: 0,
                prev_time: 0,
            },
        );
    }

    pub fn add_trigger(&mut self, label: impl AsRef<str>, system: SystemId) {
        if let Some(binding) = self.get_mut(label.as_ref()) {
            binding.triggers.push(system);
        }
    }

    pub fn is_activated(&self, label: impl AsRef<str>) -> bool {
        self.get(label.as_ref())
            .is_some_and(|binding| binding.is_activated())
    }

    pub fn is_deactivated(&self, label: impl AsRef<str>) -> bool {
        self.get(label.as_ref())
            .is_some_and(|binding| !binding.is_activated())
    }

    pub fn just_activated(&self, label: impl AsRef<str>) -> bool {
        self.get(label.as_ref())
            .is_some_and(|binding| binding.just_activated())
    }

    pub fn just_deactivated(&self, label: impl AsRef<str>) -> bool {
        self.get(label.as_ref())
            .is_some_and(|binding| binding.just_deactivated())
    }
}

pub struct Binding {
    default: Vec<Button>,
    custom: Vec<Button>,
    triggers: Vec<SystemId>,
    time_active: u64,
    prev_time: u64,
}

impl Binding {
    pub fn is_activated(&self) -> bool {
        self.time_active != 0
    }

    pub fn just_activated(&self) -> bool {
        self.time_active == 1
    }

    pub fn just_deactivated(&self) -> bool {
        self.time_active == 0 && self.prev_time != 0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Button {
    Key(KeyCode),
    Mouse(MouseButton),
}

impl From<KeyCode> for Button {
    fn from(value: KeyCode) -> Self {
        Self::Key(value)
    }
}

impl From<MouseButton> for Button {
    fn from(value: MouseButton) -> Self {
        Self::Mouse(value)
    }
}

pub fn update_actions(
    mut actions: ResMut<Actions>,
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    for binding in actions.bindings.values_mut() {
        binding.prev_time = binding.time_active;

        if !binding.custom.iter().any(|button| {
            if match button {
                Button::Key(code) => keys.pressed(*code),
                Button::Mouse(btn) => mouse.pressed(*btn),
            } {
                binding.time_active += 1;
                true
            } else {
                false
            }
        }) {
            binding.time_active = 0;
        }

        if binding.just_activated() {
            for trigger in &binding.triggers {
                commands.run_system(*trigger);
            }
        }
    }
}

/// Run Condition that checks if the action is activated.
pub fn activated(action: impl AsRef<str>) -> impl FnMut(Res<Actions>) -> bool {
    let label = action.as_ref().to_string();
    move |actions: Res<Actions>| actions.is_activated(&label)
}

/// Run condition that checks if the action is deactivated.
pub fn deactivated(action: impl AsRef<str>) -> impl FnMut(Res<Actions>) -> bool {
    let label = action.as_ref().to_string();
    move |actions: Res<Actions>| actions.is_deactivated(&label)
}

/// Run condition that checks if the action just activated.
pub fn just_activated(action: impl AsRef<str>) -> impl FnMut(Res<Actions>) -> bool {
    let label = action.as_ref().to_string();
    move |actions: Res<Actions>| actions.just_activated(&label)
}

/// Run condition that checks if the action just deactivated.
pub fn just_deactivated(action: impl AsRef<str>) -> impl FnMut(Res<Actions>) -> bool {
    let label = action.as_ref().to_string();
    move |actions: Res<Actions>| actions.just_deactivated(&label)
}

/// Handles menu transitions when the "close-menu" action fires.
pub fn handle_close_menu_transitions(
    mut focus: Focus,
    curr_menu: Res<State<Menu>>,
    mut next_menu: ResMut<NextState<Menu>>,
    app_state: Res<State<AppState>>,
) {
    match app_state.get() {
        AppState::InMenus => {
            // App is in-menus.

            if let Some(_) = focus.curr() {
                // todo: un-focus focused ui element.
            } else {
                // nothing focused, return to previous menu.

                match curr_menu.get() {
                    Menu::WorldSelect => {
                        // return to Title Menu
                        next_menu.set(Menu::Title);
                    }
                    Menu::ServerSelect => {
                        // return to Title Menu
                        next_menu.set(Menu::Title);
                    }
                    other => {
                        error!(
                            "[C414] Menu '{other:?}' not meant to be reachable while in the title menu."
                        );
                    }
                }
            }
        }
        AppState::InGame => {
            // App is in-game.

            if let Some(_) = focus.curr() {
                // an entity has focus currently.

                if focus.player_has_focus() {
                    // if player has focus and we are in-game, then
                    // transition to the pause menu and set focus to None.

                    next_menu.set(Menu::Pause);
                    focus.none();
                }
            } else {
                // no entity has focus, they may be in a menu.

                match curr_menu.get() {
                    Menu::Pause => {
                        // Game is paused, un-pause.
                        next_menu.set(Menu::None);
                        // transfer focus back to player.
                        focus.to_player();
                    }
                    Menu::Options => {
                        // return to Pause menu.
                        next_menu.set(Menu::Pause);
                    }
                    Menu::None => {}
                    // menu not meant to be reachable while in-game.
                    other => {
                        error!("[C413] Menu '{other:?}' not meant to be reachable while in-game.");
                    }
                }
            }
        }
        _ => {}
    }
}
