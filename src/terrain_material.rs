use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const TERRAIN_SHADER_PATH: &str = "shaders/terrain.wgsl";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainMaterial {
    #[uniform(0)]
    pub settings: TerrainSettings,

    #[texture(1)]
    #[sampler(2)]
    pub grass_texture: Option<Handle<Image>>,

    #[texture(3)]
    #[sampler(4)]
    pub rock_texture: Option<Handle<Image>>,

    #[texture(5)]
    #[sampler(6)]
    pub sand_texture: Option<Handle<Image>>,

    #[texture(7)]
    #[sampler(8)]
    pub snow_texture: Option<Handle<Image>>,
}

#[derive(ShaderType, Debug, Clone)]
pub struct TerrainSettings {
    pub time: f32,
    pub detail_scale: f32,
    pub detail_strength: f32,
    pub snow_height: f32,
    pub amplitude: f32,
    pub tile_scale: f32,
    pub sun_intensity: f32,
    pub ambient_brightness: f32,
    pub sun_direction: Vec4,
    pub sun_color: Vec4,
    pub ambient_color: Vec4,
}

impl Material for TerrainMaterial {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER_PATH.into()
    }
}

impl Default for TerrainSettings {
    fn default() -> Self {
        Self {
            time: 0.0,
            detail_scale: 0.8,
            detail_strength: 0.12,
            snow_height: 12.0,
            amplitude: 18.0,
            tile_scale: 0.15,
            sun_intensity: 1.0,
            ambient_brightness: 0.35,
            sun_direction: Vec4::new(0.5, 0.7, 0.3, 0.0),
            sun_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            ambient_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
        }
    }
}
