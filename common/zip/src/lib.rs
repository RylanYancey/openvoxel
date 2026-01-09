#![feature(allocator_api)]
#![feature(slice_ptr_get)]

use std::{
    alloc::{Allocator, Layout},
    io::{self, Write},
    ptr::NonNull,
};

use bytemuck::Pod;
use bytes::{Buf, TryGetError};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u32)]
pub enum Algorithm {
    #[default]
    Zstd = 0,
    Lz4 = 1,
}

impl Algorithm {
    pub fn from_u32(u: u32) -> Result<Self, UnzipError> {
        Ok(match u {
            0 => Algorithm::Zstd,
            1 => Algorithm::Lz4,
            _ => return Err(UnzipError::UnknownAlgorithm(u)),
        })
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum ZipLevel {
    Low,
    #[default]
    Medium,
    High,
    Ultra,
}

/// A span that was created by decompressing a region of memory,
/// which can now be shared in multiple places.
/// Spans are always aligned to a multiple of 8.
pub struct UnzippedSpan<A: Allocator> {
    span: NonNull<u8>,
    size: usize,
    alloc: A,
}

impl<A: Allocator + Clone> UnzippedSpan<A> {
    pub fn unzip(src: &[u8], alloc: &A) -> Result<Self, UnzipError> {
        unzip_to_span(src, alloc)
    }

    pub fn reader(&self) -> SpanReader {
        SpanReader {
            ptr: self.span,
            rem: self.size,
        }
    }
}

impl<A: Allocator> Drop for UnzippedSpan<A> {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align(self.size, 8).unwrap();
            self.alloc.deallocate(self.span, layout);
        }
    }
}

/// A reader for getting bytes from a shared, unzipped span.
pub struct SpanReader {
    ptr: NonNull<u8>,
    rem: usize,
}

impl SpanReader {
    pub const fn as_slice(&self) -> &'static [u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.rem) }
    }

    pub fn take_as<T: Pod + Copy>(&mut self) -> Result<T, TryGetError> {
        Ok(*bytemuck::from_bytes(
            self.take_slice(std::mem::size_of::<T>())?,
        ))
    }

    /// Take `count` bytes as a slice.
    pub const fn take_slice(&mut self, count: usize) -> Result<&'static [u8], TryGetError> {
        match self.take(count) {
            Ok(ptr) => Ok(unsafe { std::slice::from_raw_parts(ptr.as_ptr(), count) }),
            Err(e) => Err(e),
        }
    }

    /// Take `count` bytes as a pointer.
    pub const fn take(&mut self, count: usize) -> Result<NonNull<u8>, TryGetError> {
        if self.rem < count {
            return Err(TryGetError {
                requested: count,
                available: self.rem,
            });
        }
        let ptr = self.ptr;
        self.rem -= count;
        self.ptr = unsafe { self.ptr.add(count) };
        Ok(ptr)
    }

    pub const fn advance(&mut self, count: usize) -> Result<(), TryGetError> {
        match self.take(count) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum UnzipError {
    #[error("[Z111] Unexpected End Of Input while unzipping.")]
    UnexpectedEoi(#[from] TryGetError),

    #[error(
        "[Z112] Attempted to unzip data, but its unzipped size was too large. (found: {0}, expected less than 5_000_000)"
    )]
    TooBig(usize),

    #[error("[Z113] Attempted to unzip data, but it had an unknown algorithm id: {0}.")]
    UnknownAlgorithm(u32),

    #[error("[Z114] Attempted to unzip data, but it failed while unzipping: {0}")]
    AlgorithmFailed(#[from] io::Error),

    #[error("[Z115] Attempted to unzip data with Lz4, but it failed: {0}")]
    Lz4Failed(#[from] lz4_flex::block::DecompressError),
}

/// Decompress a zipped span to a pointer, which can then be shared.
/// The unzipped size (u32) and algorithm (u32) should be present
/// in the first 8 bytes of the src slice.
pub fn unzip_to_span<A: Allocator + Clone>(
    mut src: &[u8],
    alloc: &A,
) -> Result<UnzippedSpan<A>, UnzipError> {
    const ALIGN: usize = 8;

    // get unzipped size of the data.
    let unzipped_size = src.try_get_u32_le()? as usize;
    if unzipped_size > 5_000_000 {
        return Err(UnzipError::TooBig(unzipped_size));
    }

    // get algorithm used to compress the data.
    let algorithm = Algorithm::from_u32(src.try_get_u32_le()?)?;

    // allocate pointer
    let layout = Layout::from_size_align(unzipped_size, ALIGN).unwrap();
    let mut ptr = alloc.allocate(layout).unwrap().as_non_null_ptr();
    let slice = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), unzipped_size) };

    // perform decompression
    match match algorithm {
        Algorithm::Zstd => zstd::bulk::decompress_to_buffer(src, slice).map_err(|e| e.into()),
        Algorithm::Lz4 => lz4_flex::decompress_into(src, slice).map_err(|e| e.into()),
    } {
        Ok(cnt) => {
            if cnt < unzipped_size {
                let new_layout = Layout::from_size_align(cnt, ALIGN).unwrap();
                ptr = unsafe {
                    alloc
                        .shrink(ptr, layout, new_layout)
                        .unwrap()
                        .as_non_null_ptr()
                };
            } else if cnt > unzipped_size {
                // shouldn't happen because this error is handled by the algorithm.
                unreachable!()
            }

            Ok(UnzippedSpan {
                span: ptr,
                size: cnt,
                alloc: alloc.clone(),
            })
        }
        Err(e) => {
            // clean up data if failure occurs to avoid memory leak.
            unsafe { alloc.deallocate(ptr, layout) };
            Err(e)
        }
    }
}

