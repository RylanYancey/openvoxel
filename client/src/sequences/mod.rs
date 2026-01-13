use bevy::prelude::*;
use data::sequence::{SequenceEnded, Sequences, SequencesPlugin};

use crate::{
    sequences::{connect::ConnectSeq, starting::StartupSeq},
    states::AppState,
    ui::menus::Menu,
};

pub mod connect;
pub mod starting;
