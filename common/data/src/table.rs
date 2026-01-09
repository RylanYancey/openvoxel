use std::{
    alloc::{Allocator, Global, Layout},
    ops::BitAnd,
    ptr::NonNull,
};

/// Stores a set of indices. Uses linear probing to search.
pub struct IndexSet<I, A: Allocator = Global> {
    /// Pointer to set elements.
    ptr: NonNull<I>,
    /// Capacity minus one.
    bits: usize,
    /// Number of elements in the set.
    len: usize,
    alloc: A,
}

impl<I: IndexType, A: Allocator> IndexSet<I, A> {
    pub fn new_in(alloc: A) -> Self {
        Self::with_capacity_in(2, alloc)
    }

    #[inline]
    pub fn with_capacity_in(capacity: usize, alloc: A) -> Self {
        let cap = if capacity < 2 {
            2
        } else {
            capacity.next_power_of_two()
        };
        let layout = Layout::array::<u16>(capacity).unwrap();
        let ptr = unsafe {
            let ptr = alloc
                .allocate(layout)
                .unwrap()
                .as_non_null_ptr()
                .cast::<I>();
            ptr.write_bytes(u8::MAX, capacity);
            ptr
        };

        Self {
            ptr,
            bits: cap - 1,
            len: 0,
            alloc,
        }
    }

    #[inline]
    pub fn contains(&self, index: I) -> bool {
        let mut hash = index.as_usize() & self.bits;
        loop {
            // index is known to be in-bounds for the pointer
            // because of the bit-and by self.bits.
            let v = unsafe { *self.ptr.add(hash).as_ref() };

            // return true if a match is found.
            if v == index {
                return true;
            }

            // return false if an empty slot is found.
            if v == I::MAX {
                return false;
            }

            // advance to next index.
            hash = (hash + 1) & self.bits;
        }
    }
}

pub trait IndexType: Sized + Copy + PartialEq + Eq {
    const MAX: Self;

    fn as_usize(self) -> usize;
}
