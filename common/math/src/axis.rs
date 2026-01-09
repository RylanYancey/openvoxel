use std::{
    hash::Hash,
    iter::Enumerate,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Index, IndexMut, Not},
    slice::{Iter, IterMut},
};

use bevy::{ecs::intern::Internable, prelude::*};
use fxhash::FxHashMap;
use serde::Deserialize;

/// An axis-aligned direction.
/// NEGX/POSX are East/West
/// NEGZ/POSZ are North/South
/// NEGY/POSY are Up/Down
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Axis {
    PosX = 0,
    NegX = 1,
    PosY = 2,
    NegY = 3,
    PosZ = 4,
    NegZ = 5,
}

impl Axis {
    /// Try to convert get an Axis from a u8.
    /// None wil be returned if v is not in the range [0,6)
    pub const fn try_from_u8(v: u8) -> Option<Self> {
        if v < 6 {
            Some(unsafe { Self::from_u8_unchecked(v) })
        } else {
            None
        }
    }

    /// Convert from u8 to Axis without checking for a valid tag.
    pub const unsafe fn from_u8_unchecked(v: u8) -> Self {
        unsafe { std::mem::transmute::<u8, Self>(v) }
    }

    /// Get the inverse of this axis.
    ///  - POSX => NEGX
    ///  - NEGX => POSX
    ///  - POSY => NEGY
    ///  - NEGY => POSY
    ///  - POSZ => NEGZ
    ///  - NEGZ => POSZ
    pub const fn invert(self) -> Self {
        // result is known to be in-range so its' safe.
        unsafe { Self::from_u8_unchecked(self as u8 ^ 1) }
    }

    /// If the axis is negative, convert to positive.
    pub const fn abs(self) -> Self {
        // all negative axes are odd, just convert to even.
        unsafe { Self::from_u8_unchecked(self as u8 & !1) }
    }

    /// Get the "next" axis. Wraps around to POSX if self is NEGZ.
    /// - POSX => NEGX => POSY => NEGY => POSZ => NEGZ
    pub const fn next(self) -> Self {
        unsafe {
            Self::from_u8_unchecked(if let Self::NegZ = self {
                0
            } else {
                self as u8 + 1
            })
        }
    }

    /// Get the axis as an &'static str.
    /// Uses two characters, "+x" for PosX etc.
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::PosX => "+x",
            Self::NegX => "-x",
            Self::PosY => "+y",
            Self::NegY => "-y",
            Self::PosZ => "+z",
            Self::NegZ => "-z",
        }
    }

    /// Get the axis as an &'static str.
    /// Format uses cardinal directions, "south", "east", "down", etc.
    pub const fn as_dir_str(&self) -> &'static str {
        match *self {
            Self::PosX => "east",
            Self::NegX => "west",
            Self::PosY => "up",
            Self::NegY => "down",
            Self::PosZ => "north",
            Self::NegZ => "south",
        }
    }

    /// Convert from string to
    pub fn from_str(s: impl AsRef<str>) -> Option<Self> {
        Some(match s.as_ref() {
            "+x" | "east" => Self::PosX,
            "-x" | "west" => Self::NegX,
            "+y" | "up" => Self::PosY,
            "-y" | "down" => Self::NegY,
            "+z" | "north" => Self::PosZ,
            "-z" | "south" => Self::NegZ,
            _ => return None,
        })
    }

    pub fn as_ivec3(self) -> IVec3 {
        Self::AS_IVEC3[self as usize]
    }

    pub const ALL: [Self; 6] = [
        Self::PosX,
        Self::NegX,
        Self::PosY,
        Self::NegY,
        Self::PosZ,
        Self::NegZ,
    ];

    pub const AS_VEC3: [Vec3; 6] = [
        vec3(1.0, 0.0, 0.0),
        vec3(-1.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        vec3(0.0, -1.0, 0.0),
        vec3(0.0, 0.0, 1.0),
        vec3(0.0, 0.0, -1.0),
    ];

    pub const AS_IVEC3: [IVec3; 6] = [
        ivec3(1, 0, 0),
        ivec3(-1, 0, 0),
        ivec3(0, 1, 0),
        ivec3(0, -1, 0),
        ivec3(0, 0, 1),
        ivec3(0, 0, -1),
    ];

    pub const AS_I8VEC3: [[i8; 3]; 6] = [
        [127, 0, 0],
        [-127, 0, 0],
        [0, 127, 0],
        [0, -127, 0],
        [0, 0, 127],
        [0, 0, -127],
    ];
}

