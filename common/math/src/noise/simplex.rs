use std::simd::{Simd, f32x4, num::SimdFloat};

use crate::rng::Permutation;
use bevy::math::{Vec2, Vec3, ivec2, ivec3, vec2, vec3};

/// 2-Dimensional Simplex Noise
/// If the input X and Y are whole numbers, the output will be 0.0.
#[inline]
pub fn simplex2(perm: &Permutation, point: Vec2) -> f32 {
    // factors for skewing to simplex space.
    const SQRT_3: f32 = 1.73205080757;
    const F2: f32 = 0.5 * (SQRT_3 - 1.0);
    const G2: f32 = (3.0 - SQRT_3) / 6.0;
    const GRAD2: [Vec2; 8] = [
        vec2(1.0, 1.0),
        vec2(-1.0, 1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, -1.0),
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ];
    // skew input space to determine which simplex cell we're in
    let s = point.element_sum() * F2;
    let ij = (point + s).floor().as_ivec2();
    // Unskew back to (x, y) space ?
    let t = ij.element_sum() as f32 * G2;
    let xy = point - (ij.as_vec2() - t);
    // Offsets for second and third corner of the simplex cell.
    let ij1x = (xy.x > xy.y) as i32;
    let ij1y = ij1x ^ 1;
    let ij1 = ivec2(ij1x, ij1y);
    // compute offsets for corners in (x, y) coords.
    let xy0 = xy;
    let xy1 = xy - ij1.as_vec2() + G2;
    let xy2 = xy - 1.0 + 2.0 * G2;
    // Compute vertices in square space.
    let ij0 = ij;
    let ij2 = ij + ivec2(1, 1);
    // compute component hashes
    let hx0 = perm[(ij0.x & 255) as usize] as usize;
    let hy0 = perm[(ij0.y & 255) as usize] as usize;
    let hx1 = perm[(ij2.x & 255) as usize] as usize;
    let hy1 = perm[(ij2.y & 255) as usize] as usize;
    // Mix values together
    let h0 = perm[hx0 + hy0] as usize;
    let h1 = perm[if xy.x > xy.y { hx1 + hy0 } else { hx0 + hy1 }] as usize;
    let h2 = perm[hx1 + hy1] as usize;
    // Select gradients for the four corners.
    let gi0 = GRAD2[h0 & 7];
    let gi1 = GRAD2[h1 & 7];
    let gi2 = GRAD2[h2 & 7];
    // pack values into SIMD vectors.
    let x = Simd::from_array([xy0.x, xy1.x, xy2.x]);
    let y = Simd::from_array([xy0.y, xy1.y, xy2.y]);
    // compute attenuation factors
    let mut att = Simd::from_array([0.5; 3]) - (x * x) - (y * y);
    att *= att;
    att *= att;
    // Compute dot products
    let dot =
        Simd::from_array([gi0.x, gi1.x, gi2.x]) * x + Simd::from_array([gi0.y, gi1.y, gi2.y]) * y;
    // Sum the contributions and convert to range [-1,1]
    (dot * att).reduce_sum() * 70.0
}

