use std::{ops::Index, sync::Arc};

use rand::SeedableRng;

/// A Permutation to generate small random numbers very quickly, mostly used
/// for noise algorithms like perlin, simplex, and worley to generate offsets
/// within grid cells.
///
/// The reason the permutation has a length of 512 and not 256 is to avoid
/// the extra wrap by 255 that would have to be done while mixing
/// if the length was only 256. It's slightly faster.
#[derive(Copy, Clone, Debug)]
pub struct Permutation([u8; 512]);

impl Permutation {
    /// A Permutation where each value is equal to its index.
    pub const DEFAULT: Self = {
        let mut arr = [0u8; 512];
        let mut i = 0;
        while i < 512 {
            arr[i] = (i & 255) as u8;
            i += 1;
        }
        Self(arr)
    };

    pub fn new(s: u128) -> Arc<Self> {
        Arc::new(<Self as SeedableRng>::from_seed(s.to_ne_bytes()))
    }

    pub fn from_entropy() -> Arc<Self> {
        Arc::new(<Self as SeedableRng>::from_os_rng())
    }

    /// Mix some i32 numbers into a single value by indexing the permutation.
    /// Values provided by the iterator will be wrapped to be in-range (0..256).
    #[inline]
    pub fn mix(&self, nums: impl IntoIterator<Item = i32>) -> u8 {
        nums.into_iter()
            .fold(0u8, |curr, num| self[curr as usize + (num & 255) as usize])
    }
}

impl Index<usize> for Permutation {
    type Output = u8;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl SeedableRng for Permutation {
    type Seed = [u8; core::mem::size_of::<u128>()];

    fn from_seed(seed: Self::Seed) -> Self {
        let s1: u64 = u64::from_ne_bytes(seed[..8].try_into().unwrap());
        let s2: u64 = u64::from_ne_bytes(seed[8..].try_into().unwrap());

        // initialize rng for shuffling.
        let mut rng1 = super::BitRng::new(s1);
        let mut rng2 = super::BitRng::new(s2);

        let mut ret = Self::DEFAULT;

        // shuffle lower 256 elements
        for i in 0..256 {
            // alternative between rng1 and rng2.
            if i & 2 == 0 {
                ret.0.swap(i, rng1.take(8) as usize);
            } else {
                ret.0.swap(i, rng2.take(8) as usize);
            }
        }

        // copy lower 256 to upper 256.
        ret.0.copy_within(..256, 256);

        ret
    }
}
