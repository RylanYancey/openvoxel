use bevy::prelude::*;
use data::registry::Registry;
use protocol::types::PlayerInputUpdate;

use crate::{
    net::channel::Channel,
    player::{Player, table::Players},
};

pub fn apply_input_updates(
    channels: Res<Registry<Channel>>,
    players: Res<Players>,
    mut q: Query<(&mut Transform, &mut Player)>,
) {
    for packet in channels.get_by_name("player-input").unwrap() {
        if let Some(update) = packet.cast::<PlayerInputUpdate>() {
            if let Some(entity) = players.entity(packet.session) {
                if let Ok((mut transform, mut player)) = q.get_mut(entity) {
                    if player.version.update(update.version) {
                        transform.translation = update.translation;
                    }
                }
            }
        }
    }
}