impl std::ops::Add<IVec3> for Axis {
    type Output = IVec3;

    fn add(self, rhs: IVec3) -> Self::Output {
        rhs + self.as_ivec3()
    }
}

/// A boolean for all 6 axes.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct AxisMask(u8);

impl AxisMask {
    /// Mask with PosX,PosY,PosZ
    pub const POSITIVE: Self = Self(0b101010);
    /// Mask with NegX,NegY,NegZ
    pub const NEGATIVE: Self = Self(0b010101);
    /// Mask with PosX,NegX,PosZ,NegZ set
    pub const HORIZONTAL: Self = Self(0b110011);
    /// Mask with PosY,NegY
    pub const VERTICAL: Self = Self(0b001100);
    /// Mask with PosX,NegX
    pub const X: Self = Self(0b110000);
    /// Mask with PosY,NegY
    pub const Y: Self = Self(0b001100);
    /// Mask with PosZ,NegZ
    pub const Z: Self = Self(0b000011);

    /// A new empty mask.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// A mask with all axes set.
    pub const fn full() -> Self {
        Self(0b111111)
    }

    /// Whether all axes are zeroes.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether the mask has this axis set.
    pub const fn has(self, axis: Axis) -> bool {
        self.0 & (1 << axis as u8) != 0
    }

    /// Whether all axes are ones.
    pub const fn is_all_set(self) -> bool {
        self.0 == 0b111111
    }

    /// Get the number of set axes in the mask.
    pub const fn count_ones(self) -> u32 {
        self.0.count_ones()
    }

    /// Get the number of unset axes in the mask.
    pub const fn count_zeros(self) -> u32 {
        // ignore upper 2 bits, which aren't used.
        6 - self.count_ones()
    }

    /// Iter the axes in this mask that are set to true.
    #[inline]
    pub fn iter(&self) -> AxisMaskIter {
        self.into_iter()
    }
}

impl Not for AxisMask {
    type Output = Self;

    fn not(self) -> Self::Output {
        // regular not, but keep it in-range.
        Self(!self.0 & 0b111111)
    }
}

impl BitOr<Self> for AxisMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign<Self> for AxisMask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0
    }
}

impl BitOr<Axis> for AxisMask {
    type Output = Self;

    fn bitor(self, rhs: Axis) -> Self::Output {
        Self(self.0 | (1 << rhs as u8))
    }
}

impl BitOrAssign<Axis> for AxisMask {
    fn bitor_assign(&mut self, rhs: Axis) {
        self.0 |= 1 << rhs as u8
    }
}

impl BitAnd<Self> for AxisMask {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign<Self> for AxisMask {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0
    }
}

impl BitAnd<Axis> for AxisMask {
    type Output = Self;

    fn bitand(self, rhs: Axis) -> Self::Output {
        Self(self.0 & (1 << rhs as u8))
    }
}

impl BitAndAssign<Axis> for AxisMask {
    fn bitand_assign(&mut self, rhs: Axis) {
        self.0 &= 1 << rhs as u8
    }
}

impl BitXor<Self> for AxisMask {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl BitXorAssign<Self> for AxisMask {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0
    }
}

impl BitXor<Axis> for AxisMask {
    type Output = Self;

