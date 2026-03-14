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

    /// Smoothed display texture (currently visible + explored fade).
    #[texture(1)]
    #[sampler(2)]
    pub visible_texture: Option<Handle<Image>>,

    /// Permanent explored layer (binary: 0 or 1).
    #[texture(3)]
    #[sampler(4)]
    pub explored_texture: Option<Handle<Image>>,
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
    // Unexplored fog noise controls
    pub fog_noise_scale: f32,
    pub fog_noise_speed: f32,
    pub fog_noise_warp: f32,
    pub fog_noise_contrast: f32,
    pub fog_noise_octaves: f32,
    pub fog_tendril_scale: f32,
    pub fog_tendril_strength: f32,
    pub fog_warp_speed: f32,
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
            noise_scale: 5.5,
            edge_glow_width: 0.08,
            edge_glow_intensity: 0.6,
            fog_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            glow_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            explored_tint: Vec4::new(1.0, 1.0, 1.0, 0.15),
            fog_noise_scale: 10.0,
            fog_noise_speed: 0.01,
            fog_noise_warp: 0.8,
            fog_noise_contrast: 0.2,
            fog_noise_octaves: 4.0,
            fog_tendril_scale: 6.0,
            fog_tendril_strength: 0.3,
            fog_warp_speed: 0.5,
        }
    }
}
