// Goblin impostor shader — yaw-billboards a unit-height quad toward the
// camera and samples one cell of `goblin_atlas.png`.
//
// Per-entity material: ImpostorMaterial (see materials/impostor.rs).
// Uniform layout matches `ImpostorParams`.
//
// Atlas layout (see scripts/bake_goblin_atlas.py):
//   8 columns (yaws) × 80 rows (states × 16 frames each). Column 0 is the
//   FRONT view (unit facing camera). Columns increase CCW around the unit's
//   up axis. Row layout: Idle[0..16], Walk[16..32], Run[32..48],
//   Attack[48..64], Death[64..80].

#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::view_transformations::position_world_to_clip

struct ImpostorParams {
    time: f32,
    time_phase: f32,
    yaw_facing: f32,
    size: f32,
    state_row_offset: u32,
    frame_count: u32,
    total_rows: u32,
    directions: u32,
    fps: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: ImpostorParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) direction: u32,
    @location(2) @interpolate(flat) frame: u32,
};

const PI: f32 = 3.1415926535897932;
const TWO_PI: f32 = 6.2831853071795864;
const EIGHTH: f32 = 0.7853981633974483; // π/4

@vertex
fn vertex(v: Vertex) -> VOut {
    // Pivot = world origin of the child entity = unit's feet (we place the
    // impostor quad as an identity-transform child of the unit).
    let model = get_world_from_local(v.instance_index);
    let pivot = model[3].xyz;

    // Yaw-only billboard basis: build right/up from the camera's horizontal
    // direction so the quad always faces the camera around the Y axis.
    let to_pivot = pivot - view.world_position.xyz;
    let horiz_len_sq = to_pivot.x * to_pivot.x + to_pivot.z * to_pivot.z;
    // Fallback if camera is exactly above the unit — pick an arbitrary
    // stable axis so the quad doesn't collapse.
    var horiz: vec3<f32>;
    if horiz_len_sq < 1e-6 {
        horiz = vec3<f32>(0.0, 0.0, 1.0);
    } else {
        horiz = normalize(vec3<f32>(to_pivot.x, 0.0, to_pivot.z));
    }
    let up = vec3<f32>(0.0, 1.0, 0.0);
    // right = up × (-horiz): camera-right on the world horizon.
    let right = normalize(cross(up, -horiz));

    // Quad mesh has local x ∈ [-0.5, 0.5], y ∈ [0, 1], z = 0.
    // Place y=0 at the pivot (feet) and scale uniformly by `size`.
    let world_pos = pivot
        + right * (v.position.x * params.size)
        + up * (v.position.y * params.size);

    // ── Direction index (0..7) ──
    // Unit's "forward" in world XZ. Bevy: Transform::forward() = rot * (0,0,-1),
    // so for a pure Y rotation of `yaw`, forward_xz = (-sin yaw, -cos yaw).
    let cy = cos(params.yaw_facing);
    let sy = sin(params.yaw_facing);
    let face = vec2<f32>(-sy, -cy);

    // Unit → camera direction in XZ plane.
    let to_cam = vec2<f32>(
        view.world_position.x - pivot.x,
        view.world_position.z - pivot.z,
    );
    let to_cam_len_sq = to_cam.x * to_cam.x + to_cam.y * to_cam.y;
    var to_cam_n: vec2<f32>;
    if to_cam_len_sq < 1e-6 {
        to_cam_n = vec2<f32>(0.0, 1.0);
    } else {
        to_cam_n = to_cam / sqrt(to_cam_len_sq);
    }

    // Signed angle from `face` to `to_cam` (CCW positive around +Y).
    // cross_z of (face → to_cam) is face.x*to_cam.y - face.y*to_cam.x.
    let cross_z = face.x * to_cam_n.y - face.y * to_cam_n.x;
    let dot_fc = face.x * to_cam_n.x + face.y * to_cam_n.y;
    // relative = 0 when unit faces camera (→ col 0 FRONT).
    // Negated so positive yaw offsets pick increasing column indices to the
    // CCW side (col 2 = screen-right, col 6 = screen-left) matching the
    // bake.
    let relative = atan2(-cross_z, dot_fc);

    // Normalize to [0, 2π), bin into 8 equally-spaced directions.
    var a = relative;
    if a < 0.0 {
        a = a + TWO_PI;
    }
    let col_f = a / EIGHTH + 0.5;
    var col = i32(floor(col_f));
    col = ((col % 8) + 8) % 8;
    let direction = u32(col);

    // ── Frame index ──
    // fract of (time*fps + phase) wrapped by frame_count. `time_phase` is a
    // per-instance seconds offset so a crowd doesn't animate in lockstep.
    let fcount = max(params.frame_count, 1u);
    let frame_f = (params.time + params.time_phase) * params.fps;
    let frame = u32(floor(frame_f)) % fcount;

    var out: VOut;
    out.position = position_world_to_clip(world_pos);
    out.uv = v.uv;
    out.direction = direction;
    out.frame = frame;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let dirs = max(params.directions, 1u);
    let rows = max(params.total_rows, 1u);
    let cell = vec2<f32>(1.0 / f32(dirs), 1.0 / f32(rows));

    // UV origin: Bevy textures sample with V=0 at top. Atlas row 0 is top of
    // the PNG, and within a row the sprite's feet are at the bottom pixels
    // (larger V). The quad has uv.y=0 at feet (bottom) and uv.y=1 at head
    // (top), so atlas V grows as quad uv.y shrinks.
    let row = params.state_row_offset + in.frame;
    let u = (f32(in.direction) + in.uv.x) * cell.x;
    let v_atlas = (f32(row) + (1.0 - in.uv.y)) * cell.y;

    let sample = textureSample(atlas_tex, atlas_sampler, vec2<f32>(u, v_atlas));
    if sample.a < 0.5 {
        discard;
    }
    return sample;
}
