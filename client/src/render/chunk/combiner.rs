use bevy::{
    asset::RenderAssetUsages,
    mesh::{
        Indices, Mesh, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues, VertexFormat,
    },
};
use data::blockstates::{
    Transparency,
    quad::{Normal, Quad, Vertex},
};
use math::axis::{AxisArray, AxisMask};

#[derive(Default)]
pub struct QuadCombiner {
    groups: [Group; 3],
}

impl QuadCombiner {
    pub const fn new() -> Self {
        Self {
            groups: [const { Group::new() }; 3],
        }
    }

    pub fn add(&mut self, quad: Quad, alpha: Transparency, normal: Normal) {
        self.groups[alpha as usize].push(quad.0, normal)
    }

    pub fn clear_all(&mut self) {
        self.groups.iter_mut().for_each(|group| group.clear_all())
    }

    pub fn combine(&mut self, alpha: Transparency) -> Option<Mesh> {
        self.combine_on_axes(AxisMask::full(), true, alpha)
    }

    pub fn combine_on_axes(
        &mut self,
        axes: AxisMask,
        unaligned: bool,
        alpha: Transparency,
    ) -> Option<Mesh> {
        let group = &mut self.groups[alpha as usize];
        group.combine_on_axes(axes);
        group.build_on_axes(axes, unaligned)
    }
}

#[derive(Default)]
struct Group {
    aligned: AxisArray<Vec<[Vertex; 4]>>,
    unaligned: Vec<[Vertex; 4]>,
    normals: Vec<[i8; 4]>,
}

impl Group {
    const fn new() -> Self {
        Self {
            aligned: AxisArray::new([const { Vec::new() }; 6]),
            unaligned: Vec::new(),
            normals: Vec::new(),
        }
    }

    fn push(&mut self, verts: [Vertex; 4], norm: Normal) {
        match norm {
            Normal::Aligned(axis) => self.aligned[axis].push(verts),
            Normal::Unaligned(norm) => {
                self.unaligned.push(verts);
                self.normals.push([norm[0], norm[1], norm[2], 6]);
            }
        }
    }

    fn clear_all(&mut self) {
        self.aligned.values_mut().for_each(|vec| vec.clear());
        self.unaligned.clear();
        self.normals.clear();
    }

    /// Execute quad combination
    fn combine_on_axes(&mut self, axes: AxisMask) {
        for (axis, quads) in self.aligned.iter_mut_in(axes) {
            combiner_fn::combine_quads(quads, axis);
        }
    }

    fn num_quads(&self, axes: AxisMask, unaligned: bool) -> usize {
        self.aligned
            .values_in(axes)
            .map(|buf| buf.len())
            .sum::<usize>()
            + if unaligned { self.unaligned.len() } else { 0 }
    }

    /// Construct a mesh from the (already combined!!!) quads.
    fn build_on_axes(&mut self, axes: AxisMask, unaligned: bool) -> Option<Mesh> {
        // count total number of quads
        let num_quads = self.num_quads(axes, unaligned);
        if num_quads == 0 {
            return None;
        }

        // allocate space in the output
        let num_verts = num_quads * 4;
        let mut verts = Vec::with_capacity(num_verts);
        let mut norms = Vec::with_capacity(num_verts);

        // build vertex and normal buffer
        if unaligned {
            verts.extend_from_slice(self.unaligned.as_flattened());
            norms.extend_from_slice(&self.normals);
        }
        for (axis, quads) in self.aligned.iter_in(axes) {
            verts.extend_from_slice(quads.as_flattened());
            let norm = Normal::Aligned(axis).to_array();
            norms.resize(norms.len() + quads.len() * 4, norm);
        }

        // Build index buffer.
        let idx = if num_verts > 65535 {
            let mut idx = Vec::<u32>::with_capacity(num_quads * 6);
            for i in 0..num_quads as u32 {
                let base = i * 4;
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3])
            }
            Indices::U32(idx)
        } else {
            let mut idx = Vec::<u16>::with_capacity(num_quads * 6);
            for i in 0..num_quads as u16 {
                let base = i * 4;
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3])
            }
            Indices::U16(idx)
        };

        // we have to do this because bytemuck refuses to cast Vec<Vertex> to Vec<[i16; 4]> because
        // I've aligned Vertex to 8-bytes to make copying efficient, and [i16; 4] is aligned to 2 bytes.
        let verts = unsafe {
            debug_assert_eq!(
                std::mem::size_of::<Vertex>(),
                std::mem::size_of::<[i16; 4]>()
            );
            let (ptr, len, cap) = verts.into_raw_parts();
            Vec::from_raw_parts(ptr as *mut [i16; 4], len, cap)
        };

        // put it all together
        Some(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
                .with_inserted_indices(idx)
                .with_inserted_attribute(
                    MeshVertexAttribute::new("voxel_pos", 0, VertexFormat::Sint16x4),
                    VertexAttributeValues::Sint16x4(verts),
                )
                .with_inserted_attribute(
                    MeshVertexAttribute::new("voxel_norm", 1, VertexFormat::Snorm8x4),
                    VertexAttributeValues::Snorm8x4(norms),
                ),
        )
    }
}

