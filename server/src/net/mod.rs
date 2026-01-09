use std::{
    io,
    net::{SocketAddr, TcpListener, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{log::error, prelude::*};
use data::registry::Registry;
use protocol::{
    bytes::Bytes,
    codec::{TcpDecoder, TcpEncoder, UdpDecoder, UdpEncoder},
    exit::ExitCode,
    packet::{ChannelId, Packet, Protocol},
    session::Session,
    types::{AuthAccepted, RegistrySyncPacket},
};

mod connection;
use connection::Connections;
mod runtime;
use runtime::Runtime;
pub mod channel;

pub use connection::{Connection, Pending};

use crate::{
    events::{PlayerJoined, PlayerLeft},
    net::channel::Channel,
};

#[derive(Resource)]
pub struct Server {
    /// Tagged Arena of connected clients.
    connections: Connections,

    /// Sockets available for joining the server.
    /// Can be a mix of localhost, LAN, and/or external.
    /// Sockets can be added at runtime, but not removed,
    /// which allows for singleplayer sessions (localhost)
    /// that can then open for LAN, and then to an
    /// external port via UPNP.
    sockets: Vec<Socket>,

    /// Buffer for outgoing TCP packets.
    /// At the end of the tick, these packets
    /// will be submitted to the TCP Runtime for
    /// transmission.
    outgoing_tcp: Vec<Packet>,

    /// Buffer for incoming packets.
    incoming: Vec<Vec<Packet>>,

    /// Handle to dedicated runtime for TCP IO
    runtime: Runtime,
}

impl Server {
    /// Start serving on a socket.
    /// Can be a localhost socket, a LAN socket, or a dedicated server socket.
    /// The same addr will be used for both TCP and UDP.
    pub fn bind(&mut self, addr: SocketAddr) -> io::Result<()> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let udp_socket = Arc::new(UdpSocket::bind(addr)?);
        udp_socket.set_nonblocking(true)?;
        self.sockets.push(Socket {
            listener,
            decoder: UdpDecoder::new(udp_socket.clone()),
        });
        Ok(())
    }

    /// Send a Packet on this Protocol/Channel to the user with the Packets' Session.
    /// Returns "false" if no users exist with the session.
    pub fn send(&mut self, protocol: Protocol, packet: Packet) -> bool {
        if let Some(conn) = self.connections.get_mut(packet.session) {
            match protocol {
                Protocol::Udp => conn.udp_send(packet.channel, &packet.payload),
                Protocol::Tcp => self.outgoing_tcp.push(packet),
            }
            true
        } else {
            false
        }
    }

    /// Send a UDP Packet on this channel to the user with this session.
    /// Returns "false" if no users exist with the session.
    pub fn udp_send(&mut self, session: Session, channel: ChannelId, data: &[u8]) -> bool {
        if let Some(conn) = self.connections.get_mut(session) {
            conn.udp_send(channel, data);
            true
        } else {
            false
        }
    }

    /// Send a TCP Packet on this channel to the user with this session.
    /// Returns "false" if no users exist with the session.
    pub fn tcp_send(&mut self, packet: Packet) -> bool {
        if let Some(_) = self.connections.get_mut(packet.session) {
            self.outgoing_tcp.push(packet);
            true
        } else {
            false
        }
    }

    /// Flush UDP and TCP send buffers.
    pub fn flush(&mut self) {
        // Flush UDP buffers
        for (_, connection) in &mut self.connections {
            connection.flush()
        }

        // Submit TCP messages to the runtime.
        self.runtime.submit(std::mem::take(&mut self.outgoing_tcp));
    }

    ///
    pub fn read_events(&mut self, events: &mut Vec<ServerEvent>) {
        // read incoming connection requests.
        for socket in &mut self.sockets {
            socket.accept_incoming(events);
        }

        // read events from TCP runtime
        while let Some(rt_ev) = self.runtime.read_event() {
            use runtime::RuntimeEvent::*;
            match rt_ev {
                Disconnected { session, exit_code } => {
                    info!("Player Disconnected, session: {session:?}");
                    if self.connections.remove(session).is_some() {
                        events.push(ServerEvent::Exited(session, exit_code));
                    }
                }
                RecvPackets { packets } => {
                    self.incoming.push(packets);
                }
            }
        }
    }

    /// Receive incoming TCP and UDP packets.
    /// Recommended to use this in the start of the tick, after calling read_events.
    pub fn recv(&mut self) -> Vec<Vec<Packet>> {
        let udp = self.recv_udp();
        self.incoming.push(udp);
        std::mem::take(&mut self.incoming)
    }

    /// Receive incoming TCP packets.
    pub fn recv_tcp(&mut self) -> Vec<Vec<Packet>> {
        std::mem::take(&mut self.incoming)
    }

    /// Receive incoming UDP packets.
    pub fn recv_udp(&mut self) -> Vec<Packet> {
        let mut packets = Vec::new();

        // iterate sockets
        for socket in &mut self.sockets {
            // while there is something available to read,
            while let Some((addr, session)) = socket.decoder.read() {
                if let Some(conn) = self.connections.get_mut(session) {
                    conn.udp_encoder.set_address(addr);
                    while let Some((channel, data)) = socket.decoder.decode() {
                        packets.push(Packet {
                            payload: data,
                            channel,
                            session,
                        });
                    }
                }
            }
        }

        packets
    }

    /// Accept a pending connection.
    pub fn accept(&mut self, pending: Pending) -> Session {
        use connection::Connection;
        let session = self.connections.insert(Connection {
            join_time: pending.join_time,
            udp_encoder: UdpEncoder::new(Session::ZERO, pending.socket.clone(), pending.address),
        });
        self.connections
            .get_mut(session)
            .unwrap()
            .udp_encoder
            .set_session(session);
        self.runtime.insert(pending, session);
        session
    }

    /// Reject a pending connection.
    pub fn reject(&mut self, _: Pending) {
        // at some point we will need to send a rejection payload
    }
}

impl Default for Server {
    fn default() -> Self {
        Self {
            connections: Connections::new(),
            sockets: Vec::new(),
            outgoing_tcp: Vec::new(),
            incoming: Vec::new(),
            runtime: Runtime::start(Duration::from_secs_f32(1.0 / 20.0)).unwrap(),
        }
    }
}

struct Socket {
    listener: TcpListener,
    decoder: UdpDecoder,
}

impl Socket {
    fn accept_incoming(&mut self, events: &mut Vec<ServerEvent>) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    // set stream to nonblocking because I'm a gigachad who does nonblocking I/O.
                    if let Err(e) = stream.set_nonblocking(true) {
                        error!("[N156] Failed to set TcpStream to nonblocking with error: '{e}'.");
                        continue;
                    }

                    // set stream to no delay because we are buffering manually.
                    if let Err(e) = stream.set_nodelay(true) {
                        error!("[N157] Failed to set TcpStream to nodelay with error: '{e}'.");
                        continue;
                    }

                    // push event as pending.
                    events.push(ServerEvent::Joined(Pending {
                        stream: mio::net::TcpStream::from_std(stream),
                        address: addr,
                        encoder: TcpEncoder::new(),
                        decoder: TcpDecoder::new(),
                        socket: self.decoder.socket(),
                        packets: Vec::new(),
                        join_time: Instant::now(),
                    }));
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    error!("[N155] Failed to accept from TcpListener with error: '{e}'");
                    return;
                }
            }
        }
    }
}

