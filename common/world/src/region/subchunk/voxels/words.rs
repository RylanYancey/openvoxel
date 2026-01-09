use std::alloc::{Allocator, Layout};
use std::ptr::NonNull;

use super::{BPI_ZERO, Bpi};
use crate::region::alloc::AllocationKind;

pub(super) struct Words {
    words: NonNull<usize>,
    ptr_kind: AllocationKind,
    bpi_mask: usize,
    ipw_mod: usize,
    bpi_mul: u8,
    ipw_div: u8,
}

impl Words {
    #[allow(static_mut_refs)]
    #[inline]
    pub const fn empty() -> Self {
        Self {
            words: unsafe { NonNull::new_unchecked(&BPI_ZERO as *const _ as *mut usize) },
            ptr_kind: AllocationKind::Borrowed,
            bpi_mask: 0,
            ipw_mod: 0,
            bpi_mul: 0,
            ipw_div: 14,
        }
    }

    /// !!! DOES NOT DEALLOCATE SELF !!!
    #[inline]
    pub unsafe fn assign_borrowed_ptr_unchecked(&mut self, ptr: NonNull<usize>, bpi: Bpi) {
        *self = Self {
            words: ptr,
            ptr_kind: AllocationKind::Borrowed,
            bpi_mask: bpi.bpi_mask,
            ipw_mod: bpi.ipw_mod,
            bpi_mul: bpi.bpi_mul,
            ipw_div: bpi.ipw_div,
        };
    }

    #[cfg(test)]
    fn from_bpi<A: Allocator>(alloc: &A, bpi: Bpi) -> Self {
        if bpi.bpi_mask == 0 {
            Self::empty()
        } else {
            let layout = Layout::array::<usize>(bpi.words_len()).unwrap();
            let ptr = alloc
                .allocate_zeroed(layout)
                .unwrap()
                .as_non_null_ptr()
                .cast::<usize>();
            Self {
                words: ptr,
                ptr_kind: AllocationKind::Owned,
                bpi_mask: bpi.bpi_mask,
                ipw_mod: bpi.ipw_mod,
                bpi_mul: bpi.bpi_mul,
                ipw_div: bpi.ipw_div,
            }
        }
    }

    pub const fn bpi(&self) -> u8 {
        self.bpi_mask.count_ones() as u8
    }

    /// Get the number of words in the buffer.
    #[inline(always)]
    pub const fn num_words(&self) -> usize {
        32768 >> self.ipw_div
    }

    /// Get the words data as native endian bytes.
    /// In the case of BPI=0, this will be 8 zeroes.
    #[inline(always)]
    pub fn as_ne_bytes(&self) -> &[u8] {
        let mut num_bytes = (usize::BITS as usize / 8) * self.num_words();
        num_bytes = std::hint::select_unpredictable(self.is_bpi0(), 0, num_bytes);
        unsafe { std::slice::from_raw_parts(self.words.cast::<u8>().as_ptr(), num_bytes) }
    }

    /// Get the palette index of the voxel at i.
    #[inline(always)]
    pub const unsafe fn get(&self, i: usize) -> usize {
        debug_assert!(i < 32768);
        unsafe {
            // words[i / ipw] (index of containing word)
            let word = *self.words.add(i >> self.ipw_div).as_ref();
            // (i % ipw) * bpi (offset relative to start of word)
            let offs = (i & self.ipw_mod) << self.bpi_mul;
            // isolate target bits
            (word >> offs) & self.bpi_mask
        }
    }

    /// Assign the palette index of the voxel at i.
    #[inline(always)]
    pub unsafe fn set(&mut self, i: usize, p: usize) {
        debug_assert!(i < 32768);
        unsafe {
            // words[i / ipw] (index of containing word)
            let word = self.words.add(i >> self.ipw_div).as_mut();
            // (i % ipw) * bpi (offset relative to start of word)
            let offs = (i & self.ipw_mod) << self.bpi_mul;
            // set target bits to 0
            let clear = *word & !(self.bpi_mask << offs);
            // assign palette index to target bits
            *word = clear | (p << offs);
        }
    }

    /// Assign the palette index of the voxel at i
    /// and return the previous value.
    #[inline(always)]
    pub unsafe fn replace(&mut self, i: usize, p: usize) -> usize {
        debug_assert!(i < 32768);
        unsafe {
            // words[i / ipw)] (index of containing word)
            let word = self.words.add(i >> self.ipw_div).as_mut();
            // (i % ipw) * bpi (offset relative to start of word)
            let offs = (i & self.ipw_mod) << self.bpi_mul;
            // isolate relevant bits
            let old = (*word >> offs) & self.bpi_mask;
            // assign new palette index to target bits
            *word ^= (old ^ p) << offs;
            // return previous palette index.
            old
        }
    }

