use std::{
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::Instant,
};

use mio::net::TcpStream;
use protocol::{
    codec::{TcpDecoder, TcpEncoder, UdpEncoder},
    exit::ExitCode,
    packet::{ChannelId, Packet},
    session::Session,
};

pub struct Connection {
    pub join_time: Instant,
    pub udp_encoder: UdpEncoder,
}

impl Connection {
    pub fn new(session: Session, socket: Arc<UdpSocket>, addr: SocketAddr) -> Self {
        Self {
            join_time: Instant::now(),
            udp_encoder: UdpEncoder::new(session, socket, addr),
        }
    }

    #[inline]
    pub fn join_time(&self) -> Instant {
        self.join_time
    }

    #[inline]
    pub fn udp_send(&mut self, channel: ChannelId, data: &[u8]) {
        self.udp_encoder.encode(channel, data);
    }

    #[inline]
    pub fn flush(&mut self) {
        self.udp_encoder.flush();
    }
}

/// A connection that has not yet been accepted.
pub struct Pending {
    pub(crate) stream: TcpStream,
    pub(crate) encoder: TcpEncoder,
    pub(crate) decoder: TcpDecoder,
    pub(crate) socket: Arc<UdpSocket>,
    pub(crate) packets: Vec<Packet>,
    pub(crate) address: SocketAddr,
    pub(crate) join_time: Instant,
}

impl Pending {
    pub fn try_recv(&mut self) -> Result<Option<Packet>, ExitCode> {
        if let Some(packet) = self.packets.pop() {
            Ok(Some(packet))
        } else {
            self.decoder.read(&mut self.stream)?;
            Ok(None)
        }
    }
}

pub(crate) struct Connections {
    slots: Vec<Slot>,
    free_head: Option<usize>,
    len: usize,
}

impl Connections {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.slots.capacity()
    }

    pub fn reserve(&mut self, additional: usize) {
        let start = self.slots.len();
        let end = self.slots.len() + additional;
        let old_head = self.free_head;
        self.slots.reserve_exact(additional);
        self.slots.extend((start..end).map(|i| {
            if i == end - 1 {
                Slot::Vacant {
                    next_free: old_head,
                }
            } else {
                Slot::Vacant {
                    next_free: Some(i + 1),
                }
            }
        }));
        self.free_head = Some(start);
    }

    pub fn insert(&mut self, value: Connection) -> Session {
        let i = self.alloc_slot();
        let tag = getrandom::u64().unwrap();
        let session = Session::new(i, tag);
        self.slots[i] = Slot::Occupied { value, session };
        session
    }

    pub fn contains(&self, session: Session) -> bool {
        self.get(session).is_some()
    }

    pub fn get(&self, session: Session) -> Option<&Connection> {
        match self.slots.get(session.index()) {
            Some(Slot::Occupied { value, session: s }) if *s == session => Some(value),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, session: Session) -> Option<&mut Connection> {
        match self.slots.get_mut(session.index()) {
            Some(Slot::Occupied { value, session: s }) if *s == session => Some(value),
            _ => None,
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Session, &mut Connection)> {
        self.slots.iter_mut().filter_map(|slot| match slot {
            Slot::Occupied { session, value } => Some((*session, value)),
            _ => None,
        })
    }

    pub fn remove(&mut self, session: Session) -> Option<Connection> {
        let index = session.index();
        if index >= self.slots.len() {
            return None;
        }

        match self.slots[index] {
            Slot::Occupied { session: s, .. } if s == session => {
                let slot = std::mem::replace(
                    &mut self.slots[index],
                    Slot::Vacant {
                        next_free: self.free_head,
                    },
                );
                self.free_head = Some(index);
                self.len -= 1;

                match slot {
                    Slot::Occupied { value, .. } => Some(value),
                    _ => unreachable!(),
                }
            }
            _ => None,
        }
    }

    fn alloc_slot(&mut self) -> usize {
        match self.free_head {
            None => {
                let additional = if self.capacity() == 0 {
                    1
                } else {
                    self.slots.len()
                };

                self.reserve(additional);
                self.alloc_slot()
            }
            Some(i) => match self.slots[i] {
                Slot::Occupied { .. } => panic!("Free List Corruption"),
                Slot::Vacant { next_free } => {
                    self.free_head = next_free;
                    self.len += 1;
                    i
                }
            },
        }
    }
}

enum Slot {
    Occupied { session: Session, value: Connection },
    Vacant { next_free: Option<usize> },
}

pub struct Iter<'a> {
    values: std::slice::Iter<'a, Slot>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (Session, &'a Connection);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(slot) = self.values.next() {
            if let Slot::Occupied { session, value } = slot {
                return Some((*session, value));
            }
        }
        None
    }
}

impl<'a> IntoIterator for &'a Connections {
    type IntoIter = Iter<'a>;
    type Item = (Session, &'a Connection);

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            values: self.slots.iter(),
        }
    }
}

pub struct IterMut<'a> {
    values: std::slice::IterMut<'a, Slot>,
}

impl<'a> Iterator for IterMut<'a> {
    type Item = (Session, &'a mut Connection);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(slot) = self.values.next() {
            if let Slot::Occupied { session, value } = slot {
                return Some((*session, value));
            }
        }
        None
    }
}

impl<'a> IntoIterator for &'a mut Connections {
    type IntoIter = IterMut<'a>;
    type Item = (Session, &'a mut Connection);
    fn into_iter(self) -> Self::IntoIter {
        IterMut {
            values: self.slots.iter_mut(),
        }
    }
}
