#import bevy_pbr::forward_io::VertexOutput

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

@group(3) @binding(0) var<uniform> settings: WaterSettings;

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

fn fbm(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.1;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.1;
    val += amp * simplex2d(pos); amp *= 0.5; pos *= 2.1;
    val += amp * simplex2d(pos);
    return val;
}

// ── Fragment shader ──

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world_pos = in.world_position.xyz;
    let normal = normalize(in.world_normal);
    let t = settings.time * settings.wave_speed;

    // Animated noise for surface detail — two scrolling layers
    let noise_uv1 = world_pos.xz * 0.04 + vec2<f32>(t * 0.05, t * 0.03);
    let noise_uv2 = world_pos.xz * 0.08 + vec2<f32>(-t * 0.03, t * 0.06);
    let surface_noise = fbm(noise_uv1) * 0.5 + fbm(noise_uv2) * 0.3;

    // Depth-based color variation
    let depth_factor = clamp(surface_noise * 0.5 + 0.5, 0.0, 1.0);
    let water_color = mix(settings.shallow_color.rgb, settings.deep_color.rgb, depth_factor);

    // Fresnel effect — brighter at glancing angles
    let view_dir = normalize(settings.camera_position.xyz - world_pos);

    // Perturb normal with animated noise for wave-like appearance
    let eps_val = 0.5;
    let n0 = fbm(world_pos.xz * 0.1 + vec2<f32>(t * 0.04, t * 0.03));
    let nx = fbm((world_pos.xz + vec2<f32>(eps_val, 0.0)) * 0.1 + vec2<f32>(t * 0.04, t * 0.03));
    let nz = fbm((world_pos.xz + vec2<f32>(0.0, eps_val)) * 0.1 + vec2<f32>(t * 0.04, t * 0.03));
    let wave_normal = normalize(normal + vec3<f32>((n0 - nx) * 0.5, 0.0, (n0 - nz) * 0.5));

    let fresnel = pow(1.0 - max(dot(wave_normal, view_dir), 0.0), 3.0);
    let fresnel_color = mix(water_color, vec3<f32>(0.6, 0.75, 0.85), fresnel * 0.4);

    // Specular highlight (sun reflection)
    let sun_dir = normalize(settings.sun_direction.xyz);
    let reflect_dir = reflect(-sun_dir, wave_normal);
    let spec = pow(max(dot(reflect_dir, view_dir), 0.0), 64.0);
    let color_with_spec = fresnel_color + settings.specular_color.rgb * spec * 0.5;

    // Animated caustic-like sparkle
    let sparkle = simplex2d(world_pos.xz * 0.5 + vec2<f32>(t * 0.3, t * 0.2));
    let sparkle_intensity = smoothstep(0.6, 0.8, sparkle) * 0.15;

    return vec4<f32>(color_with_spec + sparkle_intensity, settings.opacity);
}
