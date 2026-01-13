//! Shared message types

use std::net::SocketAddr;

use bevy::prelude::*;
use bytemuck::{Pod, Zeroable};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use fxhash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{ChannelId, packet::Version, session::Session};

#[derive(Copy, Clone, Deref, DerefMut)]
pub struct EntityUpdate<T> {
    pub version: u32,
    pub entity_id: u32,
    #[deref]
    pub payload: T,
}

impl<T> EntityUpdate<T> {
    pub fn new(version: u32, entity_id: u32, payload: T) -> Self {
        Self {
            version,
            entity_id,
            payload,
        }
    }
}

/// Sent from the client to server to inform the server of player state change.
/// Does not include scale because that is controlled by the server.
#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct PlayerInputUpdate {
    pub version: Version,
    pub translation: Vec3,
    pub look_dir: Quat,
}

#[derive(Default, Serialize, Deserialize)]
pub struct RegistrySyncPacket<A: AsRef<str> = String> {
    pub registries: FxHashMap<String, Vec<A>>,
}

impl<A: AsRef<str>> RegistrySyncPacket<A> {
    pub fn get(&self, name: impl AsRef<str>) -> Option<&Vec<A>> {
        self.registries.get(name.as_ref())
    }
}

#[derive(Serialize, Deserialize)]
pub struct AuthRequest {
    pub udp_addr: SocketAddr,
}

impl AuthRequest {
    pub fn encode(self) -> Bytes {
        let payload = serde_json::to_string(&self).unwrap();
        let mut buffer = BytesMut::with_capacity(payload.len() + 6);
        buffer.put_u32_le(payload.len() as u32);
        buffer.put_u16_le(ChannelId::AUTH_REQ.0 as u16);
        buffer.put_slice(payload.as_bytes());
        buffer.freeze()
    }
}

#[derive(Message, Serialize, Deserialize)]
pub struct AuthAccepted {
    pub session: Session,
    pub udp_addr: SocketAddr,
}
