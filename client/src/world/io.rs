use bevy::prelude::*;
use data::registry::Registry;
use world::region::format::UnzippedChunk;

use crate::{net::channel::Channel, render::chunk::ChunkRenderQueue};
use ::world::World;

pub fn recv_chunk_data(
    channels: Res<Registry<Channel>>,
    mut world: ResMut<World>,
    mut queue: ResMut<ChunkRenderQueue>,
) {
    let channel = channels.get_by_name("chunk-data").unwrap();
    for packet in channel.recv() {
        let unzip = UnzippedChunk::unzip(&packet.payload).unwrap();
        let success = world.read_unzipped_chunk(unzip, true).unwrap();
        queue.add(success.origin.xz());
    }
}
