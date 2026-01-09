use crate::ui::UiVars;

use super::{menus::Menu, util::Last};
use bevy::prelude::*;

/// Event that fires when a menu button is clicked.
#[derive(Message)]
pub struct ButtonClicked {
    /// Entity the ButtonAction is attached to.
    pub entity: Entity,

    /// The action of the button that fired.
    pub action: ButtonAction,
}

#[derive(Component, Clone, Debug, Default)]
#[require(Interaction, Last<Interaction>)]
pub enum ButtonAction {
    /// Button does nothing.
    #[default]
    None,

    /// Request to transition to another menu.
    /// Handled in the handle_menu_button_ix system.
    Transition(Menu),

    /// Set to whatever and listen for click events.
    Other(String),

    /// Exit was requested.
    /// Handled in the handle_menu_button_ix system.
    Quit,
}

/// Detects and handles button clicks.
/// A click only triggers when the pointer is released over a button.
/// Emits `ButtonClicked` events.
pub fn handle_menu_button_ix(
    mut click_evs: MessageWriter<ButtonClicked>,
    mut next_menu: ResMut<NextState<Menu>>,
    mut quit_evs: MessageWriter<AppExit>,
    mut query: Query<
        (Entity, &ButtonAction, &Interaction, &mut Last<Interaction>),
        Changed<Interaction>,
    >,
) {
    for (entity, action, ix, mut last) in &mut query {
        if *ix == Interaction::Hovered && last.0 == Interaction::Pressed {
            match action {
                ButtonAction::Transition(menu) => {
                    info!("Transition to menu: '{menu:?}'");
                    next_menu.set(*menu);
                }
                ButtonAction::Quit => {
                    info!("Quit requested via button action.");
                    quit_evs.write(AppExit::Success);
                }
                _ => {}
            }

            click_evs.write(ButtonClicked {
                action: action.clone(),
                entity,
            });
        }

        last.0 = *ix;
    }
}

/// Descriptor for a button's visuals.
/// Applied automatically by the apply_button_visuals function.
#[derive(Component)]
pub enum ButtonVisuals {
    Text { content: String, width: Val },
}

impl ButtonVisuals {
    pub fn text(content: impl Into<String>, width: Val) -> Self {
        Self::Text {
            content: content.into(),
            width,
        }
    }

    pub fn bundle(self, vars: &UiVars) -> impl Bundle {
        match &self {
            Self::Text { content, width } => (
                Text::new(content),
                TextLayout::new_with_justify(Justify::Center),
                BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
                TextFont {
                    font: vars.font.clone(),
                    font_size: 20.0,
                    ..default()
                },
                Node {
                    width: *width,
                    padding: UiRect::axes(Val::Px(30.0), Val::Px(20.0)),
                    margin: UiRect::vertical(Val::Px(30.0)),
                    ..default()
                },
            ),
        }
    }
}
