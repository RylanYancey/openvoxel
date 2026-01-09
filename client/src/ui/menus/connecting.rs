use bevy::prelude::*;
use data::locale::Locale;

use crate::ui::{
    UiVars,
    menus::{Menu, MenuBody, MenuRoot},
};

/// Text to show whilst connecting to a server..
#[derive(Resource)]
pub struct ConnectingText {
    /// The text that shows at the top of the screen.
    /// Should be something like "Connecting To Server..." or "Loading World...".
    pub header: String,

    /// Text used to inform the client of what is happening.
    pub hint_text: String,

    /// Optional progress amount. If unused set to -1.
    pub progress: f32,

    /// Whether to show a cancel button.
    pub show_cancel: bool,

    /// The menu to transition to on cancel.
    pub cancel_to: Menu,
}

#[rustfmt::skip]
pub fn draw(
    mut commands: Commands,
    locale: Res<Locale>,
    vars: Res<UiVars>,
) {
    commands.spawn(MenuRoot::bundle(Menu::Connecting)).with_children(|parent| {
        parent.spawn(MenuBody::bundle(&*vars)).with_children(|parent| {
            parent.spawn((
                Text::new(locale.get("ui.connecting")),
                TextLayout::new_with_justify(Justify::Center),
                Node {
                    width: Val::Percent(100.0),
                    ..default()
                }
            ));

            parent.spawn((
                Text::new(locale.get("placeholder")),
                TextLayout::new_with_justify(Justify::Center),
                TextFont {
                    font: vars.font(),
                    font_size: 20.0,
                    ..default()
                },
                Node {
                    width: Val::Percent(100.0),
                    ..default()
                }
            ));
        });
    });
}
