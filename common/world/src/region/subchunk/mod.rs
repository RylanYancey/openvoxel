use std::{alloc::Allocator, ptr::NonNull};

use super::{alloc::RegionAlloc, format::SubchunkHeader};
use crate::{
    Chunk,
    voxel::{Light, Voxel, VoxelState},
};
use bevy::math::IVec3;
use lights::Lights;
use voxels::Voxels;
use zip::Zipper;

mod lights;
mod voxels;

/// A 32x32x32 Volume of Voxels.
/// Memory ordering is YXZ (Y-major), meaning the memory is linear on the Y axis.
pub struct Subchunk<A: Allocator + Clone = RegionAlloc> {
    /// 32x32x32 Array of Voxel State Indices.
    voxels: Voxels<A>,

    /// 32x32x32 Array of Voxel Light states.
    lights: Lights<A>,

    /// Minimum coordinate contained by this subchunk.
    /// Guaranteed to be a multiple of 32.
    origin: IVec3,

    /// Pointer to the Subchunk's parent chunk.
    _parent: NonNull<Chunk<A>>,
}

impl<A: Allocator + Clone> Subchunk<A> {
    // Initialize a subchunk that is all air with light values of AMBIENT_FULL.
    #[inline(always)]
    pub(crate) unsafe fn new(alloc: A, origin: IVec3, parent: NonNull<Chunk<A>>) -> Self {
        Self {
            voxels: Voxels::empty(alloc.clone()),
            lights: Lights::new(Light::AMBIENT_FULL, alloc),
            origin,
            _parent: parent,
        }
    }

    /// Get the minimum coordinate contained by the subchunk.
    /// X,Y, and Z components are guaranteed to be a multiple of 32.
    pub const fn origin(&self) -> IVec3 {
        self.origin
    }

    /// Get the header for the subchunk.
    /// This is used when compressing chunk data, at the start
    /// of each subchunk's section.
    /// Always comforms to the latest SubchunkHeader version.
    pub const fn header(&self) -> SubchunkHeader {
        let palette_len = self.voxels.palette_len() as u16;
        SubchunkHeader {
            y_origin: self.origin.y,
            palette_len,
            padding_size: ((8 - ((palette_len << 1) & 7)) & 7) as u8,
            bpi: self.voxels.bpi(),
        }
    }

    /// Get the state of the Voxel at this position.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn get_state(&self, pos: IVec3) -> VoxelState {
        let i = to_voxel_index_wrapping(pos);
        unsafe {
            VoxelState {
                voxel: Voxel(self.voxels.get(i)),
                light: self.lights.get(i),
            }
        }
    }

    /// Assign the state of the Voxel at this position.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn set_state(&mut self, pos: IVec3, state: VoxelState) {
        let i = to_voxel_index_wrapping(pos);
        unsafe {
            self.voxels.set(i, state.voxel.0);
            self.lights.set(i, state.light);
        }
    }

    /// Assign the state of the Voxel at this position, returning the previous state.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn replace_state(&mut self, pos: IVec3, state: VoxelState) -> VoxelState {
        let i = to_voxel_index_wrapping(pos);
        unsafe {
            VoxelState {
                voxel: Voxel(self.voxels.replace(i, state.voxel.0)),
                light: self.lights.replace(i, state.light),
            }
        }
    }

    /// Get the value of the voxel at this position.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn get_voxel(&self, pos: IVec3) -> Voxel {
        Voxel(unsafe { self.voxels.get(to_voxel_index_wrapping(pos)) })
    }

    /// Assign the value of the voxel at this position.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn set_voxel(&mut self, pos: IVec3, v: Voxel) {
        unsafe { self.voxels.set(to_voxel_index_wrapping(pos), v.0) }
    }

    /// Assign the value of the voxel at this position, returning the previous value.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn replace_voxel(&mut self, pos: IVec3, v: Voxel) -> Voxel {
        Voxel(unsafe { self.voxels.replace(to_voxel_index_wrapping(pos), v.0) })
    }

    /// Get the light value of the voxel at this position.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn get_light(&self, pos: IVec3) -> Light {
        unsafe { self.lights.get(to_voxel_index_wrapping(pos)) }
    }

    /// Assign the light value of the voxel at this position.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn set_light(&mut self, pos: IVec3, v: Light) {
        unsafe { self.lights.set(to_voxel_index_wrapping(pos), v) }
    }

    /// Assign the light value of the voxel at this position, returning the previous value.
    /// This operation is wrapping and cannot fail.
    #[inline(always)]
    pub fn replace_light(&mut self, pos: IVec3, v: Light) -> Light {
        unsafe { self.lights.replace(to_voxel_index_wrapping(pos), v) }
    }

    pub const fn is_empty(&self) -> bool {
        self.voxels.is_empty()
    }

    /// Assign a value of 0 to all voxels in the subchunk.
    pub fn fill_air(&mut self) {
        self.voxels.set_empty();
    }

    /// Assign palette/words buffer pointers directly.
    /// NOTE: palette_len is the number of elements, words_size is the number of bytes.
    pub(crate) unsafe fn assign_voxel_ptrs(
        &mut self,
        palette_len: u16,
        words_size: usize,
        bpi: u8,
        palette: NonNull<u16>,
        words: NonNull<usize>,
    ) {
        unsafe {
            self.voxels.assign_borrowed_ptrs_unchecked(
                palette_len,
                words_size,
                bpi,
                palette,
                words,
            );
        }
    }

    pub(crate) fn zip<Z: Zipper>(&self, zipper: &mut Z) {
        // get the header of this subchunk.
        let header = self.header();
        debug_assert!(header.padding_size <= 7);
        // write header data
        zipper.put_as(&header);
        // write palette
        zipper.put(self.voxels.palette_as_bytes());
        // pad to 8-byte alignment
        for _ in 0..header.padding_size {
            zipper.put(&[0]);
        }
        // write words
        zipper.put(self.voxels.words_as_bytes());
    }

    pub(crate) fn get_size_estimate(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            self.voxels.palette_as_bytes().len() + self.voxels.words_as_bytes().len()
        }
    }

    #[cfg(test)]
    pub(crate) fn assert_voxels_eq(&self, other: &Self) {
        assert_eq!(self.origin, other.origin);
        assert_eq!(
            self.voxels.palette_len(),
            other.voxels.palette_len(),
            "origin: {}",
            self.origin
        );
        assert_eq!(
            self.voxels.bpi(),
            other.voxels.bpi(),
            "origin: {}",
            self.origin
        );
        assert_eq!(
            self.voxels.palette_as_bytes(),
            other.voxels.palette_as_bytes(),
            "origin: {}",
            self.origin
        );
        assert_eq!(self.voxels.words_as_bytes(), other.voxels.words_as_bytes());
    }
}

/// Convert a world position to an index in a subchunk.
///
/// This function is guaranteed to return an index in the range [0,32768), since it
/// works by wrapping.
///
/// Formula: i = (y % 32) | ((x % 32) * 32) | ((z $ 32 * 1024))
#[inline(always)]
const fn to_voxel_index_wrapping(pos: IVec3) -> usize {
    (pos.y as usize & 31) | ((pos.x as usize & 31) << 5) | ((pos.z as usize & 31) << 10)
}
