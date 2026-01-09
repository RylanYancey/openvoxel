use crate::util::{IsPow2, Pow2};

use super::volume::*;
use bevy::prelude::*;

/// A 2d Area.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub struct IArea {
    /// Inclusive Minimum
    pub min: IVec2,

    /// Exclusive Maximum
    pub max: IVec2,
}

impl IArea {
    pub const fn new(min: IVec2, max: IVec2) -> Self {
        Self { min, max }
    }

    /// Create an IArea from a center point and half-extents.
    ///
    pub fn from_center_extents(center: IVec2, extents: IVec2) -> Self {
        Self {
            min: center - extents,
            max: center + (extents + 1),
        }
    }

    pub const fn from_size(size: i32) -> Self {
        Self {
            min: IVec2::splat(-size),
            max: IVec2::splat(size + 1),
        }
    }

    /// Get the portion of self and is also in other.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let min = self.min.max(other.min);
        let max = self.max.min(other.max);
        if min.x < max.x && min.y < max.y {
            Some(Self { min, max })
        } else {
            None
        }
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.intersection(other).is_some()
    }

    /// Returns the X and Y extent (size).
    pub const fn extents(&self) -> IVec2 {
        ivec2(self.max.x - self.min.x, self.max.y - self.min.y)
    }

    /// Returns X extent.
    pub const fn width(&self) -> i32 {
        self.max.x - self.min.x
    }

    /// Returns the Y extent.
    pub const fn height(&self) -> i32 {
        self.max.y - self.min.y
    }

    /// Sometimes Y is actually Z, so this is provided as a convenience.
    pub const fn depth(&self) -> i32 {
        self.height()
    }

    pub const fn contains_x(&self, x: i32) -> bool {
        x >= self.min.x && x < self.max.x
    }

    pub const fn contains_y(&self, y: i32) -> bool {
        y >= self.min.y && y < self.max.y
    }

    pub const fn contains(&self, pt: IVec2) -> bool {
        self.contains_x(pt.x) && self.contains_y(pt.y)
    }

    /// Extend to an IVolume by setting the area's Y value to
    /// the Volume's Z value.
    pub const fn extend_y(self, y: i32, height: i32) -> IVolume {
        IVolume {
            min: ivec3(self.min.x, y, self.min.y),
            max: ivec3(self.max.x, y + height, self.max.y),
        }
    }

    /// Extend to an IVolume by adding a Z coordinate.
    pub const fn extend_z(self, z: i32, depth: i32) -> IVolume {
        IVolume {
            min: ivec3(self.min.x, self.min.y, z),
            max: ivec3(self.max.x, self.max.y, z + depth),
        }
    }

    /// Get all square cells that this area overlaps.
    ///
    /// This is accomplished by rounding the min down to the previous
    /// multiple of `size`, and rounding max up.
    ///
    /// The origins of the cells are relative to the world origin.
    /// If size=512, the origin of each cell is guaranteed to be
    /// a multiple of 512.
    #[inline]
    pub fn cells(&self, size: i32) -> ICells2d {
        let rounded = self.rounded_up_to(IVec2::splat(size));
        ICells2d {
            area: rounded,
            next: rounded.min,
            back: rounded.max - 1,
            stride: size,
        }
    }

    pub const fn cells_pow2<const SIZE: i32>(&self) -> ICells2d
    where
        Pow2<SIZE>: IsPow2,
    {
        let rounded = self.rounded_up_to_pow2::<SIZE, SIZE>();
        ICells2d {
            area: rounded,
            next: rounded.min,
            back: ivec2(rounded.max.x - 1, rounded.max.y - 1),
            stride: SIZE,
        }
    }

    pub const fn round_up_to_pow2<const X: i32, const Y: i32>(&mut self)
    where
        Pow2<X>: IsPow2,
        Pow2<Y>: IsPow2,
    {
        let fx = !(X - 1);
        let fy = !(Y - 1);
        self.min.x &= fx;
        self.min.y &= fy;
        self.max.x += X - (self.max.x & fx);
        self.max.y += Y - (self.max.y & fy);
    }

    pub const fn rounded_up_to_pow2<const X: i32, const Y: i32>(mut self) -> Self
    where
        Pow2<X>: IsPow2,
        Pow2<Y>: IsPow2,
    {
        self.round_up_to_pow2::<X, Y>();
        self
    }

    /// Round the IArea's min down to a previous multiple, and
    /// round the max up to the next multiple.
    #[inline]
    pub fn round_up_to(&mut self, multiples: IVec2) {
        self.min -= self.min.rem_euclid(multiples);
        self.max += (multiples - self.max.rem_euclid(multiples)).rem_euclid(multiples);
    }

    /// Round the IArea's min down to a previous multiple, and
    /// round the max up to the next multiple.
    #[inline]
    pub fn rounded_up_to(mut self, multiples: IVec2) -> Self {
        self.round_up_to(multiples);
        self
    }

    /// Get an iterator over the points in this area.
    ///
    /// Panics if stride is less than 1. A stride of 0 or negative 1
    /// would cause the iter to go on forever.
    ///
    /// Returned iterator is X-major.
    #[inline]
    pub fn iter(&self, stride: i32) -> IAreaIter {
        assert!(
            stride > 0,
            "Expected stride of IAreaIter to be greater than 0, found: '{stride}'"
        );
        IAreaIter {
            area: *self,
            next: self.min,
            back: self.max - 1,
            stride,
        }
    }
}

