//! I hope you're ready for a wild ride :>
//!
//! This is the Subscriber. The role of this resource is to keep track of which regions players are subscribed to,
//! their interest in that region, and which chunks within those regions players should be sent next.
//!
//! The information recorded by the Subscriber is used to determine which Regions need to be loaded or unloaded based
//! on the number of subscriptions. If a region has subscribers but doesn't exist in the World, load it. If it does not
//! have subscribers, unload it. (although sometimes regions are kept around)
//!
//!

use std::{cmp::Ordering, collections::BinaryHeap, ops::Range, ptr::NonNull, time::Instant};

use aligned_vec::{AVec, CACHELINE_ALIGN};
use bevy::prelude::*;
use data::{queue::Queue, registry::Registry};
use fxhash::FxHashMap;
use math::{activity::Activity, space::area::IArea};
use protocol::{
    ChannelId, Packet,
    session::{Session, SessionMap},
};
use world::{
    World,
    region::{
        RegionId,
        attribute::ChunkMask,
        chunk::{ChunkId, flags::ChunkState},
        format::UnzippedChunk,
    },
};

use crate::{
    events::{SubscChangeKind, SubscChanged, SubscInterest},
    net::{Server, channel::Channel},
    player::Player,
    world::{
        generator::WorldGenerator,
        loader::{ChunkReadError, WorldLoader},
    },
};

/// The distance a player can travel before their subscriptions are recomputed.
const SUBSCRIPTION_RECOMPUTATION_DISTANCE: i32 = 32;

/// Rate of change of tracker activity when a recomputation occurs.
const ACTIVITY_RISE_ALPHA: f32 = 1.0;

/// Rate of change of tracker activity when no recomputation occurs.
const ACTIVITY_FALL_ALPHA: f32 = -0.1;

/// Structure that keeps track of which regions/chunks players are subscribed to.
#[derive(Resource)]
pub struct Subscriber {
    draw_distance: u32,
    sim_distance: u32,
    sends_per_tick_limit: u32,
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

