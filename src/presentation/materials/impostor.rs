//! Goblin impostor material for the far-distance LOD tier.
//!
//! At Impostor tier, `unit_lod_swap_system` spawns a child entity with a
//! billboard quad textured by this material. The shader yaw-billboards the
//! quad toward the camera and samples `assets/impostors/goblin_atlas.png`
//! using (direction, frame) picked from `ImpostorParams` every frame.
//!
//! The JSON sidecar baked alongside the PNG defines each animation state's
//! row-offset and frame-count — we load it once at startup and cache it in
//! the `GoblinImpostor` resource. The `impostor_sync_system` in
//! `presentation::unit_lod` reads each impostor entity's gameplay state
//! (health / move target / attack target) once per frame and pokes the
//! matching row-offset + frame-count + yaw into that entity's material.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use serde::Deserialize;

const IMPOSTOR_SHADER_PATH: &str = "shaders/impostor.wgsl";
const IMPOSTOR_ATLAS_PNG: &str = "impostors/goblin_atlas.png";
const IMPOSTOR_ATLAS_JSON: &str = "assets/impostors/goblin_atlas.json";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ImpostorMaterial {
    #[uniform(0)]
    pub params: ImpostorParams,
    #[texture(1)]
    #[sampler(2)]
    pub atlas: Handle<Image>,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct ImpostorParams {
    // -- 16 bytes --
    pub time: f32,
    pub time_phase: f32,
    pub yaw_facing: f32,
    pub size: f32,
    // -- 16 bytes --
    pub state_row_offset: u32,
    pub frame_count: u32,
    pub total_rows: u32,
    pub directions: u32,
    // -- 16 bytes --
    pub fps: f32,
    pub light_mix: f32,
    pub top_light: f32,
    pub bottom_light: f32,
    // -- 16 bytes --
    pub light_tint: Vec4,
    // -- 16 bytes --
    pub shadow_tint: Vec4,
    // -- 16 bytes --
    pub sun_direction: Vec4,
    // -- 16 bytes --
    pub local_visibility: f32,
    pub wrap_amount: f32,
    pub rim_strength: f32,
    pub _pad0: f32,
    // -- 16 bytes --
    /// Per-tier multiplicative tint applied to the sampled atlas color.
    /// `[1, 1, 1, 1]` for the default Runner tier.
    pub tier_tint: Vec4,
}

impl Default for ImpostorParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            time_phase: 0.0,
            yaw_facing: 0.0,
            size: 3.95,
            state_row_offset: 0,
            frame_count: 16,
            total_rows: 80,
            directions: 8,
            fps: 12.0,
            light_mix: 1.0,
            top_light: 1.08,
            bottom_light: 0.92,
            light_tint: Vec4::new(1.0, 1.0, 1.0, 1.0),
            shadow_tint: Vec4::new(0.70, 0.74, 0.88, 1.0),
            sun_direction: Vec4::new(0.5, 0.7, 0.3, 0.0),
            local_visibility: 0.0,
            wrap_amount: 0.35,
            rim_strength: 0.18,
            _pad0: 0.0,
            tier_tint: Vec4::new(1.0, 1.0, 1.0, 1.0),
        }
    }
}

impl Material for ImpostorMaterial {
    fn vertex_shader() -> ShaderRef {
        IMPOSTOR_SHADER_PATH.into()
    }
    fn fragment_shader() -> ShaderRef {
        IMPOSTOR_SHADER_PATH.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        // Mask gives us depth-writing + fragment discard, which is what the
        // shader does; Blend would mess up depth sort for a crowd.
        AlphaMode::Mask(0.5)
    }
}

// ── Atlas sidecar ────────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
pub struct ImpostorAtlasMeta {
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub directions: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub fps: u32,
    pub states: Vec<ImpostorStateMeta>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ImpostorStateMeta {
    pub name: String,
    pub clip: String,
    pub frame_count: u32,
    pub row_offset: u32,
}

impl ImpostorAtlasMeta {
    pub fn total_rows(&self) -> u32 {
        self.atlas_height / self.cell_height.max(1)
    }

    /// Find a state by name (matches the JSON `name` field). Returns
    /// `row_offset, frame_count`. Caller falls back to Idle if absent.
    pub fn state(&self, name: &str) -> Option<(u32, u32)> {
        self.states
            .iter()
            .find(|s| s.name == name)
            .map(|s| (s.row_offset, s.frame_count))
    }
}

/// A Bevy-internal enum we map our `AnimState` onto before indexing the
/// atlas. Kept tiny — the atlas has exactly these five states baked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImpostorAnimState {
    Idle,
    Walk,
    Run,
    Attack,
    Death,
}

impl ImpostorAnimState {
    pub fn atlas_name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Walk => "Walk",
            Self::Run => "Run",
            Self::Attack => "Attack",
            Self::Death => "Death",
        }
    }
}

/// Cached goblin impostor assets. Created once at game-setup time.
#[derive(Resource, Debug, Clone)]
pub struct GoblinImpostor {
    pub atlas: Handle<Image>,
    pub meta: ImpostorAtlasMeta,
    pub quad_mesh: Handle<Mesh>,
}

impl GoblinImpostor {
    /// Returns (row_offset, frame_count) for the given state, falling back
    /// to the Idle state if the requested state is absent from the atlas
    /// (e.g., a partial bake with `--only-idle-walk`).
    pub fn lookup(&self, state: ImpostorAnimState) -> (u32, u32) {
        self.meta
            .state(state.atlas_name())
            .or_else(|| self.meta.state("Idle"))
            .unwrap_or((0, self.meta.states.first().map_or(16, |s| s.frame_count)))
    }
}

// ── Plugin ───────────────────────────────────────────────────────────────

pub struct ImpostorMaterialPlugin;

impl Plugin for ImpostorMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ImpostorMaterial>::default())
            .add_systems(Startup, load_goblin_impostor);
    }
}

fn load_goblin_impostor(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Parse the JSON sidecar synchronously. The file lives at a known path
    // in the repo, so a missing/malformed file is a build-time error — warn
    // and skip the resource; the LOD system short-circuits if it's absent.
    let meta = match std::fs::read_to_string(IMPOSTOR_ATLAS_JSON) {
        Ok(s) => match serde_json::from_str::<ImpostorAtlasMeta>(&s) {
            Ok(m) => m,
            Err(e) => {
                warn!("impostor atlas JSON parse error ({IMPOSTOR_ATLAS_JSON}): {e}");
                return;
            }
        },
        Err(e) => {
            warn!("impostor atlas JSON missing ({IMPOSTOR_ATLAS_JSON}): {e}");
            return;
        }
    };

    let atlas: Handle<Image> = asset_server.load(IMPOSTOR_ATLAS_PNG);
    let quad_mesh = meshes.add(build_impostor_quad());

    commands.insert_resource(GoblinImpostor {
        atlas,
        meta,
        quad_mesh,
    });
}

/// Unit-height quad with feet at local y=0, head at y=1, centered in x.
/// UV (0,0) at bottom-left (feet) so the shader can sample sprite cells
/// bottom-up the way the atlas was laid out.
fn build_impostor_quad() -> Mesh {
    let positions = vec![
        [-0.5, 0.0, 0.0], // bl
        [0.5, 0.0, 0.0],  // br
        [0.5, 1.0, 0.0],  // tr
        [-0.5, 1.0, 0.0], // tl
    ];
    let normals = vec![[0.0, 0.0, 1.0]; 4];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let indices = Indices::U32(vec![0, 1, 2, 0, 2, 3]);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(indices);
    mesh
}
