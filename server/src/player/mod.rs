use bevy::{ecs::entity::EntityHashMap, prelude::*};
use data::registry::Registry;
use protocol::{
    session::Session,
    types::{EntityUpdate, PlayerInput},
};

use crate::{
    events::{PlayerJoined, PlayerLeft},
    net::channel::Channel,
};
use table::Players;

pub mod table;

#[derive(Component, Copy, Clone)]
pub struct Player {
    /// The Session assigned to the Player.
    pub session: Session,

    /// The Version of the players input state.
    /// Used to determine if updates from the client
    /// should be discarded or applied.
    pub version: u32,
}

pub fn spawn_player_on_join(
    mut commands: Commands,
    mut joined_evs: MessageReader<PlayerJoined>,
    mut players: ResMut<Players>,
) {
    for ev in joined_evs.read() {
        info!("Player Joined with session: '{:?}'", ev.session);
        let id = commands
            .spawn((
                // TODO: load from memory if player has joined before, otherwise choose spawn location.
                Transform::from_xyz(0.0, 0.0, 0.0),
                //
                Player {
                    session: ev.session,
                    version: 0,
                },
            ))
            .id();
        players.insert(ev.session, id);
    }
}

pub fn despawn_player_on_leave(
    mut commands: Commands,
    mut left_evs: MessageReader<PlayerLeft>,
    mut players: ResMut<Players>,
) {
    for ev in left_evs.read() {
        info!(
            "Player with session: '{:?}' has left with exit code '{:?}'.",
            ev.session, ev.exit
        );
        if let Some(entry) = players.remove(ev.session) {
            commands.entity(entry.entity).despawn();
        }
    }
}

pub fn apply_player_input(// channels: Res<Registry<Channel>>,
    // players: Res<Players>,
    // mut q: Query<(&mut Transform, &mut Player)>,
) {
    // let channel = channels
    //     .get_by_name("player-input")
    //     .expect("player-input channel not added to channels registry.");

    // for (session, payload) in channel.decode::<EntityUpdate<PlayerInput>>() {
    //     match payload {
    //         Err(e) => panic!("[S382] Error while reading player input payload: '{e:?}'"),
    //         Ok(payload) => {
    //             if let Some(entity) = players.entity(session) {
    //                 if let Ok((mut transform, mut player)) = q.get_mut(entity)
    //                     && player.version < payload.version
    //                 {
    //                     player.version = payload.version;
    //                     transform.translation = payload.position;
    //                     transform.rotation = payload.look_rot;
    //                 }
    //             }
    //         }
    //     }
    // }
}