/// 2D Simplex Noise with derivatives. Returns [value, dx, dy]
#[inline]
pub fn simplex2_derivative(perm: &Permutation, point: Vec2) -> [f32; 3] {
    // factors for skewing to simplex space.
    const SQRT_3: f32 = 1.73205080757;
    const F2: f32 = 0.5 * (SQRT_3 - 1.0);
    const G2: f32 = (3.0 - SQRT_3) / 6.0;
    const GRAD2: [Vec2; 8] = [
        vec2(1.0, 1.0),
        vec2(-1.0, 1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, -1.0),
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ];
    // skew input space to determine which simplex cell we're in
    let s = point.element_sum() * F2;
    let ij = (point + s).floor().as_ivec2();
    // Unskew back to (x, y) space
    let t = ij.element_sum() as f32 * G2;
    let xy = point - (ij.as_vec2() - t);

    // Offsets for second and third corner of the simplex cell.
    let ij1x = (xy.x > xy.y) as i32;
    let ij1y = ij1x ^ 1;
    let ij1 = ivec2(ij1x, ij1y);

    // compute offsets for corners in (x, y) coords.
    let xy0 = xy;
    let xy1 = xy - ij1.as_vec2() + G2;
    let xy2 = xy - 1.0 + 2.0 * G2;

    // Compute vertices in square space.
    let ij0 = ij;
    let ij2 = ij + ivec2(1, 1);

    // compute component hashes
    let hx0 = perm[(ij0.x & 255) as usize] as usize;
    let hy0 = perm[(ij0.y & 255) as usize] as usize;
    let hx1 = perm[(ij2.x & 255) as usize] as usize;
    let hy1 = perm[(ij2.y & 255) as usize] as usize;

    // Mix values together
    let h0 = perm[hx0 + hy0] as usize;
    let h1 = perm[if xy.x > xy.y { hx1 + hy0 } else { hx0 + hy1 }] as usize;
    let h2 = perm[hx1 + hy1] as usize;

    // Select gradients for the four corners.
    let gi0 = GRAD2[h0 & 7];
    let gi1 = GRAD2[h1 & 7];
    let gi2 = GRAD2[h2 & 7];

    // pack values into SIMD vectors.
    let x = Simd::from_array([xy0.x, xy1.x, xy2.x, 0.0]);
    let y = Simd::from_array([xy0.y, xy1.y, xy2.y, 0.0]);
    let gx = Simd::from_array([gi0.x, gi1.x, gi2.x, 0.0]);
    let gy = Simd::from_array([gi0.y, gi1.y, gi2.y, 0.0]);

    // compute t = 0.5 - x^2 - y^2 (attenuation factor before raising to power)
    let t = Simd::from_array([0.5; 4]) - (x * x) - (y * y);

    // Clamp negative values to zero (contributions outside influence radius)
    let t_clamped = t.simd_max(Simd::splat(0.0));

    // Compute dot products: g · r
    let dot = gx * x + gy * y;

    // Compute t^2, t^3, t^4
    let t2 = t_clamped * t_clamped;
    let t3 = t2 * t_clamped;
    let t4 = t2 * t2;

    // Noise value: sum of (t^4 * dot)
    let value = (t4 * dot).reduce_sum() * 70.0;

    // Derivative computation:
    // dn/dx = sum of [t^4 * gx - 8 * x * t^3 * dot]
    // dn/dy = sum of [t^4 * gy - 8 * y * t^3 * dot]
    let temp = t3 * dot * Simd::splat(8.0);

    let dx_contrib = t4 * gx - temp * x;
    let dy_contrib = t4 * gy - temp * y;

    let dx = dx_contrib.reduce_sum() * 70.0;
    let dy = dy_contrib.reduce_sum() * 70.0;

    [value, dx, dy]
}

