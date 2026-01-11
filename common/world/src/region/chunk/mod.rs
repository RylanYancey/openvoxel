use std::ops::{Index, IndexMut, Not};
use std::ptr::NonNull;
use std::{alloc::Allocator, ops::Range};

use super::{
    alloc::RegionAlloc,
    format::{ChunkFormat, ChunkHeader},
    subchunk::Subchunk,
};
use crate::region::RegionId;
use crate::region::chunk::flags::ChunkState;
use crate::region::format::ZippedChunk;
use crate::voxel::{Light, Voxel, VoxelState};
use bevy::math::{IVec2, IVec3, Vec2, Vec3, Vec3Swizzles, ivec2, ivec3};
use bytes::Bytes;
use column::{ColumnData, ColumnMap};
use math::space::area::IArea;
use zip::{Algorithm, UnzippedSpan, ZipLevel, Zipper};

pub mod column;
pub mod flags;

/// A vertical column of subchunks in a Region.
pub struct Chunk<A: Allocator + Clone = RegionAlloc> {
    /// Non-owning pointer to Subchunk 0 in this chunk.
    /// Borrowed from the subchunks pointer in the containing Region.
    /// Stride between subchunks in this chunk is 256, since the layout of Subchunks is YXZ.
    pub(crate) subchunks: NonNull<Subchunk<A>>,

    /// 32x32 map of column data, stores biome and heightmap info.
    /// No touchy for now.
    pub(crate) columns: ColumnMap<A>,

    /// Number of subchunks in the Chunk.
    pub(crate) length: usize,

    /// Distance from minimum to max y.
    pub(crate) height: i32,

    /// Version number to identify the chunk.
    /// Used to determine if clients need to be sent updates.
    pub(crate) revision: u64,

    /// Minimum position contained by this chunk.
    pub(crate) origin: IVec3,

    /// De-compressed chunk data that is shared across all subchunks.
    pub(crate) span: Option<UnzippedSpan<A>>,

    /// The load state of the chunk.
    pub(crate) state: ChunkState,

    /// Whether the chunk needs to be saved to disk.
    pub(crate) needs_save: bool,

    /// Compressed chunk data.
    /// If this is "None", it means the chunk needs saving.
    pub(crate) zip: Option<ZippedChunk>,
}

impl<A: Allocator + Clone> Chunk<A> {
    /// Construct new Chunk and initialize Subchunks.
    pub(crate) unsafe fn new(
        origin: IVec3,
        height: i32,
        self_ptr: NonNull<Self>,
        subchunks: NonNull<Subchunk<A>>,
        alloc: A,
    ) -> Self {
        let length = height as usize / 32;

        // initialize subchunks within chunk.
        for i in 0..length {
            unsafe {
                subchunks.add(i << 8).write(Subchunk::new(
                    alloc.clone(),
                    origin + ivec3(0, i as i32 * 32, 0),
                    self_ptr,
                ));
            }
        }

        Self {
            subchunks,
            length,
            height,
            columns: ColumnMap::new(ColumnData::default(), alloc),
            span: None,
            revision: 0,
            needs_save: false,
            state: ChunkState::Unloaded,
            origin,
            zip: None,
        }
    }

    pub const fn id(&self) -> ChunkId {
        ChunkId::new(ivec2(self.origin.x, self.origin.z))
    }

    /// The minimum coordinate contained by this chunk.
    pub const fn origin(&self) -> IVec3 {
        self.origin
    }

    /// The number of voxels tall the chunk is.
    pub const fn height(&self) -> i32 {
        self.height
    }

    /// The number of subchunks in the chunk.
    pub const fn length(&self) -> usize {
        self.length
    }

    /// The max y value contained by this chunk.
    pub const fn max_y(&self) -> i32 {
        self.origin.y + self.height
    }

    pub const fn min_y(&self) -> i32 {
        self.origin.y
    }

    pub const fn area(&self) -> IArea {
        IArea {
            min: ivec2(self.origin.x, self.origin.z),
            max: ivec2(self.origin.x + 32, self.origin.z + 32),
        }
    }

    pub const fn load_state(&self) -> ChunkState {
        self.state
    }

    pub const fn load_state_mut(&mut self) -> &mut ChunkState {
        &mut self.state
    }

