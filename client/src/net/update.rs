use bevy::prelude::*;
use data::registry::Registry;

use std::{
    io::{self, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket},
    sync::Arc,
};

use protocol::{
    ChannelId, ExitCode, Packet,
    codec::{TcpDecoder, TcpEncoder, UdpDecoder, UdpEncoder},
    packet::SentBy,
    session::Session,
    types::{AuthAccepted, AuthRequest},
};

use crate::{
    events::{PlayerConnected, SyncRegistries},
    net::{Client, channel::Channel},
};
pub fn client_recv(
    mut client: Option<ResMut<Client>>,
    mut channels: ResMut<Registry<Channel>>,
    mut sync_msgs: MessageWriter<SyncRegistries>,
    mut connect_msgs: MessageWriter<PlayerConnected>,
) {
    if let Some(client) = &mut client {
        let mut packets = client.recv().unwrap();
        for packet in packets.drain(..) {
            match packet.channel {
                channel if !packet.channel.is_special() => {
                    if let Some(channel) = channels.get_mut(channel) {
                        channel.incoming.push(packet);
                    } else {
                        warn!(
                            "[C933] Packet was received, but its channel was unknown. '{channel:?}'"
                        )
                    }
                }
                ChannelId::SYNC_DATA => {
                    let payload = serde_json::from_slice(&packet.payload).unwrap();
                    sync_msgs.write(SyncRegistries { payload });
                }
                ChannelId::AUTH_REQ => {
                    if !client.authenticated {
                        let response =
                            serde_json::from_slice::<AuthAccepted>(&packet.payload).unwrap();
                        client.auth_accepted(response.session, response.udp_addr);
                        connect_msgs.write(PlayerConnected {
                            session: response.session,
                        });
                    }
                }
                channel => {
                    warn!(
                        "[C932] Special packet was received, but its channel was unknown. '{channel:?}'"
                    );
                }
            }
        }

        client.packets = packets;
    }
}

pub fn client_flush(mut client: Option<ResMut<Client>>) {
    if let Some(client) = &mut client {
        client.flush().unwrap()
    }
}

pub fn clear_channels(mut channels: ResMut<Registry<Channel>>) {
    for channel in channels.iter_mut() {
        channel.incoming.clear();
    }
}
