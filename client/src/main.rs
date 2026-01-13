use bevy::{prelude::*, window::WindowMode};
use data::{
    OpenvoxelDataPlugin,
    registry::Registry,
    sequence::{SequenceEnded, Sequences, SequencesPlugin},
};
use protocol::packet::SentBy;

use crate::{
    events::{PlayerConnected, SyncRegistries},
    focus::{Focus, PlayerFocusedSet, PlayerNotFocusedSet},
    input::{Actions, Button},
    net::channel::Channel,
    player::Player,
    render::atlases::{BlockTextureMeta, TextureArrayPlugin},
    sequences::{connect::ConnectSeq, starting::StartupSeq},
    settings::Settings,
    states::{AppState, CursorMode, IntoSetConfigs},
    ui::menus::Menu,
    window::WindowState,
};

pub mod events;
pub mod focus;
pub mod input;
pub mod net;
pub mod player;
pub mod render;
pub mod sequences;
pub mod settings;
pub mod states;
pub mod ui;
pub mod window;
pub mod world;

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
            SequencesPlugin::<ConnectSeq>::default(),
            SequencesPlugin::<StartupSeq>::default(),
            MaterialPlugin::<render::chunk::ChunkMaterial>::default(),
            TextureArrayPlugin::<BlockTextureMeta>::default()
                .with_file("textures/blocks/stone.png"),
        ))
        // initialize resources
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(::world::World::new(256, -128))
        .init_resource::<Settings>()
        .init_resource::<focus::FocusManager>()
        .init_resource::<input::Actions>()
        .init_resource::<ui::hint::HintTextContent>()
        .init_resource::<ui::UiVars>()
        .init_resource::<ui::util::UiLabels>()
        .init_resource::<ui::chat::ChatBox>()
        .init_resource::<render::chunk::ChunkRenderQueue>()
        .init_resource::<render::chunk::ChunkRenderer>()
        // initialize states
        .init_state::<AppState>()
        .init_state::<CursorMode>()
        .init_state::<Menu>()
        .init_state::<WindowState>()
        // initialize registries that need to be synchronized with the server on join.
        .init_sync_registry::<Channel>("channels")
        // initialize channels for sending/receiving packets.
        .add_channel("player-input", SentBy::Client)
        .add_channel("chunk-data", SentBy::Server)
        // add messages
        .add_message::<SyncRegistries>()
        .add_message::<PlayerConnected>()
        .add_message::<focus::FocusRequested>()
        .add_message::<focus::FocusChanged>()
        .add_message::<ui::button::ButtonClicked>()
        // add keybinds
        .add_action("forward", [KeyCode::KeyW.into()])
        .add_action("back", [KeyCode::KeyS.into()])
        .add_action("left", [KeyCode::KeyA.into()])
        .add_action("right", [KeyCode::KeyD.into()])
        .add_action("interact", [MouseButton::Right.into()])
        .add_action("punch", [MouseButton::Left.into()])
        .add_action("close-menu", [KeyCode::Escape.into()])
        .add_action("focus-chatbox", [KeyCode::KeyT.into(), KeyCode::Slash.into()])
        // add action handlers
        .add_action_handler("close-menu", input::handle_close_menu_transitions)
        .add_action_handler("focus-chatbox", ui::chat::handle_focus_chatbox)
        // configure system sets
        .configure_set_all(PlayerFocusedSet)
        .configure_set_all(PlayerNotFocusedSet)
        // Add Startup systems
        .add_systems(Startup, (
            player::spawn_player,
            ui::UiVars::load,
        ))
        // Add update systems
        .add_systems(First, (
            ui::util::on_ui_label_add,
            ui::util::on_ui_label_remove,
        ))
        .add_systems(PreUpdate, (
            input::update_actions,
            net::update::client_recv,
        ))
        .add_systems(Update, (
            close_on_q,
            show_info_on_p,
            window::make_window_visible
                .run_if(in_state(WindowState::Starting)),
            states::update_cursor_mode,
            ui::button::handle_menu_button_ix,
            ui::hint::insert_hint_text_visuals,
            ui::hint::update_hint_text_entities,
            (
                world::io::recv_chunk_data,
                player::player_apply_look_deltas,
                player::player_compute_look_deltas
                    .before(player::player_apply_look_deltas)
                    .run_if(in_state(CursorMode::Locked)),
                player::player_apply_move_deltas,
                player::player_compute_move_deltas
                    .before(player::player_apply_move_deltas),
            ).run_if(in_state(AppState::InGame)),
            sequences::connect::establish_initial_connection
                .run_if(in_state(ConnectSeq::Establishing)),
            sequences::connect::authenticate_connection
                .run_if(in_state(ConnectSeq::Authenticating)),
            sequences::connect::synchronize_registries
                .run_if(in_state(ConnectSeq::Syncronizing)),
            render::skybox::load_skybox_assets
                .run_if(in_state(StartupSeq::LoadTextures))
        ))
        .add_systems(FixedUpdate, (
            (
                ui::chat::update_chatbox,
                player::send_player_input_update,
                render::chunk::render_chunks
            ).run_if(in_state(AppState::InGame)),
        ))
        .add_systems(PostUpdate, (
            net::update::clear_channels,
        ))
        .add_systems(FixedPostUpdate, (
            net::update::client_flush,
        ))
        .add_systems(Last, (
            focus::update_focus_manager,
            ui::menus::starting::update_progress_bar,
            (
                data::util::transition(Menu::Title),
            ).run_if(on_message::<SequenceEnded<StartupSeq>>),
            (
                data::util::transition(AppState::InGame),
            ).run_if(on_message::<SequenceEnded<ConnectSeq>>),
        ))
        // Add Transitional Systems
        .add_systems(OnEnter(Menu::Connecting), (
            trigger_connect_sequence,
        ))
        .add_systems(OnEnter(Menu::Starting), (
            data::util::transition(StartupSeq::first())
                .run_if(in_state(StartupSeq::Inactive)),
            ui::menus::starting::draw,
        ))
        .add_systems(OnEnter(Menu::Title), (
            ui::menus::title::draw,
        ))
        .add_systems(OnEnter(AppState::InGame), (
            player::on_connect_success,
            render::skybox::spawn_skybox,
            ui::chat::draw_chatbox,
        ))
        .add_systems(OnExit(AppState::InGame), (
            render::skybox::despawn_skybox,
        ))
        .add_systems(OnEnter(CursorMode::Normal), window::apply_cursor_changes)
        .add_systems(OnEnter(CursorMode::Locked), window::apply_cursor_changes)
        .run()
}

