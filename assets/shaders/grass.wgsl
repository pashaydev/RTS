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

// ── Hash helper for per-blade randomness ──

fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// ── Vertex shader ──

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    // Per-blade packed data from merge function
    // vertex.position = blade base world position (same for all 3 verts)
    // vertex.color.x = baked y_rot, vertex.color.y = scale
    let base_pos = vertex.position;

#ifdef VERTEX_COLORS
    let baked_yaw = vertex.color.x;
    let scale = vertex.color.y;
#else
    let baked_yaw = 0.0;
    let scale = 1.0;
#endif

#ifdef VERTEX_UVS_A
    let height_pct = vertex.uv.y; // 0=root, 1=tip
    let width_pct = vertex.uv.x;  // 0=left, 1=right, 0.5=center
    out.uv = vertex.uv;
#else
    let height_pct = 0.0;
    let width_pct = 0.5;
#endif

    // Pass height_pct to fragment via UV_B channel (avoids conflict with PBR vertex colors)
#ifdef VERTEX_UVS_B
    out.uv_b = vec2<f32>(height_pct, 0.0);
#endif

    // Per-blade randomness from world XZ hash
    let base_xz = base_pos.xz;
    let rand_yaw = hash12(base_xz);
    let rand_lean = hash12(base_xz + vec2<f32>(127.1, 311.7));
    let rand_scale_var = hash12(base_xz + vec2<f32>(73.7, 157.3));

    // Small random yaw perturbation (+-15 degrees = +-0.26 rad)
    let extra_yaw = (rand_yaw - 0.5) * 0.52;
    let total_yaw = baked_yaw + extra_yaw;

    // Per-blade lean direction
    let lean_dir = (rand_lean - 0.5) * 2.0;

    // Minor per-blade scale variation
    let final_scale = scale * (0.9 + rand_scale_var * 0.2);

    // Blade local axes from total yaw
    let cy = cos(total_yaw);
    let sy = sin(total_yaw);
    let blade_right = vec3<f32>(cy, 0.0, -sy);
    let blade_forward = vec3<f32>(sy, 0.0, cy);

    // View-angle width thickening
    let cam_pos = view.world_position.xyz;
    let to_cam_xz = normalize(cam_pos.xz - base_xz);
    let blade_right_xz = vec2<f32>(cy, -sy);
    let edge_on = 1.0 - abs(dot(to_cam_xz, blade_right_xz));
    let thicken = 1.0 + edge_on * settings.width_thicken;

    // Reconstruct blade vertex in world space from UVs + uniforms
    let w = settings.blade_width * final_scale * thicken;
    let h = settings.blade_height * final_scale;

    // Width narrows toward tip: root is full width, tip is zero
    let width_offset = (width_pct - 0.5) * w * (1.0 - height_pct);

    // Lean: forward offset grows with height
    let lean_offset = lean_dir * settings.random_lean * height_pct * height_pct;

    var world_pos = base_pos;
    world_pos += blade_right * width_offset;
    world_pos.y += h * height_pct;
    world_pos += blade_forward * lean_offset;

    // Wind sway — two overlapping sine gusts + per-blade flutter
    let t = settings.time * settings.wind_speed;
    let wind_dir = normalize(settings.wind_direction.xy);
    let hf = height_pct * height_pct; // quadratic so roots stay planted

    let gust = sin(dot(base_xz, wind_dir) * 0.15 + t) * 0.6
             + sin(dot(base_xz, wind_dir * 1.3) * 0.4 + t * 1.7) * 0.4;
    let flutter = sin(base_xz.x * 3.7 + base_xz.y * 2.3 + t * 2.5) * 0.3;
    let wind_disp = (gust + flutter) * settings.wind_strength * hf;

    world_pos.x += wind_disp * wind_dir.x;
    world_pos.z += wind_disp * wind_dir.y;
    world_pos.y -= abs(wind_disp) * 0.3;

    // Transform through Bevy pipeline (model is identity for grass chunks)
    let model = get_world_from_local(vertex.instance_index);
    out.world_position = mesh_position_local_to_world(model, vec4<f32>(world_pos, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);

    // Normal: biased upward, blended to pure-up at distance
    let face_normal = normalize(cross(blade_forward, blade_right));
    let biased_normal = normalize(mix(face_normal, vec3<f32>(0.0, 1.0, 0.0), settings.normal_up_bias));
    let dist = length(cam_pos - world_pos);
    let blend = smoothstep(settings.normal_blend_start, settings.normal_blend_end, dist);
    out.world_normal = normalize(mix(biased_normal, vec3<f32>(0.0, 1.0, 0.0), blend));

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

    // Base-to-tip gradient
    let grass_color = mix(
        settings.base_color.rgb,
        settings.tip_color.rgb,
        pow(height_pct, 1.4)
    );

    // Root darkening
    let root_darken = mix(0.4, 1.0, smoothstep(0.0, 0.25, height_pct));

    let color = grass_color * root_darken;

    // Hook into Bevy PBR pipeline for proper scene lighting
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = vec4<f32>(color, 1.0);
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
