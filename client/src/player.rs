use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};
use protocol::session::Session;

use crate::{events::PlayerConnected, states::InputState};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, (
                spawn_player,
            ))
            .add_systems(Update, (
                player_apply_look_deltas,
                player_compute_look_deltas
                    .before(player_apply_look_deltas)
                    .run_if(in_state(InputState::Free)),
                player_apply_move_deltas,
                player_compute_move_deltas
                    .run_if(in_state(InputState::Free))
                    .before(player_apply_move_deltas),
                on_connect_success
                    .run_if(on_message::<PlayerConnected>)
            ))
        ;
    }
}

#[derive(Component)]
pub struct Player {
    pub session: Session,
}

#[derive(Component, Default)]
pub struct PlayerController {
    pub look_deltas: Vec2,
    pub move_deltas: Vec3,
}

#[derive(Component, Default)]
pub struct MainCamera;

#[derive(Component)]
pub struct PlayerHead;

#[derive(Component)]
pub struct PlayerBody;

fn on_connect_success(mut player: Single<&mut Player>, mut evs: MessageReader<PlayerConnected>) {
    for ev in evs.read() {
        info!("Player connected with session: {:?}", ev.session);
        player.session = ev.session;
    }
}

/// Player spawns at program start, but can't move until they join a game.
#[rustfmt::skip]
fn spawn_player(mut commands: Commands) {
    commands
        .spawn((
            Player {
                session: Session::ZERO,
            },
            PlayerBody,
            PlayerController::default(),
            InheritedVisibility::VISIBLE,
            Transform::default(),
        ))
        .with_child((
            PlayerHead,
            Transform::default(),
            Camera3d::default(),
            MainCamera,
        ));
}

fn player_compute_look_deltas(
    mut player: Single<&mut PlayerController>,
    motion: Res<AccumulatedMouseMotion>,
) {
    player.look_deltas = motion.delta * 0.005;
}

fn player_compute_move_deltas(
    player: Single<(&mut PlayerController, &Children)>,
    q_head: Query<&Transform, With<PlayerHead>>,
    buttons: Res<ButtonInput<KeyCode>>,
) {
    let (mut player, children) = player.into_inner();
    player.move_deltas = Vec3::ZERO;

    for child in children {
        if let Ok(head_pos) = q_head.get(*child) {
            let forward = head_pos.forward().as_vec3().with_y(0.0).normalize();
            let right = head_pos.right().as_vec3().with_y(0.0).normalize();

            if buttons.pressed(KeyCode::KeyW) {
                player.move_deltas += forward;
            }

            if buttons.pressed(KeyCode::KeyS) {
                player.move_deltas -= forward;
            }

            if buttons.pressed(KeyCode::KeyA) {
                player.move_deltas -= right;
            }

            if buttons.pressed(KeyCode::KeyD) {
                player.move_deltas += right;
            }

            if buttons.pressed(KeyCode::KeyC) {
                player.move_deltas.y -= 1.0;
            }

            if buttons.pressed(KeyCode::Space) {
                player.move_deltas.y += 1.0;
            }

            return;
        }
    }
}

fn player_apply_move_deltas(
    player: Single<(&mut PlayerController, &mut Transform)>,
    time: Res<Time>,
) {
    let (mut player, mut transform) = player.into_inner();
    if player.move_deltas != Vec3::ZERO {
        let deltas = player.move_deltas * time.delta_secs() * 0.01;
        transform.translation += deltas.normalize();
        player.move_deltas = Vec3::ZERO;
    }
}

fn player_apply_look_deltas(
    player: Single<(&PlayerController, &Children), With<Player>>,
    mut q_head: Query<&mut Transform, With<PlayerHead>>,
) {
    const PITCH_LIMIT: f32 = 1.5707964 - 0.01;

    let (player, children) = player.into_inner();
    if player.look_deltas != Vec2::ZERO {
        for child in children {
            if let Ok(mut transform) = q_head.get_mut(*child) {
                let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::YXZ);
                transform.rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    yaw - player.look_deltas.x,
                    (pitch - player.look_deltas.y).clamp(-PITCH_LIMIT, PITCH_LIMIT),
                    roll,
                );
            }
        }
    }
}
