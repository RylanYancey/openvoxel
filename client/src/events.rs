use bevy::prelude::*;
use protocol::{ExitCode, session::Session, types::RegistrySyncPacket};

/// Sent when the server sends a map of entry names to indices.
#[derive(Message)]
pub struct SyncRegistries {
    pub payload: RegistrySyncPacket,
}

/// The player is fully connected and ready to enter the simulation.
#[derive(Message)]
pub struct PlayerConnected {
    pub session: Session,
}

/// The player submitted a message through the chat box.
#[derive(Message, Clone)]
pub struct ChatBoxSubmit(pub String);
