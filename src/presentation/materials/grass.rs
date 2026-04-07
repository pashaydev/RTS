use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const GRASS_SHADER_PATH: &str = "shaders/grass.wgsl";

pub type GrassMaterial = ExtendedMaterial<StandardMaterial, GrassExtension>;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GrassExtension {
    #[uniform(100)]
    pub settings: GrassSettings,
}

#[derive(ShaderType, Debug, Clone)]
pub struct GrassSettings {
    // -- 16 bytes: wind/animation --
    pub time: f32,
    pub wind_strength: f32,
    pub wind_speed: f32,
    pub random_lean: f32,
    // -- 16 bytes: wind direction --
    pub wind_direction: Vec4,
    // -- 16 bytes: base color --
    pub base_color: Vec4,
    // -- 16 bytes: tip color --
    pub tip_color: Vec4,
    // -- 16 bytes: blade geometry --
    pub blade_width: f32,
    pub blade_height: f32,
    pub width_thicken: f32,
    pub normal_up_bias: f32,
    // -- 16 bytes: distance blending --
    pub normal_blend_start: f32,
    pub normal_blend_end: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

impl Default for GrassSettings {
    fn default() -> Self {
        Self {
            time: 0.0,
            wind_strength: 0.08,
            wind_speed: 1.2,
            random_lean: 0.35,
            wind_direction: Vec4::new(1.0, 0.0, 0.6, 0.0),
            base_color: Vec4::new(0.12, 0.28, 0.04, 1.0),
            tip_color: Vec4::new(0.45, 0.65, 0.15, 1.0),
            blade_width: 0.06,
            blade_height: 2.5,
            width_thicken: 1.5,
            normal_up_bias: 0.4,
            normal_blend_start: 40.0,
            normal_blend_end: 120.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }
}

impl MaterialExtension for GrassExtension {
    fn vertex_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }
}
