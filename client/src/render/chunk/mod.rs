use std::collections::VecDeque;

use bevy::{
    prelude::*,
    render::{render_resource::AsBindGroup, storage::ShaderStorageBuffer},
};
use data::blockstates::Transparency;
use world::{Region, VoxelState, World, region::chunk_is_fully_contained};

use crate::render::{
    atlases::{BlockTextureMeta, TextureArray},
    chunk::combiner::QuadCombiner,
};

pub mod combiner;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ChunkMaterial {
    /// Comes from TextureArray::<BlockTextureMeta>.images
    #[texture(0, dimension = "2d_array")]
    #[sampler(1)]
    pub atlas: Handle<Image>,

    /// Comes from TextureArray::<BlockTextureMeta>.gpu_data.
    /// Used for getting additional information about an entry
    /// in the atlas, including the index of the entry in the atlas.
    #[storage(2, read_only)]
    pub table: Handle<ShaderStorageBuffer>,

    /// Transparency mode of the quads in the mesh.
    pub alpha: AlphaMode,
}

impl Material for ChunkMaterial {
    fn alpha_mode(&self) -> AlphaMode {
        self.alpha
    }

    fn vertex_shader() -> bevy::shader::ShaderRef {
        "shaders/chunk.wgsl".into()
    }

    fn fragment_shader() -> bevy::shader::ShaderRef {
        "shaders/chunk.wgsl".into()
    }
}

struct Task {
    origin: IVec2,
}

#[derive(Resource, Default)]
pub struct ChunkRenderQueue {
    queue: VecDeque<Task>,
}

impl ChunkRenderQueue {
    pub fn add(&mut self, origin: IVec2) {
        self.queue.push_back(Task {
            origin: IVec2 {
                x: origin.x & !31,
                y: origin.y & !31,
            },
        });
    }

    fn take(&mut self, limit: usize) -> impl Iterator<Item = Task> {
        self.queue.drain(0..usize::min(self.queue.len(), limit))
    }
}

#[derive(Resource)]
pub struct ChunkRenderer {
    chunks_per_tick: usize,
    combiner: QuadCombiner,
}

impl Default for ChunkRenderer {
    fn default() -> Self {
        Self {
            chunks_per_tick: 1,
            combiner: QuadCombiner::new(),
        }
    }
}

pub fn render_chunks(
    mut tasks: ResMut<ChunkRenderQueue>,
    mut renderer: ResMut<ChunkRenderer>,
    atlas: Res<TextureArray<BlockTextureMeta>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ChunkMaterial>>,
    world: Res<World>,
) {
    let stone_texture = atlas.resolve("textures/blocks/stone.png").unwrap() as i16;

    for task in tasks.take(renderer.chunks_per_tick) {
        renderer.combiner.clear_all();
        let origin = ivec3(task.origin.x, world.min_y(), task.origin.y);
        let is_contained = chunk_is_fully_contained(origin.xz());
        if let Some(region) = world.get_region(origin.xz()) {
            for y in (origin.y..world.max_y()).step_by(32) {
                let origin = origin.with_y(y);
                if is_contained {
                    // build using `Region::get_state()`.
                    subchunk_fn::build_subchunk(
                        &mut renderer.combiner,
                        region,
                        origin,
                        stone_texture,
                    );
                } else {
                    // build using `World::get_state()`.
                    subchunk_fn::build_subchunk(
                        &mut renderer.combiner,
                        &*world,
                        origin,
                        stone_texture,
                    );
                }
            }
        }

        if let Some(mesh) = renderer.combiner.combine(Transparency::Opaque) {
            commands.spawn((
                Transform {
                    translation: origin.as_vec3(),
                    ..default()
                },
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(ChunkMaterial {
                    atlas: atlas.image(),
                    table: atlas.table(),
                    alpha: AlphaMode::Opaque,
                })),
            ));
        }
    }
}

trait GetBlock {
    fn get_block(&self, pos: IVec3) -> Option<VoxelState>;
}

impl GetBlock for World {
    fn get_block(&self, pos: IVec3) -> Option<VoxelState> {
        self.get_state(pos)
    }
}

impl GetBlock for Region {
    fn get_block(&self, pos: IVec3) -> Option<VoxelState> {
        self.get_state(pos)
    }
}

mod subchunk_fn {
    use super::GetBlock;
    use bevy::math::{IVec3, ivec3};
    use data::blockstates::{
        Transparency,
        quad::{FULL_BLOCK, Normal},
    };
    use math::axis::{Axis, AxisArray};
    use world::{Voxel, World};

    use crate::render::chunk::combiner::QuadCombiner;

    pub fn build_subchunk<G: GetBlock>(
        combiner: &mut QuadCombiner,
        get: &G,
        origin: IVec3,
        stone_texture: i16,
    ) {
        for y in 0..32 {
            let offs_y = ((origin.y + y) * 16) as i16;
            for x in 0..32 {
                let offs_x = (x * 16) as i16;
                for z in 0..32 {
                    let pt = origin + ivec3(x, y, z);
                    let center = get.get_block(pt).unwrap();
                    if center.voxel == Voxel(1) {
                        let offs = [offs_x, offs_y, (z * 16) as i16];
                        for axis in Axis::ALL {
                            if get
                                .get_block(axis + pt)
                                .is_none_or(|state| state.voxel == Voxel::AIR)
                            {
                                let quad =
                                    FULL_BLOCK[axis].offset_with_texture(offs, stone_texture);
                                combiner.add(quad, Transparency::Opaque, Normal::Aligned(axis));
                            }
                        }
                    }
                }
            }
        }
    }
}
