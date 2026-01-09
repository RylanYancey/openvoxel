use std::net::SocketAddr;

use bevy::prelude::*;
use math::{noise::worley::Worley3, rng::Permutation};

use crate::net::Server;
use world::{Voxel, World};

/// Binds Server to LocalHost.
pub fn bind_server_to_localhost(mut server: ResMut<Server>) {
    server
        .bind("127.0.0.1:51423".parse::<SocketAddr>().unwrap())
        .unwrap();
}

pub fn build_world(mut world: ResMut<World>) {
    let chunk = world
        .get_or_insert_region(IVec2::new(0, 0))
        .get_chunk_mut(IVec2::new(32, 32))
        .unwrap();
    let origin = chunk.origin().with_y(0);
    let perm = Permutation::from_entropy();
    let mut worley = Worley3::new(perm, Vec3::splat(1.0 / 32.0));

    for x in 0..32 {
        for z in 0..32 {
            for y in world.min_y()..world.max_y() {
                let pt = origin + ivec3(x, y, z);
                let (_, l1, l2) = worley.l2(pt.as_vec3());
                if (l2 - l1) < 0.2 {
                    world.set_voxel(pt, Voxel(1));
                }
            }
        }
    }
}
