use bevy::prelude::*;
use data::sequence::{SequenceEnded, Sequences, SequencesPlugin};

use crate::{
    sequences::{connect::ConnectSeq, starting::StartupSeq},
    states::{AppState, InputState},
    ui::menus::Menu,
};

pub mod connect;
pub mod starting;

pub struct SequencePlugin;

impl Plugin for SequencePlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                SequencesPlugin::<ConnectSeq>::default(),
                SequencesPlugin::<StartupSeq>::default(),
            ))
            .add_systems(OnEnter(Menu::Starting), (
                // trigger starting sequence when the "Menu::Starting" state is entered.
                data::util::transition(StartupSeq::first())
                    .run_if(in_state(StartupSeq::Inactive)),
            ))
            .add_systems(Update, (
                // Rivulets related to connecting to a server.
                connect::establish_initial_connection
                    .run_if(in_state(ConnectSeq::Establishing)),
                connect::authenticate_connection
                    .run_if(in_state(ConnectSeq::Authenticating)),
                connect::synchronize_registries
                    .run_if(in_state(ConnectSeq::Syncronizing)),
            ))
            .add_systems(Last, (
                // on StartupSeq completion
                (
                    data::util::transition(Menu::Title),
                ).run_if(on_message::<SequenceEnded<StartupSeq>>),
                // on ConnectSeq completion
                (
                    data::util::transition(InputState::Free),
                    data::util::transition(AppState::InGame),
                ).run_if(on_message::<SequenceEnded<ConnectSeq>>),
            ))
        ;
    }
}
