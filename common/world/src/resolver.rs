use std::{
    alloc::{Allocator, Global, Layout},
    ptr::NonNull,
};

use bevy::log::info;

use crate::{Region, World, region::RegionId};

pub(crate) struct OwnedResolver<A: Allocator = Global> {
    /// A Bucket is uninit if its key is RegionId::MAX.
    buckets: NonNull<Bucket>,

    /// The number of available buckets.
    capacity: usize,

    /// Magic factor used for hashing.
    magic: u64,

    /// Shift factor used to isolate upper N bits.
    /// It is `64 - capacity.trailing_zeros()`.
    /// This makes the algorithm a Black Magic PHF.
    shift: u32,

    /// RNG state used to generate magics.
    state: u64,

    /// Generation used to make rebuilding faster.
    _gen: u32,

    /// Allocator for bucket ptr.
    alloc: A,
}

impl OwnedResolver {
    pub fn new() -> Self {
        Self::new_in(Global)
    }
}

impl<A: Allocator> OwnedResolver<A> {
    fn new_in(alloc: A) -> Self {
        const DEFAULT_CAPACITY: usize = 16;
        let ptr = unsafe {
            let layout = Layout::array::<Bucket>(DEFAULT_CAPACITY).unwrap();
            let ptr = alloc
                .allocate(layout)
                .unwrap()
                .as_non_null_ptr()
                .cast::<Bucket>();
            for i in 0..DEFAULT_CAPACITY {
                ptr.add(i).write(Bucket::EMPTY)
            }
            ptr
        };

        Self {
            buckets: ptr,
            capacity: DEFAULT_CAPACITY,
            magic: 0,
            shift: 63,
            state: 0xda3e_39cb_94b9_5bdb,
            _gen: 0,
            alloc,
        }
    }

