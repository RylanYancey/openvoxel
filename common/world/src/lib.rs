#![feature(slice_ptr_get)]
#![feature(allocator_api)]
#![feature(int_roundings)]
#![feature(box_vec_non_null)]
#![feature(pointer_is_aligned_to)]

use std::ptr::NonNull;

use bevy::{
    ecs::resource::Resource,
    math::{IVec2, IVec3, Vec3Swizzles, ivec3},
};
use zip::UnzippedSpan;

use crate::region::{
    RegionAlloc, RegionId,
    format::{ChunkReadError, ChunkReadSuccess, UnzippedChunk},
};
pub use crate::{
    region::{BiomeId, Chunk, ColumnData, Region, Subchunk},
    voxel::{Light, Voxel, VoxelState},
};

pub mod knn;
pub mod region;
pub mod resolver;
pub mod voxel;

#[derive(Resource)]
pub struct World {
    regions: Vec<NonNull<Region>>,
    resolver: resolver::OwnedResolver,
    height: usize,
    max_y: i32,
    min_y: i32,
}

impl World {
    pub fn new(max_y: i32, min_y: i32) -> Self {
        assert_eq!(max_y & 31, 0);
        assert_eq!(min_y & 31, 0);
        assert!(max_y > min_y);
        let height = (max_y - min_y) as usize;
        assert_eq!(height & 31, 0);
        Self {
            regions: Vec::new(),
            resolver: resolver::OwnedResolver::new(),
            height,
            max_y,
            min_y,
        }
    }

    #[inline(always)]
    pub const fn min_y(&self) -> i32 {
        self.min_y
    }

    #[inline(always)]
    pub const fn max_y(&self) -> i32 {
        self.max_y
    }

    #[inline(always)]
    pub const fn height(&self) -> i32 {
        self.height as i32
    }

    /// Insert a region.
    /// This will invalidate any borrowed region maps.
    /// Panics if the region already exists.
    pub fn insert(&mut self, region: Box<Region>) {
        assert_eq!(region.origin().x & 511, 0);
        assert_eq!(region.origin().z & 511, 0);
        assert_eq!(region.origin().y, self.min_y);
        assert_eq!(region.limit().y, self.max_y);
        assert!(!self.has_region(region.id()));
        self.regions.push(Box::into_non_null(region));
        unsafe { self.resolver.insert_and_rebuild_if_needed(&self.regions) }
    }

    /// This may invalidate borrowed region maps if used.
    /// Only really intended for test purposes.
    pub fn get_or_insert_region(&mut self, id: impl Into<RegionId>) -> &mut Region {
        let id = id.into();
        if !self.has_region(id) {
            let v2 = id.as_ivec2();
            self.insert(Box::new(Region::new(
                ivec3(v2.x, self.min_y, v2.y),
                self.height(),
            )));
        }

        self.get_region_mut(id).unwrap()
    }

    pub fn read_unzipped_chunk(
        &mut self,
        data: UnzippedChunk,
        allow_region_load: bool,
    ) -> Result<ChunkReadSuccess, ChunkReadError> {
        region::format::read_chunk_from_span(self, data.0, allow_region_load)
    }

    /// Remove the Region that contains this XZ position.
    /// This will invalidate any borrowed region maps.
    pub fn remove(&mut self, id: impl Into<RegionId>) -> Option<Box<Region>> {
        let id = id.into();
        if let Some(i) = unsafe { self.resolver.remove(id) } {
            let ptr = if i != self.regions.len() - 1 {
                // region at last index in `regions` will be moved to removed index
                let last_id = unsafe { self.regions.last().unwrap().as_ref().id() };
                self.resolver.set_bucket_index(last_id, i);
                self.regions.swap_remove(i)
            } else {
                self.regions.pop().unwrap()
            };
            Some(unsafe { Box::from_non_null(ptr) })
        } else {
            None
        }
    }

    /// Check whether the region exists in the map.
    #[inline]
    pub fn has_region(&self, id: impl Into<RegionId>) -> bool {
        self.resolver.get(id.into()).is_some()
    }

    /// Get the region that contains this XZ position.
    /// Returns None if the region does not exist in the World.
    #[inline]
    pub fn get_region(&self, id: impl Into<RegionId>) -> Option<&Region> {
        self.resolver.get(id.into())
    }

    /// Get the region that contains this XZ position.
    /// Returns None if the region does not exist in the World.
    #[inline]
    pub fn get_region_mut(&mut self, id: impl Into<RegionId>) -> Option<&mut Region> {
        self.resolver.get_mut(id.into())
    }

    /// Get the chunk that contains this XZ position.
    /// Returns None if the chunk's containing region does not exist in the World.
    #[inline]
    pub fn get_chunk(&self, xz: IVec2) -> Option<&Chunk> {
        if let Some(region) = self.get_region(xz & !511) {
            Some(region.get_chunk_wrapping(xz))
        } else {
            None
        }
    }

    /// Get the chunk that contains this XZ position.
    /// Returns None if the chunk's containing region does not exist in the World.
    #[inline]
    pub fn get_chunk_mut(&mut self, xz: IVec2) -> Option<&mut Chunk> {
        if let Some(region) = self.get_region_mut(xz) {
            Some(region.get_chunk_mut_wrapping(xz))
        } else {
            None
        }
    }

    /// Get the subchunk that contains this position.
    /// Returns None if the subchunk's containing region does not exist in the World,
    /// or the y value is above or below bounds.
    #[inline]
    pub fn get_subchunk(&self, pos: IVec3) -> Option<&Subchunk> {
        if let Some(region) = self.get_region(pos.xz()) {
            unsafe {
                // we can skip the XZ check because it is checked by `get_region`.
                if let Some(subchunk) = region.get_subchunk_skip_xz_check(pos) {
                    return Some(subchunk.as_ref());
                }
            }
        }

        None
    }

