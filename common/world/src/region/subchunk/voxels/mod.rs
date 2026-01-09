use std::ptr::NonNull;

use std::alloc::{Allocator, Global};

mod palette;
use palette::Palette;
mod words;
use words::Words;

/// Palette/Cache/Words pointer points to this when BPI=0, or
/// when the cache is uninit. The lower 32 bits are 0, the upper are 1.
static mut BPI_ZERO: usize = 0xFFFFFFFF_00000000;

pub struct Voxels<A: Allocator = Global> {
    palette: Palette,
    words: Words,
    alloc: A,
}

impl<A: Allocator> Voxels<A> {
    pub const fn empty(alloc: A) -> Self {
        Self {
            palette: Palette::empty(),
            words: Words::empty(),
            alloc,
        }
    }

    /// Must not be used with BPI=0.
    /// BPI must be 4,8, or 16.
    /// palette_len must be greater than 1.
    pub unsafe fn assign_borrowed_ptrs_unchecked(
        &mut self,
        palette_len: u16,
        words_size: usize,
        bpi: u8,
        palette: NonNull<u16>,
        words: NonNull<usize>,
    ) {
        let bpi = match bpi {
            4 => Bpi::BPI4,
            8 => Bpi::BPI8,
            16 => Bpi::BPI16,
            _ => panic!("[W221] Bpi of '{bpi}' is invalid."),
        };

        // double check just to be safe.
        let words_len = words_size / (usize::BITS as usize / 8);
        if bpi.words_len() != words_len {
            panic!(
                "[W224] Invalid words len '{}' for bpi '{}'.",
                words_len,
                bpi.bpi_mask.count_ones()
            );
        }

        if palette_len < 2 {
            panic!(
                "[W223] Cannot construct Voxels from palette len of less than 2, found: '{palette_len}'."
            );
        }

        unsafe {
            self.palette.dealloc_if_owned(&self.alloc);
            self.palette
                .assign_borrowed_ptr_unchecked(palette, palette_len);
            self.words.dealloc_if_owned(&self.alloc);
            self.words.assign_borrowed_ptr_unchecked(words, bpi);
        }
    }

    pub const fn palette_len(&self) -> usize {
        self.palette.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.palette.is_empty()
    }

    pub fn set_empty(&mut self) {
        unsafe {
            self.words.dealloc_if_owned(&self.alloc);
            self.palette.dealloc_if_owned(&self.alloc);
        }

        self.words = Words::empty();
        self.palette = Palette::empty();
    }

    pub const fn bpi(&self) -> u8 {
        self.words.bpi()
    }

    /// Get the palette data as native endian bytes.
    pub fn palette_as_bytes(&self) -> &[u8] {
        self.palette.as_ne_bytes()
    }

    /// Get the word data as native endian bytes.
    pub fn words_as_bytes(&self) -> &[u8] {
        self.words.as_ne_bytes()
    }

    #[inline(always)]
    pub unsafe fn get(&self, i: usize) -> u16 {
        unsafe { self.palette.get(self.words.get(i)) }
    }

    #[inline(always)]
    pub unsafe fn set(&mut self, i: usize, v: u16) {
        unsafe {
            let pidx = self.find_or_insert(v);
            self.words.set(i, pidx);
        }
    }

    #[inline(always)]
    pub unsafe fn replace(&mut self, i: usize, v: u16) -> u16 {
        unsafe {
            let pidx = self.find_or_insert(v);
            let pidx = self.words.replace(i, pidx);
            self.palette.get(pidx)
        }
    }

    #[inline(always)]
    unsafe fn find_or_insert(&mut self, v: u16) -> usize {
        unsafe {
            match self.palette.search(v) {
                Some(i) => i,
                None => self.cache_miss(v),
            }
        }
    }

    /// Called when a call to `self.palette.search()` returns None.
    /// In this case, the cache may not be initialized, so it will be
    /// initialized and searched. If that fails, or cache is already
    /// initialized, the palette will try to grow.
    ///
    /// If Err() is returned from Palette::insert(), then the palette's
    /// capacity has changed and the BPI of words may need to expand
    /// accordingly.
    #[inline(never)]
    unsafe fn cache_miss(&mut self, v: u16) -> usize {
        unsafe {
            match self.palette.insert(v, &self.alloc) {
                Ok(i) => i,
                // Palette::insert() returns Err if the palette grows,
                Err(i) => {
                    match self.palette.capacity() {
                        16 => self.words.grow_bpi0_to_bpi4(&self.alloc),
                        32 => self.words.grow_bpi4_to_bpi8(&self.alloc),
                        512 => self.words.grow_bpi8_to_bpi16(&self.alloc),
                        _ => {}
                    }
                    i
                }
            }
        }
    }
}

impl<A: Allocator> Drop for Voxels<A> {
    fn drop(&mut self) {
        unsafe {
            self.palette.dealloc_if_owned(&self.alloc);
            self.words.dealloc_if_owned(&self.alloc);
        }
    }
}