    #[inline(always)]
    pub fn get<'a>(&'a self, id: RegionId) -> Option<&'a Region> {
        let hash = (self.magic.wrapping_mul(id.0) >> self.shift) as usize;
        unsafe {
            let bucket = self.buckets.add(hash).as_ref();
            if bucket.key == id {
                Some(bucket.ptr.as_ref())
            } else {
                None
            }
        }
    }

    #[inline(always)]
    pub fn get_mut<'a>(&'a mut self, id: RegionId) -> Option<&'a mut Region> {
        let hash = (self.magic.wrapping_mul(id.0) >> self.shift) as usize;
        unsafe {
            let bucket = self.buckets.add(hash).as_mut();
            if bucket.key == id {
                Some(bucket.ptr.as_mut())
            } else {
                None
            }
        }
    }

    /// Compute the hash by multiplying the key (created with to_key) by the computed magic.
    /// Then, shift right such that only the last N bits remain. The shift factor is set such that
    /// after this shift, the value is known to be in-range for the buckets.
    ///
    /// Alternatively, we could have masked with &, but because the key always has its first 9 bits
    /// set to 0 we would have to shift anyway, so we're saving time by only shifting instead of &.
    const fn hash(&self, key: u64) -> usize {
        (self.magic.wrapping_mul(key) >> self.shift) as usize
    }

    unsafe fn get_bucket(&self, id: RegionId) -> Option<&Bucket> {
        let hash = self.hash(id.0);
        unsafe { self.buckets.add(hash).as_ref().key_eq(id) }
    }

    unsafe fn get_bucket_mut(&mut self, id: RegionId) -> Option<&mut Bucket> {
        let hash = self.hash(id.0);
        unsafe { self.buckets.add(hash).as_mut().key_eq_mut(id) }
    }

    /// Replaces the bucket at `id` with Bucket::EMPTY and returns its index.
    /// Returns None if no region exists with this ID.
    pub unsafe fn remove(&mut self, id: RegionId) -> Option<usize> {
        unsafe {
            if let Some(bucket) = self.get_bucket_mut(id) {
                let ret = Some(bucket.idx as usize);
                *bucket = Bucket::EMPTY;
                ret
            } else {
                None
            }
        }
    }

    /// Change the index that the bucket at 'id' has with the new index.
    ///
    /// Panics if the region does not exist.
    pub fn set_bucket_index(&mut self, id: RegionId, new_index: usize) {
        unsafe {
            self.get_bucket_mut(id).unwrap().idx = new_index as u32;
        }
    }

    /// Assumes the newly inserted region is the last one in the slice.
    /// Panics if the region already exists.
    /// Rebuilds if the slot is already taken.
    pub unsafe fn insert_and_rebuild_if_needed(&mut self, ptrs: &[NonNull<Region>]) {
        if let Some(last) = ptrs.last() {
            unsafe {
                let key = last.as_ref().id();
                let hash = self.hash(key.0);
                let bucket = self.buckets.add(hash).as_mut();
                if bucket._gen == self._gen {
                    assert_ne!(bucket.key, key);
                    self.rebuild(ptrs);
                } else {
                    bucket.idx = (ptrs.len() - 1) as u32;
                    bucket.key = key;
                    bucket.ptr = *last;
                    bucket._gen = self._gen;
                }
            }
        }
    }

    unsafe fn rebuild(&mut self, ptrs: &[NonNull<Region>]) {
        // The size of regions is the next power of two of the number of regions, multiplied by 4.
        let size = (ptrs.len().next_power_of_two() << 2).max(self.capacity);

        // Shift-Right factor ensures computed index is in-bounds.
        // Upper bits of a multiplication-based hash are higher quality then the lower bits.
        self.shift = 64 - size.trailing_zeros();

        // Grow buckets to new capacity if needed.
        // We dont handle the `capacity=0` case because we initialize the
        // resolver with a capacity of 1.
        if size > self.capacity {
            let new_layout = Layout::array::<Bucket>(size).unwrap();
            let old_layout = Layout::array::<Bucket>(self.capacity).unwrap();
            self.capacity = size;
            self.buckets = unsafe {
                self.alloc
                    .grow(self.buckets.cast::<u8>(), old_layout, new_layout)
                    .unwrap()
                    .as_non_null_ptr()
                    .cast::<Bucket>()
            };

            // initialize buckets to EMPTY
            for i in 0..self.capacity {
                unsafe {
                    self.buckets.add(i).write(Bucket::EMPTY);
                }
            }
        }

        let mut num_iterations = 0;
        'outer: loop {
            num_iterations += 1;

            // generate next magic
            self.magic = rng(&mut self.state);

            // increment generation, skipping u32::MAX, because it is
            // used to indicate an uninitialized bucket.
            if self._gen >= u32::MAX - 1 {
                self._gen = 0;
                for i in 0..self.capacity {
                    unsafe {
                        self.buckets.add(i).write(Bucket::EMPTY);
                    }
                }
            } else {
                self._gen += 1;
            }

            // force LLVM to load _gen onto the stack.
            let _gen = self._gen;

            // hash region ids until a conflict occurs.
            for i in 0..ptrs.len() {
                unsafe {
                    let key = ptrs[i].as_ref().id();
                    let hash = self.hash(key.0);
                    let bucket = self.buckets.add(hash).as_mut();
                    if bucket._gen == _gen {
                        continue 'outer;
                    } else {
                        bucket._gen = _gen;
                    }
                }
            }

            // if the loop completes, we have found a magic value.
            // Initialize buckets fully.
            for i in 0..ptrs.len() {
                unsafe {
                    let key = ptrs[i].as_ref().id();
                    let hash = self.hash(key.0);
                    let bucket = self.buckets.add(hash).as_mut();
                    bucket.idx = i as u32;
                    bucket.key = key;
                    bucket.ptr = ptrs[i];
                }
            }

            // set empty buckets to be actually Bucket::EMPTY
            for i in 0..self.capacity {
                unsafe {
                    let bucket = self.buckets.add(i).as_mut();
                    if bucket._gen != _gen {
                        *bucket = Bucket::EMPTY;
                    }
                }
            }

            info!("Region Resolver rebuilt in {num_iterations} iterations.");
            return;
        }
    }
}

impl<A: Allocator> Drop for OwnedResolver<A> {
    fn drop(&mut self) {
        let layout = Layout::array::<Bucket>(self.capacity).unwrap();
        unsafe { self.alloc.deallocate(self.buckets.cast::<u8>(), layout) }
    }
}

unsafe impl Send for OwnedResolver {}
unsafe impl Sync for OwnedResolver {}

struct Bucket {
    ptr: NonNull<Region>,
    key: RegionId,
    idx: u32,
    _gen: u32,
}

impl Bucket {
    const EMPTY: Self = Self {
        ptr: NonNull::dangling(),
        key: RegionId(u64::MAX),
        idx: u32::MAX,
        _gen: u32::MAX,
    };

    const fn key_eq(&self, key: RegionId) -> Option<&Bucket> {
        if self.key.0 == key.0 {
            Some(self)
        } else {
            None
        }
    }

    const fn key_eq_mut(&mut self, key: RegionId) -> Option<&mut Bucket> {
        if self.key.0 == key.0 {
            Some(self)
        } else {
            None
        }
    }
}

/// Basic Wyrand impl to generate magic
fn rng(state: &mut u64) -> u64 {
    const P0: u64 = 0xa076_1d64_78bd_642f;
    const P1: u64 = 0xe703_7ed1_a0b4_28db;
    *state = state.wrapping_add(P0);
    let r = u128::from(*state).wrapping_mul(u128::from(*state ^ P1));
    ((r >> 64) ^ r) as u64
}