    pub fn iter_mut<'a>(&'a mut self) -> protocol::session::IterMut<'a, Tracker> {
        self.trackers.iter_mut()
    }

    /// Returns true if anything is removed, and resets the `exists` var to false.
    /// Removed players do not trigger any events.
    fn remove_if_not_exists(&mut self) -> bool {
        self.trackers
            .extract_if(|_, tracker| !std::mem::replace(&mut tracker.exists, false))
            .count()
            != 0
    }

    fn clear_buckets(&mut self) {
        for bucket in self.buckets.values_mut() {
            bucket.clear();
        }
    }

    fn execute_player_recomputation(&mut self, session: Session) {
        use {SubscChangeKind::*, SubscInterest::*};
        // get the players tracker
        let tracker = self.trackers.get_mut(session).unwrap();
        if !tracker.recompute {
            // no region changes, but we still need to create subscriptions.
            for i in 0..tracker.keys.len() {
                let id = tracker.keys[i];
                // write subscription to bucket
                if let Some(bucket) = self.buckets.get_mut(&id) {
                    bucket.add(session, tracker.prev_pos, &tracker.vals[i]);
                } else {
                    let mut bucket = Bucket::new();
                    bucket.add(session, tracker.prev_pos, &tracker.vals[i]);
                    self.buckets.insert(id, bucket);
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
                chunks.in_draw = ChunkMask::from_area(&cell.intersection(&draw_area).unwrap());
                if let Some(area) = cell.intersection(&sim_area) {
                    chunks.in_sim = ChunkMask::from_area(&area);

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
                    // write subscription to bucket
                    if let Some(bucket) = self.buckets.get_mut(&id) {
                        bucket.add(session, tracker.prev_pos, &tracker.vals[i]);
                    } else {
                        let mut bucket = Bucket::new();
                        bucket.add(session, tracker.prev_pos, &tracker.vals[i]);
                        self.buckets.insert(id, bucket);
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
            draw_distance: 64,
            sim_distance: 32,
            sends_per_tick_limit: 5,
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
    // Determine which players need recomputation.
    // If no players need recomputation, we can skip it entirely.
    let mut needs_recompute = false;
    for (pos, player) in &q {
        if let Some(tracker) = subscriber.trackers.get_mut(player.session) {
            tracker.exists = true;
            if tracker.needs_recompute(pos.translation.as_ivec3().xz()) {
                needs_recompute = true;
            }
        } else {
            info!("Inserting Subscription Tracker: {:?}", player.session);

            // insert a new tracker if it does not exist.
            subscriber.trackers.insert(
                player.session,
                Tracker::new(pos.translation.as_ivec3().xz()),
            );

            // Recomputation can be activated on a newly inserted tracker without
            // needing to re-compute all buckets because they do not already exist
            // in any buckets.
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
        subscriber.clear_buckets();

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

/// Sends one chunk from each tracker's send queues.
pub fn process_chunk_send_queues(
    mut subscriber: ResMut<Subscriber>,
    mut generator: ResMut<WorldGenerator>,
    mut server: ResMut<Server>,
    mut loader: ResMut<WorldLoader>,
    channels: Res<Registry<Channel>>,
    mut world: ResMut<World>,
) {
    let channel: ChannelId = channels.resolve("chunk-data").unwrap().into();
    let sends_limit = subscriber.sends_per_tick_limit;

    for (session, tracker) in subscriber.trackers.iter_mut() {
        let mut sends = 0;
        loop {
            if let Some((id, distance)) = tracker.peek_next_chunk() {
                let origin = id.as_ivec2();
                let mut needs_load = false;
                if let Some(chunk) = world.get_chunk_mut(origin) {
                    match chunk.load_state() {
                        // Region is loaded, but chunk is not. Load it.
                        ChunkState::Unloaded => needs_load = true,

                        // Chunk is in the process of being generated.
                        ChunkState::Generating => {
                            // Push this chunk up in the queue.
                            generator.enqueue(id, distance);
                            break;
                        }

                        // Chunk is loaded and ready to be sent.
                        ChunkState::Loaded => {
                            // zip the data if needed and send to client.
                            server.tcp_send(Packet {
                                payload: chunk
                                    .get_cached_or_zip(loader.algorithm(), loader.zip_level())
                                    .0,
                                session,
                                channel,
                            });

                            // send successful, pop off the tracker.
                            tracker.pop_next_chunk();
                        }
                    }
                } else {
                    // request region load.
                    loader.open_region(id, distance);
                    break;
                }

                if needs_load {
                    match loader.read_chunk(id) {
                        Ok(data) => {
                            let span = UnzippedChunk::unzip(&data.0).expect("[S555] Unzip fail.");
                            world
                                .read_unzipped_chunk(span, false)
                                .expect("[S556] Chunk load fail.");
                            world
                                .get_chunk_mut(origin)
                                .unwrap()
                                .set_cached_zip(data.clone());
                            server.tcp_send(Packet {
                                payload: data.0,
                                session,
                                channel,
                            });
                            tracker.pop_next_chunk();
                        }
                        Err(ChunkReadError::NoData) => {
                            let chunk = world.get_chunk_mut(origin).unwrap();
                            *chunk.load_state_mut() = ChunkState::Generating;
                            generator.enqueue(id, distance);
                            break;
                        }
                        Err(ChunkReadError::RegionNotLoaded) => {
                            todo!("handle this")
                        }
                    }
                }
            }

            sends += 1;
            if sends >= sends_limit {
                break;
            }
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

    /// The frequency the player triggers recomputation.
    activity: Activity,

    /// Whether the player needs recomputation.
    /// Note that the player will still need to write its subscriptions
    /// to the Buckets if a recomputation was triggered, even by another
    /// tracker.
    recompute: bool,

    /// whether the player eixsted during recomputation.
    exists: bool,

    /// Queue of chunks waiting to be sent to the player.
    send_queue: Vec<QueuedChunk>,
}

impl Tracker {
    fn new(pos: IVec2) -> Self {
        Self {
            keys: Vec::new(),
            vals: Vec::new(),
            activity: Activity::new(),
            prev_pos: pos,
            recompute: true,
            exists: true,
            send_queue: Vec::new(),
        }
    }

    pub fn peek_next_chunk(&self) -> Option<(ChunkId, u32)> {
        self.send_queue
            .last()
            .map(|q| (ChunkId::new(self.prev_pos + q.rel), q.dist))
    }

    pub fn pop_next_chunk(&mut self) -> Option<(ChunkId, u32)> {
        if let Some(queued) = self.send_queue.pop() {
            let chunk_origin = self.prev_pos + queued.rel;
            let id = ChunkId::new(chunk_origin);
            if let Some(i) = self.get_region_idx(id.to_region_id()) {
                self.vals[i].sent.set_index(id.to_chunk_idx(), true);
                return Some((id, queued.dist));
            } else {
                // If you see this error, it means you recomputed but did not rebuild the
                // send queue, because there was a chunk in the queue whose containing region
                // did not exist in this tracker.
                unreachable!(
                    "[S888] Chunk send queue should be rebuilt every time subscription recomputes."
                )
            }
        }

        None
    }

    fn needs_recompute(&mut self, pos: IVec2) -> bool {
        let yes =
            self.prev_pos.chebyshev_distance(pos) as i32 > SUBSCRIPTION_RECOMPUTATION_DISTANCE;
        if yes {
            // recomputations updates matter more than non-recomputations.
            self.activity.update(ACTIVITY_RISE_ALPHA);
            self.prev_pos = pos;
            self.recompute = true;
        } else {
            self.activity.update(ACTIVITY_FALL_ALPHA);
        }
        yes
    }

    fn rebuild_send_queue(&mut self) {
        let player_pos = self.prev_pos;
        self.send_queue.clear();
        for i in 0..self.vals.len() {
            let region_origin = self.vals[i].origin;
            for offs in self.vals[i].in_draw_and_not_sent().iter_ones() {
                let chunk_origin = region_origin + offs;
                self.send_queue
                    .push(QueuedChunk::new(chunk_origin, player_pos));
            }
        }

        self.send_queue.sort_unstable();
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

#[derive(Eq, PartialEq, Copy, Clone)]
struct QueuedChunk {
    /// relative to previous player position.
    rel: IVec2,

    /// distance to previous player position.
    dist: u32,
}

impl QueuedChunk {
    fn new(chunk_origin: IVec2, player_pos: IVec2) -> Self {
        let rel = chunk_origin - player_pos;
        let dist = u32::max(rel.x.unsigned_abs(), rel.y.unsigned_abs());
        Self { rel, dist }
    }
}

impl Ord for QueuedChunk {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.dist != other.dist {
            other.dist.cmp(&self.dist)
        } else if self.rel.x != other.rel.x {
            other.rel.x.cmp(&self.rel.x)
        } else {
            other.rel.y.cmp(&self.rel.y)
        }
    }
}

impl PartialOrd for QueuedChunk {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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
    pub in_draw: ChunkMask,

    /// Chunks within render distance that have been sent.
    pub sent: ChunkMask,

    /// Chunks in simulation distance.
    /// This indicates entity updates from within this chunk
    /// should be sent to this client.
    pub in_sim: ChunkMask,
}

impl ChunkTracker {
    pub const fn new(origin: IVec2) -> Self {
        Self {
            origin,
            interest: SubscInterest::Visual,
            in_draw: ChunkMask::new(),
            in_sim: ChunkMask::new(),
            sent: ChunkMask::new(),
        }
    }

    fn in_draw_and_not_sent(&self) -> ChunkMask {
        self.in_draw & !self.sent
    }
}

/// One bucket per region that players are subscribed to.
/// There can be buckets for regions that have not loaded yet.
pub struct Bucket {
    /// Players subscribed to the Region.
    players: AVec<Entry>,

    /// The amount of time this bucket
    /// has not had any players subscribed
    /// to it.
    timestamp: Instant,

    /// Chunks that have players within simulation distance.
    in_sim: ChunkMask,

    /// Chunks that have players within draw distance.
    in_draw: ChunkMask,
}

impl Bucket {
    fn new() -> Self {
        Self {
            players: AVec::new(CACHELINE_ALIGN),
            timestamp: Instant::now(),
            in_draw: ChunkMask::new(),
            in_sim: ChunkMask::new(),
        }
    }

    fn add(&mut self, session: Session, pos: IVec2, tracker: &ChunkTracker) {
        self.in_sim |= tracker.in_sim;
        self.in_draw |= tracker.in_draw;
        self.players.push(Entry { session, pos });
    }

    fn clear(&mut self) {
        if !self.players.is_empty() {
            self.timestamp = Instant::now();
        }

        self.players.clear();
        self.in_sim.clear();
        self.in_draw.clear();
    }
}
