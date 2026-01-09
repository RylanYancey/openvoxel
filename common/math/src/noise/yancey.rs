use bevy::math::{Vec2, ivec2};

use crate::rng::Permutation;

/// 2d yancey noise.
/// Similar to perlin noise but without smoothing and much faster.
/// Intended for use in point hashing for Worley Noise for dynamic scaling.
#[inline]
pub fn yancey2(perm: &Permutation, point: Vec2) -> f32 {
    // compute cell min/max
    let c00 = point.floor().as_ivec2();
    let c11 = c00 + ivec2(1, 1);

    // hash point components
    let hx0 = perm[(c00.x & 255) as usize] as usize;
    let hy0 = perm[(c00.y & 255) as usize] as usize;
    let hx1 = perm[(c11.x & 255) as usize] as usize;
    let hy1 = perm[(c11.y & 255) as usize] as usize;

    // hash points to heights
    let h00 = perm[hx0 + hy0] as f32;
    let h10 = perm[hx1 + hy0] as f32;
    let h01 = perm[hx0 + hy1] as f32;
    let h11 = perm[hx1 + hy1] as f32;

    // mix
    let [fx, fy] = (point - point.floor()).to_array();
    let l1 = lerp(fx, h00, h10);
    let l2 = lerp(fx, h01, h11);
    lerp(fy, l1, l2) * const { 1.0 / 255.0 }
}

const fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}
