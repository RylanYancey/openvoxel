use rand::{RngCore, SeedableRng};

/// An RNG for generating a few bits at a time.
///
/// Using 32 or 64 bit RNGs for small numbers in limited ranges is
/// inefficient because the majority of the generated bits will be
/// discarded. This RNG allows taking only a few bits at a time, so
/// the actual RNG algorithm only has to be called when there are no
/// more bits to take from the state.
///
/// Internally uses WyRand for generating the random u64s.
#[derive(Copy, Clone)]
pub struct BitRng {
    /// The current seed of the RNG.
    seed: u64,

    /// The current state of the RNG.
    state: u64,

    /// The number of bits remaining to be taken.
    remaining: u32,
}

impl BitRng {
    pub const fn new(s: u64) -> Self {
        Self {
            seed: s,
            state: 0,
            remaining: 0,
        }
    }

    pub fn from_entropy() -> Self {
        Self::from_os_rng()
    }

    /// Get a number where only the first `count` bits are random, and the rest are zeroes.
    #[inline]
    pub fn take(&mut self, count: u32) -> u64 {
        // Shifting u32 by a value greater than or equal to 64 is UB, so we check
        // for that here.
        debug_assert!(
            count < 64,
            "Number of bits to take from `BitRng` must be less than 64, found: '{count}'"
        );

        // if the state doesn't have enough bits remaining, generate next state value.
        if count > self.remaining {
            self.regenerate()
        }

        self.remaining -= count;
        let ret = self.state & ((1 << count) - 1);
        self.state >>= count;
        ret
    }

    /// Uses WyRand to generate new random bits.
    #[inline]
    fn regenerate(&mut self) {
        self.seed = self.seed.wrapping_add(0xa0761d6478bd642f);
        let t = (self.seed as u128).wrapping_mul((self.seed ^ 0xe7037ed1a0b428db) as u128);
        self.state = (t.wrapping_shr(64) ^ t) as u64;
        self.remaining = 64;
    }
}

impl SeedableRng for BitRng {
    type Seed = [u8; core::mem::size_of::<u64>()];

    #[inline]
    fn from_seed(seed: Self::Seed) -> Self {
        Self {
            seed: u64::from_le_bytes(seed),
            state: 0,
            remaining: 0,
        }
    }

    #[inline]
    fn seed_from_u64(state: u64) -> Self {
        Self {
            seed: state,
            state: 0,
            remaining: 0,
        }
    }

    #[inline]
    fn from_rng(rng: &mut impl RngCore) -> Self {
        Self::seed_from_u64(rng.next_u64())
    }
}
