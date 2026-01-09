use crate::rng::Permutation;
use bevy::math::{Vec2, Vec3, ivec2, ivec3, vec2, vec3};

/// 2-dimensional Perlin Noise.
/// If the input X and Y is a whole number, the output will be 0.0.
#[inline]
pub fn perlin2(perm: &Permutation, point: Vec2) -> f32 {
    const GRAD2: [Vec2; 8] = [
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(-1.0, 0.0),
        vec2(0.0, -1.0),
        vec2(1.0, 1.0),
        vec2(-1.0, 1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, -1.0),
    ];

    // compute cell min/max
    let c00 = point.floor().as_ivec2();
    let c11 = c00 + ivec2(1, 1);

    // compute position within the cell
    let [x0, y0] = (c00.as_vec2() - point).to_array();
    let [x1, y1] = (c11.as_vec2() - point).to_array();

    // hash point components
    let hx0 = perm[(c00.x & 255) as usize] as usize;
    let hy0 = perm[(c00.y & 255) as usize] as usize;
    let hx1 = perm[(c11.x & 255) as usize] as usize;
    let hy1 = perm[(c11.y & 255) as usize] as usize;

    // hash points
    let h00 = perm[hx0 + hy0] as usize;
    let h10 = perm[hx1 + hy0] as usize;
    let h01 = perm[hx0 + hy1] as usize;
    let h11 = perm[hx1 + hy1] as usize;

    // select gradients and compute dot products.
    let n00 = GRAD2[h00 & 7].dot(vec2(x0, y0));
    let n10 = GRAD2[h10 & 7].dot(vec2(x1, y0));
    let n01 = GRAD2[h01 & 7].dot(vec2(x0, y1));
    let n11 = GRAD2[h11 & 7].dot(vec2(x1, y1));

    // calculate attenuations
    let [fx, fy] = (point - point.floor()).to_array();
    let u = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let v = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);

    // mix
    let l1 = lerp(u, n00, n10);
    let l2 = lerp(u, n01, n11);
    lerp(v, l1, l2)
}

/// 3-Dimensional Perlin Noise
/// If the input X, Y, and Z are whole numbers, the output will be 0.0.
#[inline]
pub fn perlin3(perm: &Permutation, point: Vec3) -> f32 {
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

    // compute cell min/max
    let c000 = point.floor().as_ivec3();
    let c111 = c000 + ivec3(1, 1, 1);

    // compute fractional part of point
    let [x0, y0, z0] = (c000.as_vec3() - point).to_array();
    let [x1, y1, z1] = (c111.as_vec3() - point).to_array();

    // hash point components
    let hx0 = perm[(c000.x & 255) as usize] as usize;
    let hy0 = perm[(c000.y & 255) as usize] as usize;
    let hz0 = perm[(c000.z & 255) as usize] as usize;
    let hx1 = perm[(c111.x & 255) as usize] as usize;
    let hy1 = perm[(c111.y & 255) as usize] as usize;
    let hz1 = perm[(c111.z & 255) as usize] as usize;

    // calculate fade coefficients
    let [fx, fy, fz] = (point - point.floor()).to_array();
    let u = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let v = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let w = fz * fz * fz * (fz * (fz * 6.0 - 15.0) + 10.0);

    // compute contributions
    let k = perm[hy0 + hz0] as usize;
    let n000 = GRAD3[(hx0 + k) % 12].dot(vec3(x0, y0, z0));
    let n100 = GRAD3[(hx1 + k) % 12].dot(vec3(x1, y0, z0));
    let nx00 = lerp(u, n000, n100);
    let k = perm[hy1 + hz0] as usize;
    let n010 = GRAD3[(hx0 + k) % 12].dot(vec3(x0, y1, z0));
    let n110 = GRAD3[(hx1 + k) % 12].dot(vec3(x1, y1, z0));
    let nx10 = lerp(u, n010, n110);
    let k = perm[hy0 + hz1] as usize;
    let n001 = GRAD3[(hx0 + k) % 12].dot(vec3(x0, y0, z1));
    let n101 = GRAD3[(hx1 + k) % 12].dot(vec3(x1, y0, z1));
    let nx01 = lerp(u, n001, n101);
    let k = perm[hy1 + hz1] as usize;
    let n011 = GRAD3[(hx0 + k) % 12].dot(vec3(x0, y1, z1));
    let n111 = GRAD3[(hx1 + k) % 12].dot(vec3(x1, y1, z1));
    let nx11 = lerp(u, n011, n111);

    // mix
    let nxy0 = lerp(v, nx00, nx10);
    let nxy1 = lerp(v, nx01, nx11);
    lerp(w, nxy0, nxy1)
}

const fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}
