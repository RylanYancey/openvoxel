use bevy::prelude::*;

#[derive(Resource)]
pub struct Config {
    /// A radius describing how close a player needs to
    /// be to a chunk for them to receive voxel updates
    /// from that chunk.
    pub draw_distance: i32,

    /// A radius describing how close a player needs to be
    /// to a chunk for entity updates from that chunk to
    /// be sent.
    pub sim_distance: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            draw_distance: 8,
            sim_distance: 4,
        }
    }
}