impl IntoIterator for IArea {
    type IntoIter = IAreaIter;
    type Item = IVec2;

    fn into_iter(self) -> Self::IntoIter {
        self.iter(1)
    }
}

/// X-Major
#[derive(Clone)]
pub struct IAreaIter {
    area: IArea,
    next: IVec2,
    back: IVec2,
    stride: i32,
}

impl IAreaIter {
    pub fn with_stride(mut self, stride: i32) -> Self {
        assert!(
            stride > 0,
            "Expected stride of IAreaIter to be greater than 0, found: '{stride}'"
        );
        self.stride = stride;
        self
    }
}

impl Iterator for IAreaIter {
    type Item = IVec2;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next.y >= self.area.max.y {
            return None;
        }

        let result = self.next;

        self.next.x += self.stride;
        if self.next.x >= self.area.max.x {
            self.next.x = self.area.min.x;
            self.next.y += self.stride;
        }

        Some(result)
    }
}

impl DoubleEndedIterator for IAreaIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.back.y < self.area.min.y {
            return None;
        }

        let result = self.back;

        self.back.x -= self.stride;
        if self.back.x < self.area.min.x {
            self.back.x = self.area.max.x - 1;
            self.back.y -= self.stride;
        }

        Some(result)
    }
}

/// Unlike IAreaIter, this is EXCLUSIVE on the max.
#[derive(Clone)]
pub struct ICells2d {
    area: IArea,
    next: IVec2,
    back: IVec2,
    stride: i32,
}

impl Iterator for ICells2d {
    type Item = IArea;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next.y >= self.area.max.y {
            return None;
        }

        let result = self.next;

        self.next.x += self.stride;
        if self.next.x >= self.area.max.x {
            self.next.x = self.area.min.x;
            self.next.y += self.stride;
        }

        Some(IArea {
            min: result,
            max: result + self.stride,
        })
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::{IVec2, ivec2};

    use crate::space::area::IArea;

    #[test]
    fn area_width_height() {
        let area = IArea::from_size(1);
        assert_eq!(area.width(), 3);
        assert_eq!(area.height(), 3);

        let area = IArea::from_center_extents(ivec2(50, 25), ivec2(50, 50));
        assert_eq!(area.min, ivec2(0, -25));
        assert_eq!(area.max, ivec2(101, 76));
        assert_eq!(area.width(), 101);
        assert_eq!(area.height(), 101);

        let area = IArea::from_center_extents(ivec2(-150, -190), ivec2(50, 50));
        assert_eq!(area.min, ivec2(-200, -240));
        assert_eq!(area.max, ivec2(-99, -139));
        assert_eq!(area.width(), 101);
        assert_eq!(area.height(), 101);
    }

