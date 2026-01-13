use bevy::{input::keyboard::KeyboardInput, prelude::*};
use data::text::{SpecialKey, TextRecorder};

use crate::{
    focus::{Focus, Focused},
    input::Actions,
    states::AppState,
    ui::UiVars,
};

#[derive(Resource, Default)]
pub struct ChatBox {
    recorder: TextRecorder,
}

#[derive(Component)]
pub struct ChatContainer;

#[derive(Component)]
pub struct ChatScrollView;

#[derive(Component)]
pub struct ChatInputContainer;

/// Action handler for "focus-chatbox"
pub fn handle_focus_chatbox(
    q_box: Query<Entity, With<ChatContainer>>,
    mut q_input: Query<(&mut InheritedVisibility, &mut Text), With<ChatInputContainer>>,
    mut data: ResMut<ChatBox>,
    app_state: Res<State<AppState>>,
    mut focus: Focus,
) {
    if *app_state == AppState::InGame && focus.player_has_focus() {
        if let Ok(container) = q_box.single() {
            // clear any existing text
            data.recorder.clear();

            // transfer focus to the chat box.
            focus.to_entity(container);

            // update visibility of input container and clear any existing text.
            if let Ok((mut vis, mut text)) = q_input.single_mut() {
                *vis = InheritedVisibility::VISIBLE;
                text.clear();
            }
        }
    }
}

pub fn update_chatbox(
    mut data: ResMut<ChatBox>,
    mut keyboard: MessageReader<KeyboardInput>,
    q_container: Query<(&Children, Option<&Focused>), With<ChatContainer>>,
    mut q_input_box: Query<(&mut Text, &mut InheritedVisibility), With<ChatInputContainer>>,
    mut focus: Focus,
    actions: Res<Actions>,
) {
    if let Ok((_, focused)) = q_container.single() {
        if focused.is_some() {
            let mut lose_focus = false;

            if actions.just_activated("close-menu") {
                lose_focus = true;
            } else {
                for ev in keyboard.read() {
                    if let Some(special) = data.recorder.update(&ev) {
                        match special {
                            SpecialKey::Submit => {
                                let content = data.recorder.submit();
                                info!("Chatbox Submit: {content}");
                                lose_focus = true;
                                break;
                            }
                            SpecialKey::NoEffect => {
                                info!("Key had no effect.")
                            }
                            SpecialKey::Autocomplete => {
                                info!("Autocomplete Requested")
                            }
                            SpecialKey::HistoryUp => {
                                info!("History Up Requested")
                            }
                            SpecialKey::HistoryDown => {
                                info!("History Down Requested")
                            }
                        }
                    }
                }
            }

            if let Ok((mut text, mut vis)) = q_input_box.single_mut() {
                if lose_focus {
                    *vis = InheritedVisibility::HIDDEN;
                    text.clear();
                    focus.to_player();
                } else {
                    text.0 = data.recorder.read().to_owned();
                }
            }
        }
    }
}

/// Run OnEnter(AppState::InGame)
#[rustfmt::skip]
pub fn draw_chatbox(
    mut commands: Commands,
    vars: Res<UiVars>,
) {
    commands.spawn((
        ChatContainer,
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
        Node {
            width: Val::Vw(30.0),
            max_height: Val::Vh(30.0),
            flex_direction: FlexDirection::ColumnReverse,
            bottom: Val::Px(0.0),
            left: Val::Px(0.0),
            padding: UiRect::all(Val::Px(5.0)),
            ..default()
        },
    )).with_children(|parent| {
        parent.spawn((
            ChatInputContainer,
            Text::new(""),
            InheritedVisibility::HIDDEN,
            TextFont {
                font: vars.font(),
                font_size: 10.0,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            TextLayout::new_with_justify(Justify::Left),
            Node {
                min_height: Val::Px(10.0),
                width: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(2.0)),
                ..default()
            }
        ));
    });
}
