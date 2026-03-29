use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const TERRAIN_SHADER_PATH: &str = "shaders/terrain.wgsl";

/// Type alias so the rest of the codebase can keep using `TerrainMaterial`.
pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainExtension {
    #[uniform(100)]
    pub settings: TerrainSettings,

    #[texture(101)]
    #[sampler(102)]
    pub grass_texture: Option<Handle<Image>>,

    #[texture(103)]
    #[sampler(104)]
    pub rock_texture: Option<Handle<Image>>,

    #[texture(105)]
    #[sampler(106)]
    pub sand_texture: Option<Handle<Image>>,

    #[texture(107)]
    #[sampler(108)]
    pub snow_texture: Option<Handle<Image>>,
}

#[derive(ShaderType, Debug, Clone)]
pub struct TerrainSettings {
    pub detail_scale: f32,
    pub detail_strength: f32,
    pub snow_height: f32,
    pub amplitude: f32,
    pub tile_scale: f32,
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER_PATH.into()
    }
}

impl Default for TerrainSettings {
    fn default() -> Self {
        Self {
            detail_scale: 0.8,
            detail_strength: 0.12,
            snow_height: 12.0,
            amplitude: 18.0,
            tile_scale: 0.15,
        }
    }
}
