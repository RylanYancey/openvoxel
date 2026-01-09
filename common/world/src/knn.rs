use std::ops::Range;

use crate::region::RegionId;
use aligned_vec::{AVec, CACHELINE_ALIGN};
use bevy::prelude::*;
use fxhash::FxHashMap;

/// 2d KdTree that stores entities in the same region as linear in memory.
/// Only sorts according to Region, entities within same region may not be in-order.
pub struct SpatialQuery<T> {
    entries: AVec<Entry<T>>,
    buckets: FxHashMap<RegionId, Range<usize>>,
    next: AVec<Entry<T>>,
}

impl<T> SpatialQuery<T>
where
    T: Copy,
{
    pub fn new() -> Self {
        Self {
            entries: AVec::new(CACHELINE_ALIGN),
            next: AVec::new(CACHELINE_ALIGN),
            buckets: FxHashMap::default(),
        }
    }

    /// Add an entity to the KD-Tree.
    /// The Entity will not be present in the map until `rebuild` is called.
    pub fn push(&mut self, item: T, pos: IVec3) {
        self.next.push(Entry { item, pos })
    }

    /// Re-construct the KdTree with new entries.
    pub fn rebuild(&mut self) {
        self.entries.clear();
        self.buckets.clear();
        std::mem::swap(&mut self.entries, &mut self.next);

        // sort into 512x512 region buckets.
        self.entries
            .sort_unstable_by_key(|entry| RegionId::new(entry.pos.xz()));

        if !self.entries.is_empty() {
            let mut curr_bucket_id = RegionId::from(self.entries[0].pos);
            let mut bucket_start = 0;

            for i in 1..self.entries.len() {
                let entry_bucket_id = RegionId::from(self.entries[i].pos);
                if entry_bucket_id != curr_bucket_id {
                    self.buckets.insert(curr_bucket_id, bucket_start..i);
                    curr_bucket_id = entry_bucket_id;
                    bucket_start = i;
                }
            }

            self.buckets
                .insert(curr_bucket_id, bucket_start..self.entries.len());
        }
    }

    /// Get all items in this region.
    /// If the region does not have any items, an empty iterator is returned.
    #[inline]
    pub fn in_region(&self, id: impl Into<RegionId>) -> impl Iterator<Item = (T, IVec3)> {
        let range = self.buckets.get(&id.into()).cloned().unwrap_or(0..0);
        self.entries[range]
            .iter()
            .map(|entry| (entry.item, entry.pos))
    }
}

impl<T> Default for SpatialQuery<T>
where
    T: Copy,
{
    fn default() -> Self {
        Self::new()
    }
}

struct Entry<T> {
    item: T,
    pos: IVec3,
}

pub struct SpatialQuery2d<T> {
    entries: AVec<Entry2d<T>>,
    buckets: FxHashMap<RegionId, Range<usize>>,
    next: AVec<Entry2d<T>>,
}

impl<T> SpatialQuery2d<T>
where
    T: Copy,
{
    pub fn new() -> Self {
        Self {
            entries: AVec::new(CACHELINE_ALIGN),
            next: AVec::new(CACHELINE_ALIGN),
            buckets: FxHashMap::default(),
        }
    }

    /// Add an entity to the KD-Tree.
    /// The Entity will not be present in the map until `rebuild` is called.
    pub fn push(&mut self, item: T, pos: IVec2) {
        self.next.push(Entry2d { item, pos })
    }

    /// Re-construct the KdTree with new entries.
    pub fn rebuild(&mut self) {
        self.entries.clear();
        self.buckets.clear();
        std::mem::swap(&mut self.entries, &mut self.next);

        // sort into 512x512 region buckets.
        self.entries
            .sort_unstable_by_key(|entry| (entry.pos.x & !511, entry.pos.y & !511));

        if !self.entries.is_empty() {
            let mut curr_bucket_id = RegionId::from(self.entries[0].pos);
            let mut bucket_start = 0;

            for i in 1..self.entries.len() {
                let entry_bucket_id = RegionId::from(self.entries[i].pos);
                if entry_bucket_id != curr_bucket_id {
                    self.buckets.insert(curr_bucket_id, bucket_start..i);
                    curr_bucket_id = entry_bucket_id;
                    bucket_start = i;
                }
            }

            self.buckets
                .insert(curr_bucket_id, bucket_start..self.entries.len());
        }
    }

    /// Get all items in this region.
    /// If the region does not have any items, an empty iterator is returned.
    #[inline]
    pub fn in_region(&self, id: impl Into<RegionId>) -> impl Iterator<Item = (T, IVec2)> {
        let range = self.buckets.get(&id.into()).cloned().unwrap_or(0..0);
        self.entries[range]
            .iter()
            .map(|entry| (entry.item, entry.pos))
    }
}

impl<T> Default for SpatialQuery2d<T>
where
    T: Copy,
{
    fn default() -> Self {
        Self::new()
    }
}

struct Entry2d<T> {
    item: T,
    pos: IVec2,
}
