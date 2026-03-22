use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::blueprints::{EntityKind, IsRanged};
use crate::components::*;
use crate::multiplayer::NetRole;
use crate::spatial::{SpatialHashGrid, WallSpatialGrid};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                player_auto_acquire_target,
                approach_attack_target,
                execute_attacks,
                explode_props,
                handle_death,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn explode_props(
    mut commands: Commands,
    vfx_assets: Option<Res<VfxAssets>>,
    net_role: Res<NetRole>,
    mut queries: ParamSet<(
        Query<(Entity, &Transform, &ExplosiveProp, &Health)>,
        Query<(Entity, &mut Transform, &mut Health), Without<Projectile>>,
    )>,
) {
    let Some(vfx) = vfx_assets else { return };
    // Client: skip explosion damage — host handles it and syncs health
    if *net_role == NetRole::Client {
        return;
    }

    let detonations: Vec<_> = queries
        .p0()
        .iter()
        .filter(|(_, _, _, health)| health.current <= 0.0)
        .map(|(entity, tf, prop, _)| (entity, tf.translation, *prop))
        .collect();

    for (source_entity, origin, prop) in detonations {
        commands.spawn((
            VfxFlash {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                start_scale: 0.4,
                end_scale: prop.radius * 0.55,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.impact_material.clone()),
            Transform::from_translation(origin).with_scale(Vec3::splat(0.4)),
            NotShadowCaster,
            NotShadowReceiver,
        ));

        for (target_entity, mut target_tf, mut health) in &mut queries.p1() {
            if target_entity == source_entity {
                continue;
            }

            let offset = target_tf.translation - origin;
            let dist = offset.length();
            if dist > prop.radius {
                continue;
            }

            let falloff = 1.0 - (dist / prop.radius).min(1.0);
            if falloff <= 0.0 {
                continue;
            }

            health.current -= prop.damage * falloff;
            if dist > 0.05 {
                let push = Vec3::new(offset.x, 0.0, offset.z).normalize_or_zero() * falloff * 0.9;
                target_tf.translation += push;
            }
        }
    }
}

pub fn player_auto_acquire_target(
    mut commands: Commands,
    teams: Res<TeamConfig>,
    spatial_grid: Res<SpatialHashGrid>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    idle_units: Query<
        (
            Entity,
            &Transform,
            &AttackRange,
            &Faction,
            Option<&UnitState>,
            Option<&UnitStance>,
        ),
        (With<Unit>, Without<MoveTarget>, Without<AttackTarget>),
    >,
    factions: Query<&Faction>,
    building_check: Query<(), With<Building>>,
) {
    for (unit_entity, unit_tf, range, faction, unit_state, opt_stance) in &idle_units {
        // Client: only auto-acquire for local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
        // Skip units that are busy (not idle)
        if let Some(state) = unit_state {
            if !matches!(state, UnitState::Idle) {
                continue;
            }
        }

        let stance = opt_stance.copied().unwrap_or_default();

        // Passive units never auto-acquire
        if stance == UnitStance::Passive {
            continue;
        }

        let scan_range = range.0 * stance.scan_multiplier();
        if scan_range <= 0.0 {
            continue;
        }

        let mut closest_dist = f32::MAX;
        let mut closest_target = None;

        // Use spatial hash to find nearby entities
        let nearby = spatial_grid.query_radius(unit_tf.translation, scan_range);
        for (target_entity, target_pos) in &nearby {
            if *target_entity == unit_entity {
                continue;
            }
            // Skip buildings unless aggressive stance
            if stance != UnitStance::Aggressive && building_check.get(*target_entity).is_ok() {
                continue;
            }
            let Some(target_faction) = factions.get(*target_entity).ok() else {
                continue;
            };
            if !teams.is_hostile(faction, target_faction) {
                continue;
            }
            let dx = target_pos.x - unit_tf.translation.x;
            let dz = target_pos.z - unit_tf.translation.z;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist < closest_dist {
                closest_dist = dist;
                closest_target = Some(*target_entity);
            }
        }

        if let Some(target) = closest_target {
            // Record leash origin for defensive stance
            if stance == UnitStance::Defensive {
                commands
                    .entity(unit_entity)
                    .insert(LeashOrigin(unit_tf.translation));
            }
            commands.entity(unit_entity).insert(AttackTarget(target));
        }
    }
}

