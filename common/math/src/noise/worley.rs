use crate::rng::Permutation;
use bevy::math::{IVec2, IVec3, Vec2, Vec2Swizzles, Vec3, vec2, vec3};
use std::mem;
use std::{simd::prelude::*, sync::Arc};

/// 2D Worley Noise.
#[derive(Clone)]
pub struct Worley2 {
    cache: AdjCells2,
    perm: Arc<Permutation>,
    scale: Vec2,
}

impl Worley2 {
    pub fn new(perm: Arc<Permutation>, scale: Vec2) -> Self {
        Self {
            cache: AdjCells2::new(&perm, Vec2::ZERO),
            perm,
            scale,
        }
    }
}

impl Worley2 {
    /// Compute L1 point and value of 2d Worley noise.
    #[inline]
    pub fn l1(&mut self, mut point: Vec2) -> (Vec2, f32) {
        // -- // COMPUTE ADJACENCIES // -- //
        point *= self.scale.xy();
        let co = self.cache.origin;
        let po = point.floor();
        let pi = po.as_ivec2();
        // if the cached origin is not the same as the point origin, rebuild the cache.
        if (pi.x ^ co.x) | (pi.y ^ co.y) != 0 {
            self.cache.origin = pi;
            for i in 0..9 {
                let cell = po + ADJACENT2D[i];
                let hash = self.perm.mix(cell.as_ivec2().to_array());
                self.cache.cell_x[i] = cell.x + ((hash & 0xF) as f32 / 15.0);
                self.cache.cell_y[i] = cell.y + ((hash >> 4) as f32 / 15.0);
            }
        }

        // -- // ACCUMULATE DISTANCES // -- //
        let adj = &self.cache;
        // distances for points 0..4
        let mut dsq1 = dsq_2d(0, &adj.cell_x, &adj.cell_y, point);
        let mut idx1 = u32x4::from_array([0, 1, 2, 3]);
        // distances for points 4..8
        let dsq2 = dsq_2d(4, &adj.cell_x, &adj.cell_y, point);
        let idx2 = u32x4::from_array([4, 5, 6, 7]);
        let lt1 = dsq2.simd_le(dsq1);
        dsq1 = lt1.select(dsq2, dsq1);
        idx1 = lt1.select(idx2, idx1);

        // -- // REDUCE TO MINIMUM -- //
        let mut l1 = dsq1[0];
        let mut i1 = 0;
        for i in 1..4 {
            if dsq1[i] < l1 {
                l1 = dsq1[i];
                i1 = i;
            }
        }
        i1 = idx1[i1] as usize;

        // -- // HANDLE FINAL CELL // -- //
        let dsq9 = (point.x - adj.cell_x[8]).powi(2) + (point.y - adj.cell_y[8]).powi(2);
        if dsq9 < l1 {
            l1 = dsq9;
            i1 = 8;
        }

        // extract the l1 position and return the dsq
        (vec2(adj.cell_x[i1], adj.cell_y[i1]), l1.sqrt())
    }

