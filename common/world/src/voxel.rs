use data::registry::RegistryId;

/// Information about an instance of a voxel in the world,
/// including its index and light value.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct VoxelState {
    pub voxel: Voxel,
    pub light: Light,
}

impl VoxelState {
    pub const AIR: Self = Self::DEFAULT;
    pub const DEFAULT: Self = Self {
        voxel: Voxel::DEFAULT,
        light: Light::DEFAULT,
    };
}

impl Default for VoxelState {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// An index of an entry in the Voxels registry.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct Voxel(pub u16);

impl Voxel {
    pub const DEFAULT: Self = Self::AIR;
    pub const AIR: Self = Self(0);
}

impl Default for Voxel {
    fn default() -> Self {
        Self::AIR
    }
}

impl Into<RegistryId> for Voxel {
    fn into(self) -> RegistryId {
        RegistryId(self.0 as usize)
    }
}

impl From<RegistryId> for Voxel {
    fn from(value: RegistryId) -> Self {
        Self(value.0 as u16)
    }
}

impl From<u16> for Voxel {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

/// A Voxel's Light value.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Light {
    /// The 4 low bits are ambient intensity,
    /// the 4 high bits are torch intensity.
    pub intensity: u8,

    /// The low 4 bits are HSL hue,
    /// The high 4 bits are HSL lightness.
    ///
    /// This is only meant to be used on the client.
    #[cfg(feature = "colored_lights")]
    pub color: u8,
}

impl Light {
    pub const DEFAULT: Self = Self::AMBIENT_FULL;

    /// A light value where all fields are 0.
    pub const ZERO: Self = Self {
        intensity: 0,
        #[cfg(feature = "colored_lights")]
        color: 0,
    };

    /// A light value where all fields are 0.
    pub const AMBIENT_NONE: Self = Self::ZERO;

    /// A light value with ambient intensity set to 15 and
    /// everything else set to 0.
    pub const AMBIENT_FULL: Self = Self {
        intensity: 0xF,
        #[cfg(feature = "colored_lights")]
        color: 0,
    };

    /// Instantiate a new Light instance.
    /// Values must be in the range 0..=15.
    pub const fn new(
        ambient_intensity: u8,
        torch_intensity: u8,
        color_hue: u8,
        color_lightness: u8,
    ) -> Self {
        Self {
            intensity: ambient_intensity | (torch_intensity << 4),
            #[cfg(feature = "colored_lights")]
            color: color_hue | (color_lightness << 4),
        }
    }

    /// Assign the HSL Hue of the light color.
    /// Value must be in the range 0..=15.
    /// Does nothing if the colored_lights feature is disabled.
    pub const fn set_color_hue(&mut self, v: u8) {
        #[cfg(feature = "colored_lights")]
        {
            self.color &= !0xF;
            self.color |= v;
        }
    }

    /// Assign the HSL Hue of the light color with an f32.
    /// Value must be in the range 0.0..=1.0
    /// Does nothing if the colored_lights feature is disabled.
    pub const fn set_color_hue_f32(&mut self, f: f32) {
        self.set_color_hue((f * 15.0) as u8)
    }

    /// Assign the HSL Lightness of the light color.
    /// Value must be in the range 0..=15.
    /// Does nothing if the colored_lights feature is disabled.
    pub const fn set_color_lightness(&mut self, v: u8) {
        #[cfg(feature = "colored_lights")]
        {
            self.color &= 0xF;
            self.color |= v << 4;
        }
    }

    /// Assign the HSL Lightness of the light color with an f32.
    /// Value must be in the range 0.0..=1.0.
    /// Does nothing if the colored_lights feature is disabled.
    pub const fn set_color_lightness_f32(&mut self, f: f32) {
        self.set_color_lightness((f * 15.0) as u8)
    }

    /// Assign the HSL Hue and Lightness.
    /// Values must be in the range 0..=15.
    /// Does nothing if the colored_lights feature is disabled.
    pub const fn set_color(&mut self, color_hue: u8, color_lightness: u8) {
        #[cfg(feature = "colored_lights")]
        {
            self.color = color_hue | (color_lightness << 4);
        }
    }

    /// Assign the HSL hue and lightness via RGB.
    pub fn set_color_rgb(&mut self, rgb: [u8; 3]) {
        let r = rgb[0] as f32 / 255.0;
        let g = rgb[1] as f32 / 255.0;
        let b = rgb[2] as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;

        let lightness = (max + min) / 2.0;
        let mut hue = if chroma == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / chroma) % 6.0
        } else if max == g {
            ((b - r) / chroma) + 2.0
        } else {
            ((r - g) / chroma) + 4.0
        } * 42.5;

        if hue < 0.0 {
            hue += 360.0;
        }

        self.set_color((hue * 255.0) as u8, (lightness * 255.0) as u8)
    }

    /// Assign ambient intensity.
    /// Value must be in the range 0..=15
    pub const fn set_ambient_intensity(&mut self, v: u8) {
        self.intensity &= !0xF;
        self.intensity |= v;
    }

    /// Assign torch intensity.
    /// Value must be in the range 0..=15
    pub const fn set_torch_intensity(&mut self, v: u8) {
        self.intensity &= 0xF;
        self.intensity |= v << 4;
    }

    /// Assign torch intensity and ambient intensity.
    /// Values must be in the range 0..=15
    pub const fn set_intensity(&mut self, ambient: u8, torch: u8) {
        self.intensity = ambient | (torch << 4);
    }

    /// Get the ambient intensity.
    /// Will be in the range 0..=15
    pub const fn ambient_intensity(&self) -> u8 {
        self.intensity & 0xF
    }

    /// Get the torch intensity.
    /// Will be in the range 0..=15
    pub const fn torch_intensity(&self) -> u8 {
        self.intensity >> 4
    }

    /// The HSL Hue of the light color.
    /// Will be in the range 0..=15.
    /// If colored_lights are disabled, the return value will always be 0.
    pub const fn color_hue(&self) -> u8 {
        #[cfg(feature = "colored_lights")]
        return self.color & 0xF;
        #[cfg(not(feature = "colored_lights"))]
        return 0;
    }

    /// The HSL lightness of the light color.
    /// Will be in the range 0..=15.
    /// If colored_lights are disabled, the return value will always be 0.
    pub const fn color_lightness(&self) -> u8 {
        #[cfg(feature = "colored_lights")]
        return self.color >> 4;
        #[cfg(not(feature = "colored_lights"))]
        return 0;
    }

    /// Get the RGB color of the light as an array of bytes.
    pub const fn color_as_rgb(&self) -> [u8; 3] {
        todo!()
    }
}

impl Default for Light {
    fn default() -> Self {
        Self::AMBIENT_FULL
    }
}