pub trait Zipper {
    fn init(buf: Vec<u8>, level: ZipLevel) -> Self;
    fn put(&mut self, data: &[u8]);
    fn finish(self) -> Vec<u8>;
    fn put_as<T: Pod>(&mut self, item: &T) {
        self.put(bytemuck::bytes_of(item))
    }
}

pub struct ZstdZipper {
    encoder: zstd::Encoder<'static, Vec<u8>>,
    unzipped_size: usize,
}

impl Zipper for ZstdZipper {
    fn init(mut buf: Vec<u8>, level: ZipLevel) -> Self {
        buf.clear();
        // reserve 8 bytes for unwrapped_size (u32) and algorithm (u32)
        buf.extend(&[0, 0, 0, 0]);
        buf.extend(&(Algorithm::Zstd as u32).to_le_bytes());
        Self {
            encoder: zstd::Encoder::new(
                buf,
                match level {
                    ZipLevel::Low => 1,
                    ZipLevel::Medium => 3,
                    ZipLevel::High => 5,
                    ZipLevel::Ultra => 7,
                },
            )
            .unwrap(),
            unzipped_size: 0,
        }
    }

    #[inline]
    fn put(&mut self, data: &[u8]) {
        self.encoder.write_all(data).unwrap();
        self.unzipped_size += data.len();
    }

    fn finish(self) -> Vec<u8> {
        let mut buf = self.encoder.finish().unwrap();
        let unzipped_size = (self.unzipped_size as u32).to_le_bytes();
        buf[0..4].copy_from_slice(&unzipped_size);
        buf
    }
}

impl ZstdZipper {
    pub fn len(&self) -> usize {
        self.unzipped_size
    }
}

pub struct Lz4Zipper {
    encoder: lz4_flex::frame::FrameEncoder<Vec<u8>>,
    unzipped_size: usize,
}

impl Zipper for Lz4Zipper {
    fn init(mut buf: Vec<u8>, _: ZipLevel) -> Self {
        buf.clear();
        // reserve 8 bytes for unwrapped_size (u32) and algorithm (u32)
        buf.extend(&[0, 0, 0, 0]);
        buf.extend(&(Algorithm::Lz4 as u32).to_le_bytes());
        Self {
            encoder: lz4_flex::frame::FrameEncoder::new(buf),
            unzipped_size: 0,
        }
    }

    #[inline]
    fn put(&mut self, data: &[u8]) {
        self.unzipped_size += data.len();
        self.encoder.write_all(data).unwrap()
    }

    fn finish(self) -> Vec<u8> {
        let mut buf = self.encoder.finish().unwrap();
        let unzipped_size = (self.unzipped_size as u32).to_le_bytes();
        buf[0..4].copy_from_slice(&unzipped_size);
        buf
    }
}

#[cfg(test)]
mod tests {
    use bytemuck::{Pod, Zeroable};

    use crate::{UnzippedSpan, ZipLevel, Zipper, ZstdZipper};

    #[test]
    fn zstd() {
        let data = &[0, 1, 2, 3, 4, 5, 6, 7];

        #[derive(Copy, Clone, Pod, Zeroable, Debug, Eq, PartialEq)]
        #[repr(C)]
        struct Thing {
            a: u64,
            b: u64,
            c: u32,
            d: u32,
        }

        let thing1 = Thing {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
        };
        let thing2 = Thing {
            a: 5,
            b: 6,
            c: 7,
            d: 8,
        };

        // zip 8 bytes
        let mut zipper = ZstdZipper::init(Vec::new(), ZipLevel::default());
        zipper.put(data);
        zipper.put_as(&thing1);
        zipper.put(&[0; 8]);
        zipper.put_as(&thing2);
        let buf = zipper.finish();

        // unzip
        let span = UnzippedSpan::unzip(&buf, &std::alloc::Global).unwrap();
        let mut reader = span.reader();

        // check data slice
        assert_eq!(reader.take_slice(8).unwrap(), data);

        // check thing 1
        assert_eq!(reader.take_as::<Thing>().unwrap(), thing1);

        // advance past padding
        reader.advance(8).unwrap();

        // check thing 2
        assert_eq!(reader.take_as::<Thing>().unwrap(), thing2);
    }
}