    /// Get the subchunk that contains this position.
    /// Returns None if the subchunk's containing region does not exist in the World,
    /// or the y value is above or below bounds.
    #[inline]
    pub fn get_subchunk_mut(&mut self, pos: IVec3) -> Option<&mut Subchunk> {
        if let Some(region) = self.get_region_mut(pos.xz() & !511) {
            unsafe {
                // we can skip the XZ check because it is checked by `get_region`.
                if let Some(mut subchunk) = region.get_subchunk_skip_xz_check(pos) {
                    return Some(subchunk.as_mut());
                }
            }
        }

        None
    }

    /// Get the voxel state at this position.
    /// Returns None if the containing region does not exist in the World, or
    /// the Y value is above or below bounds.
    #[inline]
    pub fn get_state(&self, pos: IVec3) -> Option<VoxelState> {
        if let Some(subchunk) = self.get_subchunk(pos) {
            Some(subchunk.get_state(pos))
        } else {
            None
        }
    }

    /// Assign the voxel state at this position.
    /// Returns None if the containing region does not exist in the World,
    /// or the Y value is above or below bounds.
    #[inline]
    pub fn set_state(&mut self, pos: IVec3, state: VoxelState) -> bool {
        if let Some(subchunk) = self.get_subchunk_mut(pos) {
            subchunk.set_state(pos, state);
            true
        } else {
            false
        }
    }

    /// Assign the voxel state at this position, returning the previous state.
    /// Returns None if the containing region does not exist in the World,
    /// or the Y value is above or below bounds.
    #[inline]
    pub fn replace_state(&mut self, pos: IVec3, state: VoxelState) -> Option<VoxelState> {
        if let Some(subchunk) = self.get_subchunk_mut(pos) {
            Some(subchunk.replace_state(pos, state))
        } else {
            None
        }
    }

    /// Get the voxel at this position.
    /// Returns None if the containing region does not exist in the World, or
    /// the Y value is above or below bounds.
    #[inline]
    pub fn get_voxel(&self, pos: IVec3) -> Option<Voxel> {
        if let Some(subchunk) = self.get_subchunk(pos) {
            Some(subchunk.get_voxel(pos))
        } else {
            None
        }
    }

    /// Assign the voxel at this position.
    /// Returns None if the containing region does not exist in the World,
    /// or the Y value is above or below bounds.
    #[inline]
    pub fn set_voxel(&mut self, pos: IVec3, voxel: Voxel) -> bool {
        if let Some(subchunk) = self.get_subchunk_mut(pos) {
            subchunk.set_voxel(pos, voxel);
            true
        } else {
            false
        }
    }

    /// Assign the voxel at this position, returning the previous state.
    /// Returns None if the containing region does not exist in the World,
    /// or the Y value is above or below bounds.
    #[inline]
    pub fn replace_voxel(&mut self, pos: IVec3, voxel: Voxel) -> Option<Voxel> {
        if let Some(subchunk) = self.get_subchunk_mut(pos) {
            Some(subchunk.replace_voxel(pos, voxel))
        } else {
            None
        }
    }

    /// Get the voxel light at this position.
    /// Returns None if the containing region does not exist in the World, or
    /// the Y value is above or below bounds.
    #[inline]
    pub fn get_light(&self, pos: IVec3) -> Option<Light> {
        if let Some(subchunk) = self.get_subchunk(pos) {
            Some(subchunk.get_light(pos))
        } else {
            None
        }
    }

    /// Assign the voxel light at this position.
    /// Returns None if the containing region does not exist in the World,
    /// or the Y value is above or below bounds.
    #[inline]
    pub fn set_light(&mut self, pos: IVec3, light: Light) -> bool {
        if let Some(subchunk) = self.get_subchunk_mut(pos) {
            subchunk.set_light(pos, light);
            true
        } else {
            false
        }
    }

    /// Assign the voxel light at this position, returning the previous state.
    /// Returns None if the containing region does not exist in the World,
    /// or the Y value is above or below bounds.
    #[inline]
    pub fn replace_light(&mut self, pos: IVec3, light: Light) -> Option<Light> {
        if let Some(subchunk) = self.get_subchunk_mut(pos) {
            Some(subchunk.replace_light(pos, light))
        } else {
            None
        }
    }
}

unsafe impl Send for World {}
unsafe impl Sync for World {}

#[cfg(test)]
mod tests {
    use bevy::math::{IVec2, ivec2, ivec3};

    use crate::World;

    #[test]
    fn get_region() {
        let mut world = World::new(256, -128);
        world.get_or_insert_region(ivec2(64, 64));
        assert_eq!(
            world.get_region(ivec2(64, 64)).unwrap().origin(),
            ivec3(0, -128, 0)
        );
        assert_eq!(
            world.get_region_mut(ivec2(64, 64)).unwrap().origin(),
            ivec3(0, -128, 0)
        );
    }

    #[test]
    fn get_chunk() {
        let mut world = World::new(256, -128);
        world.get_or_insert_region(ivec2(64, 64));
        assert_eq!(
            world.get_chunk(ivec2(64, 64)).unwrap().origin(),
            ivec3(64, -128, 64)
        );
        assert_eq!(
            world.get_chunk_mut(ivec2(64, 64)).unwrap().origin(),
            ivec3(64, -128, 64)
        );
    }
}