    pub fn get_cached_zip(&self) -> Option<ZippedChunk> {
        self.zip.clone()
    }

    /// Get the column value for this column.
    /// This operation is wrapping.
    #[inline(always)]
    pub fn get_column(&self, xz: IVec2) -> ColumnData {
        self.columns.get(xz)
    }

    /// Get a mutable reference to the column value for this column.
    /// This operation is wrapping.
    #[inline(always)]
    pub fn get_column_mut(&mut self, xz: IVec2) -> &mut ColumnData {
        self.columns.get_mut(xz)
    }

    /// Assign the column value for this column.
    /// This operation is wrapping.
    #[inline(always)]
    pub fn set_column(&mut self, xz: IVec2, value: ColumnData) {
        self.columns.set(xz, value)
    }

    #[inline(always)]
    fn subchunk_index(&self, y: i32) -> Option<usize> {
        let oy = ((y - self.origin.y) as usize) >> 5;
        if oy < self.length { Some(oy) } else { None }
    }

    /// Get the subchunk containing this Y coordinate within the chunk.
    #[inline]
    pub fn get_subchunk(&self, y: i32) -> Option<&Subchunk<A>> {
        if let Some(i) = self.subchunk_index(y) {
            Some(unsafe { self.subchunks.add(i << 8).as_ref() })
        } else {
            None
        }
    }

    /// Get the subchunk containing this Y coordinate within the chunk.
    #[inline]
    pub fn get_subchunk_mut(&self, y: i32) -> Option<&mut Subchunk<A>> {
        if let Some(i) = self.subchunk_index(y) {
            Some(unsafe { self.subchunks.add(i << 8).as_mut() })
        } else {
            None
        }
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, A> {
        Iter {
            chunk: self,
            range: 0..self.length,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, A> {
        IterMut {
            range: 0..self.length,
            chunk: self,
        }
    }

    /// Get the state of the Voxel at this position.
    /// Returns "None" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn get_state(&self, pos: IVec3) -> Option<VoxelState> {
        if let Some(sub) = self.get_subchunk(pos.y) {
            Some(sub.get_state(pos))
        } else {
            None
        }
    }

    /// Assign the state of the Voxel at this position.
    /// Returns "false" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn set_state(&mut self, pos: IVec3, state: VoxelState) -> bool {
        if let Some(sub) = self.get_subchunk_mut(pos.y) {
            sub.set_state(pos, state);
            true
        } else {
            false
        }
    }

    /// Assign the state of the Voxel at this position, returning the previous state.
    /// Returns "None" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn replace_state(&mut self, pos: IVec3, state: VoxelState) -> Option<VoxelState> {
        if let Some(sub) = self.get_subchunk_mut(pos.y) {
            Some(sub.replace_state(pos, state))
        } else {
            None
        }
    }