    #[test]
    fn area_iter() {
        let area = IArea::from_center_extents(ivec2(0, 0), ivec2(1, 1));
        let expected = vec![
            ivec2(-1, -1),
            ivec2(0, -1),
            ivec2(1, -1),
            ivec2(-1, 0),
            ivec2(0, 0),
            ivec2(1, 0),
            ivec2(-1, 1),
            ivec2(0, 1),
            ivec2(1, 1),
        ];
        assert_eq!(expected, area.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn area_iter_strided() {
        let area = IArea::from_center_extents(ivec2(0, 0), ivec2(1, 1));
        let expected = vec![ivec2(-1, -1), ivec2(1, -1), ivec2(-1, 1), ivec2(1, 1)];
        assert_eq!(expected, area.iter(2).collect::<Vec<_>>());

        let area = IArea::from_center_extents(ivec2(0, 0), ivec2(5, 5));
        let mut expected = Vec::new();
        for y in (-5..5).step_by(3) {
            for x in (-5..5).step_by(3) {
                expected.push(ivec2(x, y));
            }
        }
        assert_eq!(expected, area.iter(3).collect::<Vec<_>>());
    }

    #[test]
    fn area_iter_negative() {
        let area = IArea::from_center_extents(ivec2(-10, -5), ivec2(5, 5));
        let mut expected = Vec::new();
        for y in 0..=10 {
            for x in 0..=10 {
                expected.push(ivec2(area.min.x + x, area.min.y + y));
            }
        }
        assert_eq!(expected, area.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn area_iter_negative_strided() {
        let area = IArea::from_center_extents(ivec2(-10, -5), ivec2(5, 5));
        let mut expected = Vec::new();
        for y in (0..=10).step_by(2) {
            for x in (0..=10).step_by(2) {
                expected.push(ivec2(area.min.x + x, area.min.y + y));
            }
        }
        assert_eq!(expected, area.iter(2).collect::<Vec<_>>());
    }

    #[test]
    fn area_iter_rev() {
        let center = ivec2(-10, -10);
        let area = IArea::from_center_extents(center, ivec2(5, 5));
        let mut expected = Vec::new();
        for y in (-5..=5).rev() {
            for x in (-5..=5).rev() {
                expected.push(ivec2(center.x + x, center.y + y));
            }
        }
        assert_eq!(expected, area.into_iter().rev().collect::<Vec<_>>());
    }

    #[test]
    fn area_round_up_to() {
        let mut area = IArea::from_size(2);
        area.round_up_to(IVec2::splat(5));
        assert_eq!(area.min, ivec2(-5, -5));
        assert_eq!(area.max, ivec2(5, 5));

        let mut area = IArea::from_size(4);
        area.round_up_to(IVec2::splat(5));
        assert_eq!(area.max, ivec2(5, 5));
    }

    #[test]
    fn area_cells() {
        let area = IArea::from_size(2);
        let expected = vec![
            IArea::new(ivec2(-5, -5), ivec2(0, 0)),
            IArea::new(ivec2(0, -5), ivec2(5, 0)),
            IArea::new(ivec2(-5, 0), ivec2(0, 5)),
            IArea::new(ivec2(0, 0), ivec2(5, 5)),
        ];
        assert_eq!(expected, area.cells(5).collect::<Vec<_>>());

        let area = IArea::from_size(4);
        let expected = vec![
            IArea::new(ivec2(-5, -5), ivec2(0, 0)),
            IArea::new(ivec2(0, -5), ivec2(5, 0)),
            IArea::new(ivec2(-5, 0), ivec2(0, 5)),
            IArea::new(ivec2(0, 0), ivec2(5, 5)),
        ];
        assert_eq!(expected, area.cells(5).collect::<Vec<_>>());
    }
}