    fn bitxor(self, rhs: Axis) -> Self::Output {
        Self(self.0 ^ (1 << rhs as u8))
    }
}

impl BitXorAssign<Axis> for AxisMask {
    fn bitxor_assign(&mut self, rhs: Axis) {
        self.0 ^= 1 << rhs as u8
    }
}

impl IntoIterator for AxisMask {
    type IntoIter = AxisMaskIter;
    type Item = Axis;

    fn into_iter(self) -> Self::IntoIter {
        AxisMaskIter(self.0)
    }
}

/// Iterator over axes present in a axis bitmask.
#[derive(Copy, Clone)]
pub struct AxisMaskIter(u8);

impl Iterator for AxisMaskIter {
    type Item = Axis;

    fn next(&mut self) -> Option<Self::Item> {
        let j = self.0.trailing_zeros();
        if j == 8 {
            None
        } else {
            self.0 ^= 1 << j;
            Some(unsafe { Axis::from_u8_unchecked(j as u8) })
        }
    }
}

/// Storage for one element per axis.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct AxisArray<T>([T; 6]);

impl<T> AxisArray<T> {
    /// Construct a new Axis Array from an array.
    pub const fn new(axes: [T; 6]) -> AxisArray<T> {
        AxisArray(axes)
    }

    /// Construct a new axis array from a function.
    pub fn from_fn(mut f: impl FnMut(Axis) -> T) -> Self {
        Self(core::array::from_fn::<T, 6, _>(|i| {
            (f)(unsafe { Axis::from_u8_unchecked(i as u8) })
        }))
    }

    pub fn map<K>(self, mut f: impl FnMut(Axis, T) -> K) -> AxisArray<K> {
        let mut i = 0;
        let new = self.0.map(|t| {
            let axis = unsafe { Axis::from_u8_unchecked(i as u8) };
            i += 1;
            (f)(axis, t)
        });
        AxisArray(new)
    }

    /// Always 6, because there are six axes.
    pub const fn len(&self) -> usize {
        6
    }

    /// Convert to an array of T.
    pub fn to_array(self) -> [T; 6] {
        self.0
    }

    /// Axis Array as slice with length 6.
    pub const fn as_slice(&self) -> &[T] {
        self.0.as_slice()
    }

    /// Axis array as mutable slice with length 6
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.0.as_mut_slice()
    }

    pub fn values<'a>(&'a self) -> impl DoubleEndedIterator<Item = &'a T> {
        self.as_slice().iter()
    }

    pub fn values_mut<'a>(&'a mut self) -> impl DoubleEndedIterator<Item = &'a mut T> {
        self.as_mut_slice().iter_mut()
    }

    pub fn values_in<'a>(&'a self, mask: AxisMask) -> impl Iterator<Item = &'a T> {
        mask.iter().map(|axis| &self[axis])
    }

    /// Get an iterator over the elements.
    pub fn iter<'a>(&'a self) -> AxisArrayIter<'a, T> {
        self.into_iter()
    }

    /// Get an iterator over the elements mutably.
    pub fn iter_mut<'a>(&'a mut self) -> AxisArrayIterMut<'a, T> {
        self.into_iter()
    }

    pub fn iter_in<'a>(&'a self, mask: AxisMask) -> impl Iterator<Item = (Axis, &'a T)> {
        mask.iter().map(|axis| (axis, &self[axis]))
    }

    pub fn iter_mut_in<'a>(
        &'a mut self,
        mask: AxisMask,
    ) -> impl Iterator<Item = (Axis, &'a mut T)> {
        self.iter_mut().filter(move |(axis, _)| mask.has(*axis))
    }
}

impl<T: Clone> AxisArray<T> {
    pub fn to_vec(self) -> Vec<T> {
        self.0.to_vec()
    }
}