/// 3-Dimensional Simplex Noise
/// If the input X, Y, and Z are whole numbers, the output will be 0.0.
#[inline]
pub fn simplex3(perm: &Permutation, point: Vec3) -> f32 {
    const F3: f32 = 1.0 / 3.0;
    const G3: f32 = 1.0 / 6.0;
    const GRAD3: [Vec3; 12] = [
        vec3(1.0, 1.0, 0.0),
        vec3(-1.0, 1.0, 0.0),
        vec3(1.0, -1.0, 0.0),
        vec3(-1.0, -1.0, 0.0),
        vec3(1.0, 0.0, 1.0),
        vec3(-1.0, 0.0, 1.0),
        vec3(1.0, 0.0, -1.0),
        vec3(-1.0, 0.0, -1.0),
        vec3(0.0, 1.0, 1.0),
        vec3(0.0, -1.0, 1.0),
        vec3(0.0, 1.0, -1.0),
        vec3(0.0, -1.0, -1.0),
    ];
    // skew input space to determine which simplex cell we're in
    let s = point.element_sum() * F3;
    let ijk0 = (point + s).floor().as_ivec3();
    let ijk3 = ijk0 + ivec3(1, 1, 1);
    let t = ijk0.element_sum() as f32 * G3;
    let xyz = point - (ijk0.as_vec3() - t);
    // Offsets and hash values for second and third corner of the simplex cell.
    let (ijk1, ijk2) = if xyz.x >= xyz.y {
        if xyz.y >= xyz.z {
            (vec3(1.0, 0.0, 0.0), vec3(1.0, 1.0, 0.0))
        } else if xyz.x >= xyz.z {
            (vec3(1.0, 0.0, 0.0), vec3(1.0, 0.0, 1.0))
        } else {
            (vec3(0.0, 0.0, 1.0), vec3(1.0, 0.0, 1.0))
        }
    } else {
        if xyz.y < xyz.z {
            (vec3(0.0, 0.0, 1.0), vec3(0.0, 1.0, 1.0))
        } else if xyz.x < xyz.z {
            (vec3(0.0, 1.0, 0.0), vec3(0.0, 1.0, 1.0))
        } else {
            (vec3(0.0, 1.0, 0.0), vec3(1.0, 1.0, 0.0))
        }
    };
    // compute offsets for corners in (x, y, z) coords.
    let xyz0 = xyz;
    let xyz1 = xyz - ijk1 + G3;
    let xyz2 = xyz - ijk2 + 2.0 * G3;
    let xyz3 = xyz - 1.0 + 3.0 * G3;
    // Multiply by large primes for hashing.
    let hx0 = perm[(ijk0.x & 255) as usize] as usize;
    let hy0 = perm[(ijk0.y & 255) as usize] as usize;
    let hz0 = perm[(ijk0.z & 255) as usize] as usize;
    let hx1 = perm[(ijk3.x & 255) as usize] as usize;
    let hy1 = perm[(ijk3.y & 255) as usize] as usize;
    let hz1 = perm[(ijk3.z & 255) as usize] as usize;
    // Mix points together to get a hash.
    let h0 = perm[hx0 + perm[hy0 + hz0] as usize] as usize;
    let h1 = perm[if ijk1.x == 0.0 { hx0 } else { hx1 }
        + perm[if ijk1.y == 0.0 { hy0 } else { hy1 } + if ijk1.z == 0.0 { hz0 } else { hz1 }]
            as usize] as usize;
    let h2 = perm[if ijk2.x == 0.0 { hx0 } else { hx1 }
        + perm[if ijk2.y == 0.0 { hy0 } else { hy1 } + if ijk2.z == 0.0 { hz0 } else { hz1 }]
            as usize] as usize;
    let h3 = perm[hx1 + perm[hy1 + hz1] as usize] as usize;
    // index gradients for the four corners with the hash.
    let gi0 = GRAD3[h0 % 12];
    let gi1 = GRAD3[h1 % 12];
    let gi2 = GRAD3[h2 % 12];
    let gi3 = GRAD3[h3 % 12];
    // pack SIMD vectors with point values
    let x = f32x4::from_array([xyz0.x, xyz1.x, xyz2.x, xyz3.x]);
    let y = f32x4::from_array([xyz0.y, xyz1.y, xyz2.y, xyz3.y]);
    let z = f32x4::from_array([xyz0.z, xyz1.z, xyz2.z, xyz3.z]);
    // Compute dot products
    let dot = f32x4::from_array([gi0.x, gi1.x, gi2.x, gi3.x]) * x
        + f32x4::from_array([gi0.y, gi1.y, gi2.y, gi3.y]) * y
        + f32x4::from_array([gi0.z, gi1.z, gi2.z, gi3.z]) * z;
    // Compute attenuation factors
    let mut att = f32x4::splat(0.6) - (x * x) - (y * y) - (z * z);
    // Zero negative contributions.
    att = att.simd_max(f32x4::splat(0.0));
    att *= att;
    att *= att;
    // Sum the contributions and convert to range [-1,1]
    (dot * att).reduce_sum() * 32.0
}
