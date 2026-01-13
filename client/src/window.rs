use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use crate::{settings::Settings, states::CursorMode};

#[derive(States, Default, Clone, Eq, PartialEq, Hash, Debug)]
pub enum WindowState {
    #[default]
    Starting,
    Ready,
}

/// Because of weirdness with the way windowing APIs work, the window
/// needs to be invisible for a few frames or it will be laggy for some reason.
/// Don't hate the player hate the game man.
pub fn make_window_visible(
    window: Single<(&mut Window,), With<PrimaryWindow>>,
    mut state: ResMut<NextState<WindowState>>,
    frames: Res<FrameCount>,
    settings: Res<Settings>,
) {
    let (mut window,) = window.into_inner();
    if frames.0 >= 10 {
        window.visible = true;
        window.mode = settings.window_mode;
        state.set(WindowState::Ready);
    }
}

pub fn apply_cursor_changes(
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    window_state: Res<State<WindowState>>,
    state: Res<State<CursorMode>>,
) {
    if *window_state.get() == WindowState::Ready {
        match state.get() {
            CursorMode::Normal => {
                **cursor = CursorOptions {
                    grab_mode: CursorGrabMode::None,
                    visible: true,
                    ..default()
                };
            }
            CursorMode::Locked => {
                **cursor = CursorOptions {
                    grab_mode: CursorGrabMode::Locked,
                    visible: false,
                    ..default()
                };
            }
        }
    }
}
