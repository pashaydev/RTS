use bevy::prelude::*;
use rand::Rng;

use crate::presentation::camera::InternalRenderTarget;
use crate::types::*;
use crate::world::ground::HeightMap;
use crate::presentation::materials::tree_occlusion::{
    TreeOcclusionExtension, TreeOcclusionMaterial, TreeOcclusionMaterialCache,
    TreeOcclusionUniform, TREE_OCCLUSION_MAX_UNITS,
};

use super::spawning::{random_tree, terrain_translation};

// ── Tree Growth Systems ──

pub(super) fn spawn_saplings_system(
    mut commands: Commands,
    time: Res<Time>,
    mut config: ResMut<TreeGrowthConfig>,
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    biome_map: Res<BiomeMap>,
    height_map: Res<HeightMap>,
    model_assets: Res<ModelAssets>,
    mature_trees: Query<&Transform, With<MatureTree>>,
    saplings: Query<&Sapling>,
    growing: Query<&GrowingTree>,
    game_config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    if *net_role == crate::infrastructure::multiplayer::NetRole::Client {
        return;
    }

    config.spawn_timer.tick(time.delta());
    if !config.spawn_timer.just_finished() {
        return;
    }

    let sapling_count = saplings.iter().count() as u32;
    let _growing_count = growing.iter().count() as u32;
    if sapling_count >= config.max_saplings {
        return;
    }

    if model_assets.trees.is_empty() {
        return;
    }

    let mut rng = rand::rng();
    let trees: Vec<Vec3> = mature_trees.iter().map(|t| t.translation).collect();
    if trees.is_empty() {
        return;
    }

    // Try to spawn a few saplings near random existing trees
    let spawns_per_tick = 3u32.min(config.max_saplings - sapling_count);
    for _ in 0..spawns_per_tick {
        let parent_pos = trees[rng.random_range(0..trees.len())];
        let angle = rng.random_range(0.0..std::f32::consts::TAU);
        let dist = rng.random_range(4.0..config.spawn_radius);
        let x = parent_pos.x + angle.cos() * dist;
        let z = parent_pos.z + angle.sin() * dist;

        // Only spawn in forest biome
        if biome_map.get_biome(x, z) != Biome::Forest {
            continue;
        }

        // Don't spawn too close to any player base
        let sapling_spawn_positions = game_config.spawn_positions(map_seed.0);
        let mut near_base = false;
        for &(_, (sx, sz)) in &sapling_spawn_positions {
            let dx = x - sx;
            let dz = z - sz;
            if (dx * dx + dz * dz).sqrt() < 25.0 {
                near_base = true;
                break;
            }
        }
        if near_base {
            continue;
        }

        let (scene_handle, base_scale) = random_tree(&mut rng, &model_assets).unwrap();
        let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
        let target_scale = rng.random_range(0.8_f32..1.2) * base_scale;
        let initial_scale = 0.15 * base_scale;

        commands.spawn((
            GameWorld,
            Sapling {
                timer: Timer::from_seconds(config.sapling_duration, TimerMode::Once),
                target_scale,
            },
            FogHideable::Object,
            SceneRoot(scene_handle),
            Transform::from_translation(terrain_translation(&height_map, x, z, 0.0))
                .with_rotation(Quat::from_rotation_y(y_rotation))
                .with_scale(Vec3::splat(initial_scale)),
        ));
    }
}

pub(super) fn grow_saplings_system(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<TreeGrowthConfig>,
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    mut saplings: Query<(Entity, &mut Sapling, &mut Transform), Without<FrustumCulled>>,
) {
    if *net_role == crate::infrastructure::multiplayer::NetRole::Client {
        return;
    }

    for (entity, mut sapling, mut tf) in &mut saplings {
        sapling.timer.tick(time.delta());
        let progress = sapling.timer.fraction();
        // Lerp scale from ~15% to ~40% of target
        let start = sapling.target_scale * 0.15;
        let end = sapling.target_scale * 0.4;
        let scale = start + progress * (end - start);
        tf.scale = Vec3::splat(scale);

        if sapling.timer.is_finished() {
            commands.entity(entity).remove::<Sapling>();
            commands.entity(entity).insert(GrowingTree {
                stage: 0,
                timer: Timer::from_seconds(config.growth_stage_duration, TimerMode::Once),
                target_scale: sapling.target_scale,
            });
        }
    }
}

