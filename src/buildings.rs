use std::time::Duration;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline};
use crate::blueprints::{BlueprintRegistry, EntityCategory, EntityKind, EntityVisualCache, LevelBonus, spawn_from_blueprint};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};

fn footprint_for_kind(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Base | EntityKind::Storage => 7.0,
        _ => 2.5,
    }
}

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingPlacementState>()
            .init_resource::<CompletedBuildings>()
            .init_resource::<LastPlayerResources>()
            .add_systems(Startup, create_ghost_materials)
            .add_systems(
                Update,
                (
                    update_placement_preview,
                    confirm_placement,
                    cancel_placement,
                    construction_progress_system,
                    tower_auto_attack,
                    training_queue_system,
                    update_completed_buildings_tracker,
                    building_upgrade_system,
                    demolish_system,
                    building_scale_anim_system,
                    healing_aura_system,
                    level_indicator_system,
                    sync_storage_on_spend,
                    update_storage_piles,
                )
                    .chain(),
            );
    }
}

// ── Asset creation (ghost materials only) ──

fn create_ghost_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(BuildingGhostMaterials {
        ghost_valid: materials.add(StandardMaterial {
            base_color: Color::srgba(0.2, 0.8, 0.3, 0.4),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        ghost_invalid: materials.add(StandardMaterial {
            base_color: Color::srgba(0.8, 0.2, 0.2, 0.4),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        under_construction: materials.add(StandardMaterial {
            base_color: Color::srgba(0.7, 0.65, 0.5, 0.6),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    });
}

// ── Placement preview ──

fn cursor_ground_pos(
    camera_q: &Query<(&Camera, &GlobalTransform)>,
    windows: &Query<&Window, With<PrimaryWindow>>,
) -> Option<Vec3> {
    let Ok(window) = windows.single() else {
        return None;
    };
    let cursor = window.cursor_position()?;
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return None;
    };
    let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
        return None;
    };
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(ray.get_point(dist))
}

fn update_placement_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ghosts: Query<
        (&mut Transform, &mut MeshMaterial3d<StandardMaterial>),
        With<GhostBuilding>,
    >,
    existing_buildings: Query<(&Transform, &BuildingFootprint), (With<Building>, Without<GhostBuilding>)>,
    biome_map: Option<Res<BiomeMap>>,
    height_map: Res<HeightMap>,
) {
    let PlacementMode::Placing(kind) = placement.mode else {
        return;
    };

    let bp = registry.get(kind);
    let half_h = bp.building.as_ref().map(|b| b.half_height).unwrap_or(1.0);
    let new_footprint = footprint_for_kind(kind);

    // Spawn ghost if it doesn't exist
    if placement.preview_entity.is_none() {
        let mesh = cache.meshes.get(&kind).expect("Missing mesh").clone();
        let ghost = commands
            .spawn((
                GhostBuilding,
                Mesh3d(mesh),
                MeshMaterial3d(ghost_mats.ghost_valid.clone()),
                Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
            ))
            .id();
        placement.preview_entity = Some(ghost);
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };

    let Some(ghost_entity) = placement.preview_entity else {
        return;
    };
    let Ok((mut ghost_tf, mut ghost_mat)) = ghosts.get_mut(ghost_entity) else {
        return;
    };

    let y = height_map.sample(world_pos.x, world_pos.z) + half_h;
    ghost_tf.translation = Vec3::new(world_pos.x, y, world_pos.z);

    let mut valid = true;

    if let Some(ref bm) = biome_map {
        if bm.get_biome(world_pos.x, world_pos.z) == Biome::Water {
            valid = false;
        }
    }

    for (building_tf, existing_footprint) in &existing_buildings {
        let min_dist = existing_footprint.0 + new_footprint;
        if building_tf.translation.distance(ghost_tf.translation) < min_dist {
            valid = false;
            break;
        }
    }

    let half_map = 250.0;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        valid = false;
    }

    *ghost_mat = if valid {
        MeshMaterial3d(ghost_mats.ghost_valid.clone())
    } else {
        MeshMaterial3d(ghost_mats.ghost_invalid.clone())
    };
}

fn confirm_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut player_res: ResMut<PlayerResources>,
    completed: Res<CompletedBuildings>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    height_map: Res<HeightMap>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    existing_buildings: Query<(&Transform, &BuildingFootprint), (With<Building>, Without<GhostBuilding>)>,
    biome_map: Option<Res<BiomeMap>>,
) {
    let PlacementMode::Placing(kind) = placement.mode else {
        return;
    };

    let new_footprint = footprint_for_kind(kind);

    // Phase 1: awaiting initial mouse release
    if placement.awaiting_release {
        if mouse.just_released(MouseButton::Left) {
            placement.awaiting_release = false;

            if let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) {
                let on_water = biome_map.as_ref()
                    .map_or(false, |bm| bm.get_biome(world_pos.x, world_pos.z) == Biome::Water);
                let too_close = existing_buildings.iter().any(|(building_tf, existing_fp)| {
                    let check_pos = Vec3::new(world_pos.x, building_tf.translation.y, world_pos.z);
                    building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint
                });
                let half_map = 250.0;
                let out_of_bounds = world_pos.x.abs() > half_map - 5.0
                    || world_pos.z.abs() > half_map - 5.0;

                if !on_water && !too_close && !out_of_bounds {
                    // Valid drag-and-drop
                } else {
                    return;
                }
            } else {
                return;
            }
        } else {
            return;
        }
    } else if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };

    let bp = registry.get(kind);

    // Check prerequisite
    let prereq_met = if let Some(ref bd) = bp.building {
        match bd.prerequisite {
            None => true,
            Some(prereq_kind) => completed.has(prereq_kind),
        }
    } else {
        true
    };
    if !prereq_met {
        return;
    }

    // Check validity
    if let Some(ref bm) = biome_map {
        if bm.get_biome(world_pos.x, world_pos.z) == Biome::Water {
            return;
        }
    }
    for (building_tf, existing_fp) in &existing_buildings {
        let check_pos = Vec3::new(world_pos.x, building_tf.translation.y, world_pos.z);
        if building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint {
            return;
        }
    }
    let half_map = 250.0;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        return;
    }

    // Check affordability
    if !bp.cost.can_afford(&player_res) {
        return;
    }

    // Deduct resources
    bp.cost.deduct(&mut player_res);

    // Despawn ghost
    if let Some(ghost) = placement.preview_entity {
        commands.entity(ghost).despawn();
    }

    // Spawn building using blueprint
    let bp = registry.get(kind);
    let is_gltf = bp.visual.mesh_kind.is_gltf();
    let entity_id = spawn_from_blueprint(&mut commands, &cache, kind, world_pos, &registry, building_models.as_deref(), None, &height_map);

    // Override material with under_construction (only for non-GLTF buildings)
    if !is_gltf {
        commands.entity(entity_id).insert(
            MeshMaterial3d(ghost_mats.under_construction.clone()),
        );
    }

    // Tower gets combat components from blueprint already

    // Reset placement
    placement.mode = PlacementMode::None;
    placement.preview_entity = None;
}

