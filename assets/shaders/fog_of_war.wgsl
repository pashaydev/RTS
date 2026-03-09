#import bevy_pbr::forward_io::VertexOutput

struct FogSettings {
    time: f32,
    noise_scale: f32,
    edge_glow_width: f32,
    edge_glow_intensity: f32,
    fog_color: vec4<f32>,
    glow_color: vec4<f32>,
    explored_tint: vec4<f32>,
    // Unexplored fog noise controls
    fog_noise_scale: f32,
    fog_noise_speed: f32,
    fog_noise_warp: f32,
    fog_noise_contrast: f32,
    fog_noise_octaves: f32,
    fog_tendril_scale: f32,
    fog_tendril_strength: f32,
    fog_warp_speed: f32,
};

@group(3) @binding(0) var<uniform> settings: FogSettings;
@group(3) @binding(1) var visible_tex: texture_2d<f32>;
@group(3) @binding(2) var visible_sampler: sampler;
@group(3) @binding(3) var explored_tex: texture_2d<f32>;
@group(3) @binding(4) var explored_sampler: sampler;

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
    x12 = vec4<f32>(x12.x - i1.x, x12.y - i1.y, x12.z, x12.w);
    i = mod289_2(i);
    let p = permute3(permute3(i.y + vec3<f32>(0.0, i1.y, 1.0)) + i.x + vec3<f32>(0.0, i1.x, 1.0));
    var m = max(0.5 - vec3<f32>(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), vec3<f32>(0.0));
    m = m * m; m = m * m;
    let x = 2.0 * fract(p * C.www) - 1.0;
    let h = abs(x) - 0.5;
    let a0 = x - floor(x + 0.5);
    m = m * (1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h));
    return 130.0 * dot(m, vec3<f32>(a0.x * x0.x + h.x * x0.y, a0.y * x12.x + h.y * x12.y, a0.z * x12.z + h.z * x12.w));
}

fn fbm(p: vec2<f32>, octaves: i32) -> f32 {
    var value = 0.0; var amplitude = 0.5; var pos = p;
    let rot = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    for (var i = 0; i < octaves; i++) {
        value += amplitude * simplex2d(pos);
        pos = rot * pos * 2.02; amplitude *= 0.5;
    }
    return value;
}

fn domain_warp(p: vec2<f32>, t: f32) -> vec2<f32> {
    let q = vec2<f32>(fbm(p + t * 0.02, 3), fbm(p + vec2<f32>(5.2, 1.3) + t * 0.015, 3));
    return vec2<f32>(
        fbm(p + 4.0 * q + vec2<f32>(1.7, 9.2) + t * 0.01, 3),
        fbm(p + 4.0 * q + vec2<f32>(8.3, 2.8) + t * 0.008, 3)
    );
}

// ── Fragment ──

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;
    let time = settings.time;

    // Sample two independent layers
    // visible_tex: smoothed display (0=hidden, ~0.35=explored-only, 0.5-1.0=visible)
    // explored_tex: permanent binary (0=never seen, 1=explored)
    let vis = textureSample(visible_tex, visible_sampler, uv).r;
    let explored = textureSample(explored_tex, explored_sampler, uv).r;

    // ── Noise distortion at boundaries for organic edges ──
    let noise_uv = uv * settings.noise_scale + time * 0.03;
    let noise_val = fbm(noise_uv, 3);

    let vis_boundary = smoothstep(0.0, 0.15, vis) * (1.0 - smoothstep(0.7, 1.0, vis));
    let explore_boundary = smoothstep(0.0, 0.1, explored) * (1.0 - smoothstep(0.8, 1.0, explored));
    let boundary = max(vis_boundary, explore_boundary);
    let distorted_vis = clamp(vis + noise_val * 0.12 * boundary, 0.0, 1.0);

    // ── Three-zone classification ──
    // Zone 1: Unexplored (never seen)
    let unexplored_factor = 1.0 - smoothstep(0.0, 0.15, explored);
    // Zone 2: Explored but not currently visible
    let explored_only = smoothstep(0.0, 0.15, explored) * (1.0 - smoothstep(0.4, 0.7, distorted_vis));
    // Zone 3: Currently visible (clear)
    // (visible_factor used implicitly as alpha goes to 0)

    // ── Alpha per zone ──
    // Unexplored = opaque fog, explored = subtle darkening, visible = transparent
    let explored_pulse = 0.85 + 0.15 * sin(time * 0.5 + uv.x * 3.0 + uv.y * 2.0);
    let alpha = unexplored_factor * settings.fog_color.a
              + explored_only * settings.explored_tint.a * explored_pulse;

    if alpha < 0.005 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // ── Color blending ──
    let color_blend = explored_only / max(unexplored_factor + explored_only, 0.001);
    var fog_rgb = mix(settings.fog_color.rgb, settings.explored_tint.rgb, color_blend);

    // ── Animated swirling in unexplored fog ──
    let octaves = i32(clamp(settings.fog_noise_octaves, 1.0, 6.0));
    if unexplored_factor > 0.01 {
        // Domain warp for organic flowing patterns
        let warp_uv = uv * (settings.fog_noise_scale * 0.6);
        let warp = domain_warp(warp_uv, time * settings.fog_warp_speed);

        // Primary swirl noise
        let swirl_uv = uv * settings.fog_noise_scale + warp * settings.fog_noise_warp + time * settings.fog_noise_speed;
        let swirl = fbm(swirl_uv, octaves);              // range ~[-1, 1]
        let swirl_n = swirl * 0.5 + 0.5;                  // normalize to [0, 1]

        // Secondary layer at different scale for depth
        let detail = fbm(uv * settings.fog_noise_scale * 1.7 + warp * 0.3 + time * settings.fog_noise_speed * 0.6, max(octaves - 1, 1));
        let detail_n = detail * 0.5 + 0.5;

        let combined = mix(swirl_n, detail_n, 0.3);

        // Multiplicative darkening: carve dark channels into the fog
        let darken = 1.0 - settings.fog_noise_contrast * (1.0 - combined);
        fog_rgb = fog_rgb * darken;

        // Additive glow highlights: bright wisps using glow_color
        let highlights = pow(combined, 2.5) * settings.fog_noise_contrast * 1.5;
        fog_rgb = fog_rgb + settings.glow_color.rgb * highlights * unexplored_factor;
    }

    // ── Mist tendrils at explored/unexplored boundary ──
    if explore_boundary > 0.1 {
        let tendril_uv = uv * settings.fog_tendril_scale + time * 0.015;
        let warp2 = domain_warp(tendril_uv, time * settings.fog_warp_speed * 0.7);
        let tendril = fbm(tendril_uv + warp2 * 1.2, octaves);
        let tendril_n = tendril * 0.5 + 0.5;
        let t_strength = explore_boundary * settings.fog_tendril_strength * tendril_n;
        fog_rgb = fog_rgb + settings.glow_color.rgb * t_strength;
    }

    // ── Edge glow at visible boundary ──
    let glow_center = 0.55;
    let glow_dist = (distorted_vis - glow_center) / max(settings.edge_glow_width, 0.001);
    let glow_factor = exp(-glow_dist * glow_dist);
    let glow_strength = glow_factor * settings.edge_glow_intensity * settings.glow_color.a;
    fog_rgb = fog_rgb + settings.glow_color.rgb * glow_strength;

    return vec4<f32>(fog_rgb, alpha);
}
