use bevy::{ecs::entity::EntityHashMap, prelude::*};
use data::registry::Registry;
use protocol::{packet::Version, session::Session, types::EntityUpdate};

use crate::{
    events::{PlayerJoined, PlayerLeft},
    net::channel::Channel,
};
use table::Players;

pub mod table;
pub mod update;

pub struct ServerPlayerPlugin;

impl Plugin for ServerPlayerPlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .init_resource::<table::Players>()
            .add_systems(Update, (
                update::apply_input_updates,
                spawn_player_on_join,
                despawn_player_on_leave,
            ))
        ;
    }
}

#[derive(Component, Copy, Clone)]
pub struct Player {
    /// The Session assigned to the Player.
    pub session: Session,

    /// The Version of the players input state.
    /// Used to determine if updates from the client
    /// should be discarded or applied.
    pub version: Version,
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
                    version: Version::ZERO,
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
