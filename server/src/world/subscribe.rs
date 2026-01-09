use std::{collections::BinaryHeap, ops::Range, ptr::NonNull};

use aligned_vec::{AVec, CACHELINE_ALIGN};
use bevy::prelude::*;
use data::queue::Queue;
use fxhash::FxHashMap;
use math::space::area::IArea;
use protocol::session::{Session, SessionMap};
use world::region::{RegionId, attribute::ChunkFlags};

use crate::{
    events::{SubscChangeKind, SubscChanged, SubscInterest},
    player::Player,
};

/// The distance a player can travel before their subscriptions are recomputed.
const SUBSCRIPTION_RECOMPUTATION_DISTANCE: i32 = 32;

/// Structure that keeps track of which regions/chunks players are subscribed to.
#[derive(Resource)]
pub struct Subscriber {
    draw_distance: u32,
    sim_distance: u32,
    trackers: SessionMap<Tracker>,
    buckets: FxHashMap<RegionId, Bucket>,
    changes: Vec<SubscChanged>,
}

impl Subscriber {
    /// Get all players in-range of a point.
    ///
    /// Note that the position checked against will be the position used
    /// the last time the subscriber recomputed, and thus may be off by
    /// up to 32 blocks.
    ///
    /// Also note that the distance metric used will be Chebyshev.
    ///
    /// Also also note that only the X and Z coordinates are respected, if
    /// you want y-coordiantes you'll need to handle that separately.
    pub fn in_range<'a>(&'a self, xz: IVec2, range: u32) -> InRange<'a> {
        let slice = self
            .buckets
            .get(&RegionId::from(xz))
            .map(|bucket| bucket.players.as_slice())
            .unwrap_or(&[]);
        InRange {
            iter: slice.iter(),
            point: xz,
            range,
        }
    }

    /// Get all players whose draw area intersects the containing region of xz.
    ///
    /// Note that the position checked against will be the position used
    /// the last time the subscriber recomputed, and thus may be off by
    /// up to 32 blocks.
    ///
    /// Also note that the distance metric used will be Chebyshev.
    ///
    /// Also also note that only the X and Z coordinates are respected, if
    /// you want y-coordiantes you'll need to handle that separately.
    pub fn in_draw_range<'a>(&'a self, xz: IVec2) -> InRange<'a> {
        self.in_range(xz, self.draw_distance)
    }

    /// Get all players whose simulation area intersects the containing region of xz.
    ///
    /// Note that the position checked against will be the position used
    /// the last time the subscriber recomputed, and thus may be off by
    /// up to 32 blocks.
    ///
    /// Also note that the distance metric used will be Chebyshev.
    ///
    /// Also also note that only the X and Z coordinates are respected, if
    /// you want y-coordiantes you'll need to handle that separately.
    pub fn in_simulation_range<'a>(&'a self, xz: IVec2) -> InRange<'a> {
        self.in_range(xz, self.sim_distance)
    }

    /// Get the tracker for the player with this session.
    pub fn get(&self, session: Session) -> Option<&Tracker> {
        self.trackers.get(session)
    }

    pub fn get_mut(&mut self, session: Session) -> Option<&mut Tracker> {
        self.trackers.get_mut(session)
    }

    /// Returns true if anything is removed.
    /// Removed players do not trigger any events.
    fn remove_if_not_exists(&mut self) -> bool {
        // todo: this
        false
    }

    fn execute_player_recomputation(&mut self, session: Session) {
        use {SubscChangeKind::*, SubscInterest::*};
        // get the players tracker
        let tracker = self.trackers.get_mut(session).unwrap();
        if !tracker.recompute {
            // no region changes, but we still need to create subscriptions.
            for i in 0..tracker.keys.len() {
                let id = tracker.keys[i];
                // write region subscription to the bucket.
                let entry = Entry {
                    pos: tracker.prev_pos,
                    session,
                };
                if let Some(bucket) = self.buckets.get_mut(&id) {
                    bucket.players.push(entry);
                } else {
                    let mut players = AVec::new(CACHELINE_ALIGN);
                    players.push(entry);
                    self.buckets.insert(id, Bucket { players });
                }
            }
        } else {
            tracker.recompute = false;

            // compute draw/sim areas
            let sim_area = IArea::from_center_extents(
                tracker.prev_pos,
                IVec2::splat(self.sim_distance as i32),
            );
            let draw_area = IArea::from_center_extents(
                tracker.prev_pos,
                IVec2::splat(self.draw_distance as i32),
            );

            // iterate regions contained by the draw area.
            for cell in draw_area.cells_pow2::<512>() {
                let region = RegionId::from(cell.min);

                // get or insert the region into the player's tracker.
                let i = match tracker.get_region_idx(region) {
                    Some(i) => i,
                    None => {
                        self.changes.push(SubscChanged {
                            session,
                            region,
                            kind: Subscribed,
                            interest: Simulation,
                        });
                        tracker.push(region)
                    }
                };

                // update chunk tracker bitfields
                let chunks = &mut tracker.vals[i];
                chunks.in_draw = ChunkFlags::from_area(&cell.intersection(&draw_area).unwrap());
                if let Some(area) = cell.intersection(&sim_area) {
                    chunks.in_sim = ChunkFlags::from_area(&area);

                    // We know the player is within visual distance at this point, but they
                    // are not necessarily in simulation distance. Check for it and dispatch
                    // an event if it has changed.
                    if chunks.interest != Simulation {
                        chunks.interest = Simulation;
                        // write subscription to simulation
                        self.changes.push(SubscChanged {
                            session,
                            region,
                            kind: Subscribed,
                            interest: Simulation,
                        });
                    }
                } else {
                    // Simulation area does not intersect region area, remove
                    // subscription if it exists.
                    if chunks.interest == Simulation {
                        chunks.interest = Visual;
                        // write unsubscription to simulation
                        self.changes.push(SubscChanged {
                            session,
                            region,
                            kind: Unsubscribed,
                            interest: Simulation,
                        });
                    }
                }
            }
            // Remove regions that are no longer in draw distance, and
            // insert the player's region entries.
            let mut i = 0;
            while i < tracker.keys.len() {
                let id = tracker.keys[i];
                let area = id.area();
                if !area.intersects(&draw_area) {
                    // write unsubscription to visual
                    self.changes.push(SubscChanged {
                        session,
                        region: id,
                        kind: Unsubscribed,
                        interest: SubscInterest::Visual,
                    });

                    // remove entry from tracker.
                    tracker.keys.swap_remove(i);
                    tracker.vals.swap_remove(i);
                } else {
                    // create entry for region subscription
                    let entry = Entry {
                        pos: tracker.prev_pos,
                        session,
                    };

                    // write subscription to bucket
                    if let Some(bucket) = self.buckets.get_mut(&id) {
                        bucket.players.push(entry);
                    } else {
                        let mut players = AVec::new(CACHELINE_ALIGN);
                        players.push(entry);
                        self.buckets.insert(id, Bucket { players });
                    }

                    i += 1;
                }
            }

            tracker.rebuild_send_queue();
        }
    }
}

