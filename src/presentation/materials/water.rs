//! Water PBR material extension: animated wave normals, fog-of-war masking,
//! and view-dependent transparency for the ocean/lake planes.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const WATER_SHADER_PATH: &str = "shaders/water.wgsl";

/// Type alias so water can participate in Bevy's lit PBR pipeline.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterExtension {
    #[uniform(100)]
    pub settings: WaterSettings,

    /// Fog of war visible (smoothed display) texture — injected after fog spawns.
    #[texture(101)]
    #[sampler(102)]
    pub fog_visible_texture: Option<Handle<Image>>,

    /// Fog of war explored (binary) texture.
    #[texture(103)]
    #[sampler(104)]
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

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER_PATH.into()
    }
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            time: 0.0,
            wave_speed: 0.8,
            wave_scale: 0.3,
            opacity: 0.72,
            shallow_color: Vec4::new(0.10, 0.24, 0.28, 1.0),
            deep_color: Vec4::new(0.02, 0.06, 0.14, 1.0),
            specular_color: Vec4::new(1.0, 0.97, 0.90, 1.0),
            sun_direction: Vec4::new(0.5, 0.7, 0.3, 0.0),
            camera_position: Vec4::new(0.0, 50.0, 0.0, 0.0),
        }
    }
}