pub trait AppExt {
    fn add_action(
        &mut self,
        action: impl AsRef<str>,
        buttons: impl IntoIterator<Item = Button>,
    ) -> &mut Self;

    fn add_action_handler<M>(
        &mut self,
        action: impl AsRef<str>,
        system: impl IntoSystem<(), (), M> + 'static,
    ) -> &mut Self;

    /// Configure a system set for all schedules.
    fn configure_set_all<S: IntoSetConfigs>(&mut self, set: S) -> &mut Self;

    /// Add a channel on which data can be sent and/or received.
    fn add_channel(&mut self, name: impl Into<String>, sent_by: SentBy) -> &mut Self;

    /// Initialize a registry that must be synchronized with the server on connection.
    fn init_sync_registry<T>(&mut self, name: impl Into<String>) -> &mut Self
    where
        T: Send + Sync + 'static;
}

impl AppExt for App {
    fn add_action(
        &mut self,
        action: impl AsRef<str>,
        buttons: impl IntoIterator<Item = Button>,
    ) -> &mut Self {
        self.main_mut()
            .world_mut()
            .get_resource_mut::<Actions>()
            .unwrap()
            .add(action.as_ref(), buttons);
        self
    }

    fn add_action_handler<M>(
        &mut self,
        action: impl AsRef<str>,
        system: impl IntoSystem<(), (), M> + 'static,
    ) -> &mut Self {
        let world = self.main_mut().world_mut();
        let id = world.register_system(system);
        world
            .get_resource_mut::<Actions>()
            .unwrap()
            .add_trigger(action, id);
        self
    }

    fn configure_set_all<S: IntoSetConfigs>(&mut self, set: S) -> &mut Self {
        self.configure_sets(FixedUpdate, set.cfg())
            .configure_sets(FixedPreUpdate, set.cfg())
            .configure_sets(FixedPostUpdate, set.cfg())
            .configure_sets(Update, set.cfg())
            .configure_sets(PreUpdate, set.cfg())
            .configure_sets(PostUpdate, set.cfg())
    }

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
        // let name = name.into();
        // let f = move |mut evs: MessageReader<SyncRegistries>, mut registry: ResMut<Registry<T>>| {
        //     for ev in evs.read() {
        //         let to = ev.payload.registries.get(&*name).unwrap();
        //         registry.make_compliant(to).unwrap();
        //     }
        // };

        self.init_resource::<Registry<T>>()
        // .add_systems(OnEnter(ConnectSeq::Syncronizing), f)
    }
}

fn close_on_q(input: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if input.pressed(KeyCode::KeyQ) {
        exit.write(AppExit::Success);
    }
}

fn trigger_connect_sequence(
    mut state: ResMut<NextState<ConnectSeq>>,
    mut commands: Commands,
    mut app_state: ResMut<NextState<AppState>>,
) {
    use data::sequence::Sequences;
    use sequences::connect::ConnectSeqInfo;
    commands.insert_resource(ConnectSeqInfo {
        addr_string: "127.0.0.1:51423".into(),
    });
    state.set(ConnectSeq::first());
    app_state.set(AppState::InSequence);
}

#[rustfmt::skip]
fn show_info_on_p(
    buttons: Res<ButtonInput<KeyCode>>,
    app_state: Res<State<AppState>>,
    cursor_mode: Res<State<CursorMode>>,
    focus: Focus,
    player: Query<Entity, With<Player>>,
) {
    if buttons.just_pressed(KeyCode::KeyP) {
        let info = DebugInfo {
            app_state: *app_state.get(),
            cursor_mode: *cursor_mode.get(),
            focused: focus.curr(),
            player: player.single().unwrap(),
        };

        info!("DEBUG INFO: {info:?}");
    }
}

#[derive(Debug)]
struct DebugInfo {
    app_state: AppState,
    cursor_mode: CursorMode,
    focused: Option<Entity>,
    player: Entity,
}
