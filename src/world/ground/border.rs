//! Mountain-border mesh generation and topology constraints: hems the
//! playable area with impassable ridges.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use rand::{Rng, SeedableRng};

use crate::types::{Decoration, FogHideable, GameSetupConfig, GameWorld, MapSeed, ModelAssets};

use super::data::{BorderSettings, HeightMap};

pub fn spawn_mountain_border(
    mut commands: Commands,
    model_assets: Res<ModelAssets>,
    height_map: Res<HeightMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    if model_assets.mountains.is_empty() {
        return;
    }

    let settings = BorderSettings::from_map_size(config.map_size.world_size());
    let half_map = height_map.half_map;
    let mut rng = rand::rngs::StdRng::seed_from_u64(map_seed.0.wrapping_add(9000));
    let spacing = if height_map.map_size <= 320.0 {
        24.0
    } else if height_map.map_size <= 520.0 {
        28.0
    } else {
        34.0
    };
    let ring_depth = if height_map.map_size <= 320.0 { 1 } else { 2 };

    for layer in 0..ring_depth {
        let inset = settings.prop_inset + layer as f32 * spacing * 0.65;
        let limit = half_map - inset;
        let mut cursor = -limit;

        while cursor <= limit {
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(cursor, 0.0, -limit),
                layer,
            );
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(cursor, 0.0, limit),
                layer,
            );
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(-limit, 0.0, cursor),
                layer,
            );
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(limit, 0.0, cursor),
                layer,
            );
            cursor += spacing;
        }
    }
}

fn spawn_border_mountain(
    commands: &mut Commands,
    models: &[Handle<Scene>],
    height_map: &HeightMap,
    rng: &mut rand::rngs::StdRng,
    pos: Vec3,
    layer: usize,
) {
    let x = pos.x + rng.random_range(-5.0..5.0);
    let z = pos.z + rng.random_range(-5.0..5.0);
    let y = height_map.sample(x, z);
    let scene = models[rng.random_range(0..models.len())].clone();
    let base_scale = if layer == 0 { 2.8 * 5.0 } else { 2.1 * 5.0 };
    let scale = rng.random_range(base_scale * 0.9..base_scale * 1.15);

    commands.spawn((
        GameWorld,
        Decoration,
        FogHideable::Object,
        NotShadowCaster,
        SceneRoot(scene),
        Transform::from_translation(Vec3::new(x, y - 0.5, z))
            .with_rotation(Quat::from_rotation_y(
                rng.random_range(0.0..std::f32::consts::TAU),
            ))
            .with_scale(Vec3::splat(scale)),
    ));
}