    /// Get the value of the voxel at this position.
    /// Returns "None" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn get_voxel(&self, pos: IVec3) -> Option<Voxel> {
        if let Some(sub) = self.get_subchunk(pos.y) {
            Some(sub.get_voxel(pos))
        } else {
            None
        }
    }

    /// Assign the value of the voxel at this position.
    /// Returns "false" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn set_voxel(&mut self, pos: IVec3, v: Voxel) -> bool {
        if let Some(sub) = self.get_subchunk_mut(pos.y) {
            sub.set_voxel(pos, v);
            true
        } else {
            false
        }
    }

    /// Assign the value of the voxel at this position, returning the previous value.
    /// Returns "None" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn replace_voxel(&mut self, pos: IVec3, v: Voxel) -> Option<Voxel> {
        if let Some(sub) = self.get_subchunk_mut(pos.y) {
            Some(sub.replace_voxel(pos, v))
        } else {
            None
        }
    }

    /// Get the light value of the voxel at this position.
    /// Returns "None" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn get_light(&self, pos: IVec3) -> Option<Light> {
        if let Some(sub) = self.get_subchunk(pos.y) {
            Some(sub.get_light(pos))
        } else {
            None
        }
    }

    /// Assign the light value of the voxel at this position.
    /// Returns "false" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn set_light(&mut self, pos: IVec3, v: Light) -> bool {
        if let Some(sub) = self.get_subchunk_mut(pos.y) {
            sub.set_light(pos, v);
            true
        } else {
            false
        }
    }

    /// Assign the light value of the voxel at this position, returning the previous value.
    /// Returns "None" if the Y coordinate is above or below the chunk.
    /// X and Z components will wrap.
    #[inline(always)]
    pub fn replace_light(&mut self, pos: IVec3, v: Light) -> Option<Light> {
        if let Some(sub) = self.get_subchunk_mut(pos.y) {
            Some(sub.replace_light(pos, v))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn non_empty_mask(&self) -> SubchunkMask {
        let mut mask = SubchunkMask::EMPTY;
        for subchunk in self {
            if !subchunk.is_empty() {
                mask.set(subchunk.origin().y);
            }
        }
        mask
    }

    pub fn fill_air(&mut self) {
        for subchunk in self {
            subchunk.fill_air();
        }
    }

    pub fn set_cached_zip(&mut self, zip: ZippedChunk) {
        self.zip = Some(zip);
    }

    pub fn zip(&self, alg: Algorithm, level: ZipLevel) -> ZippedChunk {
        let result = match alg {
            Algorithm::Zstd => self.init_and_zip::<zip::ZstdZipper>(level),
            Algorithm::Lz4 => self.init_and_zip::<zip::Lz4Zipper>(level),
        };
        let buf = Bytes::from(result);
        ZippedChunk(buf)
    }

    fn init_and_zip<Z: Zipper>(&self, level: ZipLevel) -> Vec<u8> {
        let buf = Vec::with_capacity(0);
        let mut zipper = Z::init(buf, level);
        self.zip_into(&mut zipper);
        zipper.finish()
    }

    pub fn zip_into<Z: Zipper>(&self, zipper: &mut Z) {
        // get header data
        let mask = self.non_empty_mask();
        let header = ChunkHeader {
            origin: self.origin,
            height: self.height,
            format: ChunkFormat::LATEST as u16,
            length: mask.0.count_ones() as u16,
            state: self.state as u16,
            _unused: 0,
            revision: self.revision,
        };
        // write headers
        zipper.put_as(&header);
        // write subchunks
        for y in mask {
            self.get_subchunk(y).unwrap().zip(zipper);
        }
    }

    pub fn mask(&self) -> SubchunkMask {
        SubchunkMask::between_y_values(self.min_y(), self.max_y())
    }

    /// Get the cached zip, or re-zip it.
    pub fn get_cached_or_zip(&mut self, alg: Algorithm, level: ZipLevel) -> ZippedChunk {
        if let Some(cached) = &self.zip {
            cached.clone()
        } else {
            self.needs_save = true;
            let ret = self.zip(alg, level);
            self.zip = Some(ret.clone());
            ret
        }
    }

    #[cfg(test)]
    pub(crate) fn assert_voxels_eq(&self, other: &Self) {
        // check for equivalent y ranges
        assert_eq!(self.mask(), other.mask());
        // check for equivalent occupancy
        assert_eq!(self.non_empty_mask(), other.non_empty_mask());
        for (lhs, rhs) in self.iter().zip(other.iter()) {
            lhs.assert_voxels_eq(rhs);
        }
    }
}

impl<A: Allocator + Clone> Index<usize> for Chunk<A> {
    type Output = Subchunk<A>;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.length);
        unsafe { self.subchunks.add(index << 8).as_ref() }
    }
}

impl<A: Allocator + Clone> IndexMut<usize> for Chunk<A> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.length);
        unsafe { self.subchunks.add(index << 8).as_mut() }
    }
}

impl<A: Allocator + Clone> Drop for Chunk<A> {
    fn drop(&mut self) {
        unsafe {
            // ONLY drops the subchunks, not the subchunk pointer,
            // which is dropped by the Region.
            for i in 0..self.length {
                self.subchunks.add(i << 8).drop_in_place();
            }
        }
    }
}

impl<'a, A: Allocator + Clone> IntoIterator for &'a Chunk<A> {
    type IntoIter = Iter<'a, A>;
    type Item = &'a Subchunk<A>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, A: Allocator + Clone> IntoIterator for &'a mut Chunk<A> {
    type IntoIter = IterMut<'a, A>;
    type Item = &'a mut Subchunk<A>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Iterates from bottom to top.
pub struct Iter<'a, A: Allocator + Clone> {
    chunk: &'a Chunk<A>,
    range: Range<usize>,
}

impl<'a, A: Allocator + Clone> Iterator for Iter<'a, A> {
    type Item = &'a Subchunk<A>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(&self.chunk[self.range.next()?])
    }
}

