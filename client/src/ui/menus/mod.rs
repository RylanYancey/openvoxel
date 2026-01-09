use bevy::prelude::*;

use crate::ui::UiVars;

pub mod connecting;
pub mod server_select;
pub mod starting;
pub mod title;
pub mod world_select;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, States, Default)]
pub enum Menu {
    /// Main game load sequence.
    #[default]
    Starting,

    ///
    Title,

    /// Select world to join. (Singleplayer)
    WorldSelect,

    /// Select server to join. (Multiplayer)
    ServerSelect,

    /// Connecting to Server
    Connecting,

    /// Settings menu
    Options,
}

#[derive(Component)]
pub struct MenuRoot;

impl MenuRoot {
    pub fn bundle<S: States>(despawn_on_exit: S) -> impl Bundle {
        (
            Self,
            DespawnOnExit(despawn_on_exit),
            Node {
                width: Val::Vw(100.0),
                height: Val::Vh(100.0),
                ..default()
            },
        )
    }
}

#[derive(Component)]
pub struct MenuBody;

impl MenuBody {
    pub fn bundle(vars: &UiVars) -> impl Bundle {
        (
            Self,
            Node {
                width: vars.menu_body_width,
                max_width: vars.menu_body_max_width,
                height: Val::Percent(100.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                margin: UiRect {
                    top: Val::Percent(5.0),
                    left: Val::Auto,
                    right: Val::Auto,
                    bottom: Val::Px(0.0),
                },
                ..default()
            },
        )
    }
}
