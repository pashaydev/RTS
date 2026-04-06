#import bevy_pbr::{
    pbr_types,
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
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
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}
#endif

const TREE_OCCLUSION_MAX_UNITS: u32 = 8u;

struct TreeOcclusionSettings {
    screen_masks: array<vec4<f32>, TREE_OCCLUSION_MAX_UNITS>,
    active_units: u32,
    _pad0: vec3<u32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> settings: TreeOcclusionSettings;

fn occlusion_visibility(screen_pos: vec2<f32>) -> f32 {
    var visibility = 1.0;
    for (var i: u32 = 0u; i < TREE_OCCLUSION_MAX_UNITS; i = i + 1u) {
        if (i >= settings.active_units) {
            break;
        }

        let mask = settings.screen_masks[i];
        let radius = mask.z;
        let feather = mask.w;
        let inner_radius = max(radius - feather, 0.0);
        let dist = distance(screen_pos, mask.xy);
        let mask_visibility = smoothstep(inner_radius, radius, dist);
        visibility = min(visibility, mask_visibility);
    }

    return visibility;
}

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let visibility = occlusion_visibility(in.position.xy);
    pbr_input.material.base_color.a *= visibility;
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
