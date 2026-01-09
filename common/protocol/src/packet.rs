use std::marker::PhantomData;

use crate::{exit::ExitCode, session::Session};
use bevy::prelude::*;
use bytemuck::Pod;
use bytes::Bytes;
use serde::Serialize;

#[derive(Clone)]
pub struct Packet {
    pub payload: Bytes,
    pub session: Session,
    pub channel: ChannelId,
}

impl Packet {
    pub fn cast<T: Pod>(&self) -> Option<&T> {
        if self.payload.len() >= std::mem::size_of::<T>() {
            Some(bytemuck::from_bytes(&self.payload))
        } else {
            None
        }
    }

    pub fn from_json<T: Serialize>(channel: ChannelId, session: Session, item: &T) -> Self {
        let buf = serde_json::to_vec(item).unwrap();
        Self {
            payload: Bytes::from(buf),
            channel,
            session,
        }
    }
}

impl From<(Session, ExitCode)> for Packet {
    fn from(value: (Session, ExitCode)) -> Self {
        Self {
            payload: value.1.to_bytes(),
            session: value.0,
            channel: ChannelId::EXIT_CODE,
        }
    }
}

/// Unique Identifier for a Channel, on which messages
/// can be sent between the client and server.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ChannelId(pub usize);

impl ChannelId {
    /// Sent over TCP to indicate disconnection, either by
    /// the Server forcing a disconnect or a disconnection
    /// requested by the Client.
    pub const EXIT_CODE: Self = Self(65535);

    /// Sent from the Client to the Server on join.
    pub const AUTH_REQ: Self = Self(65534);

    /// Sent from the server to the client to synchronize registry state.
    pub const SYNC_DATA: Self = Self(65533);

    pub fn is_special(self) -> bool {
        self.0 >= 32768
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Protocol {
    /// Reliable, Ordered, High-Latency
    Tcp,

    /// Unreliable, Unordered, Low-Latency
    Udp,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SentBy {
    Server = 0,
    Client = 1,
    Both = 2,
}
