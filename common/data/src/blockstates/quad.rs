use bevy::prelude::*;
use bytemuck::{Pod, Zeroable};
use math::axis::{Axis, AxisArray};

pub const FULL_BLOCK: AxisArray<Quad> = AxisArray::new([
    Quad::new(POS_X, 0),
    Quad::new(NEG_X, 0),
    Quad::new(POS_Y, 0),
    Quad::new(NEG_Y, 0),
    Quad::new(POS_Z, 0),
    Quad::new(NEG_Z, 0),
]);

#[derive(Copy, Clone, Eq, PartialEq, Pod, Zeroable, Debug)]
#[repr(C, align(8))]
pub struct Vertex {
    pub pos: [i16; 3],
    pub texture: i16,
}

impl Vertex {
    pub const fn new(pos: [i16; 3], texture: i16) -> Self {
        Self { pos, texture }
    }

    pub const fn offset(self, offs: [i16; 3]) -> Self {
        Self {
            pos: [
                self.pos[0] + offs[0],
                self.pos[1] + offs[1],
                self.pos[2] + offs[2],
            ],
            texture: self.texture,
        }
    }

    pub const fn offset_with_texture(self, offs: [i16; 3], texture: i16) -> Self {
        Self {
            pos: [
                self.pos[0] + offs[0],
                self.pos[1] + offs[1],
                self.pos[2] + offs[2],
            ],
            texture,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Pod, Zeroable)]
#[repr(C, align(32))]
pub struct Quad(pub [Vertex; 4]);

impl Quad {
    pub const fn new(pos: [[i16; 3]; 4], texture: i16) -> Self {
        Self([
            Vertex::new(pos[0], texture),
            Vertex::new(pos[1], texture),
            Vertex::new(pos[2], texture),
            Vertex::new(pos[3], texture),
        ])
    }

    pub const fn full(axis: Axis, texture: i16) -> Self {
        match axis {
            Axis::PosX => Self::new(POS_X, texture),
            Axis::NegX => Self::new(NEG_X, texture),
            Axis::PosY => Self::new(POS_Y, texture),
            Axis::NegY => Self::new(NEG_Y, texture),
            Axis::PosZ => Self::new(POS_Z, texture),
            Axis::NegZ => Self::new(NEG_Z, texture),
        }
    }

    pub const fn offset(self, offs: [i16; 3]) -> Self {
        Self([
            self.0[0].offset(offs),
            self.0[1].offset(offs),
            self.0[2].offset(offs),
            self.0[3].offset(offs),
        ])
    }

    pub const fn offset_with_texture(self, offs: [i16; 3], texture: i16) -> Self {
        Self([
            self.0[0].offset_with_texture(offs, texture),
            self.0[1].offset_with_texture(offs, texture),
            self.0[2].offset_with_texture(offs, texture),
            self.0[3].offset_with_texture(offs, texture),
        ])
    }
}

const POS_X: [[i16; 3]; 4] = [[16, 16, 0], [16, 16, 16], [16, 0, 16], [16, 0, 0]];
const NEG_X: [[i16; 3]; 4] = [[0, 16, 16], [0, 16, 0], [0, 0, 0], [0, 0, 16]];
const POS_Y: [[i16; 3]; 4] = [[0, 16, 16], [16, 16, 16], [16, 16, 0], [0, 16, 0]];
const NEG_Y: [[i16; 3]; 4] = [[16, 0, 16], [0, 0, 16], [0, 0, 0], [16, 0, 0]];
const NEG_Z: [[i16; 3]; 4] = [[0, 16, 0], [16, 16, 0], [16, 0, 0], [0, 0, 0]];
const POS_Z: [[i16; 3]; 4] = [[16, 16, 16], [0, 16, 16], [0, 0, 16], [16, 0, 16]];

/// The direction a quad is facing.
#[derive(Copy, Clone, Debug)]
pub enum Normal {
    /// The face is axis-aligned.
    Aligned(Axis),

    /// The face is not axis-aligned.
    Unaligned([i8; 3]),
}

/// ASSUMES the vector is already normalized.
impl From<Vec3> for Normal {
    fn from(value: Vec3) -> Self {
        match value {
            Vec3::X => Self::Aligned(Axis::PosX),
            Vec3::NEG_X => Self::Aligned(Axis::NegX),
            Vec3::Y => Self::Aligned(Axis::PosY),
            Vec3::NEG_Y => Self::Aligned(Axis::NegY),
            Vec3::Z => Self::Aligned(Axis::PosZ),
            Vec3::NEG_Z => Self::Aligned(Axis::NegZ),
            _ => Self::Unaligned([
                (value.x * 127.0) as i8,
                (value.y * 127.0) as i8,
                (value.z * 127.0) as i8,
            ]),
        }
    }
}

impl Into<[i8; 4]> for Normal {
    fn into(self) -> [i8; 4] {
        match self {
            Self::Unaligned([x, y, z]) => [x, y, z, 6],
            Self::Aligned(axis) => {
                let [x, y, z] = Axis::AS_I8VEC3[axis as usize];
                [x, y, z, axis as i8]
            }
        }
    }
}

impl Normal {
    /// Returns "None" if the vector is not normalized.
    pub fn from_vec3(v: Vec3) -> Option<Self> {
        v.is_normalized().then(|| Self::from(v))
    }

    /// Convert to [i8; 4]
    pub fn to_array(self) -> [i8; 4] {
        self.into()
    }
}

const NORMALS: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
];