impl Default for Subscriber {
    fn default() -> Self {
        Self {
            draw_distance: 512,
            sim_distance: 256,
            trackers: SessionMap::new(),
            buckets: FxHashMap::default(),
            changes: Vec::new(),
        }
    }
}

pub struct InRange<'a> {
    iter: std::slice::Iter<'a, Entry>,
    point: IVec2,
    range: u32,
}

impl<'a> Iterator for InRange<'a> {
    type Item = (Session, IVec2);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(entry) = self.iter.next() {
            if entry.pos.chebyshev_distance(self.point) < self.range {
                return Some((entry.session, entry.pos));
            }
        }

        None
    }
}

pub fn recompute_subscriptions(
    mut subscriber: ResMut<Subscriber>,
    mut sub_evs: MessageWriter<SubscChanged>,
    q: Query<(&Transform, &Player)>,
) {
    // set all trackers to not exists so we can detect which players to remove.
    for (_, tracker) in &mut subscriber.trackers {
        tracker.exists = false;
    }

    // Determine which players need recomputation.
    // If no players need recomputation, we can skip it entirely.
    let mut needs_recompute = false;
    for (pos, player) in &q {
        if let Some(tracker) = subscriber.trackers.get_mut(player.session) {
            if tracker.needs_recompute(pos.translation.as_ivec3().xz()) {
                needs_recompute = true;
                tracker.exists = true;
            }
        } else {
            // insert a new tracker if it does not exist.
            subscriber.trackers.insert(
                player.session,
                Tracker::new(pos.translation.as_ivec3().xz()),
            );

            // Recomputation can be activated on a newly inserted tracker without
            // needing to re-compute all buckets.
            subscriber.execute_player_recomputation(player.session);
        }
    }

    // remove any trackers that don't exist anymore.
    if subscriber.remove_if_not_exists() {
        needs_recompute = true;
    }

    // Execute recomputation if any players moved far enough to require it.
    if needs_recompute {
        // clear bucket contents
        for bucket in subscriber.buckets.values_mut() {
            bucket.clear();
        }

        // execute player recomputations
        for (_, player) in &q {
            subscriber.execute_player_recomputation(player.session);
        }

        // write any changes to the MessageWriter
        if !subscriber.changes.is_empty() {
            sub_evs.write_batch(subscriber.changes.drain(..));
        }
    }
}