fn cancel_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut placement: ResMut<BuildingPlacementState>,
) {
    if placement.mode == PlacementMode::None {
        return;
    }

    if mouse.just_pressed(MouseButton::Right) || keyboard.just_pressed(KeyCode::Escape) {
        if let Some(preview) = placement.preview_entity {
            commands.entity(preview).despawn();
        }
        placement.mode = PlacementMode::None;
        placement.preview_entity = None;
        placement.awaiting_release = false;
    }
}

// ── Construction ──

fn construction_progress_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    mut buildings: Query<(
        Entity,
        &EntityKind,
        &mut BuildingState,
        &mut ConstructionProgress,
        &mut Transform,
    )>,
    workers: Query<&WorkerTask, With<Unit>>,
) {
    for (entity, kind, mut state, mut progress, mut transform) in &mut buildings {
        if *state != BuildingState::UnderConstruction {
            continue;
        }

        // Count workers actively building this entity
        let builder_count = workers
            .iter()
            .filter(|task| matches!(task, WorkerTask::Building(e) if *e == entity))
            .count();

        if builder_count == 0 {
            // No workers assigned — pause and show current scale
            progress.timer.pause();
            let bp = registry.get(*kind);
            let base_scale = bp.visual.scale;
            let fraction = progress.timer.fraction();
            let current_scale = 0.3 * base_scale + (base_scale - 0.3 * base_scale) * fraction;
            transform.scale = Vec3::splat(current_scale);
            continue;
        }

        // Unpause when workers are present
        progress.timer.unpause();
        let speed_mult = 1.0 + 0.5 * (builder_count as f32 - 1.0);

        let bp = registry.get(*kind);
        let base_scale = bp.visual.scale;

        progress.timer.tick(Duration::from_secs_f32(time.delta_secs() * speed_mult));

        // Lerp scale during construction
        let fraction = progress.timer.fraction();
        let current_scale = 0.3 * base_scale + (base_scale - 0.3 * base_scale) * fraction;
        transform.scale = Vec3::splat(current_scale);

        if progress.timer.is_finished() {
            *state = BuildingState::Complete;
            transform.scale = Vec3::splat(base_scale);

            // Swap to final material (only for non-GLTF buildings)
            let is_gltf = bp.visual.mesh_kind.is_gltf();
            if !is_gltf {
                if let Some(mat) = cache.materials_default.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
            }
            commands.entity(entity)
                .remove::<ConstructionProgress>()
                .remove::<ConstructionWorkers>();

            // Add training queue for production buildings
            if let Some(ref bd) = bp.building {
                if !bd.trains.is_empty() {
                    commands.entity(entity).insert(TrainingQueue {
                        queue: vec![],
                        timer: None,
                    });
                }
            }
        }
    }
}

