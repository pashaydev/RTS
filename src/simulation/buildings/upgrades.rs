//! Building upgrades, demolish, scale animations, and level indicators.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::time::Fixed;

use crate::blueprints::{BlueprintRegistry, EntityKind, LevelBonus};
use crate::presentation::model_assets::BuildingModelAssets;
use crate::types::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline};

// ── Building Upgrade ──

/// Start an upgrade on a building. Returns true if the upgrade was started.
pub fn start_upgrade(
    commands: &mut Commands,
    entity: Entity,
    current_level: u8,
    kind: EntityKind,
    registry: &BlueprintRegistry,
    player_res: &mut PlayerResources,
    faction: Faction,
    carried: &PlayerResources,
    pending_drains: &mut PendingCarriedDrains,
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

    // Check affordability (stored + carried)
    if !level_data.cost.can_afford_with_carried(player_res, carried) {
        return false;
    }

    // Deduct from stored first, queue carried drain for deficit
    let deficits = level_data.cost.deduct_with_carried(player_res);
    let drain = SpendFromCarried {
        faction,
        amounts: deficits,
    };
    if drain.has_deficit() {
        pending_drains.drains.push(drain);
    }

    // Insert UpgradeProgress component
    commands.entity(entity).insert(UpgradeProgress {
        timer: Timer::from_seconds(level_data.time_secs, TimerMode::Once),
        target_level: current_level + 1,
    });

    true
}

