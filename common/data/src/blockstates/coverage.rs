use bevy::ecs::intern::{Internable, Interned, Interner};
use math::axis::{Axis, AxisArray};

static COVERAGES_INTERNER: Interner<AxisArray<Mask>> = Interner::new();

#[derive(Copy, Clone)]
pub struct Coverages {
    /// (num_bits, texture)
    data: AxisArray<(u16, u16)>,

    /// Actual interned mask data.
    masks: Interned<AxisArray<Mask>>,
}

impl Coverages {
    #[inline]
    pub fn new(textures: AxisArray<u16>, masks: AxisArray<Mask>) -> Self {
        Self {
            data: textures.map(|axis, tex| (masks[axis].num_covered(), tex)),
            masks: COVERAGES_INTERNER.intern(&masks),
        }
    }

    #[inline]
    pub fn is_covered_by(&self, other: &Self, axis: Axis) -> bool {
        let rhs_axis = axis.invert();
        let (lhs_num_bits, lhs_texture) = self.data[axis];
        let (rhs_num_bits, rhs_texture) = other.data[rhs_axis];
        if lhs_num_bits > rhs_num_bits || lhs_num_bits == 0 {
            // self has more bits than other, and therefore
            // cannot be covered by other. Or self is empty and
            // doesn't interact with the boundary at all.
            false
        } else if rhs_num_bits == 256 {
            // self is partial or full and other is full.
            // Covered if other is opaque or self is transparent and they have same texture.
            lhs_texture == rhs_texture
        } else if lhs_texture == rhs_texture {
            // self is partial and other is partial, neither are empty.
            // if self is opaque and other is transparent, it can't cover.
            // if self is opaque and other is opaque, check for mask coverage.
            self.masks[axis].is_covered_by(&other.masks[rhs_axis])
        } else {
            // Self and other are known to be partial, neither are empty.
            // Self and/or other are transparent, but they are not both solid.
            lhs_texture == 0 && self.masks[axis].is_covered_by(&other.masks[rhs_axis])
        }
    }

    /// Whether the coverage on this axis is full and equal to this texture.
    pub fn is_full_and_texture(&self, axis: Axis, texture: u16) -> bool {
        let (rhs_num_bits, rhs_texture) = self.data[axis];
        rhs_num_bits == 256 && rhs_texture == texture
    }
}

#[derive(Copy, Clone)]
pub struct Coverage {
    /// Bits of the face that are covered.
    mask: Interned<Mask>,

    /// Number of bits in the mask, range=[0,256]
    /// Value of 256 indicates a mask of all ones.
    num_bits: u16,

    /// The direction the mask faces.
    axis: Axis,

    /// The texture associated with the coverage. This is
    /// used to determine if adjacent transparent faces
    /// are covering others, which only happens if the faces
    /// have the same texture.
    ///
    /// A texture value of 0 indicates an opaque or air coverage.
    texture: u16,
}

impl Coverage {
    /// Determine whether self is covered by other.
    /// When self is empty, false will always be returned.
    pub fn is_covered_by(&self, other: &Self) -> bool {
        if self.num_bits > other.num_bits || self.num_bits == 0 {
            // self has more bits than other, and therefore
            // cannot be covered by other. Or self is empty and
            // doesn't interact with the boundary at all.
            false
        } else if other.num_bits == 256 {
            // self is partial or full and other is full.
            // Covered if other is opaque or self is transparent and they have same texture.
            self.texture == other.texture
        } else if self.texture == other.texture {
            // self is partial and other is partial, neither are empty.
            // if self is opaque and other is transparent, it can't cover.
            // if self is opaque and other is opaque, check for mask coverage.
            self.mask.is_covered_by(&other.mask)
        } else {
            // Self and other are known to be partial, neither are empty.
            // Self and/or other are transparent, but they are not both solid.
            // Self.texture and other.texture are not equal.
            self.is_opaque() && self.mask.is_covered_by(&other.mask)
        }
    }

    pub const fn is_opaque(&self) -> bool {
        self.texture == 0
    }

    pub const fn is_transparent(&self) -> bool {
        self.texture != 0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Mask(pub [u64; 4]);

impl Mask {
    pub const EMPTY: Self = Self([0u64; 4]);
    pub const FULL: Self = Self([u64::MAX; 4]);

    /// Quadrant from 0,0 to 8,8
    pub const Q00: Self = Self([0xFF00_FF00_FF00_FF00, 0xFF00_FF00_FF00_FF00, 0x0, 0x0]);

    /// Quadrant from 8,0 to 16,8
    pub const Q10: Self = Self([0x00FF_00FF_00FF_00FF, 0x00FF_00FF_00FF_00FF, 0x0, 0x0]);

    /// Quadrant from 0,8 to 8,16
    pub const Q01: Self = Self([0x0, 0x0, 0xFF00_FF00_FF00_FF00, 0xFF00_FF00_FF00_FF00]);

    /// Quadrant from 8,8 to 16,16
    pub const Q11: Self = Self([0x0, 0x0, 0x00FF_00FF_00FF_00FF, 0x00FF_00FF_00FF_00FF]);

    pub const fn is_full(&self) -> bool {
        self.0[0] == u64::MAX
            && self.0[1] == u64::MAX
            && self.0[2] == u64::MAX
            && self.0[3] == u64::MAX
    }

    pub const fn is_empty(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    pub const fn is_partial(&self) -> bool {
        let sum = self.num_covered();
        sum > 0 && sum < 256
    }

    pub fn is_covered_by(&self, other: &Self) -> bool {
        self.0[0] & !other.0[0] == 0
            && self.0[1] & !other.0[1] == 0
            && self.0[2] & !other.0[2] == 0
            && self.0[3] & !other.0[3] == 0
    }

    pub const fn num_covered(&self) -> u16 {
        (self.0[0].count_ones()
            + self.0[1].count_ones()
            + self.0[2].count_ones()
            + self.0[3].count_ones()) as u16
    }
}

impl Internable for Mask {
    fn leak(&self) -> &'static Self {
        Box::leak(Box::new(*self))
    }

    fn ref_eq(&self, other: &Self) -> bool {
        std::ptr::eq(self as *const _, other as *const _)
    }

    fn ref_hash<H: core::hash::Hasher>(&self, state: &mut H) {
        let refr = self as *const Mask as usize;
        state.write_usize(refr)
    }
}
