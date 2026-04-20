//! Per-tree depth pass that hides tree interiors when the camera angle
//! shows them through buildings, avoiding visual clipping.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};
use std::collections::HashMap;

const TREE_OCCLUSION_SHADER_PATH: &str = "shaders/tree_occlusion.wgsl";
pub const TREE_OCCLUSION_MAX_UNITS: usize = 8;

pub type TreeOcclusionMaterial = ExtendedMaterial<StandardMaterial, TreeOcclusionExtension>;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TreeOcclusionExtension {
    #[uniform(100)]
    pub settings: TreeOcclusionSettings,
}

#[derive(ShaderType, Debug, Clone)]
pub struct TreeOcclusionSettings {
    pub screen_masks: [Vec4; TREE_OCCLUSION_MAX_UNITS],
    pub active_units: u32,
    pub _pad0: UVec3,
}

impl Default for TreeOcclusionSettings {
    fn default() -> Self {
        Self {
            screen_masks: [Vec4::ZERO; TREE_OCCLUSION_MAX_UNITS],
            active_units: 0,
            _pad0: UVec3::ZERO,
        }
    }
}

#[derive(Resource, Default)]
pub struct TreeOcclusionMaterialCache {
    pub handles: HashMap<AssetId<StandardMaterial>, Handle<TreeOcclusionMaterial>>,
}

#[derive(Resource, Default, Clone)]
pub struct TreeOcclusionUniform(pub TreeOcclusionSettings);

impl MaterialExtension for TreeOcclusionExtension {
    fn fragment_shader() -> ShaderRef {
        TREE_OCCLUSION_SHADER_PATH.into()
    }
}
