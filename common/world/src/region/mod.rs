use std::{
    alloc::{Allocator, Global, Layout},
    ptr::NonNull,
};

use bevy::math::{IVec2, IVec3, UVec2, UVec3, Vec2, Vec3, Vec3Swizzles, ivec2, ivec3};
use math::space::area::IArea;

pub use crate::voxel::{Light, Voxel, VoxelState};
pub use alloc::RegionAlloc;
pub use chunk::Chunk;
pub use chunk::column::{BiomeId, ColumnData};
pub use subchunk::Subchunk;

pub mod alloc;
pub mod attribute;
pub mod chunk;
pub mod format;
pub mod subchunk;

/// A 512xHx512 volume of voxels where H is a multiple of 32.
/// Regions are composed of a 16x16 array of Chunks, where each chunk is 32xHx32 voxels.
/// Chunks are composed of a column of Subchunks, where each Subchunk is a 32x32x32 volume of Voxels.
pub struct Region<A: Allocator + Clone = RegionAlloc> {
    /// 3D array of subchunks, with the memory order XZY (X-major), meaning
    /// the memory is linear on the X axis.
    subchunks: NonNull<Subchunk<A>>,

    /// 16x16 array of Chunks with memory order XZ (X-major)
    chunks: NonNull<Chunk<A>>,

    /// Number of subchunks in a Region.
    /// 256 * chunk_length.
    num_subchunks: usize,

    /// Number of subchunks in a chunk.
    chunk_length: usize,

    /// The minimum point contained by the Region.
    /// X, Y, and Z components are guaranteed to be multiples of 32.
    origin: IVec3,

    /// The maximum point contained by the Region.
    limit: IVec3,

    /// The distance from origin to limit on the Y axis.
    height: i32,
}

impl Region<RegionAlloc> {
    pub fn new(origin: IVec3, height: i32) -> Self {
        Self::new_in(origin, height, alloc::init_region_alloc())
    }
}

impl<A: Allocator + Clone> Region<A> {
    pub fn new_in(origin: IVec3, height: i32, subchunk_alloc: A) -> Self {
        assert_eq!(origin.x & 31, 0);
        assert_eq!(origin.y & 31, 0);
        assert_eq!(origin.z & 31, 0);
        assert!(height > 0);
        assert_eq!(height & 31, 0);

        let chunk_length = height / 32;
        let num_subchunks = (256 * (chunk_length)) as usize;

        unsafe {
            // allocate subchunks buffer
            let subchunks = {
                let layout = Layout::array::<Subchunk<A>>(num_subchunks).unwrap();
                Global
                    .allocate(layout)
                    .unwrap()
                    .as_non_null_ptr()
                    .cast::<Subchunk<A>>()
            };

            // allocate chunks buffer
            let chunks = {
                let layout = Layout::array::<Chunk<A>>(256).unwrap();
                Global
                    .allocate(layout)
                    .unwrap()
                    .as_non_null_ptr()
                    .cast::<Chunk<A>>()
            };

            // Construct chunks, which will initialize their subchunks.
            for x in 0..16 {
                for z in 0..16 {
                    let chunk_index = x as usize | ((z as usize) << 4);
                    let chunk_origin = origin + ivec3(x * 32, 0, z * 32);
                    let chunk_ptr = chunks.add(chunk_index);
                    chunk_ptr.write(Chunk::new(
                        chunk_origin,
                        height,
                        chunk_ptr,
                        subchunks.add(chunk_index),
                        subchunk_alloc.clone(),
                    ));
                }
            }

            Self {
                subchunks,
                chunks,
                num_subchunks,
                chunk_length: chunk_length as usize,
                origin,
                limit: origin + ivec3(512, height, 512),
                height,
            }
        }
    }

    pub const fn id(&self) -> RegionId {
        RegionId::new(ivec2(self.origin.x, self.origin.z))
    }

    /// The minimum point contained by this Region.
    #[inline(always)]
    pub const fn origin(&self) -> IVec3 {
        self.origin
    }

    /// The maximum point contained by the Region.
    #[inline(always)]
    pub const fn limit(&self) -> IVec3 {
        self.limit
    }

    /// THe distance from origin to limit on the Y axis.
    #[inline(always)]
    pub const fn height(&self) -> i32 {
        self.height
    }

    /// Check whether the region contains the XZ coordinate.
    #[inline(always)]
    pub const fn contains_xz(&self, xz: IVec2) -> bool {
        self.offset_xz(xz).is_some()
    }

    /// Check whether the region contains the XYZ coordinate.
    #[inline(always)]
    pub fn contains(&self, pos: IVec3) -> bool {
        self.offset(pos).is_some()
    }

