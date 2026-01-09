//! Dedicated process for reading/writing TCP Streams.

use std::{io, time::Duration};

use bevy::log::error;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use mio::{Interest, net::TcpStream};
use protocol::{
    codec::{TcpDecoder, TcpEncoder},
    exit::ExitCode,
    packet::{ChannelId, Packet},
    session::Session,
};

use crate::net::connection::Pending;

pub(super) struct Runtime {
    pub event_rx: Receiver<RuntimeEvent>,
    pub signal_tx: Sender<RuntimeSignal>,
}

impl Runtime {
    pub fn start(tickrate: Duration) -> io::Result<Self> {
        let (signal_tx, signal_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        let state = RuntimeThread {
            tickrate,
            signal_rx,
            event_tx,
            clients: Vec::new(),
            poll: mio::Poll::new()?,
            events: mio::Events::with_capacity(1024),
            exiting: Vec::new(),
        };

        std::thread::spawn(move || rt_main(state));
        Ok(Self {
            event_rx,
            signal_tx,
        })
    }

    pub fn insert(&self, pending: Pending, session: Session) {
        self.signal_tx
            .try_send(RuntimeSignal::Insert { pending, session })
            .unwrap();
    }

    pub fn remove(&self, session: Session, exit: Option<ExitCode>) {
        self.signal_tx
            .try_send(RuntimeSignal::Remove {
                session,
                exit: exit.unwrap_or_default(),
            })
            .unwrap();
    }

    pub fn submit(&self, packets: Vec<Packet>) {
        self.signal_tx
            .try_send(RuntimeSignal::Submit { packets })
            .unwrap();
    }

    pub fn read_event(&mut self) -> Option<RuntimeEvent> {
        self.event_rx.try_recv().ok()
    }
}

/// Sent from Runtime to Server thread.
pub enum RuntimeEvent {
    /// A user has disconnected.
    Disconnected {
        session: Session,
        exit_code: ExitCode,
    },
    /// Packets were received and are ready for processing.
    RecvPackets { packets: Vec<Packet> },
}

/// Sent from server thread to runtime thread.
pub(crate) enum RuntimeSignal {
    /// Start reading/writing to a Tcp client.
    Insert { pending: Pending, session: Session },

    /// Stop reading/writing a TcpClient, and optionally write an exit code.
    Remove { session: Session, exit: ExitCode },

    /// Submit packets to be written to TcpClients.
    Submit { packets: Vec<Packet> },
}

struct TcpClient {
    stream: TcpStream,
    encoder: TcpEncoder,
    decoder: TcpDecoder,
    session: Session,
    readable: bool,
    writable: bool,
}

impl TcpClient {
    fn tick(&mut self, packets: &mut Vec<Packet>) -> Result<(), ExitCode> {
        if self.readable {
            while self.decoder.read(&mut self.stream)? != 0 {
                while let Some((data, channel)) = self.decoder.decode()? {
                    packets.push(Packet {
                        payload: data,
                        session: self.session,
                        channel,
                    });
                }
            }
        }

        if self.writable {
            self.encoder.flush(&mut self.stream)?;
        }

        Ok(())
    }
}

struct RuntimeThread {
    tickrate: Duration,
    signal_rx: Receiver<RuntimeSignal>,
    event_tx: Sender<RuntimeEvent>,
    clients: Vec<Option<TcpClient>>,
    exiting: Vec<TcpClient>,
    events: mio::Events,
    poll: mio::Poll,
}

impl RuntimeThread {
    fn register(&mut self, mut client: TcpClient) {
        let session = client.session;

        // Ensure the client buffer has space, and de-register any clients that would be overwritten.
        if session.index() >= self.clients.len() {
            self.clients.resize_with(session.index() + 1, || None);
        } else {
            if let Some(mut client) = self.clients[session.index()].take() {
                if let Err(e) = self.poll.registry().deregister(&mut client.stream) {
                    error!("[N711] Failed to de-register TcpClient with error: '{e}'");
                }
            }
        }

        // register the new client as readable and writable.
        if let Err(e) = self.poll.registry().register(
            &mut client.stream,
            mio::Token(session.index()),
            Interest::READABLE | Interest::WRITABLE,
        ) {
            error!("[N712] Failed to register TcpClient with error: '{e}'");
        } else {
            self.clients[session.index()] = Some(client);
        }
    }