pub enum ServerEvent {
    Joined(Pending),
    Exited(Session, ExitCode),
}

pub fn process_server_events(
    mut server: ResMut<Server>,
    mut events: Local<Vec<ServerEvent>>,
    mut joined_evs: MessageWriter<PlayerJoined>,
    mut left_evs: MessageWriter<PlayerLeft>,
    mut sync_payload: ResMut<InitialMessageContent>,
) {
    server.read_events(&mut events);

    for ev in events.drain(..) {
        match ev {
            ServerEvent::Joined(pending) => {
                let udp_addr = pending.socket.local_addr().unwrap();
                let session = server.accept(pending);

                // write auth accept packet
                let payload = AuthAccepted { session, udp_addr };
                server.tcp_send(Packet::from_json(ChannelId::AUTH_REQ, session, &payload));

                // write sync payload
                server.tcp_send(sync_payload.into_packet(session));

                // write event
                joined_evs.write(PlayerJoined { session });
            }
            ServerEvent::Exited(session, exit) => {
                left_evs.write(PlayerLeft { session, exit });
            }
        }
    }
}

pub fn flush_server_buffers(mut server: ResMut<Server>) {
    server.flush();
}

#[rustfmt::skip]
pub fn recv_incoming_messages(
    mut server: ResMut<Server>,
    mut channels: ResMut<Registry<Channel>>,
) {
    for packet in server.recv().drain(..).flatten() {
        if let Some(channel) = channels.get_mut(packet.channel) {
            channel.incoming.push(packet);
        }
    }
}

pub fn send_sync_packet(
    mut evs: MessageReader<PlayerJoined>,
    mut content: ResMut<InitialMessageContent>,
    mut server: ResMut<Server>,
) {
    for player in evs.read() {
        server.tcp_send(content.into_packet(player.session));
    }
}

#[derive(Resource, Default)]
pub struct InitialMessageContent {
    pub payload: RegistrySyncPacket<&'static str>,
    pub encoded: Option<Bytes>,
}

impl InitialMessageContent {
    pub fn add_registry(&mut self, name: impl Into<String>, entries: Vec<&'static str>) {
        self.encoded = None;
        self.payload.registries.insert(name.into(), entries);
    }

    pub fn into_packet(&mut self, session: Session) -> Packet {
        Packet {
            session,
            channel: ChannelId::SYNC_DATA,
            payload: self
                .encoded
                .get_or_insert_with(|| Bytes::from(serde_json::to_string(&self.payload).unwrap()))
                .clone(),
        }
    }
}
