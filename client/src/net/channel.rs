use bevy::prelude::*;
use protocol::{Packet, packet::SentBy};

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
