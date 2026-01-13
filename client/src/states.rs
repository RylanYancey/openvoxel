use bevy::{ecs::intern::Interned, prelude::*};

use crate::focus::Focus;

pub type SetConfigs =
    bevy::ecs::schedule::ScheduleConfigs<Interned<dyn bevy::prelude::SystemSet + 'static>>;

pub trait IntoSetConfigs {
    fn cfg(&self) -> SetConfigs;
}

/// State of the application as a whole.
#[derive(States, PartialEq, Eq, Clone, Copy, Debug, Hash, Default)]
pub enum AppState {
    /// Startup sequence is executing.
    #[default]
    Starting,

    /// Player is in the title menu.
    InMenus,

    /// Player is in a transition state, either between
    /// InMenus to InGame, or InGame to InGame.
    InSequence,

    /// Player is connected to a server.
    InGame,
}

/// Describes the behavior of the cursor.
#[derive(States, Default, Eq, PartialEq, Debug, Clone, Copy, Hash)]
pub enum CursorMode {
    /// The cursor is visible and can be used to interact with UI elements.
    #[default]
    Normal,

    /// Cursor is hidden and locked in place. Mouse motion is used
    /// as camera movement. Only active when the Player is focused.
    Locked,
}

pub fn update_cursor_mode(
    app_state: Res<State<AppState>>,
    curr: ResMut<State<CursorMode>>,
    mut next: ResMut<NextState<CursorMode>>,
    focus: Focus,
) {
    match curr.get() {
        CursorMode::Normal => {
            // set cursor mode to locked if player has focus and is in-game.
            if focus.player_has_focus() && *app_state == AppState::InGame {
                next.set(CursorMode::Locked);
            }
        }
        CursorMode::Locked => {
            // set cursor mode to normal if not in game or not player has focus.
            if !focus.player_has_focus() || *app_state != AppState::InGame {
                next.set(CursorMode::Normal);
            }
        }
    }
}