impl<'a, A: Allocator + Clone> DoubleEndedIterator for Iter<'a, A> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(&self.chunk[self.range.next_back()?])
    }
}

/// Iterates from bottom to top.
pub struct IterMut<'a, A: Allocator + Clone> {
    chunk: &'a mut Chunk<A>,
    range: Range<usize>,
}

impl<'a, A: Allocator + Clone> Iterator for IterMut<'a, A> {
    type Item = &'a mut Subchunk<A>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let i = self.range.next()?;
        // has to be unsafe cuz mutable reference in iterator blah blah
        unsafe {
            let mut ptr = self.chunk.subchunks.add(i << 8);
            Some(ptr.as_mut())
        }
    }
}

impl<'a, A: Allocator + Clone> DoubleEndedIterator for IterMut<'a, A> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let i = self.range.next_back()?;
        // has to be unsafe cuz mutable reference in iterator blah blah
        unsafe {
            let mut ptr = self.chunk.subchunks.add(i << 8);
            Some(ptr.as_mut())
        }
    }
}

/// One bit for each subchunk in a span from -1024 to +992.
/// Each subchunk is 32 voxels tall. 992 is 1024-32.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct SubchunkMask(pub u64);

impl SubchunkMask {
    pub const EMPTY: Self = Self(0);

    /// The subchunk containing y_max is the first one excluded.
    #[inline]
    pub fn between_y_values(y_min: i32, y_max: i32) -> Self {
        let min = to_mask_index(y_min).min(63);
        let max = to_mask_index(y_max).min(63);

        let below_min = (1u64 << min) - 1;
        let below_max = (1u64 << max) - 1;

        Self(below_max & !below_min)
    }

    #[inline]
    pub fn iter(self) -> SubchunkMaskIter {
        SubchunkMaskIter(self.0)
    }

    #[inline]
    pub fn get(self, y: i32) -> bool {
        self.0 & (1 << to_mask_index(y).min(63)) != 0
    }

    #[inline]
    pub fn has(self, y: i32) -> bool {
        self.get(y)
    }

    /// Set the bit for the subchunk at y to false.
    /// Returns "true" if a change occurred.
    #[inline]
    pub fn clear(&mut self, y: i32) -> bool {
        let old = self.0;
        self.0 = old & !(1 << to_mask_index(y).min(63));
        old != self.0
    }

    /// Set the bit for the subchunk at y to true.
    /// Returns "true" if a change occurred.
    #[inline]
    pub fn set(&mut self, y: i32) -> bool {
        let old = self.0;
        self.0 = old | (1 << to_mask_index(y).min(63));
        old != self.0
    }

    /// Get the intersection of self and rhs.
    #[inline]
    pub fn intersection(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

fn to_mask_index(y: i32) -> u32 {
    ((y + 1024) as u32) >> 5
}

impl Not for SubchunkMask {
    type Output = SubchunkMask;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl IntoIterator for SubchunkMask {
    type IntoIter = SubchunkMaskIter;
    type Item = i32;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone)]
pub struct SubchunkMaskIter(pub u64);

impl Iterator for SubchunkMaskIter {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            None
        } else {
            let j = self.0.trailing_zeros();
            self.0 &= self.0 - 1;
            Some((j << 5) as i32 - 1024)
        }
    }
}

/// A Unique Identifier for a Region, based on its XZ origin.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd, Hash, Default)]
pub struct ChunkId(pub u64);

impl ChunkId {
    pub const MAX: Self = Self(u64::MAX);

    pub const fn new(xz: IVec2) -> Self {
        // low 32 bits are X, high 32 bits are Z.
        Self((((xz.y & !31) as u64) << 32) | ((xz.x & !31) as u32 as u64))
    }

    pub const fn x(&self) -> i32 {
        (self.0 & 0xFFFFFFFF) as i32
    }

    pub const fn z(&self) -> i32 {
        (self.0 >> 32) as i32
    }

