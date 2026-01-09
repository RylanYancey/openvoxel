use std::{alloc::Allocator, ptr::NonNull};

use bevy::math::Vec3Swizzles;
use bytemuck::{Pod, Zeroable};
use zip::{SpanReader, UnzippedSpan};

use crate::{
    Region,
    region::{
        chunk::SubchunkMask,
        format::{ChunkHeader, ChunkReadError},
    },
};

/// !! SIZE. MUST. BE. A. MULTIPLE. OF. 8. !!
#[derive(Pod, Zeroable, Copy, Clone)]
#[repr(C, align(8))]
pub struct SubchunkHeader {
    /// Y-coordinate of the subchunk within the chunk.
    pub y_origin: i32,

    /// Number of ELEMENTS in the palette.
    pub palette_len: u16,

    /// Number of BYTES to pad the palette with.
    pub padding_size: u8,

    /// The number of bits per index.
    pub bpi: u8,
}

impl SubchunkHeader {
    fn y_origin(&self) -> Result<i32, ChunkReadError> {
        if self.y_origin & 31 != 0 {
            Err(ChunkReadError::InvalidYOrigin(self.y_origin))
        } else {
            Ok(self.y_origin)
        }
    }

    fn padding(&self) -> Result<usize, ChunkReadError> {
        let padding = self.padding_size as usize;
        if padding > 7 {
            Err(ChunkReadError::InvalidPadding(padding))
        } else {
            Ok(padding)
        }
    }

    /// Size of palette, in bytes
    fn palette_size(&self) -> Result<usize, ChunkReadError> {
        if self.palette_len < 2 {
            Err(ChunkReadError::InvalidPaletteLength(
                self.palette_len as usize,
            ))
        } else {
            Ok((self.palette_len as usize) << 1)
        }
    }

    /// Number of BYTES in words
    fn words_size(&self) -> Result<usize, ChunkReadError> {
        match self.bpi {
            4 => Ok(16384),
            8 => Ok(32768),
            16 => Ok(65536),
            _ => Err(ChunkReadError::InvalidBpi {
                bpi: self.bpi,
                palette_len: self.palette_len as usize,
            }),
        }
    }
}

/// Subchunk data that hasn't been written yet.
/// This is important so we can ensure NO errors occur
/// while reading before assigning the data.
struct Intermediate {
    header: SubchunkHeader,
    words_size: usize,
    palette: NonNull<u16>,
    words: NonNull<usize>,
}

pub fn read_chunk_from_span_v1<A: Allocator + Clone>(
    span: UnzippedSpan<A>,
    mut reader: SpanReader,
    region: &mut Region<A>,
    header: &ChunkHeader,
) -> Result<(), ChunkReadError> {
    // get target chunk
    let chunk = region.get_chunk_mut(header.origin.xz()).unwrap();

    // Mask of subchunks in the chunk that are in-bounds.
    let in_bounds = chunk.mask();

    // Mask of subchunks that will be changed by the span.
    let mut changed = SubchunkMask::EMPTY;

    // Buffer of intermediate subchunk representations.
    // Used to read all data before assigning, so there won't be any pointers in the chunk
    // owned by both the previous span and the new span, in the event of an error.
    let mut intermediate = Vec::<Intermediate>::with_capacity(header.length as usize);

    // read intermediate subchunks.
    for _ in 0..header.length as usize {
        // extract header
        let header = reader.take_as::<SubchunkHeader>()?;

        // validate positioning and check for duplicates
        let y_origin = header.y_origin()?;
        let origin = chunk.origin().with_y(y_origin);

        // validate voxel ptr sizes
        let padding = header.padding()?;
        let palette_size = header.palette_size()?;
        let words_size = header.words_size()?;

        // get voxel data pointers for palette/words.
        let palette = reader.take(palette_size)?.cast::<u16>();
        reader.advance(padding)?;
        let words = reader.take(words_size)?.cast::<usize>();

        // validate palette alignment
        if !palette.is_aligned_to(2) {
            return Err(ChunkReadError::PaletteNotAligned(origin));
        }

        // validate alignment of words
        if !words.is_aligned_to(8) {
            return Err(ChunkReadError::WordsNotAligned(origin));
        }

        // Push to intermediate buffer, but only if the subchunk
        // is actually in-bounds for the chunk. We do this check here
        // because we still need to read past the subchunk in the
        // reader.
        if in_bounds.has(y_origin) {
            if !changed.set(y_origin) {
                return Err(ChunkReadError::DuplicateSubchunk(origin));
            }

            intermediate.push(Intermediate {
                header,
                words_size,
                palette,
                words,
            });
        }
    }

    // reset subchunks to air
    for subchunk in chunk.iter_mut() {
        subchunk.fill_air();
    }

    // Assign intermediate data to subchunks.
    for intermediate in intermediate.drain(..) {
        changed.clear(intermediate.header.y_origin);
        unsafe {
            chunk
                .get_subchunk_mut(intermediate.header.y_origin)
                .unwrap()
                .assign_voxel_ptrs(
                    intermediate.header.palette_len,
                    intermediate.words_size,
                    intermediate.header.bpi,
                    intermediate.palette,
                    intermediate.words,
                );
        }
    }

    assert_eq!(changed.0, 0, "mask: 0x{:x}", changed.0);

    chunk.revision = header.revision;
    chunk.span = Some(span);

    Ok(())
}