    #[inline(always)]
    const fn offset_xz(&self, xz: IVec2) -> Option<UVec2> {
        let ox = (xz.x - self.origin.x) as u32;
        let oz = (xz.y - self.origin.z) as u32;
        if (ox | oz) < 512 {
            Some(UVec2::new(ox, oz))
        } else {
            None
        }
    }

    #[inline(always)]
    fn offset(&self, pos: IVec3) -> Option<UVec3> {
        let offs = (pos - self.origin).as_uvec3();
        if (offs.x | offs.z) < 512 && offs.y < self.height as u32 {
            Some(offs)
        } else {
            None
        }
    }

    /// Get the chunk that contains the position.
    /// This operation is wrapping and cannot fail.
    #[inline]
    pub const fn get_chunk_wrapping(&self, xz: IVec2) -> &Chunk<A> {
        unsafe { self.chunks.add(to_chunk_index_wrapping(xz)).as_ref() }
    }

    /// Get a mutable reference to the chunk that contains this position.
    /// This operation is wrapping and cannot fail.
    #[inline]
    pub fn get_chunk_mut_wrapping(&mut self, xz: IVec2) -> &mut Chunk<A> {
        unsafe { self.chunks.add(to_chunk_index_wrapping(xz)).as_mut() }
    }

    /// Get the chunk that contains this position.
    /// Returns None if the position is not contained by this region.
    #[inline]
    pub fn get_chunk(&self, xz: IVec2) -> Option<&Chunk<A>> {
        if let Some(offs) = self.offset_xz(xz) {
            let i = offs_to_chunk_index(offs);
            Some(unsafe { self.chunks.add(i).as_ref() })
        } else {
            None
        }
    }

    /// Get the chunk that contains this position.
    /// Returns None if the position is not contained by this region.
    #[inline]
    pub fn get_chunk_mut(&self, xz: IVec2) -> Option<&mut Chunk<A>> {
        if let Some(offs) = self.offset_xz(xz) {
            let i = offs_to_chunk_index(offs);
            Some(unsafe { self.chunks.add(i).as_mut() })
        } else {
            None
        }
    }

    /// Get the Subchunk that contains this position within the Region.
    #[inline]
    pub fn get_subchunk(&self, pos: IVec3) -> Option<&Subchunk<A>> {
        if let Some(offs) = self.offset(pos) {
            let i = offs_to_subchunk_index(offs);
            Some(unsafe { self.subchunks.add(i).as_ref() })
        } else {
            None
        }
    }

    /// Get the Subchunk that contains this position within the Region mutably.
    #[inline]
    pub fn get_subchunk_mut(&self, pos: IVec3) -> Option<&mut Subchunk<A>> {
        if let Some(offs) = self.offset(pos) {
            let i = offs_to_subchunk_index(offs);
            Some(unsafe { self.subchunks.add(i).as_mut() })
        } else {
            None
        }
    }

    /// Get the subchunk containing this position, assuming
    /// the XZ components of the position are in-bounds for
    /// this region.
    #[inline]
    pub(crate) unsafe fn get_subchunk_skip_xz_check(
        &self,
        pos: IVec3,
    ) -> Option<NonNull<Subchunk<A>>> {
        let offs = (pos - self.origin).as_uvec3();
        if offs.y < self.height as u32 {
            let i = offs_to_subchunk_index(offs);
            Some(unsafe { self.index_subchunk_unchecked(i) })
        } else {
            None
        }
    }

    /// Get the subchunk at this index without checking that i is in-bounds.
    #[inline]
    pub(crate) unsafe fn index_subchunk_unchecked(&self, i: usize) -> NonNull<Subchunk<A>> {
        debug_assert!(
            i < self.num_subchunks,
            "Subchunk Index out of bounds. (index: '{i}', len: '{}')",
            self.num_subchunks
        );
        unsafe { self.subchunks.add(i) }
    }

