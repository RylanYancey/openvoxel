
#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip
}

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) pos: vec4<i32>,
    @location(1) norm: vec4<f32>,
}

struct Fragment {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) texture: u32,
    @location(2) brightness: f32,
}

struct BlockTexture {
    @location(0) index: u32,
    @location(1) flags: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas_texture: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<storage, read> table: array<BlockTexture>;

@vertex
fn vertex(v: Vertex) -> Fragment {
    var out: Fragment;

    // get texture descriptor from table
    let desc = table[u32(v.pos.w)];

    // The input coordinates are in pixels, so we need to convert them
    // to a coordinate relative to the mesh origin.
    let pos = vec3<f32>(v.pos.xyz) / 16.0;

    // get clip position
    var world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let world_pos = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(pos, 1.0));
    out.clip_pos = position_world_to_clip(world_pos.xyz);

    // write descriptor vars to fragment
    out.texture = desc.index;

    // Infer UVs from world-space coordinates and the base brightness based on direction.
    out.uv = uv_from_normal(pos, v.norm.xyz);
    out.brightness = compute_brightness(v.norm.xyz);

    return out;
}

@fragment
fn fragment(f: Fragment) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, fract(f.uv), f.texture);
    return vec4<f32>(color.rgb * f.brightness, color.a);
}

fn uv_from_normal(pos: vec3<f32>, norm: vec3<f32>) -> vec2<f32> {
    let n = abs(norm);
    if n.x > n.y && n.x > n.z {
        // X face -> use Y/Z
        return vec2<f32>(pos.z, pos.y);
    } else if n.y > n.z {
        // Y face -> use X/Z
        return vec2<f32>(pos.x, pos.z);
    } else {
        // Z face -> use X/Y
        return vec2<f32>(pos.x, pos.y);
    }
}

fn compute_brightness(norm: vec3<f32>) -> f32 {
    let abs = abs(norm);
    let is_y_face = abs.y > 0.9;
    let is_x_face = abs.x > 0.9;
    let is_z_face = abs.z > 0.9;

    if (is_y_face) {
        if (norm.y > 0.0) {
            return 1.0;
        } else {
            return 0.6;
        }
    } else if (is_x_face) {
        return 0.85;
    } else if (is_z_face) {
        return 0.75;
    } else {
        return 0.8;
    }
}