pub(super) fn building_upgrade_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    registry: Res<BlueprintRegistry>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    building_models: Option<Res<BuildingModelAssets>>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut buildings: Query<
        (
            Entity,
            &EntityKind,
            &mut BuildingLevel,
            &mut UpgradeProgress,
            &Transform,
            &Faction,
            Option<&mut VisionRange>,
            Option<&mut AttackRange>,
            Option<&mut AttackDamage>,
            Option<&mut StorageInventory>,
            Option<&mut ResourceProcessor>,
            Option<&mut ResourceRespawnConfig>,
        ),
        With<Building>,
    >,
    children_q: Query<&Children>,
    scene_child_q: Query<Entity, With<BuildingSceneChild>>,
) {
    for (
        entity,
        kind,
        mut level,
        mut upgrade,
        transform,
        faction,
        vision,
        attack_range,
        attack_damage,
        mut storage_inv,
        processor,
        respawn_config,
    ) in &mut buildings
    {
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
                if let Some(new_scene) = models.scene_for(*kind, new_level, transform.translation) {
                    // Despawn old scene child
                    if let Ok(children) = children_q.get(entity) {
                        for child in children.iter() {
                            if scene_child_q.contains(child) {
                                commands.entity(child).try_despawn();
                            }
                        }
                    }
                    // Spawn new scene child with calibration
                    let mut child = commands.spawn((
                        SceneRoot(new_scene),
                        BuildingSceneChild,
                        models.child_transform(*kind, 1.0),
                    ));
                    #[cfg(not(target_arch = "wasm32"))]
                    child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                    let child = child.id();
                    commands.entity(entity).add_child(child);
                    // Remove TeamColorApplied so the new scene gets recolored
                    commands.entity(entity).remove::<TeamColorApplied>();
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
            LevelBonus::RangeAndDamage {
                range_boost,
                damage_boost,
            } => {
                if let Some(mut ar) = attack_range {
                    ar.0 += range_boost;
                }
                if let Some(mut ad) = attack_damage {
                    ad.0 += damage_boost;
                }
            }
            LevelBonus::GatherAura { speed_bonus, range } => {
                commands.entity(entity).insert(StorageAura {
                    gather_speed_bonus: *speed_bonus,
                    range: *range,
                });
            }
            LevelBonus::HealAura {
                heal_per_sec,
                range,
            } => {
                commands.entity(entity).insert(HealingAura {
                    heal_per_sec: *heal_per_sec,
                    range: *range,
                });
            }
            LevelBonus::UnlocksTraining(_kinds) => {
                // Handled at UI level — train button filtering checks building level
            }
            LevelBonus::ProcessorUpgrade {
                harvest_rate_boost,
                radius_boost,
                extra_worker_slots,
                ref unlock_resources,
            } => {
                if let Some(mut proc) = processor {
                    proc.harvest_rate += harvest_rate_boost;
                    proc.harvest_radius += radius_boost;
                    proc.max_workers += extra_worker_slots;
                    for rt in unlock_resources {
                        if !proc.resource_types.contains(rt) {
                            proc.resource_types.push(*rt);
                        }
                    }
                }
                if let Some(mut rc) = respawn_config {
                    for rt in unlock_resources {
                        if !rc.resource_types.contains(rt) {
                            rc.resource_types.push(*rt);
                        }
                    }
                    // Increase max nodes on upgrade
                    rc.max_nodes = (rc.max_nodes + 2).min(12);
                    // Reduce respawn timer slightly
                    let current_secs = rc.respawn_timer.duration().as_secs_f32();
                    rc.respawn_timer =
                        Timer::from_seconds((current_secs * 0.75).max(10.0), TimerMode::Repeating);
                }
                // Grant storage capacity for newly unlocked resource types
                if let Some(ref mut inv) = storage_inv {
                    for rt in unlock_resources {
                        if inv.caps[rt.index()] == 0 {
                            inv.caps[rt.index()] = 500;
                        }
                    }
                }
            }
            LevelBonus::UnlockRecipe {
                recipe_index: _,
                extra_worker_slots,
            } => {
                // Recipe unlock is checked at runtime via building level vs requires_level
                if let Some(mut proc) = processor {
                    proc.max_workers += extra_worker_slots;
                }
            }
            LevelBonus::ProductionSpeedMultiplier(_mult) => {
                // Applied at runtime in production_chain_system by checking building level
            }
        }

        // Scale storage capacities +15% on any upgrade for buildings with storage
        if let Some(mut inv) = storage_inv {
            inv.scale_caps(1.15);
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
                        rise_speed: 0.7,
                    },
                    Mesh3d(vfx.sphere_mesh.clone()),
                    MeshMaterial3d(vfx.impact_material.clone()),
                    Transform::from_translation(center + offset).with_scale(Vec3::splat(0.8)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }

        // Log upgrade complete
        event_log.push(
            time.elapsed_secs(),
            format!("{} upgraded to L{}", kind.display_name(), new_level),
            crate::ui::event_log_widget::EventCategory::Upgrade,
            Some(transform.translation),
            Some(*faction),
        );

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

pub(super) fn demolish_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    registry: Res<BlueprintRegistry>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut all_resources: ResMut<AllPlayerResources>,
    mut wall_grid: ResMut<WallGrid>,
    mut floor_grid: ResMut<FloorGrid>,
    grid_coord_q: Query<&WallGridCoord>,
    floor_coord_q: Query<&FloorGridCoord>,
    yard_q: Query<&SawmillYard>,
    mut buildings: Query<
        (
            Entity,
            &EntityKind,
            &mut Transform,
            &mut DemolishAnimation,
            &Faction,
        ),
        With<Building>,
    >,
) {
    for (entity, kind, mut transform, mut demolish, faction) in &mut buildings {
        demolish.timer.tick(time.delta());

        let fraction = demolish.timer.fraction();
        // Lerp scale from original to zero
        transform.scale = demolish.original_scale * (1.0 - fraction);

        if demolish.timer.is_finished() {
            // Log demolish event
            event_log.push(
                time.elapsed_secs(),
                format!("{} demolished", kind.display_name()),
                crate::ui::event_log_widget::EventCategory::Demolish,
                Some(transform.translation),
                Some(*faction),
            );

            // Refund 50% of building cost
            let bp = registry.get(*kind);
            let cost = &bp.cost;
            let res = all_resources.get_mut(faction);
            for (rt, amt) in cost.cost_entries() {
                res.add(rt, amt / 2);
            }

            // Remove from wall grid if this was a wall piece
            if let Ok(coord) = grid_coord_q.get(entity) {
                let (gx, gz) = (coord.0, coord.1);
                wall_grid.cells.remove(&(gx, gz));
                // Mark neighbors dirty so they re-tile
                for (nx, nz) in WallGrid::cardinal_neighbors(gx, gz) {
                    wall_grid.dirty.push((nx, nz));
                }
            }

            if let Ok(coord) = floor_coord_q.get(entity) {
                floor_grid.cells.remove(&(coord.0, coord.1));
                floor_grid.mark_dirty(coord.0, coord.1);
            }

            // Clean up sawmill yard entities
            if let Ok(yard) = yard_q.get(entity) {
                for &e in yard.fence_entities.iter().chain(yard.tree_entities.iter()) {
                    commands.entity(e).try_despawn();
                }
            }

            // Despawn
            commands.entity(entity).try_despawn();
        }
    }
}

// ── Building Scale Animation ──

pub(super) fn building_scale_anim_system(
    mut commands: Commands,
    time: Res<Time>,
    mut buildings: Query<(Entity, &mut Transform, &mut BuildingScaleAnim), Without<FrustumCulled>>,
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

// ── Level Indicator ──

pub(super) fn level_indicator_system(
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
            let height = building_models
                .as_ref()
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
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}