    pub const fn as_ivec2(self) -> IVec2 {
        ivec2(self.x(), self.z())
    }

    pub const fn as_ivec3(self, y: i32) -> IVec3 {
        ivec3(self.x(), y, self.z())
    }

    pub const fn area(self) -> IArea {
        let min = self.as_ivec2();
        IArea {
            min,
            max: ivec2(min.x + 32, min.y + 32),
        }
    }

    /// Convert to RegionId by masking out irrelevant bits.
    pub const fn to_region_id(self) -> RegionId {
        RegionId(self.0 & !((((1 << 9) - 1) << 32) | ((1 << 9) - 1)))
    }

    /// Convert to an index of a chunk in any Region.
    /// This means low 4 bits are X, then 4 bits Z.
    pub const fn to_chunk_idx(self) -> usize {
        (((self.0 >> 33) | ((self.0 >> 5) & 0xF)) & 0xFF) as usize
    }
}

impl From<IVec2> for ChunkId {
    fn from(value: IVec2) -> Self {
        Self::new(value)
    }
}

impl From<IVec3> for ChunkId {
    fn from(value: IVec3) -> Self {
        Self::new(value.xz())
    }
}

impl From<Vec2> for ChunkId {
    fn from(value: Vec2) -> Self {
        Self::from(value.as_ivec2())
    }
}

impl From<Vec3> for ChunkId {
    fn from(value: Vec3) -> Self {
        Self::from(value.xz().as_ivec2())
    }
}

impl Into<RegionId> for ChunkId {
    fn into(self) -> RegionId {
        self.to_region_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subchunk_mask_range() {
        let mask = SubchunkMask::between_y_values(0, 256);
        assert_eq!(mask.0.count_ones(), 8, "{:b}", mask.0);
        assert_eq!(mask.0.trailing_zeros(), 32);
        assert_eq!(mask.0.leading_zeros(), 24);
        let y_values = mask.iter().collect::<Vec<i32>>();
        assert_eq!(y_values, vec![0, 32, 64, 96, 128, 160, 192, 224]);

        let mask = SubchunkMask::between_y_values(-32, 96);
        assert_eq!(mask.0.count_ones(), 4, "{:b}", mask.0);
        assert_eq!(mask.0.trailing_zeros(), 31);
        assert_eq!(mask.0.leading_zeros(), 29);
        let y_values = mask.iter().collect::<Vec<i32>>();
        assert_eq!(y_values, vec![-32, 0, 32, 64]);
        assert!(!mask.has(-72));
        assert!(mask.has(-24));
        assert!(mask.has(15));
        assert!(mask.has(40));
        assert!(mask.has(73));
        assert!(!mask.has(98));
    }

    #[test]
    fn chunk_id_to_index() {
        let id = ChunkId::new(ivec2(512, 512));
        assert_eq!(id.to_chunk_idx(), 0);
        let id = ChunkId::new(ivec2(511, 511));
        assert_eq!(id.to_chunk_idx(), 255);
        let id = ChunkId::new(ivec2(0, 511));
        assert_eq!(id.to_chunk_idx(), 240);
        let id = ChunkId::new(ivec2(511, 0));
        assert_eq!(id.to_chunk_idx(), 15);

        let id = ChunkId::new(ivec2(-510, -510));
        assert_eq!(id.to_chunk_idx(), 0);
        let id = ChunkId::new(ivec2(-1, -1));
        assert_eq!(id.to_chunk_idx(), 255);
        let id = ChunkId::new(ivec2(0, -1));
        assert_eq!(id.to_chunk_idx(), 240);
        let id = ChunkId::new(ivec2(-1, 0));
        assert_eq!(id.to_chunk_idx(), 15);
    }

    #[test]
    fn chunk_id_to_region_id() {
        assert_eq!(
            ChunkId::new(ivec2(0, 0)).to_region_id(),
            RegionId::new(ivec2(0, 0)),
        );
        assert_eq!(
            ChunkId::new(ivec2(511, 511)).to_region_id(),
            RegionId::new(ivec2(0, 0)),
        );
        assert_eq!(
            ChunkId::new(ivec2(-510, -510)).to_region_id(),
            RegionId::new(ivec2(-256, -256)),
        );
    }
}
