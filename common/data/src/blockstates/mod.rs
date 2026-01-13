use coverage::{Coverage, Coverages};
use math::axis::AxisArray;
use quad::{Normal, Quad};

pub mod coverage;
pub mod quad;

pub struct BlockState {
    pub coverages: Coverages,
    pub transparency: Transparency,
    pub model: ModelData,

    /// Identifies the variant.
    pub bits: u32,
}

pub enum ModelData {
    Empty,

    /// The block fully occupies a 1x1x1 space,
    /// and does not have any special connectivity etc.
    Full {
        /// Texture used on each side.
        textures: AxisArray<u16>,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum Transparency {
    /// Alpha values are all 1.0.
    Opaque = 0,

    /// Has alpha values in the range (0.0,1.0)
    Blend = 1,

    /// Alpha values are all either 1.0 or 0.0.
    Mask = 2,
}

pub struct Element {}
