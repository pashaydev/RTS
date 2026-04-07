use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const WATER_SHADER_PATH: &str = "shaders/water.wgsl";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    #[uniform(0)]
    pub settings: WaterSettings,

    /// Fog of war visible (smoothed display) texture — injected after fog spawns.
    #[texture(1)]
    #[sampler(2)]
    pub fog_visible_texture: Option<Handle<Image>>,

    /// Fog of war explored (binary) texture.
    #[texture(3)]
    #[sampler(4)]
    pub fog_explored_texture: Option<Handle<Image>>,
}

#[derive(ShaderType, Debug, Clone)]
pub struct WaterSettings {
    pub time: f32,
    pub wave_speed: f32,
    pub wave_scale: f32,
    pub opacity: f32,
    pub shallow_color: Vec4,
    pub deep_color: Vec4,
    pub specular_color: Vec4,
    pub sun_direction: Vec4,
    pub camera_position: Vec4,
}

impl Material for WaterMaterial {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            time: 0.0,
            wave_speed: 0.8,
            wave_scale: 0.3,
            opacity: 0.75,
            shallow_color: Vec4::new(0.15, 0.55, 0.65, 1.0),
            deep_color: Vec4::new(0.03, 0.10, 0.40, 1.0),
            specular_color: Vec4::new(1.0, 0.95, 0.8, 1.0),
            sun_direction: Vec4::new(0.5, 0.7, 0.3, 0.0),
            camera_position: Vec4::new(0.0, 50.0, 0.0, 0.0),
        }
    }
}