pub(super) fn grow_trees_system(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<TreeGrowthConfig>,
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    mut growing: Query<(Entity, &mut GrowingTree, &mut Transform), Without<FrustumCulled>>,
) {
    if *net_role == crate::infrastructure::multiplayer::NetRole::Client {
        return;
    }

    for (entity, mut tree, mut tf) in &mut growing {
        tree.timer.tick(time.delta());
        let progress = tree.timer.fraction();

        // Stage scale ranges as fractions of target: 0→(40%..60%), 1→(60%..80%), 2→(80%..100%)
        let ts = tree.target_scale;
        let (from, to) = match tree.stage {
            0 => (ts * 0.4, ts * 0.6),
            1 => (ts * 0.6, ts * 0.8),
            _ => (ts * 0.8, ts),
        };
        let scale = from + progress * (to - from);
        tf.scale = Vec3::splat(scale);

        if tree.timer.is_finished() {
            if tree.stage >= 2 {
                // Promote to mature tree
                commands.entity(entity).remove::<GrowingTree>();
                commands.entity(entity).insert((
                    MatureTree,
                    ResourceNode {
                        resource_type: ResourceType::Wood,
                        amount_remaining: config.mature_wood_amount,
                    },
                    PickRadius(3.0),
                ));
            } else {
                tree.stage += 1;
                tree.timer = Timer::from_seconds(config.growth_stage_duration, TimerMode::Once);
            }
        }
    }
}

// ── Fix Tree Alpha Mode ──

/// Converts tree leaf materials from AlphaMode::Blend to AlphaMode::Mask.
/// Blend causes massive overdraw when the camera is close because every
/// overlapping transparent leaf quad is fully shaded. Mask uses a hard cutoff
/// that enables early-Z rejection, eliminating the overdraw entirely.
/// Limit how many trees get alpha-fixed per frame to avoid spikes when many
/// trees spawn at once (e.g. map load). Unfixed trees will be processed on
/// subsequent frames.
const MAX_TREE_ALPHA_FIXES_PER_FRAME: usize = 8;

pub(super) fn update_tree_occlusion_uniform(
    active_player: Res<ActivePlayer>,
    render_target: Res<InternalRenderTarget>,
    camera_q: Query<(&Camera, &GlobalTransform), With<CullingSourceCamera>>,
    selected_units: Query<&Transform, (With<Unit>, With<Selected>, Without<Building>)>,
    player_units: Query<(&Transform, &Faction), (With<Unit>, Without<Building>)>,
    mut uniform: ResMut<TreeOcclusionUniform>,
    tree_mat_cache: Res<TreeOcclusionMaterialCache>,
    mut tree_materials: ResMut<Assets<TreeOcclusionMaterial>>,
) {
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let cam_pos = cam_gt.translation();
    let mut masks = [Vec4::ZERO; TREE_OCCLUSION_MAX_UNITS];

    let mut picked: Vec<Vec3> = selected_units.iter().map(|tf| tf.translation).collect();
    if picked.is_empty() {
        let mut player_positions: Vec<Vec3> = player_units
            .iter()
            .filter_map(|(tf, faction)| (*faction == active_player.0).then_some(tf.translation))
            .collect();

        player_positions.sort_by(|a, b| {
            a.distance_squared(cam_pos)
                .total_cmp(&b.distance_squared(cam_pos))
        });

        picked = player_positions;
    }

    let viewport_size = render_target.size.as_vec2();
    let mask_radius = viewport_size.y * 0.11;
    let feather = viewport_size.y * 0.05;

    let mut active_units = 0u32;
    for pos in picked.into_iter().take(TREE_OCCLUSION_MAX_UNITS) {
        let unit_pos = pos + Vec3::Y * 1.2;
        let Ok(screen_pos) = camera.world_to_viewport(cam_gt, unit_pos) else {
            continue;
        };
        masks[active_units as usize] = Vec4::new(screen_pos.x, screen_pos.y, mask_radius, feather);
        active_units += 1;
    }

    uniform.0.screen_masks = masks;
    uniform.0.active_units = active_units;

    for handle in tree_mat_cache.handles.values() {
        if let Some(material) = tree_materials.get_mut(handle) {
            material.extension.settings = uniform.0.clone();
        }
    }
}

