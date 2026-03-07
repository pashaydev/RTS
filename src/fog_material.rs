use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const FOG_SHADER_PATH: &str = "shaders/fog_of_war.wgsl";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct FogOfWarMaterial {
    #[uniform(0)]
    pub settings: FogSettings,

    #[texture(1)]
    #[sampler(2)]
    pub visibility_texture: Option<Handle<Image>>,
}

#[derive(ShaderType, Debug, Clone)]
pub struct FogSettings {
    pub time: f32,
    pub noise_scale: f32,
    pub edge_glow_width: f32,
    pub edge_glow_intensity: f32,
    pub fog_color: Vec4,
    pub glow_color: Vec4,
    pub explored_tint: Vec4,
}

impl Material for FogOfWarMaterial {
    fn fragment_shader() -> ShaderRef {
        FOG_SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

impl Default for FogSettings {
    fn default() -> Self {
        Self {
            time: 0.0,
            noise_scale: 8.0,
            edge_glow_width: 0.08,
            edge_glow_intensity: 0.6,
            fog_color: Vec4::new(0.01, 0.01, 0.02, 0.55),
            glow_color: Vec4::new(0.3, 0.5, 0.8, 1.0),
            explored_tint: Vec4::new(0.0, 0.0, 0.0, 0.15),
        }
    }
}