mod combiner_fn {

    // Sorting algorithm: IpnSort via `sort_unstable`.
    // Storage: quad buffer, no sort buffer.

    use data::blockstates::quad::Vertex;
    use math::axis::Axis;

    pub fn combine_quads(quads: &mut Vec<[Vertex; 4]>, axis: Axis) {
        if quads.len() < 2 {
            return;
        }
        match axis {
            Axis::PosX => {
                sort::<3, 0, 2, 1>(quads);
                combine::<0, 3, 1, 2>(quads);
                if quads.len() < 2 {
                    return;
                }
                sort::<3, 0, 1, 2>(quads);
                combine::<1, 0, 2, 3>(quads);
            }
            Axis::NegX => {
                sort::<2, 0, 2, 1>(quads);
                combine::<0, 3, 1, 2>(quads);
                if quads.len() < 2 {
                    return;
                }
                sort::<2, 0, 1, 2>(quads);
                combine::<0, 1, 3, 2>(quads);
            }
            Axis::PosY => {
                sort::<3, 1, 0, 2>(quads);
                combine::<0, 3, 1, 2>(quads);
                if quads.len() < 2 {
                    return;
                }
                sort::<3, 1, 2, 0>(quads);
                combine::<1, 0, 2, 3>(quads);
            }
            Axis::NegY => {
                sort::<2, 1, 0, 2>(quads);
                combine::<0, 3, 1, 2>(quads);
                if quads.len() < 2 {
                    return;
                }
                sort::<2, 1, 2, 0>(quads);
                combine::<0, 1, 3, 2>(quads);
            }
            Axis::PosZ => {
                sort::<3, 2, 0, 1>(quads);
                combine::<0, 3, 1, 2>(quads);
                if quads.len() < 2 {
                    return;
                }
                sort::<3, 2, 1, 0>(quads);
                combine::<1, 0, 2, 3>(quads);
            }
            Axis::NegZ => {
                sort::<2, 2, 0, 1>(quads);
                combine::<0, 3, 1, 2>(quads);
                if quads.len() < 2 {
                    return;
                }
                sort::<2, 2, 1, 0>(quads);
                combine::<0, 1, 3, 2>(quads);
            }
        }
    }

    fn sort<
        const SV: usize, // index of vertex to sort on
        const A1: usize, // index of least significant dimension
        const A2: usize, // index of middle dimension
        const A3: usize, // index of linear dimension
    >(
        quads: &mut Vec<[Vertex; 4]>,
    ) {
        quads.sort_unstable_by(|a, b| {
            let (a, b) = (a[SV].pos, b[SV].pos);
            if a[A1] != b[A1] {
                return a[A1].cmp(&b[A1]);
            }
            if a[A2] != b[A2] {
                return a[A2].cmp(&b[A2]);
            }
            a[A3].cmp(&b[A3])
        });
    }

    fn combine<const A1: usize, const B1: usize, const A2: usize, const B2: usize>(
        quads: &mut Vec<[Vertex; 4]>,
    ) {
        let mut i = 0;
        let mut j = 1;
        let mut k = 0;

        loop {
            if quads[j - 1][A1] != quads[j][B1] || quads[j - 1][A2] != quads[j][B2] {
                quads[k][A1] = quads[j - 1][A1];
                quads[k][A2] = quads[j - 1][A2];
                quads[k][B1] = quads[i][B1];
                quads[k][B2] = quads[i][B2];
                k += 1;
                i = j;
            }

            j += 1;
            if j == quads.len() {
                quads[k][A1] = quads[j - 1][A1];
                quads[k][A2] = quads[j - 1][A2];
                quads[k][B1] = quads[i][B1];
                quads[k][B2] = quads[i][B2];
                break;
            }
        }

        debug_assert!(k < quads.len());
        unsafe { quads.set_len(k + 1) }
    }
}
