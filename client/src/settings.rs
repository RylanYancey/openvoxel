use bevy::{prelude::*, window::WindowMode};

/// Settings the client has configured that need to be saved.
#[derive(Resource, Reflect)]
pub struct Settings {
    /// Fullscreen, Windowed, or BorderlessFullscreen.
    pub window_mode: WindowMode,

    /// Currently selected localization.
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            window_mode: WindowMode::Windowed,
            language: "en-us".into(),
        }
    }
}
