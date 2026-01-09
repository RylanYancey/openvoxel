use super::area::*;
use bevy::prelude::*;

/// A 3d Volume.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub struct IVolume {
    /// Inclusive Minimum.
    pub min: IVec3,

    /// Exclusive Maximum
    pub max: IVec3,
}

impl IVolume {
    pub const fn new(min: IVec3, max: IVec3) -> Self {
        Self { min, max }
    }

    pub const fn xz(&self) -> IArea {
        IArea {
            min: ivec2(self.min.x, self.min.z),
            max: ivec2(self.max.x, self.max.z),
        }
    }

    pub const fn xy(&self) -> IArea {
        IArea {
            min: ivec2(self.min.x, self.min.y),
            max: ivec2(self.max.x, self.max.y),
        }
    }
}

/// Y-major
#[derive(Clone)]
pub struct IVolumeIter {
    volume: IVolume,
    curr: IVec3,
    stride: i32,
}

impl Iterator for IVolumeIter {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr.y >= self.volume.max.y {
            return None;
        }

        let result = self.curr;

        self.curr.x += self.stride;
        if self.curr.x >= self.volume.max.x {
            self.curr.x = self.volume.min.x;
            self.curr.z += self.stride;
            if self.curr.z >= self.volume.max.z {
                self.curr.z = self.volume.min.z;
                self.curr.y += self.stride;
            }
        }

        Some(result)
    }
}
