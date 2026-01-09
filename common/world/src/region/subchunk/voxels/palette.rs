use super::BPI_ZERO;
use crate::region::alloc::AllocationKind;
use std::alloc::{Allocator, Layout};
use std::ptr::NonNull;

#[derive(Copy, Clone)]
#[repr(C)]
struct Bucket {
    index: u16,
    state: u16,
}

impl Bucket {
    const EMPTY: Self = Self {
        index: u16::MAX,
        state: u16::MAX,
    };
}

pub(super) struct Palette {
    /// A set of all voxel states that are present or have ever
    /// been present in the palette array.
    palette: NonNull<u16>,

    /// Used to resolve a voxel state to a palette index.
    ///
    /// Implemented as a linear probe hash table. Size is always
    /// double the size of the palette.
    ///
    /// Cache is only initialized the first time an assignment occurs.
    /// Uninitialized cache points to the UNINIT_CACHE static.
    cache: NonNull<Bucket>,

    /// The number of states currently in the palette.
    palette_len: u16,

    /// The number of voxel states the palette can store until
    /// the palette capacity will need to be doubled.
    palette_cap: u16,

    /// The size of the cache minus one. This is used to wrap
    /// the voxel state to be a valid index in the cache.
    cache_bits: u16,

    /// Whether or not the palette pointer is owned by this instance.
    /// When Regions are read into memory, they are read into a huge Vec<u8>.
    /// Then, each subchunk takes pointers to that big vector. This boolean
    /// indicates whether the palette is a pointer to a span that buf.
    ptr_kind: AllocationKind,
}

impl Palette {
    #[inline]
    #[allow(static_mut_refs)]
    pub const fn empty() -> Self {
        Self {
            palette: unsafe { NonNull::new_unchecked(&BPI_ZERO as *const _ as *mut u16) },
            cache: unsafe { NonNull::new_unchecked(&BPI_ZERO as *const _ as *mut Bucket) },
            palette_len: 1,
            palette_cap: 1,
            cache_bits: 1,
            ptr_kind: AllocationKind::Borrowed,
        }
    }

    /// NOTE: DOES NOT DE-ALLOCATE SELF!!!!
    #[inline]
    #[allow(static_mut_refs)]
    pub unsafe fn assign_borrowed_ptr_unchecked(&mut self, ptr: NonNull<u16>, len: u16) {
        debug_assert_eq!(self.ptr_kind, AllocationKind::Borrowed);
        self.palette = ptr;
        self.palette_len = len;
        self.palette_cap = if len <= 16 {
            16
        } else {
            len.next_power_of_two()
        };
        self.cache = unsafe { NonNull::new_unchecked(&BPI_ZERO as *const _ as *mut Bucket) };
        self.cache_bits = 1;
        self.ptr_kind = AllocationKind::Borrowed;
    }

    pub const fn capacity(&self) -> usize {
        self.palette_cap as usize
    }

    /// The number of voxel states present in the palette.
    pub const fn len(&self) -> usize {
        self.palette_len as usize
    }

    /// Whether the array is all zeroes (air).
    pub const fn is_empty(&self) -> bool {
        self.palette_len == 1
    }

    /// Get the palette data as native endian bytes.
    pub fn as_ne_bytes(&self) -> &[u8] {
        let mut num_bytes = (self.palette_len << 1) as usize;
        num_bytes = std::hint::select_unpredictable(self.is_empty(), 0, num_bytes);
        unsafe { std::slice::from_raw_parts(self.palette.cast::<u8>().as_ptr(), num_bytes) }
    }

    #[inline(always)]
    pub unsafe fn get(&self, pidx: usize) -> u16 {
        debug_assert!(pidx < self.palette_len as usize);
        unsafe { *self.palette.add(pidx).as_ref() }
    }

    #[inline(always)]
    pub unsafe fn search(&self, v: u16) -> Option<usize> {
        unsafe {
            // Hash using BITAND. This isn't very secure, but
            // the number of elements in the table is limited so
            // it shouldn't be easily exploitable. (e.g. via HASHDOS)
            let mut hash = (v & self.cache_bits) as usize;

            // Keep checking until an empty slot is found or a slot with state=V.
            // This is the linear probe algorithm.
            loop {
                let bucket = *self.cache.add(hash).as_ref();

                // if state=v, index is found, return.
                if bucket.state == v {
                    return Some(bucket.index as usize);
                }

                // if state = 65535, v does not exist in the table or
                // the lookup table is uninitialized.
                if bucket.state == u16::MAX {
                    return None;
                }

                // continue to next index
                hash = (hash + 1) & self.cache_bits as usize;
            }
        }
    }

