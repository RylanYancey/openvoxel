use bevy::prelude::*;

#[derive(States, PartialEq, Eq, Clone, Debug, Hash, Default)]
pub enum AppState {
    #[default]
    Starting,
    InMenus,
    InGame,
}

#[derive(States, PartialEq, Eq, Clone, Debug, Hash, Default)]
pub enum ConnectState {
    #[default]
    Offline,
    Connecting,
    Connected,
}

/// A system set for behavior that runs when the player is fully connected to a server
/// and is not in any transitional states, such as world transitions.
#[derive(SystemSet, Eq, PartialEq, Debug, Default, Clone, Hash)]
pub struct InGame;

/// 20 game ticks per second.
#[derive(Resource, Default, Copy, Clone, Eq, PartialEq, Debug, Deref)]
pub struct GameTick {
    #[deref]
    tick: u64,

    /// Whether the GT just changed.
    just_changed: bool,
}

/// Run condition for game tick.
/// To have a system that runs every game tick, do `run_if(on_tick(0))`.
pub fn on_tick(n: u64) -> impl FnMut(Res<GameTick>) -> bool {
    move |gt: Res<GameTick>| {
        if n != 0 {
            gt.tick % n == 0 && gt.just_changed
        } else {
            gt.just_changed
        }
    }
}

/// increment GameTick by 1
pub fn update_game_tick(mut gt: ResMut<GameTick>) {
    gt.tick += 1;
    gt.just_changed = true;
}

/// Runs before `update_game_tick` in the First schedule to track whether it has just changed.
pub fn set_game_tick_not_just_changed(mut gt: ResMut<GameTick>) {
    gt.just_changed = false;
}

/// Systems that run whenever the GameTick is a multiple of some number.
#[derive(SystemSet, PartialEq, Eq, Clone, Debug, Hash)]
pub enum TickSets {
    /// The send buffers will flush in the `PostUpdate` schedule.
    Flush,
}

#[derive(Default, States, Eq, PartialEq, Debug, Clone, Copy, Hash)]
pub enum PlayerStance {
    #[default]
    Standing,
    Crouching,
    Crawling,
}

#[derive(Default, States, Eq, PartialEq, Debug, Clone, Copy, Hash)]
pub enum InputState {
    Free,
    #[default]
    Ui,
}
