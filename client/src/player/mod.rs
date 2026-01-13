use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};
use data::registry::Registry;
use protocol::{packet::Version, session::Session, types::PlayerInputUpdate};

pub mod input;

use crate::{
    focus::{Focus, Focused},
    net::{Client, channel::Channel},
};

#[derive(Component)]
pub struct Player {
    pub session: Session,
    pub version: Version,
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

pub fn on_connect_success(
    player: Single<(Entity, &mut Player, &mut Transform)>,
    mut focus: Focus,
    client: Res<Client>,
) {
    let (_, mut player, mut transform) = player.into_inner();
    info!("Player connected with session: {:?}", client.session());
    player.session = client.session();
    player.version = Version::ZERO;
    transform.translation = vec3(0.0, 64.0, 0.0);
    focus.to_player();
}

/// Player spawns at program start, but can't move until they join a game.
#[rustfmt::skip]
pub fn spawn_player(mut commands: Commands) {
    commands
        .spawn((
            Player {
                session: Session::ZERO,
                version: Version::ZERO,
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

pub fn player_compute_look_deltas(
    mut player: Single<&mut PlayerController>,
    motion: Res<AccumulatedMouseMotion>,
) {
    player.look_deltas = motion.delta * 0.005;
}

pub fn player_compute_move_deltas(
    mut player: Query<(&mut PlayerController, &Children), With<Focused>>,
    q_head: Query<&Transform, With<PlayerHead>>,
    buttons: Res<ButtonInput<KeyCode>>,
) {
    if let Ok((mut player, children)) = player.single_mut() {
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
}

pub fn player_apply_move_deltas(
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

pub fn player_apply_look_deltas(
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

pub fn send_player_input_update(
    channels: Res<Registry<Channel>>,
    player: Single<(&mut Player, &Transform)>,
    mut client: ResMut<Client>,
) {
    let channel = channels.resolve("player-input").unwrap().into();
    let (mut player, transform) = player.into_inner();

    let update = PlayerInputUpdate {
        version: player.version.next(),
        translation: transform.translation,
        look_dir: Quat::from_rotation_x(1.0),
    };

    client.tcp_send(channel, bytemuck::bytes_of(&update));
}
