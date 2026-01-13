use bevy::prelude::*;
use data::locale::Locale;

use crate::ui::{
    UiVars,
    button::{ButtonAction, ButtonVisuals},
    menus::{Menu, MenuBody, MenuRoot},
};

#[rustfmt::skip]
pub fn draw(
    locale: Res<Locale>,
    vars: Res<UiVars>,
    mut commands: Commands,
) {
    commands.spawn(MenuRoot::bundle(Menu::Pause))
        .with_child(MenuBody::bundle(&vars))
        .with_children(|parent| {
            // Disconnect to title menu.
            parent.spawn((
                ButtonAction::Transition(Menu::Title),
                ButtonVisuals::text(locale.get("ui.common.disconnect"),  Val::Percent(100.0)).bundle(&vars)
            ));
        });
}
