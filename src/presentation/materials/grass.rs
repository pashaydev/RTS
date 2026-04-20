//! Animated grass shader extension with wind, bend, and distance LOD;
//! driven by per-cell instancing from the terrain grass system.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::alpha::AlphaMode,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

use crate::types::rendering::GrassRenderSettings;

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

impl GrassSettings {
    pub fn from_render_settings(render: &GrassRenderSettings, time: f32) -> Self {
        Self {
            time,
            wind_strength: render.wind_strength,
            wind_speed: render.wind_speed,
            random_lean: render.random_lean,
            wind_direction: render.wind_direction,
            base_color: render.base_color,
            tip_color: render.tip_color,
            blade_width: render.blade_width,
            blade_height: render.blade_height,
            width_thicken: render.width_thicken,
            normal_up_bias: render.normal_up_bias,
            normal_blend_start: render.normal_blend_start,
            normal_blend_end: render.normal_blend_end,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }

    pub fn apply_render_settings(&mut self, render: &GrassRenderSettings, time: f32) {
        *self = Self::from_render_settings(render, time);
    }
}

impl Default for GrassSettings {
    fn default() -> Self {
        Self::from_render_settings(&GrassRenderSettings::default(), 0.0)
    }
}

impl MaterialExtension for GrassExtension {
    fn vertex_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        GRASS_SHADER_PATH.into()
    }

    fn alpha_mode() -> Option<AlphaMode> {
        Some(AlphaMode::Opaque)
    }
}
