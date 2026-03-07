#import bevy_pbr::forward_io::VertexOutput

struct HoverRingSettings {
    color: vec4<f32>,
    time: f32,
    ring_width: f32,
    ring_radius: f32,
    _padding: f32,
};

@group(3) @binding(0) var<uniform> settings: HoverRingSettings;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // UV is 0..1 across the plane, center at 0.5
    let uv = mesh.uv - vec2<f32>(0.5);
    let dist = length(uv) * 2.0; // 0..1 from center to edge

    let time = settings.time;
    let radius = settings.ring_radius;
    let width = settings.ring_width;

    // Main ring
    let ring_edge = smoothstep(radius - width, radius - width * 0.5, dist)
                  * (1.0 - smoothstep(radius + width * 0.5, radius + width, dist));

    // Pulse wave expanding outward
    let pulse_phase = fract(time * 0.8);
    let pulse_radius = pulse_phase * 1.0;
    let pulse_width = 0.04;
    let pulse = smoothstep(pulse_radius - pulse_width, pulse_radius, dist)
              * (1.0 - smoothstep(pulse_radius, pulse_radius + pulse_width, dist))
              * (1.0 - pulse_phase); // fade as it expands

    // Inner glow (subtle fill)
    let inner_glow = (1.0 - smoothstep(0.0, radius, dist)) * 0.08;

    // Combine
    let alpha = (ring_edge * 0.8 + pulse * 0.5 + inner_glow) * settings.color.a;

    if alpha < 0.01 {
        discard;
    }

    let glow = ring_edge + pulse * 0.6;
    let final_color = settings.color.rgb * (1.0 + glow * 0.5);

    return vec4<f32>(final_color, alpha);
}
