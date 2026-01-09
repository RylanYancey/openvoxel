use bevy::prelude::*;
use data::{locale::Locale, sequence::Sequence};

use crate::{
    sequences::starting::StartupSeq,
    ui::{
        UiVars,
        menus::{Menu, MenuBody, MenuRoot},
    },
};

#[derive(Component)]
pub struct ProgressBar;

pub fn update_progress_bar(
    mut q: Query<&mut Text, With<ProgressBar>>,
    state: Res<State<StartupSeq>>,
    seq: Res<Sequence<StartupSeq>>,
) {
    if let Some((curr_stage, num_stages)) = state.get().stage_index() {
        let stage_width = 1.0 / num_stages as f32;
        let progress = (stage_width * curr_stage as f32) + seq.progress_in_stage() * stage_width;
        for mut text in &mut q {
            text.0 = format!("{progress:.2}%")
        }
    }
}

#[rustfmt::skip]
pub fn draw(
    mut commands: Commands,
    vars: Res<UiVars>,
    locale: Res<Locale>,
) {
    commands.spawn((
        MenuRoot::bundle(Menu::Starting),
        BackgroundColor(Color::srgb(0.188, 0.098, 0.215))
    )).with_children(|parent| {
        parent.spawn(MenuBody::bundle(&vars)).with_children(|parent| {
            parent.spawn((
                Text::new(locale.get("ui.studio-name")),
                TextFont {
                    font: vars.font(),
                    font_size: 30.0,
                    ..default()
                },
                TextColor::WHITE,
                Node {
                    width: Val::Percent(100.0),
                    ..default()
                }
            ));

            parent.spawn((
                ProgressBar,
                Text::new("0%"),
                TextColor::WHITE,
                TextFont {
                    font: vars.font(),
                    font_size: 30.0,
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
