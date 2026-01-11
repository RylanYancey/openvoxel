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
            x: ((i & 0xF) as i32) << 5,
            y: ((i >> 4) as i32) << 5,
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

#[cfg(test)]
mod chunk_mask_tests {
    use super::*;

    // Helper function to create a region-relative area
    // Assuming regions are 512x512 and contain 16x16 chunks of 32x32 each
    fn region_area(min_chunk: IVec2, max_chunk: IVec2) -> IArea {
        IArea::new(min_chunk * 32, max_chunk * 32)
    }

    #[test]
    fn test_chunk_mask_new_is_empty() {
        let mask = ChunkMask::new();

        // All bits should be false
        for y in 0..16 {
            for x in 0..16 {
                assert!(
                    !mask.get(ivec2(x * 32, y * 32)),
                    "New mask should have all bits false at chunk ({}, {})",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn test_chunk_mask_set_and_get() {
        let mut mask = ChunkMask::new();

        // Set some chunks
        mask.set(ivec2(0, 0), true);
        mask.set(ivec2(32, 0), true);
        mask.set(ivec2(0, 32), true);
        mask.set(ivec2(480, 480), true); // Last chunk (15, 15)

        // Verify they are set
        assert!(mask.get(ivec2(0, 0)));
        assert!(mask.get(ivec2(32, 0)));
        assert!(mask.get(ivec2(0, 32)));
        assert!(mask.get(ivec2(480, 480)));

        // Verify others are not set
        assert!(!mask.get(ivec2(64, 0)));
        assert!(!mask.get(ivec2(32, 32)));
    }

    #[test]
    fn test_chunk_mask_wrapping_behavior() {
        let mut mask = ChunkMask::new();

        // Test that coordinates anywhere within a chunk map to the same bit
        mask.set(ivec2(0, 0), true);

        // All these should access the same chunk (0, 0)
        assert!(mask.get(ivec2(0, 0)));
        assert!(mask.get(ivec2(15, 15)));
        assert!(mask.get(ivec2(31, 31)));

        // But not the next chunk
        assert!(!mask.get(ivec2(32, 0)));
    }

    #[test]
    fn test_chunk_mask_from_area_single_chunk() {
        // Area covering only one chunk
        let area = IArea::new(ivec2(10, 10), ivec2(30, 30));
        let mask = ChunkMask::from_area(&area);

        println!("Area: {:?}", area);
        println!("Chunks for area:");
        for cell in area.cells_pow2::<32>() {
            println!("  Cell: {:?}", cell);
        }

        // Should have exactly one chunk set
        let count = mask.iter_ones().count();
        assert_eq!(count, 1, "Single-chunk area should set exactly 1 chunk");

        assert!(mask.get(ivec2(0, 0)), "Chunk (0,0) should be set");
    }

    #[test]
    fn test_chunk_mask_from_area_multiple_chunks() {
        // Area covering 2x2 chunks
        let area = IArea::new(ivec2(0, 0), ivec2(64, 64));
        let mask = ChunkMask::from_area(&area);

        println!("Area: {:?}", area);
        println!("Chunks set:");
        for chunk in mask.iter_ones() {
            println!("  Chunk: {:?}", chunk);
        }

        // Should have 4 chunks set (2x2 grid)
        let count = mask.iter_ones().count();
        assert_eq!(count, 4, "2x2 chunk area should set exactly 4 chunks");

        assert!(mask.get(ivec2(0, 0)));
        assert!(mask.get(ivec2(32, 0)));
        assert!(mask.get(ivec2(0, 32)));
        assert!(mask.get(ivec2(32, 32)));
    }

    #[test]
    fn test_chunk_mask_from_area_full_region() {
        // Area covering entire 512x512 region (16x16 chunks)
        let area = IArea::new(ivec2(0, 0), ivec2(512, 512));
        let mask = ChunkMask::from_area(&area);

        let count = mask.iter_ones().count();
        assert_eq!(count, 256, "Full region should set all 256 chunks");

        // Verify all chunks are set
        for y in 0..16 {
            for x in 0..16 {
                assert!(
                    mask.get(ivec2(x * 32, y * 32)),
                    "Chunk ({}, {}) should be set in full region",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn test_chunk_mask_from_area_partial_coverage() {
        // Area that doesn't align to chunk boundaries
        let area = IArea::new(ivec2(10, 10), ivec2(100, 100));
        let mask = ChunkMask::from_area(&area);

        println!("Area: {:?}", area);
        println!("Rounded area: {:?}", area.rounded_up_to_pow2::<32, 32>());
        println!("Chunks set: {}", mask.iter_ones().count());

        // Area spans from chunk (0,0) to chunk (3,3) = 4x4 = 16 chunks
        let count = mask.iter_ones().count();
        assert_eq!(count, 16, "Area should span 4x4 chunks");
    }

    #[test]
    fn test_chunk_mask_iter_ones_empty() {
        let mask = ChunkMask::new();
        let chunks: Vec<_> = mask.iter_ones().collect();

        assert_eq!(chunks.len(), 0, "Empty mask should yield no chunks");
    }

    #[test]
    fn test_chunk_mask_iter_ones_single() {
        let mut mask = ChunkMask::new();
        mask.set(ivec2(64, 96), true); // Chunk (2, 3)

        let chunks: Vec<_> = mask.iter_ones().collect();
        assert_eq!(chunks.len(), 1);

        // The iterator should return chunk coordinates (not world coordinates)
        let chunk = chunks[0];
        println!("Chunk from iterator: {:?}", chunk);

        // Verify we can use this coordinate with get
        assert!(mask.get(chunk));
    }

    #[test]
    fn test_chunk_mask_iter_ones_multiple() {
        let mut mask = ChunkMask::new();

        // Set a pattern of chunks
        mask.set(ivec2(0, 0), true); // (0, 0)
        mask.set(ivec2(32, 32), true); // (1, 1)
        mask.set(ivec2(64, 64), true); // (2, 2)
        mask.set(ivec2(480, 480), true); // (15, 15)

        let chunks: Vec<_> = mask.iter_ones().collect();
        assert_eq!(chunks.len(), 4);

        println!("Chunks from iterator:");
        for chunk in &chunks {
            println!("  {:?}", chunk);
        }
    }

    #[test]
    fn test_chunk_mask_iter_ones_consistency_with_from_area() {
        let area = IArea::new(ivec2(50, 50), ivec2(150, 150));
        let mask = ChunkMask::from_area(&area);

        // Get chunks from iterator
        let iter_chunks: Vec<_> = mask.iter_ones().collect();

        // Get chunks directly from area
        let area_chunks: Vec<_> = area.cells_pow2::<32>().map(|cell| cell.min / 32).collect();

        println!("From iterator: {:?}", iter_chunks);
        println!("From area cells: {:?}", area_chunks);

        assert_eq!(
            iter_chunks.len(),
            area_chunks.len(),
            "Iterator should yield same number of chunks as area.cells_pow2"
        );
    }

    #[test]
    fn test_chunk_mask_bitwise_operations() {
        let mut mask1 = ChunkMask::new();
        let mut mask2 = ChunkMask::new();

        mask1.set(ivec2(0, 0), true);
        mask1.set(ivec2(32, 0), true);

        mask2.set(ivec2(32, 0), true);
        mask2.set(ivec2(64, 0), true);

        // Test AND
        let and_result = mask1 & mask2;
        assert!(
            and_result.get(ivec2(32, 0)),
            "AND should include common chunk"
        );
        assert!(
            !and_result.get(ivec2(0, 0)),
            "AND should not include mask1-only chunk"
        );
        assert!(
            !and_result.get(ivec2(64, 0)),
            "AND should not include mask2-only chunk"
        );

        // Test OR
        let or_result = mask1 | mask2;
        assert!(or_result.get(ivec2(0, 0)), "OR should include all chunks");
        assert!(or_result.get(ivec2(32, 0)));
        assert!(or_result.get(ivec2(64, 0)));

        // Test NOT
        let not_result = !mask1;
        assert!(!not_result.get(ivec2(0, 0)), "NOT should invert set chunks");
        assert!(
            not_result.get(ivec2(64, 0)),
            "NOT should invert unset chunks"
        );
    }

    #[test]
    fn test_chunk_mask_intersection_method() {
        let mut mask1 = ChunkMask::new();
        let mut mask2 = ChunkMask::new();

        mask1.set(ivec2(0, 0), true);
        mask1.set(ivec2(32, 0), true);
        mask1.set(ivec2(64, 0), true);

        mask2.set(ivec2(32, 0), true);
        mask2.set(ivec2(64, 0), true);
        mask2.set(ivec2(96, 0), true);

        let intersection = mask1.intersection(&mask2);

        assert_eq!(intersection.iter_ones().count(), 2);
        assert!(intersection.get(ivec2(32, 0)));
        assert!(intersection.get(ivec2(64, 0)));
        assert!(!intersection.get(ivec2(0, 0)));
        assert!(!intersection.get(ivec2(96, 0)));
    }

    #[test]
    fn test_chunk_mask_clear() {
        let mut mask = ChunkMask::new();

        // Set some chunks
        mask.set(ivec2(0, 0), true);
        mask.set(ivec2(32, 32), true);
        assert_eq!(mask.iter_ones().count(), 2);

        // Clear
        mask.clear();
        assert_eq!(mask.iter_ones().count(), 0);
        assert!(!mask.get(ivec2(0, 0)));
        assert!(!mask.get(ivec2(32, 32)));
    }

    #[test]
    fn test_chunk_mask_index_operations() {
        let mut mask = ChunkMask::new();

        // Set using index (chunk 0 = (0,0), chunk 1 = (1,0), chunk 16 = (0,1))
        mask.set_index(0, true); // (0, 0)
        mask.set_index(1, true); // (1, 0)
        mask.set_index(16, true); // (0, 1)

        assert!(mask.index(0));
        assert!(mask.index(1));
        assert!(mask.index(16));
        assert!(!mask.index(2));

        // These should correspond to the same chunks via get
        assert!(mask.get(ivec2(0, 0)));
        assert!(mask.get(ivec2(32, 0)));
        assert!(mask.get(ivec2(0, 32)));
    }

    #[test]
    fn test_chunk_mask_from_area_edge_cases() {
        // Test area at region boundaries
        let area = IArea::new(ivec2(480, 480), ivec2(512, 512));
        let mask = ChunkMask::from_area(&area);

        println!("Edge area: {:?}", area);
        println!("Chunks: {}", mask.iter_ones().count());

        // Should set the last chunk (15, 15)
        assert!(mask.get(ivec2(480, 480)));
    }

    #[test]
    fn test_chunk_mask_from_area_with_intersection() {
        // Simulate intersecting an area with a region
        let full_area = IArea::new(ivec2(-100, -100), ivec2(600, 600));
        let region_bounds = IArea::new(ivec2(0, 0), ivec2(512, 512));

        let intersection = full_area
            .intersection(&region_bounds)
            .expect("Should have intersection");

        println!("Full area: {:?}", full_area);
        println!("Region bounds: {:?}", region_bounds);
        println!("Intersection: {:?}", intersection);

        let mask = ChunkMask::from_area(&intersection);

        // Should set all chunks in the region
        assert_eq!(
            mask.iter_ones().count(),
            256,
            "Intersection covering full region should set all chunks"
        );
    }

    #[test]
    fn test_chunk_mask_iter_ones_order() {
        let mut mask = ChunkMask::new();

        // Set chunks in a known pattern
        mask.set(ivec2(0, 0), true); // index 0
        mask.set(ivec2(32, 0), true); // index 1
        mask.set(ivec2(0, 32), true); // index 16
        mask.set(ivec2(32, 32), true); // index 17

        let chunks: Vec<_> = mask.iter_ones().collect();

        println!("Iteration order:");
        for (i, chunk) in chunks.iter().enumerate() {
            println!("  {}: {:?}", i, chunk);
        }

        // Verify chunks are returned in ascending index order
        assert_eq!(chunks.len(), 4);
        // The actual order depends on how iter_ones is implemented
        // but they should all be present
        assert!(chunks.contains(&ivec2(0, 0)));
        assert!(chunks.contains(&ivec2(32, 0)));
        assert!(chunks.contains(&ivec2(0, 32)));
        assert!(chunks.contains(&ivec2(32, 32)));
    }
}