impl<'a, T> IntoIterator for &'a AxisArray<T> {
    type IntoIter = AxisArrayIter<'a, T>;
    type Item = (Axis, &'a T);

    fn into_iter(self) -> Self::IntoIter {
        AxisArrayIter {
            elements: self.0.iter().enumerate(),
        }
    }
}

impl<'a, T> IntoIterator for &'a mut AxisArray<T> {
    type IntoIter = AxisArrayIterMut<'a, T>;
    type Item = (Axis, &'a mut T);

    fn into_iter(self) -> Self::IntoIter {
        AxisArrayIterMut {
            elements: self.0.iter_mut().enumerate(),
        }
    }
}

pub struct AxisArrayIter<'a, T> {
    elements: Enumerate<Iter<'a, T>>,
}

impl<'a, T> Iterator for AxisArrayIter<'a, T> {
    type Item = (Axis, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let (i, val) = self.elements.next()?;
        let axis = unsafe { Axis::from_u8_unchecked(i as u8) };
        Some((axis, val))
    }
}

pub struct AxisArrayIterMut<'a, T> {
    elements: Enumerate<IterMut<'a, T>>,
}

impl<'a, T> Iterator for AxisArrayIterMut<'a, T> {
    type Item = (Axis, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        let (i, val) = self.elements.next()?;
        let axis = unsafe { Axis::from_u8_unchecked(i as u8) };
        Some((axis, val))
    }
}

impl<T> Index<Axis> for AxisArray<T> {
    type Output = T;

    #[inline]
    fn index(&self, index: Axis) -> &Self::Output {
        // rustc should optimize away the bounds check, need to confirm
        &self.0[index as usize]
    }
}

impl<T> IndexMut<Axis> for AxisArray<T> {
    #[inline]
    fn index_mut(&mut self, index: Axis) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}

impl<T: Clone + Hash + Eq> Internable for AxisArray<T> {
    fn leak(&self) -> &'static Self {
        Box::leak(Box::new(self.clone()))
    }

    fn ref_eq(&self, other: &Self) -> bool {
        std::ptr::eq(self as *const _, other as *const _)
    }

    fn ref_hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(self as *const _ as usize)
    }
}

/// Extension of the Axis enum that includes the diagonals, for a total of 18 directions.
/// Does not include the corner directions.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Deserialize)]
#[serde(try_from = "&str")]
#[rustfmt::skip]
pub enum AxisExt {
    /// 1,0,0
    PosX = 0,
    /// -1,0,0
    NegX = 1,
    /// 0,1,0
    PosY = 2,
    /// 0,-1,0
    NegY = 3,
    /// 0,0,1
    PosZ = 4,
    /// 0,0,-1
    NegZ = 5,
    /// 1,1,0
    PosXPosY = 6,
    /// -1,-1,0
    NegXNegY = 7,
    /// 1,0,1
    PosXPosZ = 8,
    /// -1,0,-1
    NegXNegZ = 9,
    /// 1,-1,0
    PosXNegY = 10,
    /// -1,1,0
    NegXPosY = 11,
    /// 1,0,-1
    PosXNegZ = 12,
    /// -1,0,1
    NegXPosZ = 13,
    /// 0,1,1
    PosYPosZ = 14,
    /// 0,-1,-1
    NegYNegZ = 15,
    /// 0,1,-1
    PosYNegZ = 16,
    /// 0,-1,1
    NegYPosZ = 17,
}

impl AxisExt {
    /// Convert from a u8 to Self without checking if the u8 is in-bounds.
    pub const unsafe fn from_u8_unchecked(u: u8) -> Self {
        unsafe { std::mem::transmute::<u8, Self>(u) }
    }

    /// Convert from a u8 in the range 0..18 to Self.
    pub const fn from_u8(u: u8) -> Option<Self> {
        if u > 17 {
            None
        } else {
            Some(unsafe { Self::from_u8_unchecked(u) })
        }
    }

    /// Get the opposite direction of self.
    pub const fn invert(self) -> Self {
        unsafe { Self::from_u8_unchecked(self as u8 ^ 1) }
    }

