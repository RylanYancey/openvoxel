#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd, Hash)]
pub enum ChunkState {
    /// The chunk is in the process of being generated.
    Generating = 0,

    /// The chunk's data has not been read from disk yet.
    Unloaded = 1,

    /// The chunk is loaded and ready to be sent to clients.
    Loaded = 2,
}

impl From<u16> for ChunkState {
    fn from(value: u16) -> Self {
        match value {
            0 => Self::Generating,
            1 => Self::Unloaded,
            2 => Self::Loaded,
            // make this not panic at some point
            _ => panic!("Invalid chunk state: {value}"),
        }
    }
}