    /// Insert a value into the palette, returning its assigned index.
    /// Returns `Err(pidx)` if the palette capacity has changed.
    pub unsafe fn insert<A: Allocator>(&mut self, v: u16, alloc: &A) -> Result<usize, usize> {
        unsafe {
            // If the pointer is non-owned, we will need to
            // allocate one so we can initialize the cache.
            if self.ptr_kind == AllocationKind::Borrowed {
                // Grow to BPI=4 if BPI=0.
                if self.palette_len == 1 {
                    self.grow_cap_to_16(&alloc);
                    self.palette_len += 1;
                    self.palette.add(1).write(v);
                    self.init_cache();
                    return Err(1);
                }

                // allocate pointer for cache/palette
                self.alloc_owned_ptr(alloc);

                // initialize cache for existing values.
                self.init_cache();

                // check if v already exists with the newly initialized cache.
                if let Some(i) = self.search(v) {
                    return Ok(i);
                }
            }

            // If we are out of space, grow palette cap to next power of two.
            // If a palette fault occurs, Words will need to double its capacity.
            if self.palette_len >= self.palette_cap {
                self.grow(alloc);
                let i = self.palette_len as usize;
                self.palette_len += 1;
                self.palette.add(i).write(v);
                self.init_cache();
                return Err(i);
            }

            // Insert the value into the palette and cache.
            let i = self.palette_len as usize;
            self.palette_len += 1;
            self.palette.add(i).write(v);
            let mut hash = (v & self.cache_bits) as usize;
            loop {
                let bucket = self.cache.add(hash).as_mut();
                if bucket.state == u16::MAX {
                    bucket.index = i as u16;
                    bucket.state = v;
                    return Ok(i);
                }

                hash = (hash + 1) & self.cache_bits as usize;
            }
        }
    }

    unsafe fn grow_cap_to_16<A: Allocator>(&mut self, alloc: &A) {
        debug_assert_eq!(self.palette_len, 1);
        let new_layout = Self::layout_for_cap(16);
        self.palette_cap = 16;
        self.cache_bits = 31;
        self.ptr_kind = AllocationKind::Owned;
        self.palette = alloc
            .allocate(new_layout)
            .unwrap()
            .as_non_null_ptr()
            .cast::<u16>();
        unsafe {
            self.palette.write(0);
            self.cache = self.palette.add(16).cast::<Bucket>();
        }
    }

    unsafe fn alloc_owned_ptr<A: Allocator>(&mut self, alloc: &A) {
        debug_assert_eq!(self.ptr_kind, AllocationKind::Borrowed);
        debug_assert_ne!(self.palette_len, 1);
        debug_assert_eq!(self.cache_bits, 1);
        let layout = Self::layout_for_cap(self.palette_cap as usize);
        let new_ptr = alloc
            .allocate(layout)
            .unwrap()
            .as_non_null_ptr()
            .cast::<u16>();
        unsafe {
            new_ptr.copy_from_nonoverlapping(self.palette, self.palette_len as usize);
        }
        self.ptr_kind = AllocationKind::Owned;
        self.cache_bits = (self.palette_cap << 1) - 1;
    }

    unsafe fn grow<A: Allocator>(&mut self, alloc: &A) {
        debug_assert_eq!(self.ptr_kind, AllocationKind::Owned);
        debug_assert_ne!(self.cache_bits, 1);
        let old_capacity = self.palette_cap as usize;
        let new_capacity = old_capacity << 1;
        let old_layout = Self::layout_for_cap(old_capacity);
        let new_layout = Self::layout_for_cap(new_capacity);
        unsafe {
            self.palette = alloc
                .grow(self.palette.cast::<u8>(), old_layout, new_layout)
                .unwrap()
                .as_non_null_ptr()
                .cast::<u16>();
            self.cache = self.palette.add(new_capacity).cast::<Bucket>();
            self.palette_cap = new_capacity as u16;
            self.cache_bits = ((new_capacity << 1) - 1) as u16;
        }
    }

