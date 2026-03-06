use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::blueprints::{BlueprintRegistry, EntityCategory, EntityKind, EntityVisualCache, spawn_from_blueprint};
use crate::components::*;
use crate::ground::terrain_height;

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingPlacementState>()
            .init_resource::<CompletedBuildings>()
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
    existing_buildings: Query<&Transform, (With<Building>, Without<GhostBuilding>)>,
    biome_map: Option<Res<BiomeMap>>,
) {
    let PlacementMode::Placing(kind) = placement.mode else {
        return;
    };

    let bp = registry.get(kind);
    let half_h = bp.building.as_ref().map(|b| b.half_height).unwrap_or(1.0);

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

    let y = terrain_height(world_pos.x, world_pos.z) + half_h;
    ghost_tf.translation = Vec3::new(world_pos.x, y, world_pos.z);

    let mut valid = true;

    if let Some(ref bm) = biome_map {
        if bm.get_biome(world_pos.x, world_pos.z) == Biome::Water {
            valid = false;
        }
    }

    for building_tf in &existing_buildings {
        if building_tf.translation.distance(ghost_tf.translation) < 5.0 {
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
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    existing_buildings: Query<&Transform, (With<Building>, Without<GhostBuilding>)>,
    biome_map: Option<Res<BiomeMap>>,
) {
    let PlacementMode::Placing(kind) = placement.mode else {
        return;
    };

    // Phase 1: awaiting initial mouse release
    if placement.awaiting_release {
        if mouse.just_released(MouseButton::Left) {
            placement.awaiting_release = false;

            if let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) {
                let on_water = biome_map.as_ref()
                    .map_or(false, |bm| bm.get_biome(world_pos.x, world_pos.z) == Biome::Water);
                let too_close = existing_buildings.iter().any(|building_tf| {
                    let check_pos = Vec3::new(world_pos.x, building_tf.translation.y, world_pos.z);
                    building_tf.translation.distance(check_pos) < 5.0
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
    for building_tf in &existing_buildings {
        let check_pos = Vec3::new(world_pos.x, building_tf.translation.y, world_pos.z);
        if building_tf.translation.distance(check_pos) < 5.0 {
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
    let entity_id = spawn_from_blueprint(&mut commands, &cache, kind, world_pos, &registry);

    // Override material with under_construction
    commands.entity(entity_id).insert(
        MeshMaterial3d(ghost_mats.under_construction.clone()),
    );

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
    )>,
) {
    for (entity, kind, mut state, mut progress) in &mut buildings {
        if *state != BuildingState::UnderConstruction {
            continue;
        }
        progress.timer.tick(time.delta());
        if progress.timer.is_finished() {
            *state = BuildingState::Complete;

            // Swap to final material
            if let Some(mat) = cache.materials_default.get(kind) {
                commands
                    .entity(entity)
                    .insert(MeshMaterial3d(mat.clone()))
                    .remove::<ConstructionProgress>();
            }

            // Add training queue for production buildings
            let bp = registry.get(*kind);
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
        ),
        With<Building>,
    >,
    mobs: Query<(Entity, &Transform), With<Mob>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (tower_tf, kind, state, mut cooldown, damage, range) in &mut towers {
        if *kind != EntityKind::Tower || *state != BuildingState::Complete {
            continue;
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
    mut buildings: Query<(&Transform, &EntityKind, &mut TrainingQueue), With<Building>>,
) {
    for (transform, _kind, mut queue) in &mut buildings {
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
                spawn_from_blueprint(&mut commands, &cache, unit_kind, spawn_pos, &registry);
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
