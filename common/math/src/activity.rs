use bevy::prelude::Deref;

/// Used to track update frequencies. Value is in the range 0.0..=1.0
#[derive(Copy, Clone, Debug, Deref)]
pub struct Activity {
    /// In the range 0.0..=1.0.
    #[deref]
    value: f32,
}

impl Activity {
    pub const DEFAULT: Self = Self { value: 0.0 };

    pub const fn new() -> Self {
        Self::DEFAULT
    }

    pub fn lt(self, v: f32) -> bool {
        self.value < v
    }

    pub fn le(self, v: f32) -> bool {
        self.value <= v
    }

    pub fn gt(self, v: f32) -> bool {
        self.value > v
    }

    pub fn ge(self, v: f32) -> bool {
        self.value >= v
    }

    // Update the Activity value according to an alpha.
    //
    // A negative alpha will shift the value toward zero.
    // A positive alpha will shift the value toward one.
    pub fn update(&mut self, mut alpha: f32) {
        let target = (alpha >= 0.0) as u32 as f32;
        alpha = alpha.abs();
        self.value = self.value * (1.0 - alpha) + target * alpha;
    }
}

impl Default for Activity {
    fn default() -> Self {
        Self::DEFAULT
    }
}