// ── Tower auto-attack ──

fn tower_auto_attack(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut towers: Query<
        (
            &Transform,
            &EntityKind,
            &BuildingState,
            &mut AttackCooldown,
            &AttackDamage,
            &AttackRange,
            Option<&TowerAutoAttackEnabled>,
        ),
        With<Building>,
    >,
    mobs: Query<(Entity, &Transform), With<Mob>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (tower_tf, kind, state, mut cooldown, damage, range, auto_attack) in &mut towers {
        if *kind != EntityKind::Tower || *state != BuildingState::Complete {
            continue;
        }

        // Check if auto-attack is disabled
        if let Some(enabled) = auto_attack {
            if !enabled.0 {
                continue;
            }
        }

        cooldown.timer.tick(time.delta());
        if !cooldown.timer.just_finished() {
            continue;
        }

        let mut closest_dist = f32::MAX;
        let mut closest_mob = None;
        for (mob_entity, mob_tf) in &mobs {
            let dist = tower_tf.translation.distance(mob_tf.translation);
            if dist < range.0 && dist < closest_dist {
                closest_dist = dist;
                closest_mob = Some(mob_entity);
            }
        }

        if let Some(mob_entity) = closest_mob {
            commands.spawn((
                Projectile {
                    target: mob_entity,
                    speed: 20.0,
                    damage: damage.0,
                },
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(vfx.projectile_material.clone()),
                Transform::from_translation(tower_tf.translation + Vec3::Y * 3.0)
                    .with_scale(Vec3::splat(0.2)),
            ));
        }
    }
}

// ── Training ──

fn training_queue_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    mut buildings: Query<(&Transform, &EntityKind, &mut TrainingQueue, Option<&RallyPoint>), With<Building>>,
) {
    for (transform, _kind, mut queue, rally_point) in &mut buildings {
        if queue.queue.is_empty() {
            continue;
        }

        // Start timer for first item if not started
        if queue.timer.is_none() {
            let unit_kind = queue.queue[0];
            let bp = registry.get(unit_kind);
            queue.timer = Some(Timer::from_seconds(bp.train_time_secs, TimerMode::Once));
        }

        if let Some(ref mut timer) = queue.timer {
            timer.tick(time.delta());
            if timer.is_finished() {
                let unit_kind = queue.queue.remove(0);
                let spawn_pos = transform.translation + Vec3::new(3.0, 0.0, 3.0);
                let unit_entity = spawn_from_blueprint(&mut commands, &cache, unit_kind, spawn_pos, &registry, None, unit_models.as_deref(), &height_map);

                // If building has a rally point, send the unit there
                if let Some(rally) = rally_point {
                    commands.entity(unit_entity).insert(MoveTarget(rally.0));
                }

                queue.timer = None;
            }
        }
    }
}

