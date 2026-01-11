use bevy::prelude::*;
use bitvec::{
    array::BitArray,
    ptr::{BitRef, Mut},
};
use math::space::area::IArea;

const CHUNK_FLAGS_LEN: usize = 256 / usize::BITS as usize;

pub type ChunkBits = BitArray<[usize; CHUNK_FLAGS_LEN]>;

/// Flags that describe chunks within a Region.
///
/// Has the same layout as the `chunks` array in the Region struct,
/// meaning it is X-major.
///
/// Has a fixed length of 256.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub struct ChunkMask(ChunkBits);

impl ChunkMask {
    pub const fn new() -> Self {
        Self(ChunkBits::ZERO)
    }

    pub const fn clear(&mut self) {
        self.0 = ChunkBits::ZERO;
    }

    /// Construct a ChunkMask from an Area.
    /// The IArea is expected to be created with `IArea::intersection` of some
    /// are with the area of its containing region.
    pub fn from_area(area: &IArea) -> Self {
        let mut mask = ChunkMask::new();
        for cell in area.cells_pow2::<32>() {
            mask.set(cell.min, true);
        }
        mask
    }

    /// This operation is wrapping, and therefore infallible.
    /// The XZ value does not need to be an offset, it just needs
    /// to be within the bounds of its parent region.
    pub fn get(&self, xz: IVec2) -> bool {
        let i = super::to_chunk_index_wrapping(xz);
        debug_assert!(i < self.0.len());
        unsafe { *self.0.get_unchecked(i) }
    }

    /// This operation is wrapping, and therefore infallible.
    /// The XZ value does not need to be an offset, it just needs
    /// to be within the bounds of its parent region.
    pub fn set(&mut self, xz: IVec2, v: bool) {
        let i = super::to_chunk_index_wrapping(xz);
        debug_assert!(i < self.0.len());
        unsafe { self.0.set_unchecked(i, v) }
    }

    /// This operation is wrapping, and therefore infallible.
    /// The XZ value does not need to be an offset, it just needs
    /// to be within the bounds of its parent region.
    pub fn get_mut<'a>(&'a mut self, xz: IVec2) -> BitRef<'a, Mut> {
        let i = super::to_chunk_index_wrapping(xz);
        debug_assert!(i < self.0.len());
        unsafe { self.0.get_unchecked_mut(i) }
    }

    /// Panics if i is out of bounds.
    pub fn index(&self, i: usize) -> bool {
        *self
            .0
            .get(i)
            .expect("[W456] Index out of bounds in chunk flags.")
    }

    /// Panics if i is out of bounds.
    pub fn index_mut<'a>(&'a mut self, i: usize) -> BitRef<'a, Mut> {
        self.0
            .get_mut(i)
            .expect("[W455] Index out of bounds in chunk flags.")
    }

    pub fn set_index(&mut self, i: usize, v: bool) {
        self.0.set(i, v)
    }

    /// Get the intersection of self and rhs.
    pub fn intersection(&self, rhs: &Self) -> Self {
        Self(self.0 & rhs.0)
    }

    pub fn iter_ones(&self) -> impl Iterator<Item = IVec2> {
        self.0.iter_ones().map(|i| IVec2 {
            x: (i & 0xF) as i32,
            y: (i >> 8) as i32,
        })
    }
}

impl std::ops::Not for ChunkMask {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl std::ops::BitAnd for ChunkMask {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::BitOr for ChunkMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for ChunkMask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0
    }
}
