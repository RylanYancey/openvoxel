//! Terrain generation logic.
//!
//! Order:
//!  - Attribution (temperature, karstness, elevation, biome assignment)
//!  - Terrain (landscape, rivers, caves)
//!  - Structure placement
//!  - Surface decoration (dirt, stone, grass)
//!  - Features (trees, ores, plants)
//!  - Post-processing (snow, fluid propagation)
//!
//! Note that structure placement must come before surface decor and features.
//! This is because a tree could end up blocking a structure like a house, which
//! isn't what we want. This adds some complexity because chunks with structures
//! must be generated first.

use std::sync::Arc;

use bevy::prelude::*;
use data::queue::PriorityQueue;
use math::{noise::simplex::simplex2, rng::Permutation};
use world::{
    Voxel, World,
    region::chunk::{ChunkId, flags::ChunkState},
};

pub mod structures;
pub mod terrain;

#[derive(Resource)]
pub struct WorldGenerator {
    queue: PriorityQueue<ChunkId, u32>,
    perm: Arc<Permutation>,
}

impl WorldGenerator {
    /// Enqueue a chunk for generation.
    /// The "distance" is used to compute priority, and should be
    /// the chebyshev distance from the player to the chunk's origin.
    pub fn enqueue(&mut self, id: impl Into<ChunkId>, distance: u32) {
        let prio = 16 - u32::min(distance >> 5, 15);
        self.queue.push_increase(id.into(), prio);
    }
}

impl Default for WorldGenerator {
    fn default() -> Self {
        Self {
            queue: PriorityQueue::default(),
            perm: Permutation::from_entropy(),
        }
    }
}

pub fn process_world_generator_queue(
    mut generator: ResMut<WorldGenerator>,
    mut world: ResMut<World>,
) {
    if let Some((id, _)) = generator.queue.pop() {
        if let Some(chunk) = world.get_chunk_mut(id.as_ivec2()) {
            for pt in chunk.area() {
                let y = chunk.min_y()
                    + ((1.0 + simplex2(&generator.perm, pt.as_vec2() * 0.007)) * 128.0) as i32;
                let mut top = ivec3(pt.x, y, pt.y);
                while top.y >= chunk.min_y() {
                    chunk.set_voxel(top, Voxel(1));
                    top.y -= 1;
                }
            }
            *chunk.load_state_mut() = ChunkState::Loaded;
        }
    }
}
