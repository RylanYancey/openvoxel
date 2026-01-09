use bevy::prelude::*;
use protocol::{ExitCode, session::Session};
use world::region::RegionId;

#[derive(Message)]
pub struct PlayerJoined {
    pub session: Session,
}

#[derive(Message)]
pub struct PlayerLeft {
    pub session: Session,
    pub exit: ExitCode,
}

/// A player's subscription to a Region changed.
#[derive(Message)]
pub struct SubscChanged {
    /// The Session of the affected player.
    pub session: Session,

    /// The ID of the region for which the change occurred.
    pub region: RegionId,

    /// The kind of change that occurred, either the
    /// subscription was added or removed.
    pub kind: SubscChangeKind,

    /// Whether Visual or Simulation subscription changed.
    pub interest: SubscInterest,
}

/// Attached to a SubscChanged message to inform how the
/// subscription changed.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SubscChangeKind {
    /// The player subscribed to the region.
    Subscribed,

    /// The player unsubscribed from a previously
    /// subscribed region.
    Unsubscribed,
}

/// Attached to a SubscChanged message to inform
/// what subscription interest changed.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SubscInterest {
    /// The player has subbed/unsubbed to the region visually.
    Visual,

    /// The player has subbed/unsubbed to the region's simulation.
    /// Players simulation subscribed are also subscribed to visual,
    /// because the visual distance must be greater
    /// than the simulation distance.
    Simulation,
}
