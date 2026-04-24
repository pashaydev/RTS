#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}
#endif

struct WaterSettings {
    time: f32,
    wave_speed: f32,
    wave_scale: f32,
    opacity: f32,
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    specular_color: vec4<f32>,
    sun_direction: vec4<f32>,
    camera_position: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: WaterSettings;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var fog_visible_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var fog_visible_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var fog_explored_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var fog_explored_sampler: sampler;

fn mod289_3(x: vec3<f32>) -> vec3<f32> { return x - floor(x * (1.0 / 289.0)) * 289.0; }
fn mod289_2(x: vec2<f32>) -> vec2<f32> { return x - floor(x * (1.0 / 289.0)) * 289.0; }
fn permute3(x: vec3<f32>) -> vec3<f32> { return mod289_3((x * 34.0 + 10.0) * x); }

fn simplex2d(v: vec2<f32>) -> f32 {
    let c = vec4<f32>(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
    var i = floor(v + dot(v, c.yy));
    let x0 = v - i + dot(i, c.xx);
    var i1: vec2<f32>;
    if x0.x > x0.y { i1 = vec2<f32>(1.0, 0.0); } else { i1 = vec2<f32>(0.0, 1.0); }
    var x12 = x0.xyxy + c.xxzz;
    x12 = vec4<f32>(x12.xy - i1, x12.zw);
    i = mod289_2(i);
    let p = permute3(permute3(i.y + vec3<f32>(0.0, i1.y, 1.0)) + i.x + vec3<f32>(0.0, i1.x, 1.0));
    var m = max(vec3<f32>(0.5) - vec3<f32>(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), vec3<f32>(0.0));
    m = m * m;
    m = m * m;
    let x_ = vec3<f32>(2.0 * fract(p * c.www) - 1.0);
    let h = abs(x_) - 0.5;
    let ox = floor(x_ + 0.5);
    let a0 = x_ - ox;
    m = m * (1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h));
    let g0 = a0.x * x0.x + h.x * x0.y;
    let g12 = vec2<f32>(a0.y * x12.x + h.y * x12.y, a0.z * x12.z + h.z * x12.w);
    return 130.0 * dot(m, vec3<f32>(g0, g12));
}

fn fbm(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.03;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.07;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.11;
    val += amp * simplex2d(pos);
    return val;
}

fn wave_height(world_xz: vec2<f32>, t: f32) -> f32 {
    let large = fbm(world_xz * 0.030 + vec2<f32>(t * 0.030, t * 0.018));
    let medium = fbm(world_xz * 0.070 + vec2<f32>(-t * 0.042, t * 0.028));
    let ripples = simplex2d(world_xz * 0.180 + vec2<f32>(t * 0.090, -t * 0.070));
    return large * 0.70 + medium * 0.25 + ripples * 0.05;
}

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;
    let world_pos = in.world_position.xyz;
    let base_normal = normalize(in.world_normal);
    let t = settings.time * settings.wave_speed;

    let uv = in.uv;
    let fog_vis = textureSample(fog_visible_tex, fog_visible_sampler, uv).r;
    let fog_explored = textureSample(fog_explored_tex, fog_explored_sampler, uv).r;

    if fog_explored < 0.02 && fog_vis < 0.02 {
        discard;
    }

    let wave_strength = max(settings.wave_scale, 0.02);
    let eps = 0.65;
    let h0 = wave_height(world_pos.xz, t);
    let hx = wave_height(world_pos.xz + vec2<f32>(eps, 0.0), t);
    let hz = wave_height(world_pos.xz + vec2<f32>(0.0, eps), t);
    let wave_normal = normalize(base_normal + vec3<f32>(
        (h0 - hx) * wave_strength,
        0.0,
        (h0 - hz) * wave_strength
    ));

    let view_dir = normalize(settings.camera_position.xyz - world_pos);
    let view_facing = 1.0 - max(dot(wave_normal, view_dir), 0.0);
    let depth_noise = clamp(h0 * 0.5 + 0.5, 0.0, 1.0);
    let water_color = mix(settings.shallow_color.rgb, settings.deep_color.rgb, depth_noise);

    let sun_dir = normalize(settings.sun_direction.xyz);
    let sun_facing = max(dot(wave_normal, sun_dir), 0.0);
    let horizon_boost = smoothstep(0.15, 1.0, view_facing);
    let subsurface = mix(0.92, 1.08, sun_facing) + horizon_boost * 0.08;

    let sparkle_noise = simplex2d(world_pos.xz * 0.35 + vec2<f32>(t * 0.12, -t * 0.08));
    let glint_mask = smoothstep(0.72, 0.92, sparkle_noise) * sun_facing;

    let fog_factor = clamp(fog_vis, 0.0, 1.0);
    let explored_brightness = 0.30;
    let fog_brightness = mix(explored_brightness, 1.0, smoothstep(0.18, 0.62, fog_factor));
    let alpha = mix(settings.opacity * 0.45, settings.opacity, smoothstep(0.10, 0.55, fog_factor));

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = vec4<f32>(water_color * subsurface * fog_brightness, alpha);
    let reflectance = mix(vec3<f32>(0.18), settings.specular_color.rgb * 0.30, horizon_boost);
    pbr_input.material.reflectance = reflectance;
    pbr_input.material.metallic = 0.0;
    pbr_input.material.perceptual_roughness = clamp(0.06 - glint_mask * 0.025 + (1.0 - fog_factor) * 0.04, 0.025, 0.14);
    pbr_input.N = wave_normal;
    pbr_input.world_normal = wave_normal;

    alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
#endif
}
