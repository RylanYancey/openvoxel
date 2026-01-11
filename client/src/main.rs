use std::time::Duration;

use bevy::{
    prelude::*,
    time::common_conditions::on_timer,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowMode},
};
use data::{OpenvoxelDataPlugin, registry::Registry};
use protocol::packet::SentBy;
use world::{World, region::format::UnzippedChunk};

use crate::{
    events::{PlayerConnected, SyncRegistries},
    net::Channel,
    render::chunk::ChunkRenderQueue,
    sequences::connect::ConnectSeq,
    settings::Settings,
    startup::WindowState,
    states::{AppState, ConnectState, GameTick, InputState, PlayerStance, TickSets, on_tick},
};

pub mod events;
pub mod net;
pub mod player;
pub mod render;
pub mod sequences;
pub mod settings;
pub mod startup;
pub mod states;
pub mod ui;

#[rustfmt::skip]
fn main() -> AppExit {
    App::new()
        .add_plugins((
            OpenvoxelDataPlugin,
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        visible: false,
                        resolution: (1280, 720).into(),
                        title: "Open Voxel".into(),
                        mode: WindowMode::Windowed,
                        ..default()
                    }),
                    ..default()
                }),
            player::PlayerPlugin,
            ui::OpenvoxelUiPlugin,
            sequences::SequencePlugin,
            render::OpenvoxelRenderPlugin,
        ))
        // initialize states
        .init_state::<WindowState>()
        .init_state::<AppState>()
        .init_state::<ConnectState>()
        .init_state::<PlayerStance>()
        .init_state::<InputState>()
        // initialize resources
        .init_resource::<Settings>()
        .init_resource::<GameTick>()
        .insert_resource(World::new(256, -128))
        // initialize registries that need to be synchronized with the server on join.
        .init_sync_registry::<Channel>("channels")
        // initialize channels for sending/receiving packets.
        .add_channel("player-input", SentBy::Client)
        .add_channel("chunk-data", SentBy::Server)
        // add messages
        .add_message::<SyncRegistries>()
        .add_message::<PlayerConnected>()
        // configure system sets
        .configure_sets(PostUpdate, (
            TickSets::Flush.run_if(on_tick(0)),
        ))
        .configure_sets(Update, (
            TickSets::Flush.run_if(on_tick(0)),
        ))
        .configure_sets(PreUpdate, (
            TickSets::Flush.run_if(on_tick(0)),
        ))
        // add First systems
        .add_systems(First,(
            states::update_game_tick
                .run_if(on_timer(Duration::from_secs_f32(1.0 / 20.0))),
            states::set_game_tick_not_just_changed
                .before(states::update_game_tick)
        ))
        // Add Pre-Update systems
        .add_systems(PreUpdate, (
            net::client_recv,
        ))
        // Add Update systems
        .add_systems(Update, (
            startup::make_window_visible
                .run_if(in_state(WindowState::Starting)),
            recv_chunk_data,
            close_on_q,
            on_esc,
        ))
        // on enter Connect menu
        .add_systems(OnEnter(ui::menus::Menu::Connecting), trigger_connect_sequence)
        // on enter input state free
        .add_systems(OnEnter(InputState::Free), (
            capture_mouse_cursor,
        ))
        .add_systems(OnEnter(InputState::Ui), (
            release_mouse_cursor,
        ))
        // Add Post-Update systems
        .add_systems(PostUpdate, (
            net::client_flush.in_set(TickSets::Flush),
            net::clear_channels,
        ))
        .run()
}

pub trait AppExt {
    /// Add a channel on which data can be sent and/or received.
    fn add_channel(&mut self, name: impl Into<String>, sent_by: SentBy) -> &mut Self;

    /// Initialize a registry that must be synchronized with the server on connection.
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

    fn init_sync_registry<T>(&mut self, _name: impl Into<String>) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        // let name = name.into();
        // let f = move |mut evs: MessageReader<SyncRegistries>, mut registry: ResMut<Registry<T>>| {
        //     for ev in evs.read() {
        //         let to = ev.payload.registries.get(&*name).unwrap();
        //         registry.make_compliant(to).unwrap();
        //     }
        // };

        // // self.init_resource::<Registry<T>>()
        // //     .add_systems(OnEnter(ConnectState::Syncing), f)
        // self

        self.init_resource::<Registry<T>>()
    }
}

/// Make cursor invisible
fn capture_mouse_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor.visible = false;
    cursor.grab_mode = CursorGrabMode::Locked;
}

/// Make cursor visible
fn release_mouse_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

fn on_esc(
    input: Res<ButtonInput<KeyCode>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut state: ResMut<NextState<InputState>>,
) {
    if input.pressed(KeyCode::Escape) {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
        state.set(InputState::Ui);
    }
}

fn close_on_q(input: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if input.pressed(KeyCode::KeyQ) {
        exit.write(AppExit::Success);
    }
}

fn trigger_connect_sequence(mut state: ResMut<NextState<ConnectSeq>>, mut commands: Commands) {
    use data::sequence::Sequences;
    use sequences::connect::ConnectSeqInfo;
    commands.insert_resource(ConnectSeqInfo {
        addr_string: "127.0.0.1:51423".into(),
    });
    state.set(ConnectSeq::first())
}

fn recv_chunk_data(
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