/// Storage for regions the player is subscribed to.
/// One of these exists per player.
pub struct Tracker {
    /// Keys of tracked region, corresponds to
    /// the "vals" vector.
    keys: Vec<RegionId>,

    /// Bitmasks describing tracked regions.
    vals: Vec<ChunkTracker>,

    /// The position of the player the last time they were updated.
    prev_pos: IVec2,

    /// Whether the player needs recomputation.
    /// Note that the player will still need to write its subscriptions
    /// to the Buckets if a recomputation was triggered, even by another
    /// tracker.
    recompute: bool,

    /// whether the player eixsted during recomputation.
    exists: bool,

    /// Queue of Regions waiting to be sent to the player.
    send_queue: BinaryHeap<QueuedChunk>,
}

impl Tracker {
    fn new(pos: IVec2) -> Self {
        Self {
            keys: Vec::new(),
            vals: Vec::new(),
            prev_pos: pos,
            recompute: true,
            exists: true,
            send_queue: BinaryHeap::new(),
        }
    }

    fn pop_send_queue(&mut self) -> Option<QueuedChunk> {
        if let Some(item) = self.send_queue.pop() {
            if let Some(i) = self.get_region_idx(item.pos.into()) {
                self.vals[i].sent.set(item.pos, false);
                return Some(item);
            }
        }
        None
    }

    fn needs_recompute(&mut self, pos: IVec2) -> bool {
        if self.prev_pos.chebyshev_distance(pos) as i32 > SUBSCRIPTION_RECOMPUTATION_DISTANCE {
            self.prev_pos = pos;
            true
        } else {
            false
        }
    }

    fn rebuild_send_queue(&mut self) {
        self.send_queue.clear();
        for i in 0..self.vals.len() {
            for pos in self.vals[i].in_draw_and_not_sent().iter_ones() {
                let pos = self.vals[i].origin + pos;
                let item = QueuedChunk {
                    pos,
                    dist: u32::MAX - pos.chebyshev_distance(self.prev_pos),
                };
                self.send_queue.push(item);
            }
        }
    }

    fn get_region_idx(&mut self, id: RegionId) -> Option<usize> {
        for (i, key) in self.keys.iter().enumerate() {
            if id == *key {
                return Some(i);
            }
        }
        None
    }

    fn push(&mut self, id: RegionId) -> usize {
        let ret = self.vals.len();
        self.keys.push(id);
        self.vals.push(ChunkTracker::new(id.as_ivec2()));
        ret
    }
}

pub struct Entry {
    pub pos: IVec2,
    pub session: Session,
}

#[derive(Copy, Clone)]
struct QueuedChunk {
    pos: IVec2,
    dist: u32,
}

impl Eq for QueuedChunk {}
impl PartialEq for QueuedChunk {
    fn eq(&self, other: &Self) -> bool {
        self.dist == other.dist
    }
}

impl Ord for QueuedChunk {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.dist.cmp(&other.dist)
    }
}

impl PartialOrd for QueuedChunk {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.dist.partial_cmp(&other.dist)
    }
}

/// Description of how the player is subscribed to the region.
/// One of these exists per region the player is subbed to.
///
/// Cached-aligned for optimization reasons.
#[repr(align(128))]
pub struct ChunkTracker {
    /// The origin of the containing region.
    pub origin: IVec2,

    /// Is the player subscribed only to visual or
    /// to simulation AND visual?
    pub interest: SubscInterest,

    /// Chunks within render distance.
    /// This indicates events related to voxel changes in this chunk
    /// should be sent to the client.
    pub in_draw: ChunkFlags,

    /// Chunks within render distance that have been sent.
    pub sent: ChunkFlags,

    /// Chunks in simulation distance.
    /// This indicates entity updates from within this chunk
    /// should be sent to this client.
    pub in_sim: ChunkFlags,
}

impl ChunkTracker {
    pub const fn new(origin: IVec2) -> Self {
        Self {
            origin,
            interest: SubscInterest::Visual,
            in_draw: ChunkFlags::new(),
            in_sim: ChunkFlags::new(),
            sent: ChunkFlags::new(),
        }
    }

    fn in_draw_and_not_sent(&self) -> ChunkFlags {
        self.in_draw & !self.sent
    }
}

/// One bucket per region that players are subscribed to.
/// There can be buckets for regions that have not loaded yet.
struct Bucket {
    players: AVec<Entry>,
}

impl Bucket {
    pub fn clear(&mut self) {
        self.players.clear()
    }
}