    pub unsafe fn grow_bpi0_to_bpi4<A: Allocator>(&mut self, alloc: &A) {
        debug_assert_eq!(
            self.bpi_mask, 0x0,
            "in order to fault to bpi4, bpi_mask must be 0."
        );
        let layout = Layout::array::<usize>(Bpi::BPI4.words_len()).unwrap();
        let ptr = alloc
            .allocate_zeroed(layout)
            .unwrap()
            .as_non_null_ptr()
            .cast::<usize>();
        self.words = ptr;
        self.set_bpi(Bpi::BPI4);
        self.ptr_kind = AllocationKind::Owned;
    }

    pub unsafe fn grow_bpi4_to_bpi8<A: Allocator>(&mut self, alloc: &A) {
        debug_assert_eq!(self.bpi_mask, 0xF);
        let (old_bpi, new_bpi) = (Bpi::BPI4, Bpi::BPI8);
        let (old_words, new_words) = unsafe { self.realloc(alloc, old_bpi, new_bpi) };
        // expand from BPI=4 to BPI=8
        for i in (0..old_bpi.words_len()).rev() {
            let k = i << 1;
            unsafe {
                let (lo, hi) = Self::expand_bpi::<4>(*old_words.add(i).as_ptr());
                new_words.add(k).write(lo);
                new_words.add(k + 1).write(hi);
            }
        }

        self.words = new_words;
        self.set_bpi(new_bpi);
        self.ptr_kind = AllocationKind::Owned;
    }

    pub unsafe fn grow_bpi8_to_bpi16<A: Allocator>(&mut self, alloc: &A) {
        debug_assert_eq!(self.bpi_mask, 0xFF);
        let (old_bpi, new_bpi) = (Bpi::BPI8, Bpi::BPI16);
        let (old_words, new_words) = unsafe { self.realloc(alloc, old_bpi, new_bpi) };
        for i in (0..old_bpi.words_len()).rev() {
            let k = i << 1;
            unsafe {
                let (lo, hi) = Self::expand_bpi::<8>(*old_words.add(i).as_ptr());
                new_words.add(k).write(lo);
                new_words.add(k + 1).write(hi);
            }
        }

        self.words = new_words;
        self.set_bpi(new_bpi);
        self.ptr_kind = AllocationKind::Owned;
    }

    unsafe fn realloc<A: Allocator>(
        &mut self,
        alloc: &A,
        old_bpi: Bpi,
        new_bpi: Bpi,
    ) -> (NonNull<usize>, NonNull<usize>) {
        let new_layout = Layout::array::<usize>(new_bpi.words_len()).unwrap();
        match self.ptr_kind {
            AllocationKind::Borrowed => {
                let ptr = alloc
                    .allocate(new_layout)
                    .unwrap()
                    .as_non_null_ptr()
                    .cast::<usize>();
                (self.words, ptr)
            }
            AllocationKind::Owned => {
                let old_layout = Layout::array::<usize>(old_bpi.words_len()).unwrap();
                let ptr = unsafe {
                    alloc
                        .grow(self.words.cast::<u8>(), old_layout, new_layout)
                        .unwrap()
                        .as_non_null_ptr()
                        .cast::<usize>()
                };
                (ptr, ptr)
            }
        }
    }

    /// Expands the bpi of one word from OLD to OLD*2
    /// Only intended to be used with OLD=4 and OLD=8. Anything else is invalid.
    /// Returns the (lower, upper) value.
    #[inline(always)]
    fn expand_bpi<const OLD: usize>(word: usize) -> (usize, usize) {
        const HALF: usize = usize::BITS as usize / 2;

        // Extract the lower/upper 32 bits
        let mut lower = word & const { (1 << HALF) - 1 };
        let mut upper = word >> HALF;

        // lower/upper output variables
        let (mut r1, mut r2) = (0, 0);

        // sliding window for selecting only relevant bits from lower/upper
        let mut mask = const { (1 << OLD) - 1 };

        // execute expansion from old to new into r1/r2
        for _ in 0..const { usize::BITS as usize / (OLD * 2) } {
            r1 |= lower & mask;
            r2 |= upper & mask;
            lower <<= OLD;
            upper <<= OLD;
            mask <<= const { OLD * 2 };
        }

        (r1, r2)
    }

    #[inline(always)]
    fn set_bpi(&mut self, bpi: Bpi) {
        self.bpi_mask = bpi.bpi_mask;
        self.ipw_mod = bpi.ipw_mod;
        self.ipw_div = bpi.ipw_div;
        self.bpi_mul = bpi.bpi_mul;
    }

    /// Whether the buffer is all zero (air voxel)
    #[inline(always)]
    pub fn is_bpi0(&self) -> bool {
        self.bpi_mask == 0x0
    }

    /// De-allocate the words pointer if it is owned.
    /// Words doesn't impl Drop, so this will need to be called
    /// before it is dropped if the allocation kind is owned.
    pub fn dealloc_if_owned<A: Allocator>(&mut self, alloc: A) {
        if let AllocationKind::Owned = self.ptr_kind {
            // invalid state if ptr_kind is owned and bpi is 0.
            debug_assert!(!self.is_bpi0());

            let layout = Layout::array::<usize>(self.num_words()).unwrap();
            unsafe {
                alloc.deallocate(self.words.cast::<u8>(), layout);
            }

            self.ptr_kind = AllocationKind::Borrowed;
            *self = Self::empty();
        }
    }
}

