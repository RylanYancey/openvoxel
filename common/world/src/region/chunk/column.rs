use std::{alloc::Allocator, ptr::NonNull};

use bevy::math::IVec2;
use data::registry::RegistryId;

use crate::region::alloc::MaybeOwnedArray;

#[derive(Copy, Clone, Debug)]
#[repr(C, align(8))]
pub struct ColumnData {
    /// BiomeID of the land at this column.
    pub land_biome: BiomeId,

    /// BiomeID of the cave at this column.
    pub cave_biome: BiomeId,

    /// Y value of highest non-air block in this column.
    pub height: i16,

    /// aligns self to 8 byte boundary, and reserves
    /// some extra space for future fields.
    pub _unused: [u8; 2],
}

impl Default for ColumnData {
    fn default() -> Self {
        Self {
            land_biome: BiomeId(0),
            cave_biome: BiomeId(0),
            height: i16::MIN,
            _unused: [0, 0],
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct BiomeId(pub u16);

impl From<RegistryId> for BiomeId {
    #[inline]
    fn from(value: RegistryId) -> Self {
        Self(value.0 as u16)
    }
}

impl Into<RegistryId> for BiomeId {
    #[inline]
    fn into(self) -> RegistryId {
        RegistryId(self.0 as usize)
    }
}

pub struct ColumnMap<A: Allocator>(MaybeOwnedArray<ColumnData, A, 1024>);

impl<A: Allocator> ColumnMap<A> {
    pub const unsafe fn borrowed(ptr: NonNull<[ColumnData; 1024]>, alloc: A) -> Self {
        Self(unsafe { MaybeOwnedArray::borrowed(ptr, alloc) })
    }

    pub fn new(fill: ColumnData, alloc: A) -> Self {
        Self(MaybeOwnedArray::new(alloc, fill))
    }

    /// This function is wrapping and is therefore infallible.
    pub const fn get(&self, xz: IVec2) -> ColumnData {
        let i = super::super::to_column_index_wrapping(xz);
        unsafe { *self.0.get_unchecked(i) }
    }

    /// This function is wrapping and is therefore infallible.
    pub fn get_mut(&mut self, xz: IVec2) -> &mut ColumnData {
        let i = super::super::to_column_index_wrapping(xz);
        unsafe { self.0.get_unchecked_mut(i) }
    }

    /// This function is wrapping and is therefore infallible.
    pub fn set(&mut self, xz: IVec2, val: ColumnData) {
        let i = super::super::to_column_index_wrapping(xz);
        unsafe { self.0.set_unchecked(i, val) }
    }

    pub const unsafe fn index_unchecked(&self, i: usize) -> &ColumnData {
        unsafe { self.0.get_unchecked(i) }
    }

    pub const unsafe fn index_unchecked_mut(&mut self, i: usize) -> &mut ColumnData {
        unsafe { self.0.get_unchecked_mut(i) }
    }
}
