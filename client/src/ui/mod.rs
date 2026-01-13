use bevy::prelude::*;

pub mod button;
pub mod chat;
pub mod elements;
pub mod hint;
pub mod menus;
pub mod util;

#[derive(Resource)]
pub struct UiVars {
    pub font: Handle<Font>,
    pub menu_body_max_width: Val,
    pub menu_body_width: Val,
    pub chat_box_width: Val,
}

impl UiVars {
    pub fn font(&self) -> Handle<Font> {
        self.font.clone()
    }

    pub fn load(server: Res<AssetServer>, mut vars: ResMut<UiVars>) {
        vars.font = server.load("fonts/ReturnOfTheBossRegular-E407g.ttf");
    }
}

impl Default for UiVars {
    fn default() -> Self {
        Self {
            font: Handle::default(),
            menu_body_max_width: Val::Px(1280.0),
            menu_body_width: Val::Percent(80.0),
            chat_box_width: Val::Vw(10.0),
        }
    }
}

#[derive(Component, Clone, Default, Deref, Eq, PartialEq, Hash)]
pub struct UiLabel(pub String);

impl UiLabel {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}