#[cfg(debug_assertions)]
impl Drop for Words {
    fn drop(&mut self) {
        // Verify that the pointer was dropped if it was owned.
        debug_assert_eq!(self.ptr_kind, AllocationKind::Borrowed)
    }
}

#[cfg(test)]
mod tests {
    use std::alloc::Global;

    use super::{Bpi, Words};

    fn test_at_index(words: &mut Words, index: usize, prev: usize, new: usize) {
        unsafe {
            assert_eq!(words.get(index), prev);
            assert_eq!(words.replace(index, new), prev);
            assert_eq!(words.get(index), new);
        }
    }

    #[test]
    fn bpi0() {
        unsafe {
            let mut words = Words::empty();
            assert_eq!(words.get(771), 0);
            words.set(771, 0);
            assert_eq!(words.replace(771, 0), 0);
            assert_eq!(words.get(771), 0);
        }
    }

    #[test]
    fn bpi4() {
        let mut words = Words::from_bpi(&Global, Bpi::BPI4);
        test_at_index(&mut words, 771, 0, 4);
        test_at_index(&mut words, 27819, 0, 7);
        test_at_index(&mut words, 994, 0, 2);
        test_at_index(&mut words, 0, 0, 4);
        test_at_index(&mut words, 32767, 0, 1);
        test_at_index(&mut words, 994, 2, 11);
        test_at_index(&mut words, 994, 11, 4);
        test_at_index(&mut words, 27819, 7, 15);
        test_at_index(&mut words, 771, 4, 14);
        test_at_index(&mut words, 771, 14, 9);
        words.dealloc_if_owned(&Global);
    }

    #[test]
    fn bpi8() {
        let mut words = Words::from_bpi(&Global, Bpi::BPI8);
        test_at_index(&mut words, 771, 0, 64);
        test_at_index(&mut words, 27819, 0, 99);
        test_at_index(&mut words, 994, 0, 81);
        test_at_index(&mut words, 0, 0, 71);
        test_at_index(&mut words, 32767, 0, 1);
        test_at_index(&mut words, 994, 81, 199);
        test_at_index(&mut words, 994, 199, 251);
        test_at_index(&mut words, 27819, 99, 0);
        test_at_index(&mut words, 27819, 0, 127);
        test_at_index(&mut words, 771, 64, 231);
        test_at_index(&mut words, 771, 231, 35);
        words.dealloc_if_owned(&Global);
    }

    #[test]
    fn bpi16() {
        let mut words = Words::from_bpi(&Global, Bpi::BPI16);
        test_at_index(&mut words, 771, 0, 640);
        test_at_index(&mut words, 27819, 0, 990);
        test_at_index(&mut words, 994, 0, 810);
        test_at_index(&mut words, 0, 0, 710);
        test_at_index(&mut words, 32767, 0, 10);
        test_at_index(&mut words, 994, 810, 1990);
        test_at_index(&mut words, 994, 1990, 2510);
        test_at_index(&mut words, 27819, 990, 0);
        test_at_index(&mut words, 27819, 0, 1270);
        test_at_index(&mut words, 771, 640, 2310);
        test_at_index(&mut words, 771, 2310, 350);
        words.dealloc_if_owned(&Global);
    }

    #[test]
    fn bpi_progressive() {
        let mut words = Words::empty();
        // test at BPI=0.
        test_at_index(&mut words, 771, 0, 0);

        // Grow to BPI=4 and test.
        unsafe { words.grow_bpi0_to_bpi4(&Global) }
        test_at_index(&mut words, 771, 0, 15);
        test_at_index(&mut words, 27819, 0, 12);
        test_at_index(&mut words, 995, 0, 4);
        test_at_index(&mut words, 0, 0, 2);
        test_at_index(&mut words, 32767, 0, 9);

        // Grow to BPI=8 and test.
        unsafe { words.grow_bpi4_to_bpi8(&Global) }
        test_at_index(&mut words, 771, 15, 251);
        test_at_index(&mut words, 27819, 12, 127);
        test_at_index(&mut words, 995, 4, 49);
        test_at_index(&mut words, 0, 2, 77);
        test_at_index(&mut words, 32767, 9, 201);

        // Grow to BPI=16 and test.
        unsafe { words.grow_bpi8_to_bpi16(&Global) }
        test_at_index(&mut words, 771, 251, 994);
        test_at_index(&mut words, 27819, 127, 65535);
        test_at_index(&mut words, 995, 49, 9411);
        test_at_index(&mut words, 0, 77, 23441);
        test_at_index(&mut words, 32767, 201, 1);
        test_at_index(&mut words, 771, 994, 61131);
        test_at_index(&mut words, 27819, 65535, 9);
        test_at_index(&mut words, 995, 9411, 4191);
        test_at_index(&mut words, 0, 23441, 1022);
        test_at_index(&mut words, 32767, 1, 90);

        words.dealloc_if_owned(&Global);
    }
}
