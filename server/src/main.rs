#![feature(allocator_api)]

use std::time::Duration;

use bevy::{
    app::{
        App, AppExit, PanicHandlerPlugin, ScheduleRunnerPlugin, TaskPoolPlugin,
        TerminalCtrlCHandlerPlugin,
    },
    asset::AssetPlugin,
    diagnostic::DiagnosticsPlugin,
    log::LogPlugin,
    prelude::*,
    state::app::StatesPlugin,
    time::TimePlugin,
    transform::TransformPlugin,
};

use ::world::World;
use data::{queue::Queue, registry::Registry};
use protocol::{Packet, packet::SentBy};

use crate::{
    events::{PlayerJoined, PlayerLeft, SubscChanged},
    net::{InitialMessageContent, Server, channel::Channel},
};

pub mod config;
pub mod events;
pub mod net;
pub mod player;
pub mod queues;
pub mod startup;
pub mod states;
pub mod world;

#[cfg(feature = "tui")]
pub mod tui;

#[rustfmt::skip]
fn main() -> AppExit {
    App::new()
        // add bevy plugins
        .add_plugins((
            PanicHandlerPlugin,
            #[cfg(not(feature = "tui"))]
            LogPlugin::default(),
            TaskPoolPlugin::default(),
            TimePlugin,
            TransformPlugin,
            DiagnosticsPlugin,
            ScheduleRunnerPlugin::run_loop(Duration::from_secs_f32(1.0 / 30.0)),
            TerminalCtrlCHandlerPlugin,
            AssetPlugin::default(),
            StatesPlugin,
            #[cfg(feature = "tui")]
            tui::TuiPlugin,
        ))
        // initialize resources
        .init_resource::<Server>()
        .init_resource::<InitialMessageContent>()
        .init_resource::<player::table::Players>()
        .init_sync_registry::<Channel>("channels")
        .insert_resource(World::new(256, -128))
        // initialize messages
        .add_message::<PlayerJoined>()
        .add_message::<PlayerLeft>()
        .add_message::<SubscChanged>()
        // add channels
        .add_channel("player-input", SentBy::Client)
        .add_channel("chunk-data", SentBy::Server)
        // add startup systems
        .add_systems(Startup, (
            startup::bind_server_to_localhost,
            startup::build_world,
        ))
        // add pre-update systems
        .add_systems(PreUpdate, (
            net::process_server_events,
            net::recv_incoming_messages
        ))
        // add update systems
        .add_systems(Update, (
            send_chunk_data_to_player_on_join,
            player::apply_player_input,
            player::spawn_player_on_join,
            player::despawn_player_on_leave,
        ))
        // add post-update systems
        .add_systems(PostUpdate, (
            net::flush_server_buffers,
        ))
        .run()
}

pub trait AppExt {
    /// Add a channel on which data can be sent and/or received.
    fn add_channel(&mut self, name: impl Into<String>, sent_by: SentBy) -> &mut Self;

    /// Initialize a Registry that is sent to the client on join.
    fn init_sync_registry<T>(&mut self, name: impl Into<String>) -> &mut Self
    where
        T: Send + Sync + 'static;
}

impl AppExt for App {
    fn add_channel(&mut self, name: impl Into<String>, sent_by: SentBy) -> &mut Self {
        let name = name.into();
        self.main_mut()
            .world_mut()
            .get_resource_mut::<Registry<Channel>>()
            .unwrap_or_else(|| {
                panic!("[S381] Attempted to create channel with name: '{name}', but the Channels registry has not been added.")
            })
            .insert(name, Channel::new(sent_by));
        self
    }

    fn init_sync_registry<T>(&mut self, name: impl Into<String>) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        let name = name.into();
        self.init_resource::<Registry<T>>();
        self.add_systems(
            PostStartup,
            move |registry: Res<Registry<T>>, mut initial: ResMut<InitialMessageContent>| {
                initial.add_registry(name.clone(), registry.get_names());
            },
        )
    }
}

fn send_chunk_data_to_player_on_join(
    mut evs: MessageReader<PlayerJoined>,
    world: Res<World>,
    channels: Res<Registry<Channel>>,
    mut server: ResMut<Server>,
) {
    const TEST_CHUNK_COORDS: IVec2 = IVec2::new(32, 32);
    let channel = channels.resolve("chunk-data").unwrap().into();

    for ev in evs.read() {
        let zip = world
            .get_chunk(TEST_CHUNK_COORDS)
            .unwrap()
            .zip(zip::Algorithm::Zstd, zip::ZipLevel::High);

        info!("Sent chunk data with length {}.", zip.0.len());

        assert!(server.tcp_send(Packet {
            payload: zip.0,
            session: ev.session,
            channel,
        }));
    }
}
