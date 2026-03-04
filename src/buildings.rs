use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::components::*;
use crate::ground::terrain_height;
use crate::units::spawn_unit_of_type;

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingPlacementState>()
            .init_resource::<CompletedBuildings>()
            .add_systems(Startup, create_building_assets)
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

// ── Building data ──

/// Returns (wood, copper, iron, gold, oil, construction_time_secs)
pub fn building_cost(bt: BuildingType) -> (u32, u32, u32, u32, u32, f32) {
    match bt {
        BuildingType::Base => (100, 20, 0, 0, 0, 15.0),
        BuildingType::Barracks => (80, 40, 20, 0, 0, 12.0),
        BuildingType::Workshop => (60, 60, 40, 10, 0, 18.0),
        BuildingType::Tower => (40, 30, 30, 0, 0, 10.0),
        BuildingType::Storage => (60, 10, 0, 0, 0, 8.0),
    }
}

pub fn building_prerequisite(bt: BuildingType) -> Option<BuildingType> {
    match bt {
        BuildingType::Base => None,
        _ => Some(BuildingType::Base),
    }
}

/// Returns (wood, copper, iron, gold, oil, train_time_secs)
pub fn training_cost(ut: UnitType) -> (u32, u32, u32, u32, u32, f32) {
    match ut {
        UnitType::Worker => (30, 0, 0, 0, 0, 5.0),
        UnitType::Soldier => (10, 20, 10, 0, 0, 8.0),
        UnitType::Archer => (20, 10, 5, 0, 0, 7.0),
        UnitType::Tank => (0, 30, 40, 10, 5, 15.0),
    }
}

pub fn building_type_label(bt: BuildingType) -> &'static str {
    match bt {
        BuildingType::Base => "Base",
        BuildingType::Barracks => "Barracks",
        BuildingType::Workshop => "Workshop",
        BuildingType::Tower => "Tower",
        BuildingType::Storage => "Storage",
    }
}

fn building_half_height(bt: BuildingType) -> f32 {
    match bt {
        BuildingType::Base => 1.5,
        BuildingType::Barracks => 1.25,
        BuildingType::Workshop => 1.5,
        BuildingType::Tower => 3.0,
        BuildingType::Storage => 0.75,
    }
}

fn building_hp(bt: BuildingType) -> f32 {
    match bt {
        BuildingType::Base => 500.0,
        BuildingType::Barracks => 300.0,
        BuildingType::Workshop => 350.0,
        BuildingType::Tower => 200.0,
        BuildingType::Storage => 250.0,
    }
}

// ── Asset creation ──

fn create_building_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(BuildingMeshes {
        base: meshes.add(Cuboid::new(4.0, 3.0, 4.0)),
        barracks: meshes.add(Cuboid::new(5.0, 2.5, 3.0)),
        workshop: meshes.add(Cuboid::new(4.0, 3.0, 4.0)),
        tower: meshes.add(Cylinder::new(1.0, 6.0)),
        storage: meshes.add(Cuboid::new(3.0, 1.5, 3.0)),
    });

    commands.insert_resource(BuildingMaterials {
        base: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.55, 0.45),
            ..default()
        }),
        barracks: materials.add(StandardMaterial {
            base_color: Color::srgb(0.7, 0.3, 0.25),
            ..default()
        }),
        workshop: materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.45, 0.5),
            ..default()
        }),
        tower: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.55, 0.6),
            ..default()
        }),
        storage: materials.add(StandardMaterial {
            base_color: Color::srgb(0.5, 0.4, 0.25),
            ..default()
        }),
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
    let Ok(window) = windows.get_single() else {
        return None;
    };
    let cursor = window.cursor_position()?;
    let Ok((camera, cam_gt)) = camera_q.get_single() else {
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
    building_meshes: Res<BuildingMeshes>,
    building_mats: Res<BuildingMaterials>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ghosts: Query<
        (&mut Transform, &mut MeshMaterial3d<StandardMaterial>),
        With<GhostBuilding>,
    >,
    existing_buildings: Query<&Transform, (With<Building>, Without<GhostBuilding>)>,
    biome_map: Option<Res<BiomeMap>>,
) {
    let PlacementMode::Placing(bt) = placement.mode else {
        return;
    };

    // Spawn ghost if it doesn't exist
    if placement.preview_entity.is_none() {
        let ghost = commands
            .spawn((
                GhostBuilding,
                Mesh3d(building_meshes.mesh_for(bt)),
                MeshMaterial3d(building_mats.ghost_valid.clone()),
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

    let half_h = building_half_height(bt);
    let y = terrain_height(world_pos.x, world_pos.z) + half_h;
    ghost_tf.translation = Vec3::new(world_pos.x, y, world_pos.z);

    // Check placement validity
    let mut valid = true;

    // Not on water
    if let Some(ref bm) = biome_map {
        if bm.get_biome(world_pos.x, world_pos.z) == Biome::Water {
            valid = false;
        }
    }

    // Not overlapping existing buildings (simple distance check)
    for building_tf in &existing_buildings {
        if building_tf
            .translation
            .distance(ghost_tf.translation)
            < 5.0
        {
            valid = false;
            break;
        }
    }

    // Within map bounds
    let half_map = 250.0;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        valid = false;
    }

    *ghost_mat = if valid {
        MeshMaterial3d(building_mats.ghost_valid.clone())
    } else {
        MeshMaterial3d(building_mats.ghost_invalid.clone())
    };
}

fn confirm_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut player_res: ResMut<PlayerResources>,
    completed: Res<CompletedBuildings>,
    building_meshes: Res<BuildingMeshes>,
    building_mats: Res<BuildingMaterials>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    existing_buildings: Query<&Transform, (With<Building>, Without<GhostBuilding>)>,
    biome_map: Option<Res<BiomeMap>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let PlacementMode::Placing(bt) = placement.mode else {
        return;
    };

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };

    // Check prerequisite
    let prereq_met = match building_prerequisite(bt) {
        None => true,
        Some(BuildingType::Base) => completed.has_base,
        Some(_) => false,
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
        let check_pos = Vec3::new(
            world_pos.x,
            building_tf.translation.y,
            world_pos.z,
        );
        if building_tf.translation.distance(check_pos) < 5.0 {
            return;
        }
    }
    let half_map = 250.0;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        return;
    }

    // Check affordability
    let (w, c, i, g, o, construction_time) = building_cost(bt);
    if !player_res.can_afford(w, c, i, g, o) {
        return;
    }

    // Deduct resources
    player_res.subtract(w, c, i, g, o);

    // Despawn ghost
    if let Some(ghost) = placement.preview_entity {
        commands.entity(ghost).despawn();
    }

    // Spawn building
    let half_h = building_half_height(bt);
    let y = terrain_height(world_pos.x, world_pos.z) + half_h;

    let mut entity_cmds = commands.spawn((
        Building,
        bt,
        BuildingState::UnderConstruction,
        ConstructionProgress {
            timer: Timer::from_seconds(construction_time, TimerMode::Once),
        },
        Faction::Player,
        Health {
            current: building_hp(bt),
            max: building_hp(bt),
        },
        Mesh3d(building_meshes.mesh_for(bt)),
        MeshMaterial3d(building_mats.under_construction.clone()),
        Transform::from_translation(Vec3::new(world_pos.x, y, world_pos.z)),
    ));

    // Tower gets combat components
    if bt == BuildingType::Tower {
        entity_cmds.insert((
            AttackRange(15.0),
            AttackDamage(10.0),
            AttackCooldown {
                timer: Timer::from_seconds(2.0, TimerMode::Repeating),
            },
        ));
    }

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
    }
}