fn approach_attack_target(
    mut commands: Commands,
    time: Res<Time>,
    teams: Res<TeamConfig>,
    wall_grid: Res<WallSpatialGrid>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    mut attackers: Query<
        (
            Entity,
            &mut Transform,
            &AttackTarget,
            &UnitSpeed,
            &AttackRange,
            &Faction,
            Option<&mut UnitState>,
        ),
        With<Unit>,
    >,
    wall_check: Query<
        (),
        (
            With<Building>,
            Or<(With<WallSegmentPiece>, With<WallPostPiece>)>,
        ),
    >,
    targets: Query<&Transform, Without<AttackTarget>>,
) {
    for (attacker_entity, mut tf, attack_target, speed, range, faction, opt_state) in
        &mut attackers
    {
        // Client: only approach for local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
        let Ok(target_tf) = targets.get(attack_target.0) else {
            continue;
        };

        let target_is_wall = wall_check.get(attack_target.0).is_ok();
        if !target_is_wall {
            let from = Vec2::new(tf.translation.x, tf.translation.z);
            let to = Vec2::new(target_tf.translation.x, target_tf.translation.z);
            let delta = to - from;
            let line_len = delta.length();

            if line_len > 0.5 {
                let dir = delta / line_len;
                let mut blocking_wall: Option<(Entity, f32)> = None;

                // Use wall spatial grid: check walls near the midpoint with radius = half the line length
                let mid = tf.translation.lerp(target_tf.translation, 0.5);
                let search_radius = line_len * 0.5 + 2.0;
                let nearby_walls = wall_grid.query_radius(mid, search_radius);

                for (wall_entity, wall_pos_3d, wall_fp, wall_faction) in &nearby_walls {
                    if !teams.is_hostile(faction, &wall_faction) {
                        continue;
                    }

                    let wall_pos = Vec2::new(wall_pos_3d.x, wall_pos_3d.z);
                    let rel = wall_pos - from;
                    let t = rel.dot(dir);
                    if t <= 0.3 || t >= line_len - 0.3 {
                        continue;
                    }

                    let closest = from + dir * t;
                    let perp_dist = wall_pos.distance(closest);
                    if perp_dist <= wall_fp + 0.35
                        && blocking_wall.map_or(true, |(_, best_t)| t < best_t)
                    {
                        blocking_wall = Some((*wall_entity, t));
                    }
                }

                if let Some((wall_entity, _)) = blocking_wall {
                    commands
                        .entity(attacker_entity)
                        .insert(AttackTarget(wall_entity));
                    if let Some(mut state) = opt_state {
                        if matches!(*state, UnitState::Attacking(_)) {
                            *state = UnitState::Attacking(wall_entity);
                        }
                    }
                    continue;
                }
            }
        }

        let dir = Vec3::new(
            target_tf.translation.x - tf.translation.x,
            0.0,
            target_tf.translation.z - tf.translation.z,
        );
        let dist = dir.length();

        if dist > range.0 {
            let step = dir.normalize() * speed.0 * time.delta_secs();
            let candidate = tf.translation + step;

            // Use wall spatial grid for collision check
            let nearby_walls = wall_grid.query_radius(candidate, 3.0);
            let blocked = nearby_walls.iter().any(|(wall_entity, wall_pos, wall_fp, wall_faction)| {
                if *wall_entity == attack_target.0 {
                    return false;
                }
                if !teams.is_hostile(faction, wall_faction) {
                    return false;
                }
                let a = Vec2::new(candidate.x, candidate.z);
                let b = Vec2::new(wall_pos.x, wall_pos.z);
                a.distance(b) < wall_fp + 0.6
            });
            if blocked {
                continue;
            }
            tf.translation = candidate;
        }
    }
}

