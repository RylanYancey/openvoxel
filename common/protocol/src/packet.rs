use crate::{exit::ExitCode, session::Session};
use bytemuck::{Pod, Zeroable};
use bytes::Bytes;
use serde::Serialize;

#[derive(Clone)]
pub struct Packet {
    pub payload: Bytes,
    pub session: Session,
    pub channel: ChannelId,
}

impl Packet {
    pub fn cast<T: Pod>(&self) -> Option<T> {
        bytemuck::try_pod_read_unaligned::<T>(&self.payload).ok()
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

/// Struct for versioning UDP component updates.
#[derive(Pod, Zeroable, Copy, Clone, Default, Eq, PartialEq)]
#[repr(C)]
pub struct Version(u32);

impl Version {
    pub const ZERO: Self = Self(0);

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn next(&mut self) -> Self {
        self.0 = self.0.wrapping_add(1);
        Self(self.0)
    }

    /// If other is more recent than self, self is updated.
    /// Returns "true" if an update occurred.
    pub fn update(&mut self, other: Self) -> bool {
        // wrapping around is handled by checking if self is very large and other is very small.
        // It would take 36 hours for this to occur, if at 30 updates per second.
        if other.0 > self.0 || self.0 > 32768 && other.0 < 256 {
            self.0 = other.0;
            true
        } else {
            false
        }
    }
}
