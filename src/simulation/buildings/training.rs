//! Training queue, tower auto-attack, unit ejection, and building tracker systems.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::time::Fixed;

use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityCategory, EntityKind,
    EntityVisualCache, LevelBonus,
};
use crate::infrastructure::net_bridge::NetworkId;
use crate::presentation::model_assets::UnitModelAssets;
use crate::types::*;
use crate::world::ground::HeightMap;

// ── Unit ejection from newly placed buildings ──

/// When a building entity is first added, teleport any overlapping units to the
/// nearest valid position outside its footprint so they don't get trapped.
pub(super) fn eject_units_from_buildings(
    mut commands: Commands,
    new_buildings: Query<
        (Entity, &Transform, &BuildingFootprint),
        (With<Building>, Added<Building>),
    >,
    nav_grid: Option<Res<crate::world::pathfinding::NavGrid>>,
    all_buildings: Query<(Entity, &Transform, &BuildingFootprint, &BuildingState), With<Building>>,
    mut units: Query<
        (Entity, &mut Transform, Option<&NetworkId>),
        (Or<(With<Unit>, With<Mob>)>, Without<Building>),
    >,
) {
    for (building_entity, building_tf, footprint) in &new_buildings {
        let building_pos = building_tf.translation;
        let eject_radius = footprint.0 + 1.0;

        // Collect the ejection list in a deterministic order so every peer
        // processes displaced units in the same sequence. Using NetworkId
        // (falling back to u32::MAX for anything not yet assigned) keeps
        // the ordering stable even across different archetype layouts.
        let mut trapped: Vec<(u32, Entity, Vec3)> = units
            .iter()
            .filter_map(|(entity, tf, net_id)| {
                let diff = tf.translation - building_pos;
                let flat_dist = Vec2::new(diff.x, diff.z).length();
                if flat_dist >= eject_radius {
                    return None;
                }
                Some((net_id.map(|id| id.0).unwrap_or(u32::MAX), entity, tf.translation))
            })
            .collect();
        trapped.sort_by_key(|(net, _, _)| *net);

        for (spawn_index, unit_entity, _pos) in trapped {
            let safe_pos = find_trained_unit_spawn_position(
                building_entity,
                building_pos,
                footprint.0,
                spawn_index,
                nav_grid.as_deref(),
                &all_buildings,
            );
            if let Ok((_, mut unit_tf, _)) = units.get_mut(unit_entity) {
                unit_tf.translation.x = safe_pos.x;
                unit_tf.translation.z = safe_pos.z;
            }
            commands.entity(unit_entity).insert(MoveTarget(safe_pos));
        }
    }
}

// ── Tower auto-attack ──

