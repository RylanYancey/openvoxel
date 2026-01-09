use bevy::prelude::*;
use data::queue::{Queueable, RepetitionPriorityQueue};
use world::region::RegionId;

/// Request to load a region that does not currently exist in the World.
#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub struct LoadRegionReq(pub RegionId);

impl Queueable for LoadRegionReq {
    type Inner = RepetitionPriorityQueue<Self>;
}
