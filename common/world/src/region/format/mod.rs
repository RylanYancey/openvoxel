use std::alloc::Global;

use crate::{
    World,
    region::{RegionAlloc, alloc::init_region_alloc},
};
use bevy::prelude::*;
use bytemuck::{Pod, Zeroable};
use bytes::Bytes;
use protocol::bytes::TryGetError;
use zip::{UnzipError, UnzippedSpan};

pub use v1::SubchunkHeader;

pub mod v1;

#[derive(Clone, Deref)]
pub struct ZippedChunk(pub Bytes);

impl Into<Bytes> for ZippedChunk {
    fn into(self) -> Bytes {
        self.0
    }
}

pub struct UnzippedChunk(pub UnzippedSpan<Global>);

impl UnzippedChunk {
    pub fn unzip(data: &[u8]) -> Result<Self, UnzipError> {
        Ok(Self(UnzippedSpan::unzip(data, &init_region_alloc())?))
    }

    pub fn header(&self) -> Option<&ChunkHeader> {
        self.0.header::<ChunkHeader>()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C)]
pub enum ChunkFormat {
    /// The format belongs to a future version.
    /// The game will still try to parse it, but
    /// may not be successful.
    Unknown = 0,

    V1 = 1,
}

impl ChunkFormat {
    pub const LATEST: Self = Self::V1;

    pub fn from_u16(v: u16) -> Self {
        match v {
            1 => Self::V1,
            _ => Self::Unknown,
        }
    }
}

/// Chunk headers that always exist, regardless of format version.
/// !! SIZE. MUST. BE. A. MULTIPLE. OF. 8. !!
#[derive(Copy, Clone, Pod, Zeroable, Default)]
#[repr(C, align(8))]
pub struct ChunkHeader {
    /// Origin of the chunk in world-space.
    pub origin: IVec3,

    /// Distance between origin.y and extent.y
    pub height: i32,

    /// ChunkState as u16
    pub state: u16,

    /// Format used when the chunk was encoded.
    pub format: u16,

    /// Number of non-empty subchunks present.
    pub length: u16,

    pub _unused: u16,

    /// The version number of the chunk.
    pub revision: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum ChunkReadError {
    /// Expected more information to be in the chunk span, but it did not have it.
    #[error("[W889] Unexpected End Of Input: {0}")]
    UnexpectedEoi(#[from] TryGetError),

    /// Chunks may try and load regions when reading. If it is disabled,
    /// this error will be thrown if the region has not been loaded.
    #[error("[W891] Region not found, and automatic region loading not enabled: {0}")]
    RegionNotExists(IVec2),

    /// If the `format` field of the header is `ChunkFormat::Unknown`.
    #[error("[W892] Attempted to load a chunk with an unknown (possibly future?) format.")]
    UnsupportedFormat(u32),

    /// Unzipped chunk data must pad the palette data to an 8-byte alignment, but this
    /// padding must also be less than 8.
    #[error(
        "[W983] Padding used to align palette to 8-byte boundary must be less than 8, found: {0}"
    )]
    InvalidPadding(usize),

    /// Unzipped chunk span must ensure all word buffers are aligned to an 8-byte boundary.
    #[error("[W984] Words buffer of subchunk at '{0}' was not aligned to an 8-byte boundary.")]
    WordsNotAligned(IVec3),

    #[error("[W989] Palette buffer of subchunk at '{0}' was not aligned to a 2-byte boundary.")]
    PaletteNotAligned(IVec3),

    /// The length of the palette buffer must not be 0 or 1. Empty subchunks must not be written.
    #[error("[W985] Palette length of '{0}' is invalid, must be in the range [0,65536)")]
    InvalidPaletteLength(usize),

    /// Occurs if the BPI and palette length don't match.
    #[error(
        "[W986] BPI of '{bpi}' is invalid or not valid for palette len '{palette_len}', must be one of: 4|8|16"
    )]
    InvalidBpi { bpi: u8, palette_len: usize },

    /// Subchunk existed more than once while reading from a chunk span.
    #[error("[W987] Duplicate subchunk with origin '{0}' found while reading chunk.")]
    DuplicateSubchunk(IVec3),

    /// Happens whenever a subchunk's provided y is not a multiple of 32.
    #[error("[W988] A subchunk header contained an origin that is not a multiple of 32: {0}")]
    InvalidYOrigin(i32),
}

/// Read a chunk's data from an unzipped span into the World.
/// If allow_region_load is enabled, then a region may be inserted if it does not exist.
pub fn read_chunk_from_span(
    world: &mut World,
    span: UnzippedSpan<RegionAlloc>,
    allow_region_load: bool,
) -> Result<ChunkReadSuccess, ChunkReadError> {
    let mut reader = span.reader();

    // read chunk headers
    let header = reader.take_as::<ChunkHeader>()?;

    // get the region that contains the chunk
    let region = if allow_region_load {
        world.get_or_insert_region(header.origin.xz())
    } else {
        let Some(region) = world.get_region_mut(header.origin.xz()) else {
            return Err(ChunkReadError::RegionNotExists(header.origin.xz()));
        };
        region
    };

    // Read subchunk data from the span.
    match ChunkFormat::from_u16(header.format) {
        ChunkFormat::Unknown => {
            return Err(ChunkReadError::UnsupportedFormat(header.format as u32));
        }
        ChunkFormat::V1 => v1::read_chunk_from_span_v1(span, reader, region, &header)?,
    }

    Ok(ChunkReadSuccess {
        origin: header.origin,
        format: ChunkFormat::from_u16(header.format),
    })
}

pub struct ChunkReadSuccess {
    pub origin: IVec3,
    pub format: ChunkFormat,
}