pub(super) fn tower_auto_attack(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    teams: Res<TeamConfig>,
    vfx_assets: Option<Res<VfxAssets>>,
    projectile_assets: Option<Res<crate::presentation::model_assets::ProjectileModelAssets>>,
    mut towers: Query<
        (
            Entity,
            &Transform,
            &EntityKind,
            &BuildingState,
            &mut AttackCooldown,
            &AttackDamage,
            &AttackRange,
            &AttackProfile,
            &CombatFxKind,
            Option<&AttackTiming>,
            Option<&TowerAutoAttackEnabled>,
            &Faction,
            Option<&TargetingProfile>,
            Option<&DamageType>,
            Option<&NetworkId>,
        ),
        With<Building>,
    >,
    mut hostiles: Query<
        (
            Entity,
            &Transform,
            &Faction,
            &Health,
            &ArmorType,
            Option<&ThreatValue>,
            Option<&mut ReservedIncomingDamage>,
            Option<&NetworkId>,
        ),
        Or<(With<Mob>, With<Unit>)>,
    >,
) {
    let Some(vfx) = vfx_assets else { return };

    for (
        tower_entity,
        tower_tf,
        kind,
        state,
        mut cooldown,
        damage,
        range,
        attack_profile,
        fx_kind,
        attack_timing,
        auto_attack,
        tower_faction,
        opt_profile,
        opt_dmg_type,
        tower_net_id,
    ) in &mut towers
    {
        if !kind.uses_tower_auto_attack() || *state != BuildingState::Complete {
            continue;
        }

        // Check if auto-attack is disabled
        if let Some(enabled) = auto_attack {
            if !enabled.0 {
                continue;
            }
        }

        cooldown.ready_in = (cooldown.ready_in - time.delta_secs()).max(0.0);
        if cooldown.ready_in > 0.0 {
            continue;
        }

        let mut best_score = f32::MAX;
        let mut best_key: Option<(i64, u32)> = None; // (quantized_score*1e3, target_net_id)
        let mut best_target: Option<(Entity, f32)> = None; // (entity, travel_dist)
        let tower_dmg_type = opt_dmg_type.copied().unwrap_or(DamageType::Pierce);
        let minimum_range = attack_timing.map_or(0.0, |timing| timing.minimum_range);

        for (
            target_entity,
            target_tf,
            target_faction,
            t_health,
            t_armor,
            t_threat,
            t_reserved,
            target_net_id,
        ) in hostiles.iter()
        {
            if !teams.is_hostile(tower_faction, target_faction) {
                continue;
            }

            let surface_dist = crate::simulation::combat::attack_surface_distance(
                tower_tf.translation,
                target_tf.translation,
                0.0,
            );
            if !crate::simulation::combat::is_in_attack_band(
                surface_dist,
                range.0,
                minimum_range,
                0.1,
            ) {
                continue;
            }

            let target_nid_key = target_net_id.map(|id| id.0).unwrap_or(u32::MAX);
            if let Some(profile) = opt_profile {
                let reserved_total = t_reserved.as_ref().map_or(0.0, |r| r.total());
                if let Some(score) = crate::simulation::combat::target_score(
                    &crate::simulation::combat::TargetScoreInput {
                        profile,
                        attacker_pos: tower_tf.translation,
                        attacker_damage_type: tower_dmg_type,
                        scan_range: range.0,
                        target_pos: target_tf.translation,
                        target_health: t_health,
                        target_armor: *t_armor,
                        target_threat: t_threat.map_or(0.0, |t| t.0),
                        target_is_building: false,
                        target_reserved_damage: reserved_total,
                    },
                ) {
                    // Quantize the float score to 1e-3 so ties on the
                    // score round to the same bucket on every peer; within
                    // that bucket, smaller NetworkId wins. This replaces
                    // "first through the query wins" which was Bevy
                    // iteration-order dependent.
                    let quantized = (score * 1000.0).round() as i64;
                    let key = (quantized, target_nid_key);
                    if best_key.map_or(true, |b| key < b) {
                        best_score = score;
                        best_key = Some(key);
                        best_target = Some((target_entity, surface_dist));
                    }
                }
            } else {
                // Fallback: nearest enemy, tie-broken by NetworkId.
                let quantized = (surface_dist * 1000.0).round() as i64;
                let key = (quantized, target_nid_key);
                if best_key.map_or(true, |b| key < b) {
                    best_score = surface_dist;
                    best_key = Some(key);
                    best_target = Some((target_entity, surface_dist));
                }
            }
        }
        let _ = best_score;

        if let Some((target_entity, travel_dist)) = best_target {
            cooldown.ready_in = cooldown.interval;

            // Add damage reservation on target
            let projectile_speed = attack_profile.projectile_speed.max(8.0);
            let ttl = travel_dist / projectile_speed + attack_profile.windup_secs + 0.35;
            if let Ok((_, _, _, _, _, _, Some(mut reserved), _)) = hostiles.get_mut(target_entity) {
                reserved.reservations.push((tower_entity, damage.0, ttl));
            }

            let proj_visual = crate::presentation::model_assets::projectile_visual_for(*kind);
            let orient = proj_visual.is_some()
                && !matches!(
                    proj_visual,
                    Some(crate::presentation::model_assets::ProjectileVisualKind::CatapultRock)
                );
            let spawn_pos = tower_tf.translation + Vec3::Y * 3.0;
            let target_tf = hostiles
                .get(target_entity)
                .map(|(_, tf, ..)| tf.translation)
                .unwrap_or(spawn_pos);
            let dir = (target_tf - spawn_pos).normalize_or_zero();
            let proj_component = Projectile {
                source: tower_entity,
                target: target_entity,
                velocity: dir * projectile_speed,
                speed: projectile_speed,
                damage: damage.0,
                damage_type: tower_dmg_type,
                fx_kind: *fx_kind,
                impact_scale: attack_profile.impact_scale,
                lifetime_secs: travel_dist / projectile_speed.max(0.1) + 0.35,
                orient_to_velocity: orient,
            };
            if let (Some(visual_kind), Some(ref proj_res)) = (proj_visual, &projectile_assets) {
                let proj_scale = match visual_kind {
                    crate::presentation::model_assets::ProjectileVisualKind::Arrow => 0.35,
                    crate::presentation::model_assets::ProjectileVisualKind::Bolt => 0.4,
                    crate::presentation::model_assets::ProjectileVisualKind::CatapultRock => 0.5,
                };
                let rotation = if orient {
                    Quat::from_rotation_arc(Vec3::Z, dir)
                } else {
                    Quat::IDENTITY
                };
                // Pick a projectile model variant from a stable id so every
                // peer agrees on the visual. NetworkId is the portable key;
                // we only fall back to 0 before the id lands (cosmetic
                // variation for a single tick).
                let variant_key = tower_net_id.map(|id| id.0 as usize).unwrap_or(0);
                let scene = proj_res.scene_for(visual_kind, variant_key);
                commands.spawn((
                    proj_component,
                    SceneRoot(scene),
                    Transform::from_translation(spawn_pos)
                        .with_rotation(rotation)
                        .with_scale(Vec3::splat(proj_scale)),
                ));
            } else {
                commands.spawn((
                    proj_component,
                    Mesh3d(vfx.sphere_mesh.clone()),
                    MeshMaterial3d(vfx.projectile_material.clone()),
                    Transform::from_translation(spawn_pos).with_scale(Vec3::splat(0.2)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            };
        }
    }
}

// ── Training ──

fn is_valid_trained_unit_spawn(
    pos: Vec3,
    source_building: Entity,
    source_pos: Vec3,
    source_footprint: f32,
    nav_grid: Option<&crate::world::pathfinding::NavGrid>,
    buildings: &Query<(Entity, &Transform, &BuildingFootprint, &BuildingState), With<Building>>,
) -> bool {
    if pos.distance(source_pos) < source_footprint + 1.2 {
        return false;
    }

    if nav_grid.is_some_and(|grid| !grid.is_world_passable(pos.x, pos.z)) {
        return false;
    }

    for (building_entity, tf, footprint, state) in buildings {
        if building_entity == source_building {
            continue;
        }
        if *state != BuildingState::Complete && *state != BuildingState::UnderConstruction {
            continue;
        }
        if tf.translation.distance(pos) < footprint.0 + 1.2 {
            return false;
        }
    }

    true
}

pub(super) fn find_trained_unit_spawn_position(
    source_building: Entity,
    source_pos: Vec3,
    source_footprint: f32,
    spawn_index: u32,
    nav_grid: Option<&crate::world::pathfinding::NavGrid>,
    buildings: &Query<(Entity, &Transform, &BuildingFootprint, &BuildingState), With<Building>>,
) -> Vec3 {
    let base_angle = std::f32::consts::TAU * (spawn_index as f32 * 0.618034);
    let base_radius = source_footprint + 1.8;

    for ring in 0..8 {
        let radius = base_radius + ring as f32 * 1.2;
        let samples = 10 + ring * 4;
        for step in 0..samples {
            let angle = base_angle + std::f32::consts::TAU * (step as f32 / samples as f32);
            let pos = source_pos + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
            if is_valid_trained_unit_spawn(
                pos,
                source_building,
                source_pos,
                source_footprint,
                nav_grid,
                buildings,
            ) {
                return pos;
            }
        }
    }

    source_pos + Vec3::new(base_radius, 0.0, 0.0)
}

pub(super) fn training_queue_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    nav_grid: Option<Res<crate::world::pathfinding::NavGrid>>,
    unit_factions: Query<&Faction, With<Unit>>,
    cap_buildings: Query<(&Faction, &EntityKind, &BuildingState, &BuildingLevel), With<Building>>,
    building_footprints: Query<
        (Entity, &Transform, &BuildingFootprint, &BuildingState),
        With<Building>,
    >,
    mut buildings: Query<
        (
            Entity,
            &Transform,
            &BuildingFootprint,
            &EntityKind,
            &mut TrainingQueue,
            Option<&RallyPoint>,
            &Faction,
            &BuildingLevel,
        ),
        With<Building>,
    >,
    mut event_log: ResMut<crate::ui::widgets::event_log_widget::GameEventLog>,
) {
    let mut used_by_faction: std::collections::HashMap<Faction, u32> =
        std::collections::HashMap::new();
    for faction in &unit_factions {
        *used_by_faction.entry(*faction).or_default() += 1;
    }

    let mut cap_by_faction: std::collections::HashMap<Faction, u32> = Faction::PLAYERS
        .into_iter()
        .map(|faction| (faction, DEFAULT_UNIT_CAP))
        .collect();
    for (faction, kind, state, level) in &cap_buildings {
        if *state != BuildingState::Complete {
            continue;
        }
        *cap_by_faction.entry(*faction).or_default() +=
            unit_capacity_bonus_for_building(*kind, level.0);
    }

    for (
        building_entity,
        transform,
        building_footprint,
        building_kind,
        mut queue,
        rally_point,
        building_faction,
        building_level,
    ) in &mut buildings
    {
        if queue.queue.is_empty() {
            continue;
        }

        // Start timer for first item if not started
        if queue.timer.is_none() {
            let unit_kind = queue.queue[0];
            let bp = registry.get(unit_kind);
            let mut train_secs = bp.train_time_secs;

            // Apply TrainTimeMultiplier from building level bonuses
            let building_bp = registry.get(*building_kind);
            if let Some(ref bd) = building_bp.building {
                for (i, ld) in bd.level_upgrades.iter().enumerate() {
                    if (i as u8 + 2) <= building_level.0 {
                        if let LevelBonus::TrainTimeMultiplier(mult) = ld.bonus {
                            train_secs *= mult;
                        }
                    }
                }
            }

            queue.timer = Some(Timer::from_seconds(train_secs, TimerMode::Once));
        }

        if let Some(ref mut timer) = queue.timer {
            timer.tick(time.delta());
            if timer.is_finished() {
                let used = used_by_faction.get(building_faction).copied().unwrap_or(0);
                let cap = cap_by_faction
                    .get(building_faction)
                    .copied()
                    .unwrap_or(DEFAULT_UNIT_CAP);
                if used >= cap {
                    continue;
                }

                let unit_kind = queue.queue.remove(0);

                // Scatter spawn positions around the building to avoid stacking
                let spawn_index = queue.total_trained;
                queue.total_trained = queue.total_trained.wrapping_add(1);
                let spawn_pos = find_trained_unit_spawn_position(
                    building_entity,
                    transform.translation,
                    building_footprint.0,
                    spawn_index,
                    nav_grid.as_deref(),
                    &building_footprints,
                );

                let unit_entity = spawn_from_blueprint_with_faction(
                    &mut commands,
                    &cache,
                    unit_kind,
                    spawn_pos,
                    &registry,
                    None,
                    unit_models.as_deref(),
                    &height_map,
                    *building_faction,
                );

                // Log training complete
                event_log.push(
                    time.elapsed_secs(),
                    format!("{} trained", unit_kind.display_name()),
                    crate::ui::widgets::event_log_widget::EventCategory::Training,
                    Some(spawn_pos),
                    Some(*building_faction),
                );

                *used_by_faction.entry(*building_faction).or_default() += 1;

                // If building has a rally point, set UnitState::Moving so the
                // unit actually walks there (plain MoveTarget gets stripped by
                // the unit-state executor when state is Idle).
                if let Some(rally) = rally_point {
                    commands
                        .entity(unit_entity)
                        .insert(MoveTarget(rally.0))
                        .insert(UnitState::Moving(rally.0));
                }

                queue.timer = None;
            }
        }
    }
}

// ── Track completed buildings ──

pub(super) fn update_completed_buildings_tracker(
    mut all_completed: ResMut<AllCompletedBuildings>,
    mut base_state: ResMut<FactionBaseState>,
    buildings: Query<(&EntityKind, &BuildingState, &Faction), With<Building>>,
) {
    let mut per_faction: std::collections::HashMap<Faction, Vec<EntityKind>> =
        std::collections::HashMap::new();
    let mut founded: std::collections::BTreeMap<Faction, bool> =
        std::collections::BTreeMap::new();

    for (kind, state, faction) in &buildings {
        if *kind == EntityKind::Base {
            founded.insert(*faction, true);
        }

        if *state == BuildingState::Complete && kind.category() == EntityCategory::Building {
            let list = per_faction.entry(*faction).or_default();
            if !list.contains(kind) {
                list.push(*kind);
            }
        }
    }

    if all_completed.per_faction != per_faction {
        all_completed.per_faction = per_faction;
    }

    if base_state.founded != founded {
        base_state.founded = founded;
    }
}