// ── Track completed buildings ──

fn update_completed_buildings_tracker(
    mut completed: ResMut<CompletedBuildings>,
    buildings: Query<(&EntityKind, &BuildingState), With<Building>>,
) {
    let mut new_completed = Vec::new();

    for (kind, state) in &buildings {
        if *state == BuildingState::Complete && kind.category() == EntityCategory::Building {
            if !new_completed.contains(kind) {
                new_completed.push(*kind);
            }
        }
    }

    if completed.completed != new_completed {
        completed.completed = new_completed;
    }
}

// ── Building Upgrade ──

/// Start an upgrade on a building. Returns true if the upgrade was started.
pub fn start_upgrade(
    commands: &mut Commands,
    entity: Entity,
    current_level: u8,
    kind: EntityKind,
    registry: &BlueprintRegistry,
    player_res: &mut PlayerResources,
) -> bool {
    // Must be below max level (3)
    if current_level >= 3 {
        return false;
    }

    let bp = registry.get(kind);
    let bd = match bp.building.as_ref() {
        Some(bd) => bd,
        None => return false,
    };

    // level_upgrades is 0-indexed: index 0 = upgrade from L1->L2, index 1 = L2->L3
    let upgrade_index = (current_level - 1) as usize;
    if upgrade_index >= bd.level_upgrades.len() {
        return false;
    }

    let level_data = &bd.level_upgrades[upgrade_index];

    // Check affordability
    if !level_data.cost.can_afford(player_res) {
        return false;
    }

    // Deduct resources
    level_data.cost.deduct(player_res);

    // Insert UpgradeProgress component
    commands.entity(entity).insert(UpgradeProgress {
        timer: Timer::from_seconds(level_data.time_secs, TimerMode::Once),
        target_level: current_level + 1,
    });

    true
}