    /// Compute L1 point and L1/L2 values of 2d Worley noise.
    #[inline]
    pub fn l2(&mut self, mut point: Vec2) -> (Vec2, f32, f32) {
        // -- // COMPUTE ADJACENCIES // -- //
        point *= self.scale;
        let co = self.cache.origin;
        let po = point.floor();
        let pi = po.as_ivec2();
        // if the cached origin is not the same as the point origin, rebuild the cache.
        if (pi.x ^ co.x) | (pi.y ^ co.y) != 0 {
            self.cache.origin = pi;
            for i in 0..9 {
                let cell = po + ADJACENT2D[i];
                let hash = self.perm.mix(cell.as_ivec2().to_array());
                self.cache.cell_x[i] = cell.x + ((hash & 0xF) as f32 / 15.0);
                self.cache.cell_y[i] = cell.y + ((hash >> 4) as f32 / 15.0);
            }
        }

        // -- // ACCUMULATE DISTANCES // -- //
        let adj = &self.cache;
        // distances for points 0..4
        let mut dsq1 = dsq_2d(0, &adj.cell_x, &adj.cell_y, point);
        let mut idx1 = u32x4::from_array([0, 1, 2, 3]);
        // distances for points 4..8
        let mut dsq2 = dsq_2d(4, &adj.cell_x, &adj.cell_y, point);
        let idx2 = u32x4::from_array([4, 5, 6, 7]);
        let lt1 = dsq2.simd_lt(dsq1);
        let temp = dsq1;
        dsq1 = lt1.select(dsq2, dsq1);
        idx1 = lt1.select(idx2, idx1);
        dsq2 = lt1.select(temp, dsq2);

        // -- // REDUCE TO MINIMUM // -- //
        // At this point, L1 is known to be in DSQ1.
        // L2 is known to be either in DSQ1 or "underneath" L1 in DSQ2.
        let mut l1 = dsq1[0];
        let mut i1 = 0;
        let mut l2 = dsq2[0];
        for i in 1..4 {
            let d1 = dsq1[i];
            if d1 < l1 {
                l2 = l1;
                i1 = i;
                l1 = d1;
                let d2 = dsq2[i];
                if d2 < l2 {
                    l2 = d2;
                }
            } else if d1 < l2 {
                l2 = d1;
            }
        }
        i1 = idx1[i1] as usize;

        // -- // HANDLE FINAL CELL // -- //
        let dsq9 = (point.x - adj.cell_x[8]).powi(2) + (point.y - adj.cell_y[8]).powi(2);
        if dsq9 < l2 {
            if dsq9 < l1 {
                l2 = l1;
                l1 = dsq9;
                i1 = 8;
            } else {
                l2 = dsq9;
            }
        }

        (vec2(adj.cell_x[i1], adj.cell_y[i1]), l1.sqrt(), l2.sqrt())
    }

    /// Compute the L1 point and L1/L2/L3 values of 2d Worley noise.
    #[inline]
    pub fn l3(&mut self, mut point: Vec2) -> (Vec2, f32, f32, f32) {
        // -- // COMPUTE ADJACENCIES // -- //
        point *= self.scale;
        let co = self.cache.origin;
        let po = point.floor();
        let pi = po.as_ivec2();
        // if the cached origin is not the same as the point origin, rebuild the cache.
        if (pi.x ^ co.x) | (pi.y ^ co.y) != 0 {
            self.cache.origin = pi;
            for i in 0..9 {
                let cell = po + ADJACENT2D[i];
                let hash = self.perm.mix(cell.as_ivec2().to_array());
                self.cache.cell_x[i] = cell.x + ((hash & 0xF) as f32 / 15.0);
                self.cache.cell_y[i] = cell.y + ((hash >> 4) as f32 / 15.0);
            }
        }

        // -- // ACCUMULATE DISTANCES // -- //
        let adj = &self.cache;
        // distances for points 0..4
        let mut dsq1 = dsq_2d(0, &adj.cell_x, &adj.cell_y, point);
        let mut idx1 = u32x4::from_array([0, 1, 2, 3]);
        // distances for points 4..8
        let mut dsq2 = dsq_2d(4, &adj.cell_x, &adj.cell_y, point);
        let idx2 = u32x4::from_array([4, 5, 6, 7]);
        let lt1 = dsq2.simd_lt(dsq1);
        let temp = dsq1;
        dsq1 = lt1.select(dsq2, dsq1);
        idx1 = lt1.select(idx2, idx1);
        dsq2 = lt1.select(temp, dsq2);

        // -- // REDUCE TO MINIMUM // -- //
        // At this point, L1 is known to be in DSQ1.
        // L2 is known to be either in DSQ1 or "underneath" L1 in DSQ2.
        // If L2 is in DSQ1, then L3 is underneath either L1 or L2.
        // If L2 is in DSQ2, then L3 is known to be in DSQ1.
        let mut l1 = dsq1[0];
        let mut i1 = 0;
        let mut l2 = dsq2[0];
        let mut l3 = dsq2[1];
        if l3 < l2 {
            mem::swap(&mut l3, &mut l2);
        }
        for i in 1..4 {
            let d1 = dsq1[i];
            let d2 = dsq2[i];
            if d1 < l1 {
                l3 = l2;
                l2 = l1;
                l1 = d1;
                i1 = i;
                if d2 < l2 {
                    l3 = l2;
                    l2 = d2;
                } else if d2 < l3 {
                    l3 = d2;
                }
            } else if d1 < l2 {
                l3 = l2;
                l2 = d1;
                if d2 < l3 {
                    l3 = d2;
                }
            } else if d1 < l3 {
                l3 = d1;
                if d2 < l3 {
                    l3 = d2;
                }
            } else if d2 < l3 {
                l3 = d2;
            }
        }
        i1 = idx1[i1] as usize;

        // -- // HANDLE FINAL CELL // -- //
        let dsq9 = (point.x - adj.cell_x[8]).powi(2) + (point.y - adj.cell_y[8]).powi(2);
        if dsq9 < l3 {
            if dsq9 < l2 {
                if dsq9 < l1 {
                    l3 = l2;
                    l2 = l1;
                    l1 = dsq9;
                    i1 = 8;
                } else {
                    l3 = l2;
                    l2 = dsq9;
                }
            } else {
                l3 = dsq9;
            }
        }

        (
            vec2(adj.cell_x[i1], adj.cell_y[i1]),
            l1.sqrt(),
            l2.sqrt(),
            l3.sqrt(),
        )
    }
}