    /// Get the state of the Voxel at this position.
    /// Returns "None" if the coordinate is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn get_state(&self, pos: IVec3) -> Option<VoxelState> {
        if let Some(sub) = self.get_subchunk(pos) {
            Some(sub.get_state(pos))
        } else {
            None
        }
    }

    /// Assign the state of the Voxel at this position.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn set_state(&mut self, pos: IVec3, state: VoxelState) -> bool {
        if let Some(sub) = self.get_subchunk_mut(pos) {
            sub.set_state(pos, state);
            true
        } else {
            false
        }
    }

    /// Assign the state of the Voxel at this position, returning the previous state.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn replace_state(&mut self, pos: IVec3, state: VoxelState) -> Option<VoxelState> {
        if let Some(sub) = self.get_subchunk_mut(pos) {
            Some(sub.replace_state(pos, state))
        } else {
            None
        }
    }

    /// Get the value of the voxel at this position.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn get_voxel(&self, pos: IVec3) -> Option<Voxel> {
        if let Some(sub) = self.get_subchunk(pos) {
            Some(sub.get_voxel(pos))
        } else {
            None
        }
    }

    /// Assign the value of the voxel at this position.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn set_voxel(&mut self, pos: IVec3, v: Voxel) -> bool {
        if let Some(sub) = self.get_subchunk_mut(pos) {
            sub.set_voxel(pos, v);
            true
        } else {
            false
        }
    }

    /// Assign the value of the voxel at this position, returning the previous value.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn replace_voxel(&mut self, pos: IVec3, v: Voxel) -> Option<Voxel> {
        if let Some(sub) = self.get_subchunk_mut(pos) {
            Some(sub.replace_voxel(pos, v))
        } else {
            None
        }
    }

    /// Get the light value of the voxel at this position.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn get_light(&self, pos: IVec3) -> Option<Light> {
        if let Some(sub) = self.get_subchunk(pos) {
            Some(sub.get_light(pos))
        } else {
            None
        }
    }

    /// Assign the light value of the voxel at this position.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn set_light(&mut self, pos: IVec3, v: Light) -> bool {
        if let Some(sub) = self.get_subchunk_mut(pos) {
            sub.set_light(pos, v);
            true
        } else {
            false
        }
    }

    /// Assign the light value of the voxel at this position, returning the previous value.
    /// Returns "None" if the position is out-of-bounds.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn replace_light(&mut self, pos: IVec3, v: Light) -> Option<Light> {
        if let Some(sub) = self.get_subchunk_mut(pos) {
            Some(sub.replace_light(pos, v))
        } else {
            None
        }
    }

    /// Get a slice to the Chunks in the Region.
    pub const fn chunks(&self) -> &[Chunk<A>] {
        unsafe { std::slice::from_raw_parts(self.chunks.as_ptr(), 256) }
    }

    /// Get a mutable slice of the Chunks in the Region
    /// The layout of this slice is XZ (x-major)
    pub const fn chunks_mut(&mut self) -> &mut [Chunk<A>] {
        unsafe { std::slice::from_raw_parts_mut(self.chunks.as_ptr(), 256) }
    }

    /// Get a slice of the Subchunks in the Region.
    /// The layout of this slice is XZY (x-major)
    pub const fn subchunks(&self) -> &[Subchunk<A>] {
        unsafe { std::slice::from_raw_parts(self.subchunks.as_ptr(), self.num_subchunks) }
    }

    /// Get a mutable slice of the Subchunks in the Region.
    pub const fn subchunks_mut(&mut self) -> &mut [Subchunk<A>] {
        unsafe { std::slice::from_raw_parts_mut(self.subchunks.as_ptr(), self.num_subchunks) }
    }
}

impl<A: Allocator + Clone> Drop for Region<A> {
    fn drop(&mut self) {
        unsafe {
            // drop chunks. Each chunk will drop its own Subchunks.
            for i in 0..256 {
                self.chunks.add(i).drop_in_place();
            }

            // de-allocate subchunks buffer.
            let layout = Layout::array::<Subchunk>(self.chunk_length * 256).unwrap();
            Global.deallocate(self.subchunks.cast::<u8>(), layout);

            // de-allocate chunks buffer.
            let layout = Layout::array::<Chunk>(256).unwrap();
            Global.deallocate(self.chunks.cast::<u8>(), layout);
        }
    }
}

unsafe impl<A: Allocator + Clone> Send for Region<A> {}
unsafe impl<A: Allocator + Clone> Sync for Region<A> {}

/// i = ((x / 32) % 16) + ((z / 32) % 16) * 16
const fn to_chunk_index_wrapping(xz: IVec2) -> usize {
    (((xz.x >> 5) & 0xF) | (((xz.y >> 5) & 0xF) << 4)) as usize
}

/// i = (x / 32) + (z / 32) * 16
const fn offs_to_chunk_index(offs: UVec2) -> usize {
    ((offs.x >> 5) | (offs.y >> 5) << 4) as usize
}

/// i = (x / 32) + ((z / 32) * 16) + ((y / 32) * 256)
const fn offs_to_subchunk_index(offs: UVec3) -> usize {
    ((offs.x >> 5) | ((offs.z >> 5) << 4) | ((offs.y >> 5) << 8)) as usize
}

/// i = (x & 31) + ((z & 31) << 5)
const fn to_column_index_wrapping(xz: IVec2) -> usize {
    ((xz.x & 31) | ((xz.y & 31) << 5)) as usize
}

/// Check whether a positions containing chunk has any neighbouring chunks
/// that are in another region.
pub const fn chunk_is_fully_contained(xz: IVec2) -> bool {
    let x = ((xz.x & 511) as u32) >> 5;
    let y = ((xz.y & 511) as u32) >> 5;
    x == 0 || x == 15 || y == 0 || y == 15
}