fn building_upgrade_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut buildings: Query<(
        Entity,
        &EntityKind,
        &mut BuildingLevel,
        &mut UpgradeProgress,
        &Transform,
        Option<&mut VisionRange>,
        Option<&mut AttackRange>,
        Option<&mut AttackDamage>,
    ), With<Building>>,
    children_q: Query<&Children>,
    scene_child_q: Query<Entity, With<BuildingSceneChild>>,
) {
    for (entity, kind, mut level, mut upgrade, transform, vision, attack_range, attack_damage) in &mut buildings {
        upgrade.timer.tick(time.delta());

        if !upgrade.timer.is_finished() {
            continue;
        }

        // Upgrade complete
        let new_level = upgrade.target_level;
        level.0 = new_level;

        let bp = registry.get(*kind);
        let bd = match bp.building.as_ref() {
            Some(bd) => bd,
            None => continue,
        };

        // Get the level data for the upgrade that just completed
        let upgrade_index = (new_level - 2) as usize; // L2 = index 0, L3 = index 1
        if upgrade_index >= bd.level_upgrades.len() {
            commands.entity(entity).remove::<UpgradeProgress>();
            continue;
        }

        let level_data = &bd.level_upgrades[upgrade_index];

        // For GLTF buildings: swap scene child to new level's model
        let bp = registry.get(*kind);
        let is_gltf = bp.visual.mesh_kind.is_gltf();
        if is_gltf {
            if let Some(ref models) = building_models {
                if let Some(new_scene) = models.scenes.get(&(*kind, new_level)) {
                    // Despawn old scene child
                    if let Ok(children) = children_q.get(entity) {
                        for child in children.iter() {
                            if scene_child_q.contains(child) {
                                commands.entity(child).try_despawn();
                            }
                        }
                    }
                    // Spawn new scene child with calibration
                    let cal = models.calibration.get(kind);
                    let scale = cal.map(|c| c.scale).unwrap_or(1.0);
                    let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                    let child = commands.spawn((
                        SceneRoot(new_scene.clone()),
                        BuildingSceneChild,
                        InheritOutline,
                        AsyncSceneInheritOutline::default(),
                        Transform::from_scale(Vec3::splat(scale))
                            .with_translation(Vec3::new(0.0, y_off, 0.0)),
                    )).id();
                    commands.entity(entity).add_child(child);
                }
            }
        }

        // Apply scale multiplier via animation (skip for GLTF — model swap IS the visual feedback)
        if !is_gltf {
            let current_scale = transform.scale;
            let new_scale = current_scale * level_data.scale_multiplier;
            commands.entity(entity).insert(BuildingScaleAnim {
                timer: Timer::from_seconds(0.5, TimerMode::Once),
                from: current_scale,
                to: new_scale,
            });
        }

        // Apply LevelBonus
        match &level_data.bonus {
            LevelBonus::None => {}
            LevelBonus::VisionBoost(boost) => {
                if let Some(mut vr) = vision {
                    vr.0 += boost;
                }
            }
            LevelBonus::TrainTimeMultiplier(_mult) => {
                // Stored on the building; training system reads from blueprint + level
                // No component change needed here — could be enhanced later
            }
            LevelBonus::TrainedStatBoost { .. } => {
                // Affects trained units, not the building itself
            }
            LevelBonus::RangeAndDamage { range_boost, damage_boost } => {
                if let Some(mut ar) = attack_range {
                    ar.0 += range_boost;
                }
                if let Some(mut ad) = attack_damage {
                    ad.0 += damage_boost;
                }
            }
            LevelBonus::CooldownMultiplier(_mult) => {
                // Could modify AttackCooldown timer duration — skipped for simplicity
            }
            LevelBonus::GatherAura { speed_bonus, range } => {
                commands.entity(entity).insert(StorageAura {
                    gather_speed_bonus: *speed_bonus,
                    range: *range,
                });
            }
            LevelBonus::HealAura { heal_per_sec, range } => {
                commands.entity(entity).insert(HealingAura {
                    heal_per_sec: *heal_per_sec,
                    range: *range,
                });
            }
            LevelBonus::UnlocksTraining(_kinds) => {
                // Could extend the TrainingQueue's available units — handled at UI level
            }
        }

        // Spawn VFX burst (4-6 flash entities in a ring)
        if let Some(ref vfx) = vfx_assets {
            let center = transform.translation;
            let flash_count = 5;
            for i in 0..flash_count {
                let angle = std::f32::consts::TAU * (i as f32 / flash_count as f32);
                let offset = Vec3::new(angle.cos() * 3.0, 2.0, angle.sin() * 3.0);
                commands.spawn((
                    VfxFlash {
                        timer: Timer::from_seconds(0.6, TimerMode::Once),
                        start_scale: 0.8,
                        end_scale: 0.0,
                    },
                    Mesh3d(vfx.sphere_mesh.clone()),
                    MeshMaterial3d(vfx.impact_material.clone()),
                    Transform::from_translation(center + offset)
                        .with_scale(Vec3::splat(0.8)),
                ));
            }
        }

        // Remove UpgradeProgress
        commands.entity(entity).remove::<UpgradeProgress>();
    }
}

// ── Demolish ──

/// Start the demolish animation on a building.
pub fn start_demolish(commands: &mut Commands, entity: Entity, transform: &Transform) {
    commands.entity(entity).insert(DemolishAnimation {
        timer: Timer::from_seconds(0.5, TimerMode::Once),
        original_scale: transform.scale,
    });
}

fn demolish_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    mut player_res: ResMut<PlayerResources>,
    mut completed: ResMut<CompletedBuildings>,
    mut buildings: Query<(
        Entity,
        &EntityKind,
        &mut Transform,
        &mut DemolishAnimation,
    ), With<Building>>,
) {
    for (entity, kind, mut transform, mut demolish) in &mut buildings {
        demolish.timer.tick(time.delta());

        let fraction = demolish.timer.fraction();
        // Lerp scale from original to zero
        transform.scale = demolish.original_scale * (1.0 - fraction);

        if demolish.timer.is_finished() {
            // Refund 50% of building cost
            let bp = registry.get(*kind);
            let cost = &bp.cost;
            player_res.wood += cost.wood / 2;
            player_res.copper += cost.copper / 2;
            player_res.iron += cost.iron / 2;
            player_res.gold += cost.gold / 2;
            player_res.oil += cost.oil / 2;

            // Remove from completed buildings
            completed.completed.retain(|k| k != kind);

            // Despawn
            commands.entity(entity).despawn();
        }
    }
}

