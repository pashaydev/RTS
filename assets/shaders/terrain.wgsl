#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

// ── Extension uniforms & textures ──

struct TerrainSettings {
    detail_scale: f32,
    detail_strength: f32,
    snow_height: f32,
    amplitude: f32,
    tile_scale: f32,
    floor_blend: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: TerrainSettings;

@group(#{MATERIAL_BIND_GROUP}) @binding(101) var grass_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var grass_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var rock_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var rock_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var sand_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var sand_sampler: sampler;
// For terrain: snow texture. For floors (floor_blend > 0): reused as floor stone texture.
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var snow_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var snow_sampler: sampler;

// ── Noise ──

fn mod289_3(x: vec3<f32>) -> vec3<f32> { return x - floor(x * (1.0 / 289.0)) * 289.0; }
fn mod289_2(x: vec2<f32>) -> vec2<f32> { return x - floor(x * (1.0 / 289.0)) * 289.0; }
fn permute3(x: vec3<f32>) -> vec3<f32> { return mod289_3((x * 34.0 + 10.0) * x); }

fn simplex2d(v: vec2<f32>) -> f32 {
    let C = vec4<f32>(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
    var i = floor(v + dot(v, C.yy));
    let x0 = v - i + dot(i, C.xx);
    var i1: vec2<f32>;
    if x0.x > x0.y { i1 = vec2<f32>(1.0, 0.0); } else { i1 = vec2<f32>(0.0, 1.0); }
    var x12 = x0.xyxy + C.xxzz;
    x12 = vec4<f32>(x12.xy - i1, x12.zw);
    i = mod289_2(i);
    let p = permute3(permute3(i.y + vec3<f32>(0.0, i1.y, 1.0)) + i.x + vec3<f32>(0.0, i1.x, 1.0));
    var m = max(vec3<f32>(0.5) - vec3<f32>(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), vec3<f32>(0.0));
    m = m * m;
    m = m * m;
    let x_ = vec3<f32>(2.0 * fract(p * C.www) - 1.0);
    let h = abs(x_) - 0.5;
    let ox = floor(x_ + 0.5);
    let a0 = x_ - ox;
    m = m * (1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h));
    let g0 = a0.x * x0.x + h.x * x0.y;
    let g12 = vec2<f32>(a0.y * x12.x + h.y * x12.y, a0.z * x12.z + h.z * x12.w);
    return 130.0 * dot(m, vec3<f32>(g0, g12));
}

fn fbm3(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.1;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.1;
    val += amp * simplex2d(pos);
    return val;
}

fn triplanar_weights(normal: vec3<f32>) -> vec3<f32> {
    let blend = pow(abs(normal), vec3<f32>(6.0));
    return blend / max(dot(blend, vec3<f32>(1.0, 1.0, 1.0)), 0.0001);
}

fn triplanar_sample(
    tex: texture_2d<f32>,
    tex_sampler: sampler,
    world_pos: vec3<f32>,
    blend_weights: vec3<f32>,
    scale: f32,
) -> vec3<f32> {
    let sample_x = textureSample(tex, tex_sampler, world_pos.zy * scale).rgb;
    let sample_y = textureSample(tex, tex_sampler, world_pos.xz * scale).rgb;
    let sample_z = textureSample(tex, tex_sampler, world_pos.xy * scale).rgb;
    return sample_x * blend_weights.x + sample_y * blend_weights.y + sample_z * blend_weights.z;
}

// ── Fragment shader ──

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;
    let world_pos = in.world_position.xyz;
    let vertex_color = in.color;
    let normal = normalize(in.world_normal);
    let tri_weights = triplanar_weights(normal);

    // ── Terrain texture sampling ──
    let tex_grass = triplanar_sample(grass_texture, grass_sampler, world_pos, tri_weights, settings.tile_scale);
    let tex_rock  = triplanar_sample(rock_texture, rock_sampler, world_pos, tri_weights, settings.tile_scale);
    let tex_sand  = triplanar_sample(sand_texture, sand_sampler, world_pos, tri_weights, settings.tile_scale);
    let tex_snow  = triplanar_sample(snow_texture, snow_sampler, world_pos, tri_weights, settings.tile_scale);
    // WebGPU requires implicit-derivative texture sampling to happen in uniform control flow.
    // Pre-sample the floor textures before any data-dependent branches.
    let floor_uv = world_pos.xz * 0.3;
    let tex_floor_snow = textureSample(snow_texture, snow_sampler, floor_uv).rgb;
    let tex_floor_rock = textureSample(rock_texture, rock_sampler, floor_uv).rgb;

    // ── Biome blend weights derived from vertex color ──
    let r = vertex_color.r;
    let g = vertex_color.g;
    let b = vertex_color.b;

    // Green-dominant = grass (Grassland, Forest, Wetland)
    let grass_w = max(g - max(r, b), 0.0);
    // Red/warm-dominant = sand (Desert, Beach)
    let sand_w = max(r - g * 0.7, 0.0);
    // Low-chroma gray = rock (Mountain)
    let avg = (r + g + b) / 3.0;
    let chroma = max(max(r, g), b) - min(min(r, g), b);
    let rock_w = max(avg * 0.5 - chroma, 0.0) * 2.0;
    // Normalize weights (epsilon to avoid div by zero)
    let total_w = grass_w + sand_w + rock_w + 0.001;

    let biome_tex = (tex_grass * grass_w + tex_sand * sand_w + tex_rock * rock_w) / total_w;

    // Multiply-blend: texture provides detail, vertex color provides hue
    var color = vertex_color.rgb * (biome_tex * 0.6 + 0.7);

    // Micro-detail noise — additional breakup
    let detail_uv = world_pos.xz * settings.detail_scale;
    let detail = simplex2d(detail_uv) * settings.detail_strength * 0.5;
    let detail2 = simplex2d(detail_uv * 3.7 + vec2<f32>(42.0, 17.0)) * settings.detail_strength * 0.2;
    color = color + detail + detail2;

    // Slope-based darkening (steeper = darker)
    let slope = 1.0 - max(dot(normal, vec3<f32>(0.0, 1.0, 0.0)), 0.0);
    let slope_darken = 1.0 - smoothstep(0.3, 0.8, slope) * 0.35;
    color = color * slope_darken;

    // Grass stipple for green areas (detect via green channel dominance)
    let is_green = step(vertex_color.r + 0.05, vertex_color.g) * step(vertex_color.b + 0.05, vertex_color.g);
    let grass_noise = simplex2d(world_pos.xz * 2.5) * 0.06 * is_green;
    let grass_noise2 = simplex2d(world_pos.xz * 8.0 + vec2<f32>(100.0, 200.0)) * 0.03 * is_green;
    color.g = color.g + grass_noise + grass_noise2;

    // Height-based snow on mountains — use snow texture
    let snow_blend = smoothstep(settings.snow_height, settings.snow_height + 4.0, world_pos.y);
    let snow_noise = simplex2d(world_pos.xz * 0.3) * 0.1;
    let snow_tex_color = tex_snow * (0.85 + snow_noise * 0.15);
    color = mix(color, snow_tex_color, snow_blend * (1.0 - slope * 0.5));

    // Bump-mapped normal perturbation for lighting (subtle)
    let eps_val = 0.5;
    let h0 = fbm3(world_pos.xz * settings.detail_scale);
    let hx = fbm3((world_pos.xz + vec2<f32>(eps_val, 0.0)) * settings.detail_scale);
    let hz = fbm3((world_pos.xz + vec2<f32>(0.0, eps_val)) * settings.detail_scale);
    let bump_strength = 0.05;
    let perturbed_normal = normalize(normal + vec3<f32>(
        (h0 - hx) * bump_strength,
        0.0,
        (h0 - hz) * bump_strength
    ));

    // ── Floor blending ──
    if settings.floor_blend > 0.0 {
        // Floor tile mesh: vertex_color.a encodes edge distance (0 = edge, 1 = center).
        // Floor texture is passed via the snow_texture slot (floors don't need snow).
        let edge_raw = vertex_color.a;
        let edge_factor = smoothstep(0.0, 0.6, edge_raw);

        let blend_noise = simplex2d(world_pos.xz * 3.0) * 0.08;
        let blend_t = clamp(edge_factor + blend_noise, 0.0, 1.0);

        let floor_detail = 1.0 + simplex2d(world_pos.xz * 6.0) * 0.06;
        let floor_color = tex_floor_snow * floor_detail * 0.95;

        color = mix(color, floor_color, blend_t);
    } else {
        // Main terrain: vertex_color.a < 1.0 signals floor blend amount.
        // 0.0 = full floor, 1.0 = no floor (default).
        let floor_alpha = vertex_color.a;
        if floor_alpha < 0.99 {
            let floor_t = 1.0 - floor_alpha;
            // Add noise to break up the blend seam for organic edges
            let blend_noise = simplex2d(world_pos.xz * 3.0) * 0.12;
            let blend_t = clamp(floor_t + blend_noise, 0.0, 1.0);

            // Warm stone tint applied to rock texture for a paved look
            let stone_tint = vec3<f32>(0.55, 0.50, 0.42);
            let floor_detail = 1.0 + simplex2d(world_pos.xz * 6.0) * 0.06;
            let floor_color = tex_floor_rock * stone_tint * floor_detail * 1.6;

            color = mix(color, floor_color, blend_t);
        }
    }

    // ── Hook into Bevy PBR pipeline ──
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Override base color with our computed terrain color
    pbr_input.material.base_color = vec4<f32>(color, 1.0);

    // Apply bump-mapped normal for PBR lighting
    pbr_input.N = perturbed_normal;
    pbr_input.world_normal = perturbed_normal;

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