/// A Unique Identifier for a Region, based on its XZ origin.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd, Hash, Default)]
pub struct RegionId(pub u64);

impl RegionId {
    pub const MAX: Self = Self(u64::MAX);

    pub const fn new(xz: IVec2) -> Self {
        Self((((xz.y & !511) as u64) << 32) | ((xz.x & !511) as u32 as u64))
    }

    pub const fn x(self) -> i32 {
        (self.0 & 0xFFFFFFFF) as i32
    }

    pub const fn z(self) -> i32 {
        (self.0 >> 32) as i32
    }

    pub const fn as_ivec2(&self) -> IVec2 {
        ivec2(self.x(), self.z())
    }

    pub const fn as_ivec3(&self, y: i32) -> IVec3 {
        ivec3(self.x(), y, self.z())
    }

    pub const fn area(&self) -> IArea {
        let min = self.as_ivec2();
        IArea {
            min,
            max: ivec2(min.x + 512, min.y + 512),
        }
    }
}

impl From<IVec2> for RegionId {
    fn from(value: IVec2) -> Self {
        Self::new(value)
    }
}

impl From<IVec3> for RegionId {
    fn from(value: IVec3) -> Self {
        Self::new(value.xz())
    }
}

impl From<Vec2> for RegionId {
    fn from(value: Vec2) -> Self {
        Self::from(value.as_ivec2())
    }
}

impl From<Vec3> for RegionId {
    fn from(value: Vec3) -> Self {
        Self::from(value.xz().as_ivec2())
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::{IVec3, Vec3Swizzles, ivec3};
    use zip::{Algorithm, UnzippedSpan, ZipLevel, Zipper, ZstdZipper};

    use crate::{
        World,
        region::{
            alloc::init_region_alloc,
            format::{ChunkFormat, UnzippedChunk},
        },
    };

    use super::{Region, Voxel};

    #[test]
    fn region() {
        let mut region = Region::new(ivec3(0, 0, 0), 512);

        let pos = ivec3(31, 492, 78);
        assert!(region.set_voxel(pos, Voxel(88)));
        assert_eq!(region.get_voxel(pos), Some(Voxel(88)));
        assert_eq!(region.replace_voxel(pos, Voxel(99)), Some(Voxel(88)));
    }

    #[test]
    fn iter_subchunks_in_chunk() {
        let mut origin = ivec3(416, -32, 384);
        let region = Region::new(origin, 128);
        for subchunk in region.get_chunk(origin.xz()).unwrap().iter() {
            assert_eq!(subchunk.origin(), origin);
            assert!(origin.y < 96);
            origin.y += 32;
        }
        assert_eq!(origin.y, 96);
    }

    #[test]
    fn validate_subchunk_origins() {
        let origin = IVec3::new(-1024, -128, 512);
        let region = Region::new(origin, 384);

        for y in (-128..256).step_by(32) {
            for z in (512..1024).step_by(32) {
                for x in (-1024..-512).step_by(32) {
                    let pt = ivec3(x, y, z);
                    let subchunk = region
                        .get_subchunk(pt)
                        .unwrap_or_else(|| panic!("Failed to get subchunk at point: {}", pt));
                    assert_eq!(pt, subchunk.origin());
                }
            }
        }
    }

    #[test]
    fn zip_unzip_chunk_zstd() {
        // height is equal to 128
        let mut w1 = World::new(96, -32);
        let mut w2 = World::new(96, -32);

        let origin = IVec3::new(64, -32, 64);
        let mut rng = math::rng::BitRng::from_entropy();

        // initialize region in world1 and world2
        w1.get_or_insert_region(origin.xz());
        w2.get_or_insert_region(origin.xz());

        // fill chunk with some random data
        for _ in 0..32768 {
            let voxel = Voxel(rng.take(4) as u16);
            let pt = IVec3 {
                x: origin.x + rng.take(5) as i32,
                z: origin.z + rng.take(5) as i32,
                y: origin.y + rng.take(7) as i32,
            };

            w1.set_voxel(pt, voxel);
        }

        // zip chunk in world1 to bytes
        let data = w1
            .get_chunk(origin.xz())
            .unwrap()
            .zip(Algorithm::Zstd, ZipLevel::default());

        // unzip to span, then read into world2 at same chunk.
        let span = UnzippedChunk::unzip(&data).unwrap();
        let success = w2.read_unzipped_chunk(span, false).unwrap();
        assert_eq!(success.origin, origin);
        assert_eq!(success.format, ChunkFormat::V1);

        // check that both w1 and w2 are equal to each other.
        w1.get_chunk(origin.xz())
            .unwrap()
            .assert_voxels_eq(&w2.get_chunk(origin.xz()).unwrap())
    }
}