    unsafe fn init_cache(&mut self) {
        unsafe {
            for i in 0..(self.palette_cap << 1) as usize {
                self.cache.add(i).write(Bucket::EMPTY);
            }

            for i in 0..self.palette_len as usize {
                let state = *self.palette.add(i).as_ref();
                let mut hash = (state & self.cache_bits) as usize;
                loop {
                    let bucket = self.cache.add(hash).as_mut();

                    if bucket.state == u16::MAX {
                        bucket.state = state;
                        bucket.index = i as u16;
                        break;
                    }

                    hash = (hash + 1) & self.cache_bits as usize;
                }
            }
        }
    }

    fn layout_for_cap(cap: usize) -> Layout {
        let palette_layout = Layout::array::<u16>(cap).unwrap();
        let cache_layout = Layout::array::<Bucket>(cap << 1).unwrap();
        Layout::from_size_align(palette_layout.size() + cache_layout.size(), 2).unwrap()
    }

    /// Drop the palette+cache pointer in this allocator if needed.
    /// This struct doesn't store a reference to this allocator, so will need
    /// to be called before it is dropped by its containing struct.
    pub unsafe fn dealloc_if_owned<A: Allocator>(&mut self, alloc: &A) {
        if let AllocationKind::Owned = self.ptr_kind {
            let layout = Self::layout_for_cap(self.palette_cap as usize);
            unsafe { alloc.deallocate(self.palette.cast::<u8>(), layout) }
            self.ptr_kind = AllocationKind::Borrowed;
            *self = Self::empty();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Palette;
    use std::alloc::Global;

    fn init_test_states(len: usize) -> Vec<u16> {
        let mut ret = Vec::with_capacity(len);
        let mut state: u32 = 0x831a_fa11;
        for i in 0..len {
            loop {
                state = state.wrapping_add(0x8acf_7ca1) ^ i as u32;
                let hash = state.wrapping_mul(0x1a8b_7caf);
                let hash = (hash ^ (hash >> 16)) as u16;
                if !ret.contains(&hash) && hash != 0 {
                    ret.push(hash);
                    break;
                }
            }
        }

        ret
    }

    #[test]
    fn bpi4() {
        unsafe {
            let states = init_test_states(15);
            let mut palette = Palette::empty();
            for (i, e) in states.iter().enumerate() {
                let k = palette.insert(*e, &Global).unwrap_or_else(|i| i);
                assert_eq!(i + 1, k, "e: {e}");
                assert_eq!(palette.search(*e), Some(k));
            }

            assert_eq!(palette.capacity(), 16);

            for (i, e) in states.iter().enumerate() {
                let k = palette.search(*e).unwrap();
                assert_eq!(i + 1, k, "e: {e}");
            }

            palette.dealloc_if_owned(&Global);
        }
    }

    #[test]
    fn bpi8() {
        unsafe {
            let states = init_test_states(127);
            let mut palette = Palette::empty();
            for (i, e) in states.iter().enumerate() {
                let k = palette.insert(*e, &Global).unwrap_or_else(|i| i);
                assert_eq!(i + 1, k, "e: {e}");
                assert_eq!(palette.search(*e), Some(k), "e: {e}");
            }

            assert_eq!(palette.capacity(), 128);

            for (i, e) in states.iter().enumerate() {
                let k = palette.search(*e).unwrap();
                assert_eq!(i + 1, k, "e: {e}");
            }

            palette.dealloc_if_owned(&Global);
        }
    }

    #[test]
    fn bpi16() {
        unsafe {
            let states = init_test_states(511);
            let mut palette = Palette::empty();

            for (i, e) in states.iter().enumerate() {
                let k = palette.insert(*e, &Global).unwrap_or_else(|i| i);
                assert_eq!(i + 1, k, "e: {e}");
                assert_eq!(palette.search(*e), Some(k), "e: {e}");
            }

            assert_eq!(palette.capacity(), 512);

            for (i, e) in states.iter().enumerate() {
                let k = palette.search(*e).unwrap();
                assert_eq!(i + 1, k, "e: {e}");
            }

            palette.dealloc_if_owned(&Global);
        }
    }
}