    fn disconnect(&mut self, session: Session, exit: Option<ExitCode>) {
        if let Some(Some(client)) = self.clients.get_mut(session.index()) {
            if client.session == session {
                // write exit code
                if let Some(exit) = &exit {
                    client.encoder.encode_exit(exit);
                }

                // de-register stream.
                if let Err(e) = self.poll.registry().deregister(&mut client.stream) {
                    error!("[N713] Failed to de-register TcpStream on disconnection: '{e}'");
                }

                // remove stream from buffer and push to exiting.
                self.exiting
                    .push(self.clients[session.index()].take().unwrap());
            }
        }
    }

    /// Submit packets for writing.
    fn submit(&mut self, packets: Vec<Packet>) {
        for packet in packets {
            if let Some(client) = self.get_mut(packet.session) {
                client.encoder.encode(packet.channel, &packet.payload);
            }
        }
    }

    /// Read events in the mio registry.
    fn poll(&mut self) {
        if let Err(e) = self.poll.poll(&mut self.events, Some(self.tickrate)) {
            error!("[N876] Error while polling OS events: '{e}'");
        }

        for ev in &self.events {
            let token = ev.token();

            if let Some(client) = self.clients.get_mut(token.0) {
                if let Some(client) = client {
                    if ev.is_readable() {
                        client.readable = true;
                    }

                    if ev.is_read_closed() {
                        client.readable = false;
                    }

                    if ev.is_writable() {
                        client.writable = true;
                    }

                    if ev.is_write_closed() {
                        client.writable = false;
                    }
                }
            }
        }
    }

    fn tick(&mut self) {
        let mut packets: Vec<Packet> = Vec::new();

        for slot in &mut self.clients {
            if let Some(client) = slot {
                // exit if tick fails.
                if let Err(exit) = client.tick(&mut packets) {
                    let mut client = slot.take().unwrap();
                    if let Err(e) = self.poll.registry().deregister(&mut client.stream) {
                        error!("[N117] Failed to deregister TCP Client with error: '{e}'");
                    }
                    client.encoder.encode_exit(&exit);
                    let _ = self.event_tx.send(RuntimeEvent::Disconnected {
                        session: client.session,
                        exit_code: exit,
                    });
                    self.exiting.push(client);
                }
            }
        }

        if packets.len() != 0 {
            let _ = self.event_tx.send(RuntimeEvent::RecvPackets { packets });
        }

        // try to finish writing exit codes
        let mut i = 0;
        while i < self.exiting.len() {
            let exiting = &mut self.exiting[i];
            if exiting.encoder.flush(&mut exiting.stream).is_err()
                || exiting.encoder.has_unwritten()
            {
                self.exiting.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    fn get_mut(&mut self, session: Session) -> Option<&mut TcpClient> {
        if let Some(Some(client)) = self.clients.get_mut(session.index()) {
            if client.session == session {
                return Some(client);
            }
        }
        None
    }
}

fn rt_main(mut rt: RuntimeThread) {
    loop {
        loop {
            match rt.signal_rx.try_recv() {
                Ok(RuntimeSignal::Insert { pending, session }) => {
                    let client = TcpClient {
                        stream: pending.stream,
                        encoder: pending.encoder,
                        decoder: pending.decoder,
                        session,
                        readable: true,
                        writable: true,
                    };
                    rt.register(client);
                }
                Ok(RuntimeSignal::Remove { session, exit }) => {
                    rt.disconnect(session, Some(exit));
                }
                Ok(RuntimeSignal::Submit { packets }) => rt.submit(packets),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        rt.poll();
        rt.tick();
    }
}