// ── Construction ──

fn construction_progress_system(
    mut commands: Commands,
    time: Res<Time>,
    building_mats: Res<BuildingMaterials>,
    mut buildings: Query<(
        Entity,
        &BuildingType,
        &mut BuildingState,
        &mut ConstructionProgress,
    )>,
) {
    for (entity, bt, mut state, mut progress) in &mut buildings {
        if *state != BuildingState::UnderConstruction {
            continue;
        }
        progress.timer.tick(time.delta());
        if progress.timer.finished() {
            *state = BuildingState::Complete;

            // Swap to final material
            commands
                .entity(entity)
                .insert(MeshMaterial3d(building_mats.material_for(*bt)))
                .remove::<ConstructionProgress>();

            // Add training queue for production buildings
            match bt {
                BuildingType::Barracks | BuildingType::Workshop | BuildingType::Base => {
                    commands.entity(entity).insert(TrainingQueue {
                        queue: vec![],
                        timer: None,
                    });
                }
                _ => {}
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
            &BuildingType,
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

    for (tower_tf, bt, state, mut cooldown, damage, range) in &mut towers {
        if *bt != BuildingType::Tower || *state != BuildingState::Complete {
            continue;
        }

        cooldown.timer.tick(time.delta());
        if !cooldown.timer.just_finished() {
            continue;
        }

        // Find closest mob in range
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
    mut buildings: Query<(&Transform, &BuildingType, &mut TrainingQueue), With<Building>>,
    unit_mats: Res<UnitMaterials>,
    unit_meshes: Res<UnitMeshes>,
) {
    for (transform, _bt, mut queue) in &mut buildings {
        if queue.queue.is_empty() {
            continue;
        }

        // Start timer for first item if not started
        if queue.timer.is_none() {
            let ut = queue.queue[0];
            let (_, _, _, _, _, train_time) = training_cost(ut);
            queue.timer = Some(Timer::from_seconds(train_time, TimerMode::Once));
        }

        if let Some(ref mut timer) = queue.timer {
            timer.tick(time.delta());
            if timer.finished() {
                let ut = queue.queue.remove(0);
                let spawn_pos = transform.translation + Vec3::new(3.0, 0.0, 3.0);
                spawn_unit_of_type(&mut commands, &unit_mats, &unit_meshes, ut, spawn_pos);
                queue.timer = None;
            }
        }
    }
}

// ── Track completed buildings ──

fn update_completed_buildings_tracker(
    mut completed: ResMut<CompletedBuildings>,
    buildings: Query<(&BuildingType, &BuildingState), With<Building>>,
) {
    let mut has_base = false;
    let mut has_barracks = false;
    let mut has_workshop = false;

    for (bt, state) in &buildings {
        if *state == BuildingState::Complete {
            match bt {
                BuildingType::Base => has_base = true,
                BuildingType::Barracks => has_barracks = true,
                BuildingType::Workshop => has_workshop = true,
                _ => {}
            }
        }
    }

    completed.has_base = has_base;
    completed.has_barracks = has_barracks;
    completed.has_workshop = has_workshop;
}
