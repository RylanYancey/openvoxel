use std::marker::PhantomData;

use bevy::prelude::*;

use protocol::{Packet, packet::SentBy, session::Session};

/// A description of a channel on which information may be sent or received.
pub struct Channel {
    /// The "side" that sends packets on this channel.
    pub sent_by: SentBy,

    /// Buffer of packets that are ready to be read.
    pub incoming: Vec<Packet>,
}

impl Channel {
    pub fn new(sent_by: SentBy) -> Self {
        Self {
            sent_by,
            incoming: Vec::new(),
        }
    }
}
