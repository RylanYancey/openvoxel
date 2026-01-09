use bevy::{diagnostic::FrameCount, prelude::*, window::PrimaryWindow};

use crate::settings::Settings;

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
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut state: ResMut<NextState<WindowState>>,
    frames: Res<FrameCount>,
    settings: Res<Settings>,
) {
    if frames.0 >= 10 {
        window.visible = true;
        window.mode = settings.window_mode;
        state.set(WindowState::Ready);
    }
}
