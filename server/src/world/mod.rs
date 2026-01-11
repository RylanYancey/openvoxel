use bevy::prelude::*;

pub mod generator;
pub mod loader;
pub mod subscriber;

pub struct ServerWorldPlugin;

impl Plugin for ServerWorldPlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .init_resource::<subscriber::Subscriber>()
            .init_resource::<loader::WorldLoader>()
            .init_resource::<generator::WorldGenerator>()
            .add_systems(Update, (
                subscriber::process_chunk_send_queues,
                subscriber::recompute_subscriptions,
                generator::process_world_generator_queue,
                loader::process_loader_queues,
            ))
        ;
    }
}