// ── Building Scale Animation ──

fn building_scale_anim_system(
    mut commands: Commands,
    time: Res<Time>,
    mut buildings: Query<(Entity, &mut Transform, &mut BuildingScaleAnim)>,
) {
    for (entity, mut transform, mut anim) in &mut buildings {
        anim.timer.tick(time.delta());

        let t = anim.timer.fraction();
        // Ease-in-out (smoothstep)
        let eased = t * t * (3.0 - 2.0 * t);
        transform.scale = anim.from.lerp(anim.to, eased);

        if anim.timer.is_finished() {
            transform.scale = anim.to;
            commands.entity(entity).remove::<BuildingScaleAnim>();
        }
    }
}

// ── Aura Systems ──

fn healing_aura_system(
    time: Res<Time>,
    auras: Query<(&Transform, &HealingAura, &BuildingState), With<Building>>,
    mut healable: Query<(&Transform, &mut Health, &Faction), Without<Building>>,
) {
    for (aura_tf, aura, state) in &auras {
        if *state != BuildingState::Complete {
            continue;
        }
        for (unit_tf, mut health, faction) in &mut healable {
            if *faction != Faction::Player {
                continue;
            }
            let dist = aura_tf.translation.distance(unit_tf.translation);
            if dist <= aura.range && health.current < health.max {
                health.current =
                    (health.current + aura.heal_per_sec * time.delta_secs()).min(health.max);
            }
        }
    }
}

/// Returns the highest gather speed bonus from any StorageAura in range of the given position.
pub fn storage_aura_bonus(
    worker_pos: Vec3,
    auras: &Query<(&Transform, &StorageAura, &BuildingState), With<Building>>,
) -> f32 {
    let mut bonus = 0.0f32;
    for (aura_tf, aura, state) in auras {
        if *state != BuildingState::Complete {
            continue;
        }
        let dist = aura_tf.translation.distance(worker_pos);
        if dist <= aura.range {
            bonus = bonus.max(aura.gather_speed_bonus); // Don't stack, take highest
        }
    }
    bonus
}

// ── Level Indicator ──

fn level_indicator_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    building_models: Option<Res<BuildingModelAssets>>,
    registry: Res<BlueprintRegistry>,
    buildings: Query<
        (Entity, &BuildingLevel, &Transform, &EntityKind),
        (With<Building>, Changed<BuildingLevel>),
    >,
    existing_indicators: Query<(Entity, &LevelIndicator)>,
) {
    for (building_entity, level, transform, kind) in &buildings {
        if level.0 <= 1 {
            continue;
        }

        // Remove existing indicators for this building
        for (ind_entity, indicator) in &existing_indicators {
            if indicator.building == building_entity {
                commands.entity(ind_entity).try_despawn();
            }
        }

        // Spawn pip spheres above the building
        let pip_count = (level.0 - 1) as usize; // 1 for L2, 2 for L3
        let pip_mesh = meshes.add(Sphere::new(0.2));
        let pip_material = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.2),
            emissive: LinearRgba::new(2.0, 1.7, 0.4, 1.0),
            ..default()
        });

        let bp = registry.get(*kind);
        let base_y = if bp.visual.mesh_kind.is_gltf() {
            let height = building_models.as_ref()
                .and_then(|m| m.calibration.get(kind))
                .map(|c| c.building_height)
                .unwrap_or(4.0);
            transform.translation.y + height + 1.0
        } else {
            transform.translation.y + transform.scale.y * 2.0 + 1.0
        };

        for i in 0..pip_count {
            let x_offset = if pip_count == 1 {
                0.0
            } else {
                (i as f32 - (pip_count - 1) as f32 / 2.0) * 0.6
            };

            commands.spawn((
                LevelIndicator {
                    building: building_entity,
                },
                Mesh3d(pip_mesh.clone()),
                MeshMaterial3d(pip_material.clone()),
                Transform::from_translation(Vec3::new(
                    transform.translation.x + x_offset,
                    base_y,
                    transform.translation.z,
                ))
                .with_scale(Vec3::splat(1.0)),
            ));
        }
    }
}

