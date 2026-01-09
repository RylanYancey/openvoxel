use bevy::prelude::*;

use crate::ui::UiVars;

#[derive(Resource, Default)]
pub struct HintTextContent {
    pub sections: Vec<String>,
}

impl HintTextContent {
    pub fn set(&mut self, text: impl Into<String>) {
        self.sections.clear();
        self.sections.push(text.into());
    }

    pub fn push(&mut self, text: impl Into<String>) {
        self.sections.push(text.into())
    }

    pub fn clear(&mut self) {
        self.sections.clear();
    }
}

#[derive(Component)]
pub struct HintText;

pub fn update_hint_text_content() {}

pub fn update_hint_text_entities(
    hint: Res<HintTextContent>,
    q_text: Query<&mut Text, With<HintText>>,
) {
    if hint.is_changed() {
        for mut text in q_text {
            text.0 = hint.sections.join("\n");
        }
    }
}

pub fn insert_hint_text_visuals(
    content: Res<HintTextContent>,
    q_text: Query<Entity, Added<HintText>>,
    vars: Res<UiVars>,
    mut commands: Commands,
) {
    for entity in &q_text {
        commands.entity(entity).insert((
            Text::new(content.sections.join("\n")),
            TextLayout::new_with_justify(Justify::Center),
            TextFont {
                font: vars.font.clone(),
                font_size: 15.0,
                ..default()
            },
        ));
    }
}
