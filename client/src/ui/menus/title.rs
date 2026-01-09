use super::{Menu, MenuRoot};
use crate::ui::{
    UiVars,
    button::{ButtonAction, ButtonVisuals},
    menus::MenuBody,
};
use bevy::prelude::*;
use data::{info::Version, locale::Locale};

/// Draw the title screen.
/// Should fire on enter into Menu::Title
#[rustfmt::skip]
pub fn draw(
    version: Res<Version>,
    locale: Res<Locale>,
    vars: Res<UiVars>,
    mut commands: Commands
) {
    commands.spawn(MenuRoot::bundle(Menu::Title)).with_children(|parent| {
        let text = format!("{} {}", locale.get("ui.title.version"), &**version);
        parent.spawn((
            // Version information, bottom left.
            Text::new(text),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            Node {
               position_type: PositionType::Absolute,
               bottom: Val::Px(5.0),
               left: Val::Px(15.0),
               ..default()
            }
        ));

        parent.spawn((
            // Copyright (infringement) notice, bottom-right.
            Text::new(locale.get("ui.title.copyright-notice")),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(5.0),
                right: Val::Px(15.0),
                ..default()
            }
        ));

        parent.spawn(MenuBody::bundle(&vars)).with_children(|parent| {
            parent.spawn((
                // Central Container for main content.
                Node {
                    width: Val::Percent(80.0),
                    max_width: Val::Px(1280.0),
                    height: Val::Percent(100.0),
                    margin: UiRect::horizontal(Val::Auto),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
            )).with_children(|parent| {
                parent.spawn((
                    // Game title at top of menu
                    Text::new(locale.get("ui.common.openvoxel")),
                    TextLayout::new_with_justify(Justify::Center),
                    TextFont {
                        font_size: 40.0,
                        ..default()
                    },
                    Node::default()
                ));

                parent.spawn((
                    // Container for all title menu buttons.
                    Node {
                        width: Val::Percent(80.0),
                        height: Val::Percent(100.0),
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Start,
                        margin: UiRect::horizontal(Val::Auto),
                        ..default()
                    },
                )).with_children(|parent| {
                    // Transition to World Select
                    parent.spawn((
                        ButtonAction::Transition(Menu::WorldSelect),
                        ButtonVisuals::text(locale.get("ui.common.world-select"),  Val::Percent(100.0)).bundle(&vars)
                    ));

                    // Transition to Server Select
                    parent.spawn((
                        ButtonAction::Transition(Menu::Connecting),
                        ButtonVisuals::text(locale.get("ui.common.server-select"), Val::Percent(100.0)).bundle(&vars),
                    ));

                    // Transition to options menu
                    parent.spawn((
                        ButtonAction::Transition(Menu::Options),
                        ButtonVisuals::text(locale.get("ui.common.options"), Val::Percent(100.0)).bundle(&vars),
                    ));

                    // Quit game button.
                    parent.spawn((
                       ButtonAction::Quit,
                       ButtonVisuals::text(locale.get("ui.common.quit"), Val::Percent(100.0)).bundle(&vars),
                    ));
                });
            });
        });
    });
}
