use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const HOVER_SHADER_PATH: &str = "shaders/hover_ring.wgsl";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct HoverRingMaterial {
    #[uniform(0)]
    pub settings: HoverRingSettings,
}

#[derive(ShaderType, Debug, Clone)]
pub struct HoverRingSettings {
    pub color: Vec4,
    pub time: f32,
    pub ring_width: f32,
    pub ring_radius: f32,
    pub _padding: f32,
}

impl Default for HoverRingSettings {
    fn default() -> Self {
        Self {
            color: Vec4::new(0.3, 0.8, 1.0, 0.9),
            time: 0.0,
            ring_width: 0.06,
            ring_radius: 0.42,
            _padding: 0.0,
        }
    }
}

impl Material for HoverRingMaterial {
    fn fragment_shader() -> ShaderRef {
        HOVER_SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}