/// 3D Worley Noise
#[derive(Clone)]
pub struct Worley3 {
    cache: AdjCells3,
    perm: Arc<Permutation>,
    scale: Vec3,
}

impl Worley3 {
    #[inline]
    pub fn new(perm: Arc<Permutation>, scale: Vec3) -> Self {
        Self {
            cache: AdjCells3::new(&perm, Vec3::ZERO),
            perm,
            scale,
        }
    }

    #[inline]
    pub fn l1(&mut self, mut point: Vec3) -> (Vec3, f32) {
        // -- // COMPUTE ADJACENCIES // -- //
        point *= self.scale;
        let co = self.cache.origin;
        let po = point.floor();
        let pi = po.as_ivec3();
        // if the cached origin is not the same as the point origin, rebuild the cache.
        if (pi.x ^ co.x) | (pi.y ^ co.y) | (pi.z ^ co.z) != 0 {
            self.cache.origin = pi;
            for i in 0..27 {
                let cell = po + ADJACENT3D[i];
                let hash = self.perm.mix(cell.as_ivec3().to_array());
                self.cache.cell_x[i] = cell.x + (hash & 7) as f32 / 7.0;
                self.cache.cell_y[i] = cell.y + ((hash >> 3) & 3) as f32 / 3.0;
                self.cache.cell_z[i] = cell.z + ((hash >> 5) & 7) as f32 / 7.0;
            }
        }

        // -- // ACCUMULATE DISTANCES // -- //
        let adj = &self.cache;
        let inc = u32x4::splat(4);
        let mut dsq1 = dsq_3d(0, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
        let mut idx1 = u32x4::from_array([0, 1, 2, 3]);
        let mut idx = idx1;
        let mut i = 4;
        while i <= 24 {
            idx += inc;
            let dsq2 = dsq_3d(i, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
            let lt = dsq2.simd_lt(dsq1);
            dsq1 = lt.select(dsq2, dsq1);
            idx1 = lt.select(idx, idx1);
            i += 4;
        }

        // -- // REDUCE TO MINIMUM -- //
        let mut l1 = dsq1[0];
        let mut i1 = 0;
        for i in 1..4 {
            if dsq1[i] < l1 {
                l1 = dsq1[i];
                i1 = i;
            }
        }
        i1 = idx1[i1] as usize;

        (
            vec3(adj.cell_x[i1], adj.cell_y[i1], adj.cell_z[i1]),
            l1.sqrt(),
        )
    }

    #[inline]
    pub fn l2(&mut self, mut point: Vec3) -> (Vec3, f32, f32) {
        // -- // COMPUTE ADJACENCIES // -- //
        point *= self.scale;
        let co = self.cache.origin;
        let po = point.floor();
        let pi = po.as_ivec3();
        // if the cached origin is not the same as the point origin, rebuild the cache.
        if (pi.x ^ co.x) | (pi.y ^ co.y) | (pi.z ^ co.z) != 0 {
            self.cache.origin = pi;
            for i in 0..27 {
                let cell = po + ADJACENT3D[i];
                let hash = self.perm.mix(cell.as_ivec3().to_array());
                self.cache.cell_x[i] = cell.x + (hash & 7) as f32 / 7.0;
                self.cache.cell_y[i] = cell.y + ((hash >> 3) & 3) as f32 / 3.0;
                self.cache.cell_z[i] = cell.z + ((hash >> 5) & 7) as f32 / 7.0;
            }
        }

        // -- // ACCUMULATE DISTANCES // -- //
        let adj = &self.cache;
        let inc = u32x4::splat(4);
        let mut idx = u32x4::from_array([0, 1, 2, 3]);
        let mut dsq1 = dsq_3d(0, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
        let mut idx1 = idx;
        let mut dsq2 = dsq_3d(4, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
        swap_lt(
            &mut dsq1,
            &mut dsq2,
            &mut idx1,
            &mut u32x4::from_array([4, 5, 6, 7]),
        );
        idx = idx + inc;
        let mut i = 8;
        while i < 28 {
            idx += inc;
            let dsq3 = dsq_3d(i, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
            let lt1 = dsq3.simd_lt(dsq1);
            if lt1.any() {
                dsq2 = lt1.select(dsq1, dsq2);
                dsq1 = lt1.select(dsq3, dsq1);
                idx1 = lt1.select(idx, idx1);
            }
            let lt2 = dsq3.simd_lt(dsq2) & !lt1;
            dsq2 = lt2.select(dsq3, dsq2);
            i += 4;
        }

        // -- // REDUCE TO MINIMUM -- // -- //
        let mut l1 = dsq1[0];
        let mut i1 = 0;
        let mut l2 = dsq2[0];
        for i in 1..4 {
            let d1 = dsq1[i];
            if d1 < l1 {
                l2 = l1;
                i1 = i;
                l1 = d1;
                let d2 = dsq2[i];
                if d2 < l2 {
                    l2 = d2;
                }
            } else if d1 < l2 {
                l2 = d1;
            }
        }
        i1 = idx1[i1] as usize;
        (
            vec3(adj.cell_x[i1], adj.cell_y[i1], adj.cell_z[i1]),
            l1.sqrt(),
            l2.sqrt(),
        )
    }

    #[inline]
    pub fn l3(&mut self, mut point: Vec3) -> (Vec3, f32, f32, f32) {
        // -- // COMPUTE ADJACENCIES // -- //
        point *= self.scale;
        let co = self.cache.origin;
        let po = point.floor();
        let pi = po.as_ivec3();
        // if the cached origin is not the same as the point origin, rebuild the cache.
        if (pi.x ^ co.x) | (pi.y ^ co.y) | (pi.z ^ co.z) != 0 {
            self.cache.origin = pi;
            for i in 0..27 {
                let cell = po + ADJACENT3D[i];
                let hash = self.perm.mix(cell.as_ivec3().to_array());
                self.cache.cell_x[i] = cell.x + (hash & 7) as f32 / 7.0;
                self.cache.cell_y[i] = cell.y + ((hash >> 3) & 3) as f32 / 3.0;
                self.cache.cell_z[i] = cell.z + ((hash >> 5) & 7) as f32 / 7.0;
            }
        }

        // -- // ACCUMULATE DISTANCES // -- //
        let adj = &self.cache;
        let inc = u32x4::splat(4);
        let mut idx = u32x4::from_array([0, 1, 2, 3]);
        let mut idx1 = idx;
        let mut dsq1 = dsq_3d(0, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
        let mut idx2 = idx + inc;
        let mut dsq2 = dsq_3d(4, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
        let mut idx3 = idx + inc + inc;
        let mut dsq3 = dsq_3d(8, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
        // re-order such that DSQ1 < DSQ2 < DSQ3
        swap_lt(&mut dsq1, &mut dsq2, &mut idx1, &mut idx2);
        swap_lt(&mut dsq2, &mut dsq3, &mut idx2, &mut idx3);
        swap_lt(&mut dsq1, &mut dsq2, &mut idx1, &mut idx2);
        idx = idx + inc + inc;
        let mut i = 12;
        while i < 28 {
            idx += inc;
            let dsq4 = dsq_3d(i, &adj.cell_x, &adj.cell_y, &adj.cell_z, point);
            // The goal of this sequence is to maintain the statement "dsq1 < dsq2 < dsq3"
            // We have to have 3 buffers because its possible for the index of L1, L2, and L3
            // to have the same index in DSQ1.
            let lt1 = dsq4.simd_lt(dsq1);
            if lt1.any() {
                // The goal of this sequence is to make space in DSQ1 for elements of
                // DSQ4 that are less than DSQ1. We can't overwrite DSQ1 because the value
                // of DSQ1 could be a valid L2 value, and the value in DSQ2 could be a valid L3
                // value.
                dsq3 = lt1.select(dsq2, dsq3);
                dsq2 = lt1.select(dsq1, dsq2);
                dsq1 = lt1.select(dsq4, dsq1);
                idx1 = lt1.select(idx, idx1); // < we only care about the L1 index, so only IDX1 is tracked.
            }
            let lt2 = dsq4.simd_lt(dsq2) & !lt1;
            // make space for values of DSQ4 that are less than DSQ2 and not less than DSQ1,
            // and shift those values down to DSQ3 instead of overwriting.
            dsq3 = lt2.select(dsq2, dsq3);
            dsq2 = lt2.select(dsq4, dsq2);
            let lt3 = dsq4.simd_lt(dsq3) & !(lt1 | lt2);
            // Move values of DSQ4 that are less than DSQ3 but not less than DSQ1 or DSQ2 into DSQ3.
            dsq3 = lt3.select(dsq4, dsq3);
            i += 4;
        }

        // -- // REDUCE TO MINIMUM // -- //
        // It is known that "dsq1 < dsq2 < dsq3"
        // At this point, L1 is known to be in DSQ1.
        // L2 is known to be either in DSQ1 or "underneath" L1 in DSQ2.
        // L3 is underneath L1 or L2 in either DSQ2 or DSQ3, or somewhere in DSQ1.
        let mut l1 = dsq1[0];
        let mut i1 = 0;
        let mut l2 = dsq2[0];
        let mut l3 = dsq3[0];
        for i in 1..4 {
            let d1 = dsq1[i];
            let d2 = dsq2[i];
            let d3 = dsq3[i];
            // This sequence looks overly complex, but its the fastest
            // I could get the final sort to be. Turns out its fairly branch
            // predictor efficient, despite not seeming like it.
            if d1 < l1 {
                l3 = l2;
                l2 = l1;
                l1 = d1;
                i1 = i;
                if d2 < l2 {
                    l3 = l2;
                    l2 = d2;
                    if d3 < l3 {
                        l3 = d3;
                    }
                } else if d2 < l3 {
                    l3 = d2;
                }
            } else if d1 < l2 {
                l3 = l2;
                l2 = d1;
                if d2 < l3 {
                    l3 = d2;
                }
            } else if d1 < l3 {
                l3 = d1;
            }
        }
        i1 = idx1[i1] as usize;

        (
            vec3(adj.cell_x[i1], adj.cell_y[i1], adj.cell_z[i1]),
            l1.sqrt(),
            l2.sqrt(),
            l3.sqrt(),
        )
    }
}

#[derive(Clone)]
struct AdjCells2 {
    origin: IVec2,
    cell_x: [f32; 9],
    cell_y: [f32; 9],
}

impl AdjCells2 {
    fn new(perm: &Permutation, origin: Vec2) -> Self {
        let mut cell_x = [0.0f32; 9];
        let mut cell_y = [0.0f32; 9];

        for i in 0..9 {
            let cell = origin + ADJACENT2D[i];
            let hash = perm.mix(cell.as_ivec2().to_array());
            cell_x[i] = cell.x + ((hash & 0xF) as f32 / 15.0);
            cell_y[i] = cell.y + ((hash >> 4) as f32 / 15.0);
        }

        Self {
            origin: origin.floor().as_ivec2(),
            cell_x,
            cell_y,
        }
    }
}

#[derive(Clone)]
struct AdjCells3 {
    origin: IVec3,
    cell_x: [f32; 28],
    cell_y: [f32; 28],
    cell_z: [f32; 28],
}

impl AdjCells3 {
    fn new(perm: &Permutation, origin: Vec3) -> Self {
        let mut cell_x = [f32::INFINITY; 28];
        let mut cell_y = [f32::INFINITY; 28];
        let mut cell_z = [f32::INFINITY; 28];

        for i in 0..27 {
            let cell = origin + ADJACENT3D[i];
            let hash = perm.mix(cell.as_ivec3().to_array());
            cell_x[i] = cell.x + (hash & 7) as f32 / 7.0;
            cell_y[i] = cell.y + ((hash >> 3) & 3) as f32 / 3.0;
            cell_z[i] = cell.z + ((hash >> 5) & 7) as f32 / 7.0;
        }

        Self {
            origin: origin.as_ivec3(),
            cell_x,
            cell_y,
            cell_z,
        }
    }
}

/// Compute the squared distances between the points, given a start index.
#[inline]
fn dsq_3d<const N: usize>(
    start: usize,
    cx: &[f32; N],
    cy: &[f32; N],
    cz: &[f32; N],
    pt: Vec3,
) -> f32x4 {
    let sx = f32x4::splat(pt.x) - f32x4::from_slice(&cx[start..]);
    let sy = f32x4::splat(pt.y) - f32x4::from_slice(&cy[start..]);
    let sz = f32x4::splat(pt.z) - f32x4::from_slice(&cz[start..]);
    sx * sx + sy * sy + sz * sz
}

/// Compute the squared distances between the points, given a start index.
#[inline]
fn dsq_2d<const N: usize>(start: usize, cx: &[f32; N], cy: &[f32; N], pt: Vec2) -> f32x4 {
    let sx = f32x4::splat(pt.x) - f32x4::from_slice(&cx[start..]);
    let sy = f32x4::splat(pt.y) - f32x4::from_slice(&cy[start..]);
    sx * sx + sy * sy
}

/// Swap elements of dsq2 with dsq1 if dsq2 is smaller.
#[inline]
fn swap_lt(dsq1: &mut f32x4, dsq2: &mut f32x4, idx1: &mut u32x4, idx2: &mut u32x4) {
    let mask = dsq2.simd_lt(*dsq1);
    let dsq_tmp = *dsq1;
    let idx_tmp = *idx1;
    *dsq1 = mask.select(*dsq2, *dsq1);
    *idx1 = mask.select(*idx2, *idx1);
    *dsq2 = mask.select(dsq_tmp, *dsq2);
    *idx2 = mask.select(idx_tmp, *idx2);
}

const ADJACENT2D: [Vec2; 9] = [
    vec2(-1.0, -1.0),
    vec2(0.0, -1.0),
    vec2(1.0, -1.0),
    vec2(-1.0, 0.0),
    vec2(0.0, 0.0),
    vec2(1.0, 0.0),
    vec2(-1.0, 1.0),
    vec2(0.0, 1.0),
    vec2(1.0, 1.0),
];

const ADJACENT3D: [Vec3; 27] = {
    let mut result = [Vec3::ZERO; 27];
    let mut i = 0;
    while i < 27 {
        result[i].x = ((i % 3) as i32 - 1) as f32;
        result[i].y = (((i / 3) % 3) as i32 - 1) as f32;
        result[i].z = ((i / 9) as i32 - 1) as f32;
        i += 1;
    }
    result
};
