use std::ops::Range;

pub mod variant;

pub struct Block {
    /// Range of BlockState indices.
    pub states: Range<usize>,
}