pub(super) fn fix_tree_alpha_mode(
    mut commands: Commands,
    trees: Query<
        Entity,
        (
            Or<(With<MatureTree>, With<Sapling>, With<GrowingTree>)>,
            Without<TreeAlphaFixed>,
        ),
    >,
    children_q: Query<&Children>,
    mesh_mat_q: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tree_materials: ResMut<Assets<TreeOcclusionMaterial>>,
    mut leaf_mats: ResMut<TreeLeafMaterials>,
    mut tree_mat_cache: ResMut<TreeOcclusionMaterialCache>,
    tree_occlusion_uniform: Res<TreeOcclusionUniform>,
) {
    let mut processed = 0;
    for tree_entity in &trees {
        if processed >= MAX_TREE_ALPHA_FIXES_PER_FRAME {
            break;
        }
        let fixed = fix_alpha_recursive(
            tree_entity,
            &children_q,
            &mesh_mat_q,
            &mut materials,
            &mut tree_materials,
            &mut leaf_mats.0,
            &mut tree_mat_cache,
            &tree_occlusion_uniform.0,
            &mut commands,
        );
        // Only mark as fixed when child meshes have actually been processed.
        // Scene loading is async — children may not exist yet on the first frame.
        if fixed > 0 {
            commands.entity(tree_entity).insert(TreeAlphaFixed);
            processed += 1;
        }
    }
}

fn fix_alpha_recursive(
    entity: Entity,
    children_q: &Query<&Children>,
    mesh_mat_q: &Query<&MeshMaterial3d<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
    tree_materials: &mut Assets<TreeOcclusionMaterial>,
    leaf_mats: &mut Vec<Handle<TreeOcclusionMaterial>>,
    tree_mat_cache: &mut TreeOcclusionMaterialCache,
    tree_occlusion_uniform: &crate::presentation::materials::tree_occlusion::TreeOcclusionSettings,
    commands: &mut Commands,
) -> u32 {
    let mut count = 0;
    if let Ok(mat_handle) = mesh_mat_q.get(entity) {
        if let Some(mat) = materials.get_mut(mat_handle) {
            count += 1;
            if !matches!(mat.alpha_mode, AlphaMode::Mask(_)) {
                mat.alpha_mode = AlphaMode::Mask(0.6);
            }
            mat.double_sided = true;

            let extended_handle = tree_mat_cache
                .handles
                .entry(mat_handle.id())
                .or_insert_with(|| {
                    tree_materials.add(TreeOcclusionMaterial {
                        base: mat.clone(),
                        extension: TreeOcclusionExtension {
                            settings: tree_occlusion_uniform.clone(),
                        },
                    })
                })
                .clone();

            commands
                .entity(entity)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(MeshMaterial3d(extended_handle.clone()));

            if mat.base_color_texture.is_some()
                && !leaf_mats.iter().any(|h| h == &extended_handle)
            {
                leaf_mats.push(extended_handle);
            }
        }
    }
    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            count += fix_alpha_recursive(
                child,
                children_q,
                mesh_mat_q,
                materials,
                tree_materials,
                leaf_mats,
                tree_mat_cache,
                tree_occlusion_uniform,
                commands,
            );
        }
    }
    count
}