    /// Get the axis as a Vec3
    pub const fn as_vec3(self) -> Vec3 {
        Self::AS_VEC3[self as usize]
    }

    /// Get the axis as an IVec3
    pub const fn as_ivec3(self) -> IVec3 {
        Self::AS_IVEC3[self as usize]
    }

    /// Convert from a string to self.
    pub fn from_str(s: &str) -> Option<Self> {
        Self::get_from_string_resolver_map().get(s).copied()
    }

    /// Get the string associated with this value.
    /// Example: AxisExt::PosXNegZ = "+x,-z".
    /// In the resulting string, x will always come before y and z,
    /// and y will always come before z.
    pub const fn as_str(self) -> &'static str {
        Self::AS_STR[self as usize]
    }

    /// Get the direction string associated with this value.
    /// Examples:
    ///  - AxisExt::PosYNegZ = "south-up"
    ///  - AxisExt::NegXPosZ = "north-east"
    pub const fn as_dir_str(self) -> &'static str {
        Self::AS_DIR_STR[self as usize]
    }

    /// Get the map responsible for resolving axis strings to variants.
    fn get_from_string_resolver_map() -> &'static FxHashMap<&'static str, AxisExt> {
        static RESOLVER: std::sync::LazyLock<FxHashMap<&'static str, AxisExt>> =
            std::sync::LazyLock::new(|| {
                let mut map = FxHashMap::default();
                for i in 0..18 {
                    let variant = unsafe { AxisExt::from_u8_unchecked(i as u8) };
                    map.insert(AxisExt::AS_STR[i], variant);
                    map.insert(AxisExt::AS_DIR_STR[i], variant);
                    if i > 5 {
                        map.insert(AxisExt::AS_STR_REVERSED[i], variant);
                    }
                }
                map
            });
        &RESOLVER
    }

    pub const ALL: [Self; 18] = {
        let mut ret = [Self::PosX; 18];
        let mut i = 0;
        while i < 18 {
            ret[i] = unsafe { Self::from_u8_unchecked(i as u8) };
            i += 1;
        }
        ret
    };

    pub const AS_STR: [&'static str; 18] = [
        "+x", "-x", "+y", "-y", "+z", "-z", "+x,+y", "-x,-y", "+x,+z", "-x,-z", "+x,-y", "-x,+y",
        "+x,-z", "-x,+z", "+y,+z", "-y,-z", "+y,-z", "-y,+z",
    ];

    pub const AS_STR_REVERSED: [&'static str; 18] = [
        "+x", "-x", "+y", "-y", "+z", "-z", "+y,+x", "-y,-x", "+z,+x", "-z,-x", "-y,+x", "+y,-x",
        "-z,+x", "+z,-x", "+z,+y", "-z,-y", "-z,+y", "+z,-y",
    ];

    pub const AS_DIR_STR: [&'static str; 18] = [
        "east",
        "west",
        "up",
        "down",
        "north",
        "south",
        "east-up",
        "west-down",
        "north-east",
        "south-west",
        "east-down",
        "west-up",
        "south-west",
        "north-west",
        "north-up",
        "south-down",
        "south-up",
        "north-down",
    ];

    #[rustfmt::skip]
    pub const AS_VEC3: [Vec3; 18] = [
        vec3( 1.0,  0.0,  0.0), // PosX
        vec3(-1.0,  0.0,  0.0), // NegX
        vec3( 0.0,  1.0,  0.0), // PosY
        vec3( 0.0, -1.0,  0.0), // NegY
        vec3( 0.0,  0.0,  1.0), // PosZ
        vec3( 0.0,  0.0, -1.0), // NegZ
        vec3( 1.0,  1.0,  0.0), // PosXPosY
        vec3(-1.0, -1.0,  0.0), // NegXNegY
        vec3( 1.0,  0.0,  1.0), // PosXPosZ
        vec3(-1.0,  0.0, -1.0), // NegXNegZ
        vec3( 1.0, -1.0,  0.0), // PosXNegY
        vec3(-1.0,  1.0,  0.0), // NegXPosY
        vec3( 1.0,  0.0, -1.0), // PosXNegZ
        vec3(-1.0,  0.0,  1.0), // NegXPosZ
        vec3( 0.0,  1.0,  1.0), // PosYPosZ
        vec3( 0.0, -1.0, -1.0), // NegYNegZ
        vec3( 0.0,  1.0, -1.0), // PosYNegZ
        vec3( 0.0, -1.0,  1.0), // NegYPosZ
    ];

    #[rustfmt::skip]
    pub const AS_IVEC3: [IVec3; 18] = [
        ivec3( 1,  0,  0), // PosX
        ivec3(-1,  0,  0), // NegX
        ivec3( 0,  1,  0), // PosY
        ivec3( 0, -1,  0), // NegY
        ivec3( 0,  0,  1), // PosZ
        ivec3( 0,  0, -1), // NegZ
        ivec3( 1,  1,  0), // PosXPosY
        ivec3(-1, -1,  0), // NegXNegY
        ivec3( 1,  0,  1), // PosXPosZ
        ivec3(-1,  0, -1), // NegXNegZ
        ivec3( 1, -1,  0), // PosXNegY
        ivec3(-1,  1,  0), // NegXPosY
        ivec3( 1,  0, -1), // PosXNegZ
        ivec3(-1,  0,  1), // NegXPosZ
        ivec3( 0,  1,  1), // PosYPosZ
        ivec3( 0, -1, -1), // NegYNegZ
        ivec3( 0,  1, -1), // PosYNegZ
        ivec3( 0, -1,  1), // NegYPosZ
    ];
}

