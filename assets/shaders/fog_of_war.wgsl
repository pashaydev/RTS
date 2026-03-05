#import bevy_pbr::forward_io::VertexOutput

struct FogSettings {
    time: f32,
    noise_scale: f32,
    edge_glow_width: f32,
    edge_glow_intensity: f32,
    fog_color: vec4<f32>,
    glow_color: vec4<f32>,
    explored_tint: vec4<f32>,
};

@group(3) @binding(0) var<uniform> settings: FogSettings;
@group(3) @binding(1) var visibility_tex: texture_2d<f32>;
@group(3) @binding(2) var visibility_sampler: sampler;

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

    // Sample visibility: 0 = unexplored, 0.5 = explored, 1.0 = fully visible
    let vis = textureSample(visibility_tex, visibility_sampler, uv).r;

    // Noise distortion at the boundary — makes edges organic instead of circular
    let noise_val = fbm(uv * settings.noise_scale + time * 0.03, 3);
    // Only distort near the boundary (where vis is between 0.1 and 0.9)
    let boundary = smoothstep(0.0, 0.2, vis) * (1.0 - smoothstep(0.8, 1.0, vis));
    let distorted_vis = clamp(vis + noise_val * 0.15 * boundary, 0.0, 1.0);

    // Alpha: simple mapping. Visible = transparent, fog = opaque.
    // Use a sharp cutoff so visible areas are truly clear.
    let alpha = smoothstep(1.0, 0.3, distorted_vis) * 0.55;

    // Fully transparent — skip everything
    if alpha < 0.005 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Fog color: dark with subtle animated swirling in deep fog only
    var fog_rgb = settings.fog_color.rgb;
    if distorted_vis < 0.3 {
        let warp = domain_warp(uv * 3.0, time);
        let swirl = fbm(uv * 5.0 + warp * 0.8 + time * 0.01, 4);
        fog_rgb = fog_rgb * (0.8 + 0.2 * swirl);
    }

    return vec4<f32>(fog_rgb, alpha);
}
