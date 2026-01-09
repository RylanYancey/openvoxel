use std::{mem, ptr::NonNull};

use bevy::math::{IVec2, Vec3Swizzles};

use crate::region::{Region, alloc::AllocationKind};

pub(crate) struct Regions {
    regions: Vec<NonNull<Region>>,
    buckets: Vec<Bucket>,
    shift: u32,
    magic: u64,
    state: u64,
}

impl Regions {
    #[inline(always)]
    pub fn get(&self, origin: IVec2) -> Option<&Region> {
        let (key, hash) = self.compute(origin);
        self.buckets[hash].try_get(key)
    }

    #[inline(always)]
    pub fn get_mut(&mut self, origin: IVec2) -> Option<&mut Region> {
        let (key, hash) = self.compute(origin);
        self.buckets[hash].try_get_mut(key)
    }

    pub fn remove(&mut self, origin: IVec2) -> Option<Box<Region>> {
        let key = to_key(origin);
        let hash = self.hash(key);
        let bucket = &mut self.buckets[hash];
        if bucket.key == key {
            let idx = bucket.idx;
            self.buckets[hash] = Bucket::EMPTY;
            let ret = self.regions.swap_remove(idx);
            // update the bucket index of the region we just moved from the end, if it exists.
            if let Some(region) = self.regions.get(idx) {
                let (_, hash) = self.compute(unsafe { region.as_ref().origin().xz() });
                self.buckets[hash].idx = idx;
            }
            Some(unsafe { Box::from_non_null(ret) })
        } else {
            None
        }
    }

    /// Rebuilds if a hash conflict occurs.
    pub fn insert(&mut self, region: Box<Region>) -> Option<Box<Region>> {
        let (key, hash) = self.compute(region.origin().xz());
        let bucket = &mut self.buckets[hash];
        let ptr = Box::into_non_null(region);

        if bucket.key == key {
            // replace existing if region already exists
            return Some(unsafe {
                Box::from_non_null(mem::replace(&mut self.regions[bucket.idx], ptr))
            });
        }
        if bucket.key == u64::MAX {
            // occupy bucket and push
            bucket.ptr = ptr;
            bucket.idx = self.regions.len();
            bucket.key = key;
            self.regions.push(ptr);
        } else {
            // conflict; rebuild
            self.regions.push(ptr);
            self.rebuild();
        }

        None
    }

    fn compute(&self, origin: IVec2) -> (u64, usize) {
        let key = to_key(origin);
        (key, self.hash(key))
    }

    /// Compute the hash by multiplying the key (created with to_key) by the computed magic.
    /// Then, shift right such that only the last N bits remain. The shift factor is set such that
    /// after this shift, the value is known to be in-range for the buckets.
    ///
    /// Alternatively, we could have masked with &, but because the key always has its first 9 bits
    /// set to 0 we would have to shift anyway, so we're saving time by only shifting instead of &.
    fn hash(&self, key: u64) -> usize {
        (self.magic.wrapping_mul(key) >> self.shift) as usize
    }

    fn rebuild(&mut self) {
        // The size of buckets is always twice the length of regions, rounded up to the next
        // power of two. This extra space increases the probability of finding non-colliding magics.
        let size = (self.regions.len() << 1)
            .next_power_of_two()
            .max(self.buckets.len());
        // Shift factor that ensures right shift by
        self.shift = 64 - size.trailing_zeros();
        // reshape buckets to new size and assign empties
        self.buckets.clear();
        self.buckets.resize(size, Bucket::EMPTY);

        'outer: loop {
            // generate new magic
            self.magic = rng(&mut self.state);
            // Hash region origin to bucket indices until a conflict occurs.
            for i in 0..self.regions.len() {
                let region = self.regions[i];
                unsafe {
                    let key = to_key(region.as_ref().origin().xz());
                    let hash = self.hash(key);
                    if self.buckets[hash].key == u64::MAX {
                        self.buckets[hash] = Bucket {
                            ptr: region,
                            key,
                            idx: i,
                        };
                    } else {
                        self.buckets.fill(Bucket::EMPTY);
                        continue 'outer;
                    }
                }
            }

            // if the loop completes, there were no
            // collisions while assigning buckets.
            return;
        }
    }
}

impl Default for Regions {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
            buckets: vec![Bucket::EMPTY],
            shift: 63,
            magic: 0,
            state: 0xda3e_39cb_94b9_5bdb,
        }
    }
}

impl Drop for Regions {
    fn drop(&mut self) {
        unsafe {
            while let Some(region) = self.regions.pop() {
                let _ = Box::from_non_null(region);
            }
        }
    }
}

unsafe impl Send for Regions {}
unsafe impl Sync for Regions {}

#[derive(Copy, Clone)]
struct Bucket {
    ptr: NonNull<Region>,
    key: u64,
    idx: usize,
}

impl Bucket {
    const EMPTY: Self = Self {
        ptr: NonNull::dangling(),
        key: u64::MAX,
        idx: usize::MAX,
    };

    #[inline(always)]
    fn try_get(&self, key: u64) -> Option<&Region> {
        self.key.eq(&key).then(|| unsafe { self.ptr.as_ref() })
    }

    #[inline(always)]
    fn try_get_mut(&mut self, key: u64) -> Option<&mut Region> {
        self.key.eq(&key).then(|| unsafe { self.ptr.as_mut() })
    }
}

/// Make the upper 32 bits the X origin, lower 32 bits are the Z origin.
#[inline(always)]
fn to_key(origin: IVec2) -> u64 {
    ((origin.x as u64) << 32) | (origin.y as u32 as u64)
}

/// Basic Wyrand impl
fn rng(state: &mut u64) -> u64 {
    const P0: u64 = 0xa076_1d64_78bd_642f;
    const P1: u64 = 0xe703_7ed1_a0b4_28db;
    *state = state.wrapping_add(P0);
    let r = u128::from(*state).wrapping_mul(u128::from(*state ^ P1));
    ((r >> 64) ^ r) as u64
}
