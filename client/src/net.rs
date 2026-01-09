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

use crate::events::{PlayerConnected, SyncRegistries};

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

#[derive(Resource, Default)]
pub struct Client {
    transport: Option<Transport>,
    packets: Vec<Packet>,
    authenticated: bool,
}

impl Client {
    pub async fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        // establish connection
        let mut stream = TcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;

        // create socket
        let socket = Arc::new(UdpSocket::bind("0.0.0.0:0")?);
        socket.set_nonblocking(true)?;

        // write authentication payload
        let auth_req = AuthRequest {
            udp_addr: socket.local_addr()?,
        };
        stream.write_all(&auth_req.encode())?;

        Ok(Self {
            transport: Some(Transport::new(
                TcpEncoder::new(),
                TcpDecoder::new(),
                stream,
                socket,
                "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
                Session::ZERO,
            )),
            packets: Vec::new(),
            authenticated: false,
        })
    }

    pub fn drain(&mut self) -> std::vec::Drain<'_, Packet> {
        self.packets.drain(..)
    }

    pub fn auth_accepted(&mut self, session: Session, udp_addr: SocketAddr) {
        self.authenticated = true;
        self.transport
            .as_mut()
            .unwrap()
            .on_auth_accept(session, udp_addr);
    }

    pub fn udp_send(&mut self, channel: ChannelId, payload: impl AsRef<[u8]>) {
        if let Some(transport) = &mut self.transport {
            transport.udp_send(channel, payload.as_ref());
        }
    }

    pub fn tcp_send(&mut self, channel: ChannelId, payload: impl AsRef<[u8]>) {
        if let Some(transport) = &mut self.transport {
            transport.tcp_send(channel, payload.as_ref())
        }
    }

    pub fn disconnect(&mut self, exit: Option<ExitCode>) {
        if let Some(mut transport) = self.transport.take() {
            transport.disconnect(exit.unwrap_or_default());
            self.authenticated = false;
        }
    }

    pub fn recv(&mut self) -> Result<Vec<Packet>, ExitCode> {
        if let Some(transport) = &mut self.transport {
            if let Err(e) = transport.recv(&mut self.packets) {
                self.disconnect(Some(e.clone()));
                return Err(e);
            }
        }

        Ok(std::mem::take(&mut self.packets))
    }

    pub fn take_packets(&mut self) -> Vec<Packet> {
        std::mem::take(&mut self.packets)
    }

    pub fn flush(&mut self) -> Result<(), ExitCode> {
        if let Some(transport) = &mut self.transport {
            if let Err(e) = transport.flush() {
                self.disconnect(Some(e.clone()));
                return Err(e);
            }
        }

        Ok(())
    }
}

pub struct Transport {
    pub tcp_encoder: TcpEncoder,
    pub tcp_decoder: TcpDecoder,
    pub udp_encoder: UdpEncoder,
    pub udp_decoder: UdpDecoder,
    pub udp_socket: Arc<UdpSocket>,
    pub tcp_stream: TcpStream,
    pub session: Session,
}

impl Transport {
    pub fn new(
        tcp_encoder: TcpEncoder,
        tcp_decoder: TcpDecoder,
        tcp_stream: TcpStream,
        udp_socket: Arc<UdpSocket>,
        udp_send_addr: SocketAddr,
        session: Session,
    ) -> Self {
        Self {
            tcp_encoder,
            tcp_decoder,
            udp_decoder: UdpDecoder::new(udp_socket.clone()),
            udp_encoder: UdpEncoder::new(session, udp_socket.clone(), udp_send_addr),
            udp_socket,
            tcp_stream,
            session,
        }
    }

    pub fn disconnect(&mut self, exit: ExitCode) {
        self.tcp_encoder.encode_exit(&exit);
        let _ = self.tcp_encoder.flush(&mut self.tcp_stream);
        let _ = self.tcp_stream.shutdown(std::net::Shutdown::Both);
    }

    pub fn on_auth_accept(&mut self, session: Session, udp_addr: SocketAddr) {
        self.udp_encoder.set_session(session);
        self.udp_encoder.set_address(udp_addr);
    }

    pub fn udp_send(&mut self, channel: ChannelId, data: &[u8]) {
        self.udp_encoder.encode(channel, data);
    }

    pub fn tcp_send(&mut self, channel: ChannelId, data: &[u8]) {
        self.tcp_encoder.encode(channel, data);
    }

    pub fn flush(&mut self) -> Result<(), ExitCode> {
        self.udp_encoder.flush();
        self.tcp_encoder.flush(&mut self.tcp_stream)?;
        Ok(())
    }

    pub fn recv(&mut self, packets: &mut Vec<Packet>) -> Result<(), ExitCode> {
        // recv TCP packets first
        self.tcp_decoder
            .collect(&mut self.tcp_stream, self.session, packets)?;

        // recv udp packets AFTER tcp packets (this IS intentional)
        while let Some((addr, session)) = self.udp_decoder.read() {
            if session == self.session {
                self.udp_encoder.set_address(addr);
                while let Some((channel, payload)) = self.udp_decoder.decode() {
                    packets.push(Packet {
                        session,
                        channel,
                        payload,
                    })
                }
            }
        }

        Ok(())
    }
}

/// A channel on which data can be sent and/or received.
pub struct Channel {
    /// The "side" that sends and/or receives the data.
    pub sent_by: SentBy,

    /// Messages that are ready to be processed.
    pub incoming: Vec<Packet>,
}

impl Channel {
    pub fn new(sent_by: SentBy) -> Self {
        Self {
            sent_by,
            incoming: Vec::new(),
        }
    }

    pub fn recv(&self) -> impl Iterator<Item = &Packet> {
        self.incoming.iter()
    }
}

pub fn clear_channels(mut channels: ResMut<Registry<Channel>>) {
    for channel in channels.iter_mut() {
        channel.incoming.clear();
    }
}