/// Variables used to extract palette indices from packed bitfields.
/// BPI (Bits-Per-Index), refers to the number of bits needed to store an index
/// into the palette. Supported BPIs are 0,4,8, and 16. BPIs must be powers of two.
///
/// The IPW (Indices-Per-Word), refers to the number of indices of BPI width that can
/// be packed into a single usize. In this way, one usize acts like an array of palette indices.
/// This struct contains the parameters to perform the packing and unpacking of those indices.
#[derive(Copy, Clone)]
struct Bpi {
    /// Mask of the first BPI bits.
    /// - If BPI=0, bpi_mask = 0x0
    /// - If BPI=4, bpi_mask = 0xF (0b1111)
    /// - If BPI=8, bpi_mask = 0xFF
    /// - If BPI=16, bpi_mask = 0xFFFF
    bpi_mask: usize,

    /// Mask used to modulo by the indices-per-word using BITAND
    /// - If usize::BITS=64
    ///     - If BPI=0, ipw_mod = 0
    ///     - If BPI=4, ipw_mod = 15
    ///     - If BPI=8, ipw_mod = 7
    ///     - If BPI=16, ipw_mod = 3
    /// - If usize::BITS = 32
    ///     - If BPI=0, ipw_mod = 0
    ///     - If BPI=4, ipw_mod = 7
    ///     - If BPI=8, ipw_mod = 3
    ///     - If BPI=16, ipw_mod = 1
    ipw_mod: usize,

    /// Factor used to divide by the indices-per-word using SHR
    /// - If usize::BITS=64
    ///     - If BPI=0, ipw_div = 15 (EXCEPTION: used to make index 0)
    ///     - If BPI=4, ipw_div = 4,
    ///     - If BPI=8, ipw_div = 3
    ///     - If BPI=16, ipw_div = 2
    /// - If usize::BITS=32
    ///     - If BPI=0, ipw_div = 15 (EXCEPTION: used to make index 0)
    ///     - If BPI=4, ipw_div = 3
    ///     - If BPI=8, ipw_div = 2
    ///     - If BPI=16, ipw_div = 1
    ipw_div: u8,

    /// Factor used to multiply by the bits-per-index using SHL
    /// - If BPI=0, bpi_mul = 0
    /// - If BPI=4, bpi_mul = 2
    /// - If BPI=8, bpi_mul = 3
    /// - If BPI=16, bpi_mul = 4
    bpi_mul: u8,
}

impl Bpi {
    const BPI4: Self = Self::new::<4>();
    const BPI8: Self = Self::new::<8>();
    const BPI16: Self = Self::new::<16>();

    /// Number of usize's in the words buffer at this BPI.
    /// 32768 divided by the ipw.
    /// - If usize::BITS = 64
    ///     - If BPI=0, words_len = 1
    ///     - If BPI=4, words_len = 2048
    ///     - If BPI=8, words_len = 4096
    ///     - If BPI=16, words_len = 8192
    /// - If usize::BITS = 32
    ///     - If BPI=0, words_len = 1
    ///     - If BPI=4, words_len = 4096
    ///     - If BPI=8, words_len = 8192
    ///     - If BPI=16, words_len = 16384
    const fn words_len(&self) -> usize {
        32768 >> self.ipw_div
    }

    const fn new<const BPI: usize>() -> Self {
        let ipw = usize::BITS as usize / BPI;
        Self {
            ipw_div: ipw.trailing_zeros() as u8,
            bpi_mul: BPI.trailing_zeros() as u8,
            ipw_mod: ipw - 1,
            bpi_mask: (1 << BPI) - 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Voxels;
    use std::alloc::Global;

    #[test]
    fn bpi0() {
        let mut palette = Voxels::empty(&Global);
        unsafe {
            for i in 0..32768 {
                assert_eq!(palette.get(i), 0);
                assert_eq!(palette.replace(i, 0), 0);
            }

            assert_eq!(palette.palette_len(), 1);
        }
    }

    #[test]
    fn bpi4() {
        let mut palette = Voxels::empty(&Global);
        unsafe {
            for i in 0..32768 {
                assert_eq!(palette.replace(i, (i & 15) as u16), 0);
            }

            for i in 0..32768 {
                assert_eq!(palette.get(i), (i & 15) as u16);
            }
        }
    }

    #[test]
    fn bpi8() {
        let mut palette = Voxels::empty(&Global);
        unsafe {
            for i in 0..32768 {
                assert_eq!(palette.replace(i, (i & 255) as u16), 0);
            }

            for i in 0..32768 {
                assert_eq!(palette.get(i), (i & 255) as u16);
            }
        }
    }

    #[test]
    fn bpi16() {
        let mut palette = Voxels::empty(&Global);
        unsafe {
            for i in 0..32768 {
                assert_eq!(palette.replace(i, (i & 4095) as u16), 0);
            }

            for i in 0..32768 {
                assert_eq!(palette.get(i), (i & 4095) as u16);
            }
        }
    }
}