fn execute_attacks(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    mut attackers: Query<(
        &Transform,
        &AttackTarget,
        &mut AttackCooldown,
        &AttackDamage,
        &AttackRange,
        Option<&IsRanged>,
        &Faction,
        Option<&DamageType>,
    )>,
    mut healths: Query<(&Transform, &mut Health, Option<&ArmorType>)>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (atk_tf, attack_target, mut cooldown, damage, range, is_ranged, faction, opt_dmg_type) in &mut attackers {
        // Client: only execute attacks for local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
        cooldown.timer.tick(time.delta());

        if !cooldown.timer.just_finished() {
            continue;
        }

        let Ok((target_tf, mut health, opt_armor)) = healths.get_mut(attack_target.0) else {
            continue;
        };

        let dist = atk_tf.translation.distance(target_tf.translation);
        if dist > range.0 * 1.2 {
            continue;
        }

        // Compute damage multiplier from damage type vs armor type
        let multiplier = match (opt_dmg_type, opt_armor) {
            (Some(dmg_type), Some(armor_type)) => dmg_type.multiplier_vs(*armor_type),
            _ => 1.0,
        };

        if is_ranged.is_some() {
            // Ranged: spawn projectile (carries damage_type for on-hit multiplier)
            commands.spawn((
                Projectile {
                    target: attack_target.0,
                    speed: 15.0,
                    damage: damage.0,
                    damage_type: opt_dmg_type.copied().unwrap_or(DamageType::Melee),
                },
                FogHideable::Vfx,
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(vfx.projectile_material.clone()),
                Transform::from_translation(atk_tf.translation + Vec3::Y * 0.5)
                    .with_scale(Vec3::splat(0.15)),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        } else {
            // Melee: apply damage directly with multiplier + flash VFX
            health.current -= damage.0 * multiplier;
            commands.spawn((
                VfxFlash {
                    timer: Timer::from_seconds(0.15, TimerMode::Once),
                    start_scale: 0.3,
                    end_scale: 0.8,
                },
                FogHideable::Vfx,
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(vfx.melee_material.clone()),
                Transform::from_translation(target_tf.translation).with_scale(Vec3::splat(0.3)),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

fn handle_death(
    mut commands: Commands,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    dead: Query<(
        Entity,
        &Health,
        Option<&Building>,
        Option<&Selected>,
        Option<&EntityKind>,
        Option<&Transform>,
        Option<&UnitState>,
        Option<&Faction>,
        Option<&CampReward>,
    )>,
    mut attackers_with_target: Query<(Entity, &AttackTarget, Option<&mut PatrolState>)>,
    mut all_assigned_workers: Query<&mut AssignedWorkers>,
    workers_with_state: Query<(Entity, &UnitState), With<Unit>>,
    time: Res<Time>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut all_resources: ResMut<AllPlayerResources>,
    attacker_factions: Query<&Faction, With<AttackTarget>>,
) {
    let is_client = *net_role == NetRole::Client;
    // Collect dead entities first to avoid borrow issues
    // On client: only detect death for local player's entities (remote deaths come via EntityDespawn)
    let dead_list: Vec<_> = dead
        .iter()
        .filter(|(_, health, _, _, _, _, _, opt_faction, _)| {
            if health.current > 0.0 {
                return false;
            }
            if is_client {
                // Only handle death for local player's entities
                opt_faction.map_or(false, |f| *f == active_player.0)
            } else {
                true
            }
        })
        .map(
            |(
                entity,
                _,
                opt_building,
                opt_selected,
                opt_kind,
                opt_transform,
                opt_unit_state,
                opt_faction,
                opt_reward,
            )| {
                (
                    entity,
                    opt_building.is_some(),
                    opt_selected.is_some(),
                    opt_kind.map(|k| *k),
                    opt_transform.map(|t| *t),
                    opt_unit_state.copied(),
                    opt_faction.copied(),
                    opt_reward.cloned(),
                )
            },
        )
        .collect();

    for (
        dead_entity,
        is_building,
        is_selected,
        opt_kind,
        opt_transform,
        opt_unit_state,
        opt_faction,
        opt_camp_reward,
    ) in &dead_list
    {
        // Grant camp reward resources to the killing faction (host only)
        if !is_client {
            if let Some(reward) = opt_camp_reward {
                // Find who was attacking this mob to determine the rewarded faction
                let killer_faction = attackers_with_target
                    .iter()
                    .find(|(_, at, _)| at.0 == *dead_entity)
                    .and_then(|(attacker_e, _, _)| attacker_factions.get(attacker_e).ok());
                if let Some(killer_f) = killer_faction {
                    if let Some(res) = all_resources.resources.get_mut(killer_f) {
                        for (rt, amt) in reward.resources.cost_entries() {
                            res.amounts[rt.index()] += amt;
                        }
                    }
                    event_log.push(
                        time.elapsed_secs(),
                        format!("Camp cleared! Resources gained."),
                        crate::ui::event_log_widget::EventCategory::Resource,
                        opt_transform.map(|t| t.translation),
                        Some(*killer_f),
                    );
                }
            }
        }

        for (attacker_entity, attack_target, opt_patrol) in &mut attackers_with_target {
            if attack_target.0 == *dead_entity {
                commands.entity(attacker_entity).remove::<AttackTarget>();
                if let Some(mut patrol) = opt_patrol {
                    patrol.state = PatrolStateKind::Returning;
                }
            }
        }

        // If a worker dies while assigned to a processor, remove it from AssignedWorkers
        if let Some(UnitState::AssignedGathering { building, .. }) =
            opt_unit_state
        {
            if let Ok(mut aw) = all_assigned_workers.get_mut(*building) {
                aw.workers.retain(|&w| w != *dead_entity);
            }
        }

        // If a building dies with assigned workers, eject them all
        if *is_building {
            if let Ok(aw) = all_assigned_workers.get(*dead_entity) {
                let workers_to_eject: Vec<Entity> = aw.workers.clone();
                for worker in workers_to_eject {
                    if let Ok((_, worker_state)) = workers_with_state.get(worker) {
                        if matches!(worker_state, UnitState::AssignedGathering { building, .. } if *building == *dead_entity)
                        {
                            crate::resources::unassign_worker_from_processor(&mut commands, worker);
                        }
                    }
                }
            }
        }

        // Log death event
        let name = opt_kind.map_or("Unit", |k| k.display_name());
        let pos = opt_transform.map(|t| t.translation);
        event_log.push(
            time.elapsed_secs(),
            format!("{} destroyed", name),
            crate::ui::event_log_widget::EventCategory::Combat,
            pos,
            *opt_faction,
        );

        // Clear selection if selected
        if *is_selected {
            commands.entity(*dead_entity).remove::<Selected>();
        }

        commands.entity(*dead_entity).despawn();
    }
}
