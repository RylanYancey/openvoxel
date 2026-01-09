use bevy::{
    core_pipeline::Skybox,
    prelude::*,
    render::render_resource::{TextureViewDescriptor, TextureViewDimension},
};

pub fn spawn_skybox(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut handle: Local<Option<Handle<Image>>>,
    mut is_loaded: Local<bool>,
    camera: Single<Entity, With<Camera3d>>,
    assets: Res<AssetServer>,
) {
    if *is_loaded {
        return;
    }

    match &*handle {
        None => *handle = Some(assets.load::<Image>("skybox/day/cubemap.png")),
        Some(handle) => {
            if let Some(img) = images.get_mut(handle) {
                *is_loaded = true;
                img.reinterpret_stacked_2d_as_array(img.height() / img.width());
                img.texture_view_descriptor = Some(TextureViewDescriptor {
                    dimension: Some(TextureViewDimension::Cube),
                    ..default()
                });

                commands.entity(*camera).insert(Skybox {
                    image: handle.clone(),
                    brightness: 1000.0,
                    ..default()
                });
            }
        }
    }
}
