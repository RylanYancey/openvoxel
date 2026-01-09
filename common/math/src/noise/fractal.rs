use std::{ops::Mul, sync::Arc};

use bevy::math::{Vec2, Vec3};

use crate::rng::Permutation;

/// Parameters for Fractal Brownian Motion with Perlin or Simplex noise.
#[derive(Clone)]
pub struct Fractal {
    /// The number of iterations.
    ///
    /// Recommended to be in the range [1,7].
    /// If octaves is 0, the output will always be 0.
    /// If octaves is greater than 7, the output will be very slow to compute.
    pub octaves: u8,

    /// The initial input scaling factor.
    pub frequency: f32,

    /// The initial output scaling factor.
    pub amplitude: f32,

    /// The rate of change of the frequency each octave.
    pub lacunarity: f32,

    /// The rate of change of the amplitude each octave.
    pub gain: f32,

    /// Permutation used to generate cell offsets.
    pub perm: Arc<Permutation>,
}

impl Fractal {
    /// Fractal Brownian Motion with 2D perlin noise.
    pub fn perlin2(&self, pt: Vec2) -> f32 {
        self.compute(&self.perm, pt, super::perlin::perlin2)
    }

    /// Fractal Brownian Motion with 3D perlin noise.
    pub fn perlin3(&self, pt: Vec3) -> f32 {
        self.compute(&self.perm, pt, super::perlin::perlin3)
    }

    /// Fractal Brownian Motion with 2D simplex noise.
    pub fn simplex2(&self, pt: Vec2) -> f32 {
        self.compute(&self.perm, pt, super::simplex::simplex2)
    }

    /// Fractal Brownian Motion with 3D simplex noise.
    pub fn simplex3(&self, pt: Vec3) -> f32 {
        self.compute(&self.perm, pt, super::simplex::simplex3)
    }

    #[inline]
    fn compute<T, F>(&self, perm: &Permutation, point: T, noise_fn: F) -> f32
    where
        F: Fn(&Permutation, T) -> f32,
        T: Mul<f32, Output = T> + Copy,
    {
        let mut frequency = self.frequency;
        let mut amplitude = self.amplitude;
        let mut output = 0.0f32;

        for _ in 0..self.octaves {
            output += amplitude * (noise_fn)(perm, point * frequency);
            frequency *= self.lacunarity;
            amplitude *= self.gain;
        }

        output
    }
}
