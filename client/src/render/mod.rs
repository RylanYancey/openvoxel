use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};

use crate::{
    render::{
        atlases::{BlockTextureMeta, TextureArrayPlugin},
        chunk::{ChunkMaterial, ChunkRenderQueue, ChunkRenderer},
    },
    states::AppState,
};

pub mod atlases;
pub mod chunk;
pub mod skybox;

pub struct OpenvoxelRenderPlugin;

impl Plugin for OpenvoxelRenderPlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                TextureArrayPlugin::<BlockTextureMeta>::default()
                    .with_file("textures/blocks/stone.png"),
                MaterialPlugin::<ChunkMaterial>::default(),
            ))
            .init_resource::<ChunkRenderQueue>()
            .init_resource::<ChunkRenderer>()
            .add_systems(Update, (
                skybox::spawn_skybox,
                chunk::render_chunks
                    .run_if(in_state(AppState::InGame))
                    .run_if(on_timer(Duration::from_secs_f32(1.0 / 20.0))),
            ))
        ;
    }
}
