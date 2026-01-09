use std::io;

use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task, futures_lite},
};
use data::{
    registry::Registry,
    sequence::{RivuletError, RivuletState, Sequence, Sequences},
};

use crate::{
    events::{PlayerConnected, SyncRegistries},
    net::{Channel, Client},
};

#[derive(Default, States, Eq, PartialEq, Debug, Clone, Hash)]
pub enum ConnectSeq {
    #[default]
    Inactive,
    Establishing,
    Authenticating,
    Syncronizing,
}

impl Sequences for ConnectSeq {
    fn is_active(&self) -> bool {
        *self != Self::Inactive
    }

    fn first() -> Self {
        Self::Establishing
    }

    fn next(&self) -> Option<Self> {
        match *self {
            Self::Inactive => Some(Self::Establishing),
            Self::Establishing => Some(Self::Authenticating),
            Self::Authenticating => Some(Self::Syncronizing),
            Self::Syncronizing => None,
        }
    }
}

#[derive(Resource)]
pub struct ConnectSeqInfo {
    pub addr_string: String,
}

/// Establishes a connection and writes an authentication payload.
pub fn establish_initial_connection(
    seq: Res<Sequence<ConnectSeq>>,
    info: Res<ConnectSeqInfo>,
    mut task: Local<Option<Task<io::Result<Client>>>>,
    mut commands: Commands,
) {
    let mut rivulet = seq.get("establish");
    match rivulet.state {
        RivuletState::Uninit => {
            *task = Some(IoTaskPool::get().spawn(Client::connect(info.addr_string.clone())));
            rivulet.state = RivuletState::InProgress;
        }
        RivuletState::InProgress => {
            if let Some(task) = task.take_if(|t| t.is_finished()) {
                rivulet.state = RivuletState::Finished;
                match futures_lite::future::block_on(task) {
                    Ok(client) => {
                        commands.insert_resource(client);
                    }
                    Err(e) => {
                        seq.set_error(RivuletError {
                            err_code: "[N441]",
                            err_text: e.to_string(),
                        });
                    }
                }
            }
        }
        _ => {}
    }
}

/// Wait for the server to accept the connection
pub fn authenticate_connection(
    seq: Res<Sequence<ConnectSeq>>,
    mut msgs: MessageReader<PlayerConnected>,
) {
    if let Some(mut rivulet) = seq.get_in_progress("authenticate") {
        if let Some(_) = msgs.read().next() {
            rivulet.state = RivuletState::Finished;
        }
    }
}

/// Wait for the server to send a registry synchronization payload.
pub fn synchronize_registries(
    seq: Res<Sequence<ConnectSeq>>,
    mut msgs: MessageReader<SyncRegistries>,
    mut channels: ResMut<Registry<Channel>>,
) {
    if let Some(mut rivulet) = seq.get_in_progress("synchronize") {
        if let Some(msg) = msgs.read().next() {
            channels
                .make_compliant(msg.payload.get("channels").unwrap())
                .unwrap();
            rivulet.state = RivuletState::Finished;
        }
    }
}