// ── Sync Storage on Spend ──

fn sync_storage_on_spend(
    player_res: Res<PlayerResources>,
    mut last_res: ResMut<LastPlayerResources>,
    mut storages: Query<&mut StorageInventory, (With<Building>, With<DepositPoint>)>,
) {
    let resource_types = [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ];

    for rt in resource_types {
        let current = player_res.get(rt);
        let last = match rt {
            ResourceType::Wood => last_res.wood,
            ResourceType::Copper => last_res.copper,
            ResourceType::Iron => last_res.iron,
            ResourceType::Gold => last_res.gold,
            ResourceType::Oil => last_res.oil,
        };

        if current < last {
            let spent = last - current;
            let mut remaining = spent;

            for mut inv in &mut storages {
                if remaining == 0 {
                    break;
                }
                let share = inv.get(rt);
                let deduct = share.min(remaining);
                if deduct > 0 {
                    match rt {
                        ResourceType::Wood => inv.wood -= deduct,
                        ResourceType::Copper => inv.copper -= deduct,
                        ResourceType::Iron => inv.iron -= deduct,
                        ResourceType::Gold => inv.gold -= deduct,
                        ResourceType::Oil => inv.oil -= deduct,
                    }
                    remaining -= deduct;
                }
            }
        }
    }

    // Update last values
    last_res.wood = player_res.wood;
    last_res.copper = player_res.copper;
    last_res.iron = player_res.iron;
    last_res.gold = player_res.gold;
    last_res.oil = player_res.oil;
}

// ── Storage Pile Visuals ──

fn update_storage_piles(
    mut commands: Commands,
    pile_assets: Option<Res<StoragePileAssets>>,
    height_map: Res<HeightMap>,
    mut storages: Query<
        (Entity, &Transform, &mut StorageInventory, Option<&ResourcePileVisuals>),
        (With<Building>, With<DepositPoint>),
    >,
) {
    let Some(assets) = pile_assets else { return };

    for (entity, transform, mut inventory, pile_visuals) in &mut storages {
        let new_total = inventory.total();
        if new_total == inventory.last_total {
            continue;
        }
        inventory.last_total = new_total;

        // Despawn old pile visuals
        if let Some(piles) = pile_visuals {
            for pile_entity in &piles.entities {
                commands.entity(*pile_entity).try_despawn();
            }
        }

        let mut pile_entities = Vec::new();

        // Place piles in a ring on the ground around the building
        let radius = 4.0;
        let positions = [
            (ResourceType::Wood,   Vec2::new(radius, 0.0)),               // East
            (ResourceType::Copper, Vec2::new(0.0, radius)),               // North
            (ResourceType::Iron,   Vec2::new(-radius, 0.0)),              // West
            (ResourceType::Gold,   Vec2::new(0.0, -radius)),              // South
            (ResourceType::Oil,    Vec2::new(radius * 0.707, radius * 0.707)), // NE
        ];

        for (rt, offset) in positions {
            let amount = inventory.get(rt);
            if amount == 0 {
                continue;
            }

            let scale = (amount as f32 / 100.0).min(1.0) * 0.8 + 0.2;
            let half_pile_height = scale * 0.5;
            let (mesh, mat) = match rt {
                ResourceType::Wood => (assets.cube_mesh.clone(), assets.materials.get(&rt).cloned().unwrap_or_default()),
                ResourceType::Gold => (assets.sphere_mesh.clone(), assets.materials.get(&rt).cloned().unwrap_or_default()),
                ResourceType::Oil => (assets.cylinder_mesh.clone(), assets.materials.get(&rt).cloned().unwrap_or_default()),
                _ => (assets.cube_mesh.clone(), assets.materials.get(&rt).cloned().unwrap_or_default()),
            };

            let world_x = transform.translation.x + offset.x;
            let world_z = transform.translation.z + offset.y;
            let ground_y = height_map.sample(world_x, world_z);

            let pile = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_translation(Vec3::new(world_x, ground_y + half_pile_height, world_z))
                        .with_scale(Vec3::splat(scale)),
                ))
                .id();
            pile_entities.push(pile);
        }

        commands.entity(entity).insert(ResourcePileVisuals { entities: pile_entities });
    }
}
