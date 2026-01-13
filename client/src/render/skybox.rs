use bevy::{
    asset::LoadState,
    core_pipeline::Skybox,
    prelude::*,
    render::render_resource::{TextureViewDescriptor, TextureViewDimension},
};
use data::sequence::{RivuletState, Sequence};
use fxhash::FxHashMap;

use crate::{player::MainCamera, sequences::starting::StartupSeq};

#[derive(Resource, Default)]
pub struct SkyboxAssets {
    handles: FxHashMap<String, SkyboxAsset>,
}

impl SkyboxAssets {
    pub fn get(&self, name: impl AsRef<str>) -> Option<Handle<Image>> {
        let name = name.as_ref();
        if let Some(handle) = self.handles.get(name) {
            if !handle.loading {
                return Some(handle.handle.clone());
            } else {
                warn!("[C910] Skybox image with name: '{name}' exists but is still loading.")
            }
        } else {
            warn!("[C909] Skybox image with name: '{name}' did not exist.");
        }

        None
    }
}

struct SkyboxAsset {
    handle: Handle<Image>,
    loading: bool,
}

impl SkyboxAsset {
    fn new(handle: Handle<Image>) -> Self {
        Self {
            handle,
            loading: true,
        }
    }
}

pub fn spawn_skybox(
    mut commands: Commands,
    assets: Res<SkyboxAssets>,
    camera: Single<Entity, With<MainCamera>>,
) {
    commands.entity(*camera).insert((Skybox {
        image: assets.get("day").unwrap(),
        brightness: 1000.0,
        ..default()
    },));
}

pub fn despawn_skybox(mut commands: Commands, camera: Single<Entity, With<MainCamera>>) {
    commands.entity(*camera).remove::<Skybox>();
}

/// Runs in the LoadTextures schedule.
pub fn load_skybox_assets(
    sequence: Res<Sequence<StartupSeq>>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut skyboxes: Option<ResMut<SkyboxAssets>>,
    mut commands: Commands,
) {
    let mut rivulet = sequence.get("load-skyboxes");
    match rivulet.state {
        RivuletState::Uninit => {
            rivulet.state = RivuletState::InProgress;
            let mut handles = FxHashMap::default();
            handles.insert(
                "day".into(),
                SkyboxAsset::new(assets.load("skybox/day/cubemap.png")),
            );
            commands.insert_resource(SkyboxAssets { handles });
        }
        RivuletState::InProgress => {
            let mut num_finished = 0;
            let skyboxes = skyboxes.as_mut().unwrap();
            for (name, handle) in &mut skyboxes.handles {
                if handle.loading {
                    if let Some(state) = assets.get_load_state(&handle.handle) {
                        match state {
                            LoadState::NotLoaded => {}
                            LoadState::Loading => {}
                            LoadState::Loaded => {
                                let image = images.get_mut(&handle.handle).unwrap();
                                image.reinterpret_stacked_2d_as_array(
                                    image.height() / image.width(),
                                );
                                image.texture_view_descriptor = Some(TextureViewDescriptor {
                                    dimension: Some(TextureViewDimension::Cube),
                                    ..default()
                                });
                                num_finished += 1;
                                handle.loading = false;
                            }
                            LoadState::Failed(e) => {
                                panic!(
                                    "[C811] Failed to load skybox '{}' with error: '{e:?}'",
                                    name
                                )
                            }
                        }
                    }
                } else {
                    num_finished += 1;
                }
            }

            let total = skyboxes.handles.len();
            rivulet.progress = total as f32 / num_finished as f32;
            if total == num_finished {
                rivulet.state = RivuletState::Finished;
            }
        }
        RivuletState::Finished => {}
    }
}