impl TryFrom<&str> for AxisExt {
    type Error = String;

    fn try_from(s: &str) -> std::result::Result<Self, Self::Error> {
        AxisExt::get_from_string_resolver_map()
            .get(s)
            .copied()
            .ok_or_else(|| format!("Cannot convert axis string '{s}' to AxisExt."))
    }
}

impl From<Axis> for AxisExt {
    fn from(value: Axis) -> Self {
        unsafe { Self::from_u8_unchecked(value as u8) }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct AxisMaskExt(u32);

impl AxisMaskExt {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn full() -> Self {
        Self(0x1FFFF)
    }

    pub const fn clear(&mut self) {
        self.0 = 0
    }

    pub const fn has(self, axis: AxisExt) -> bool {
        (self.0 & (1 << axis as u8)) != 0
    }

    pub const fn flip(&mut self, axis: AxisExt) {
        self.0 ^= 1 << axis as u8
    }

    pub const fn set(&mut self, axis: AxisExt, v: bool) {
        if v {
            self.set_one(axis)
        } else {
            self.set_zero(axis)
        }
    }

    pub const fn set_zero(&mut self, axis: AxisExt) {
        self.0 &= !(1 << axis as u8)
    }

    pub const fn set_one(&mut self, axis: AxisExt) {
        self.0 |= 1 << axis as u8
    }
}

/// Deserialize from a Vec<String>.
impl<'de> Deserialize<'de> for AxisMaskExt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MaskVisitor;

        impl<'de> serde::de::Visitor<'de> for MaskVisitor {
            type Value = AxisMaskExt;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a sequence of axis names")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let mut mask = AxisMaskExt::empty();
                let axis = AxisExt::try_from(v).map_err(serde::de::Error::custom)?;
                mask.set(axis, true);
                Ok(mask)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut mask = AxisMaskExt::empty();

                while let Some(axis_str) = seq.next_element::<&str>()? {
                    let axis = AxisExt::try_from(axis_str).map_err(serde::de::Error::custom)?;
                    mask.set(axis, true)
                }

                Ok(mask)
            }
        }

        deserializer.deserialize_seq(MaskVisitor)
    }
}
