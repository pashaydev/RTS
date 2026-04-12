#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

#import bevy_pbr::mesh_functions::{
    get_world_from_local,
    mesh_position_local_to_world,
}
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::mesh_view_bindings::view

// ── Uniforms ──

struct GrassSettings {
    time: f32,
    wind_strength: f32,
    wind_speed: f32,
    random_lean: f32,
    wind_direction: vec4<f32>,
    base_color: vec4<f32>,
    tip_color: vec4<f32>,
    blade_width: f32,
    blade_height: f32,
    width_thicken: f32,
    normal_up_bias: f32,
    normal_blend_start: f32,
    normal_blend_end: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: GrassSettings;

struct BladeVertexData {
    base_pos: vec3<f32>,
    height_pct: f32,
    width_pct: f32,
    baked_yaw: f32,
    base_scale: f32,
};

struct BladeRandomness {
    total_yaw: f32,
    lean_dir: f32,
    final_scale: f32,
    blade_right: vec3<f32>,
    blade_forward: vec3<f32>,
};

struct BladeSurface {
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
};

// ── Helpers ──

fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn safe_normalize2(v: vec2<f32>, fallback: vec2<f32>) -> vec2<f32> {
    if dot(v, v) < 1e-6 {
        return fallback;
    }
    return normalize(v);
}

fn unpack_blade_vertex(vertex: Vertex) -> BladeVertexData {
#ifdef VERTEX_COLORS
    let baked_yaw = vertex.color.x;
    let base_scale = vertex.color.y;
#else
    let baked_yaw = 0.0;
    let base_scale = 1.0;
#endif

#ifdef VERTEX_UVS_A
    let height_pct = vertex.uv.y;
    let width_pct = vertex.uv.x;
#else
    let height_pct = 0.0;
    let width_pct = 0.5;
#endif

    return BladeVertexData(
        vertex.position,
        height_pct,
        width_pct,
        baked_yaw,
        base_scale,
    );
}

fn blade_randomness(base_xz: vec2<f32>, baked_yaw: f32, base_scale: f32) -> BladeRandomness {
    let rand_yaw = hash12(base_xz);
    let rand_lean = hash12(base_xz + vec2<f32>(127.1, 311.7));
    let rand_scale_var = hash12(base_xz + vec2<f32>(73.7, 157.3));

    let total_yaw = baked_yaw + (rand_yaw - 0.5) * 0.52;
    let lean_dir = (rand_lean - 0.5) * 2.0;
    let final_scale = base_scale * (0.9 + rand_scale_var * 0.2);

    let cy = cos(total_yaw);
    let sy = sin(total_yaw);
    let blade_right = vec3<f32>(cy, 0.0, -sy);
    let blade_forward = vec3<f32>(sy, 0.0, cy);

    return BladeRandomness(
        total_yaw,
        lean_dir,
        final_scale,
        blade_right,
        blade_forward,
    );
}

fn width_thickening(cam_pos: vec3<f32>, base_xz: vec2<f32>, total_yaw: f32) -> f32 {
    let to_cam_xz = safe_normalize2(cam_pos.xz - base_xz, vec2<f32>(0.0, 1.0));
    let blade_right_xz = vec2<f32>(cos(total_yaw), -sin(total_yaw));
    let edge_on = 1.0 - abs(dot(to_cam_xz, blade_right_xz));
    return 1.0 + edge_on * settings.width_thicken;
}

fn apply_wind(base_xz: vec2<f32>, height_pct: f32) -> vec3<f32> {
    let t = settings.time * settings.wind_speed;
    let wind_dir = safe_normalize2(settings.wind_direction.xy, vec2<f32>(1.0, 0.0));
    let height_factor = height_pct * height_pct;

    let gust = sin(dot(base_xz, wind_dir) * 0.15 + t) * 0.6
             + sin(dot(base_xz, wind_dir * 1.3) * 0.4 + t * 1.7) * 0.4;
    let flutter = sin(base_xz.x * 3.7 + base_xz.y * 2.3 + t * 2.5) * 0.3;
    let wind_disp = (gust + flutter) * settings.wind_strength * height_factor;

    return vec3<f32>(
        wind_disp * wind_dir.x,
        -abs(wind_disp) * 0.3,
        wind_disp * wind_dir.y,
    );
}

fn compute_blade_surface(vertex_data: BladeVertexData, cam_pos: vec3<f32>) -> BladeSurface {
    let base_xz = vertex_data.base_pos.xz;
    let random = blade_randomness(base_xz, vertex_data.baked_yaw, vertex_data.base_scale);
    let thicken = width_thickening(cam_pos, base_xz, random.total_yaw);

    let blade_width = settings.blade_width * random.final_scale * thicken;
    let blade_height = settings.blade_height * random.final_scale;
    let width_offset =
        (vertex_data.width_pct - 0.5) * blade_width * (1.0 - vertex_data.height_pct);
    let lean_offset =
        random.lean_dir * settings.random_lean * vertex_data.height_pct * vertex_data.height_pct;

    var world_pos = vertex_data.base_pos;
    world_pos += random.blade_right * width_offset;
    world_pos.y += blade_height * vertex_data.height_pct;
    world_pos += random.blade_forward * lean_offset;
    world_pos += apply_wind(base_xz, vertex_data.height_pct);

    let face_normal = normalize(cross(random.blade_forward, random.blade_right));
    let biased_normal =
        normalize(mix(face_normal, vec3<f32>(0.0, 1.0, 0.0), settings.normal_up_bias));
    let dist = distance(cam_pos, world_pos);
    let blend = smoothstep(settings.normal_blend_start, settings.normal_blend_end, dist);
    let world_normal = normalize(mix(biased_normal, vec3<f32>(0.0, 1.0, 0.0), blend));

    return BladeSurface(world_pos, world_normal);
}

fn grass_albedo(height_pct: f32) -> vec3<f32> {
    let gradient = mix(settings.base_color.rgb, settings.tip_color.rgb, pow(height_pct, 1.4));
    let root_darken = mix(0.4, 1.0, smoothstep(0.0, 0.25, height_pct));
    return gradient * root_darken;
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let blade = unpack_blade_vertex(vertex);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif

    // Pass height_pct to fragment via UV_B channel (avoids conflict with PBR vertex colors)
#ifdef VERTEX_UVS_B
    out.uv_b = vec2<f32>(blade.height_pct, 0.0);
#endif

    let cam_pos = view.world_position.xyz;
    let surface = compute_blade_surface(blade, cam_pos);

    // Transform through Bevy pipeline (model is identity for grass chunks)
    let model = get_world_from_local(vertex.instance_index);
    out.world_position = mesh_position_local_to_world(model, vec4<f32>(surface.world_pos, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);

    out.world_normal = surface.world_normal;

    // Neutral vertex color so PBR base_color multiplication is 1:1
    out.color = vec4<f32>(1.0, 1.0, 1.0, 1.0);

    // Pass instance_index through for PBR pipeline
    out.instance_index = vertex.instance_index;

    return out;
}

// ── Fragment shader ──

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#ifdef VERTEX_UVS_B
    let height_pct = in.uv_b.x;
#else
    let height_pct = 0.0;
#endif

    // Hook into Bevy PBR pipeline for proper scene lighting
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = vec4<f32>(grass_albedo(height_pct), 1.0);
    pbr_input.N = normalize(in.world_normal);
    pbr_input.world_normal = normalize(in.world_normal);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
