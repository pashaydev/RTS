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
                start_attack_windups,
                resolve_attack_windups,
                tick_attack_recovery,
                explode_props,
                handle_death,
                tick_dying,
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
                rise_speed: 0.6,
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
        (With<Unit>, Without<MoveTarget>, Without<AttackTarget>, Without<Dying>),
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
            Option<&AttackWindup>,
            Option<&AttackRecovery>,
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
    for (attacker_entity, mut tf, attack_target, speed, range, faction, opt_state, windup, recovery) in
        &mut attackers
    {
        if windup.is_some() || recovery.is_some() {
            continue;
        }
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

fn start_attack_windups(
    mut commands: Commands,
    time: Res<Time>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    mut attackers: Query<(
        Entity,
        &Transform,
        &AttackTarget,
        &mut AttackCooldown,
        &AttackRange,
        &AttackProfile,
        &Faction,
        Option<&AttackWindup>,
        Option<&AttackRecovery>,
        Option<&StatusEffects>,
    )>,
    targets: Query<&Transform>,
) {
    for (entity, atk_tf, attack_target, mut cooldown, range, profile, faction, windup, recovery, opt_status) in
        &mut attackers
    {
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
        if windup.is_some() || recovery.is_some() {
            continue;
        }
        // Stunned units cannot attack
        if opt_status.map_or(false, |s| s.is_stunned()) {
            continue;
        }
        cooldown.ready_in = (cooldown.ready_in - time.delta_secs()).max(0.0);
        if cooldown.ready_in > 0.0 {
            continue;
        }

        let Ok(target_tf) = targets.get(attack_target.0) else {
            continue;
        };
        if atk_tf.translation.distance(target_tf.translation) > range.0 * 1.15 {
            continue;
        }

        cooldown.ready_in = cooldown.interval;
        commands.entity(entity).insert(AttackWindup {
            target: attack_target.0,
            remaining_secs: profile.windup_secs.max(0.01),
        });
    }
}

fn resolve_attack_windups(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    mut attackers: Query<(
        Entity,
        &Transform,
        &AttackProfile,
        &CombatFxKind,
        &AttackDamage,
        &AttackRange,
        Option<&IsRanged>,
        &Faction,
        Option<&DamageType>,
        &mut AttackWindup,
        Option<&ChargeBonus>,
    )>,
    mut healths: Query<(&Transform, &mut Health, Option<&ArmorType>)>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (entity, atk_tf, profile, fx_kind, damage, range, is_ranged, faction, opt_dmg_type, mut windup, opt_charge) in &mut attackers {
        // Client: only execute attacks for local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
        windup.remaining_secs -= time.delta_secs();
        if windup.remaining_secs > 0.0 {
            continue;
        }

        let target = windup.target;
        let Ok((target_tf, mut health, opt_armor)) = healths.get_mut(target) else {
            commands.entity(entity).remove::<AttackWindup>();
            commands.entity(entity).insert(AttackRecovery {
                remaining_secs: profile.recovery_secs,
            });
            continue;
        };

        let dist = atk_tf.translation.distance(target_tf.translation);
        if dist > range.0 * 1.2 {
            commands.entity(entity).remove::<AttackWindup>();
            commands.entity(entity).insert(AttackRecovery {
                remaining_secs: (profile.recovery_secs * 0.5).max(0.05),
            });
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
                    target,
                    speed: profile.projectile_speed.max(8.0),
                    damage: damage.0,
                    damage_type: opt_dmg_type.copied().unwrap_or(DamageType::Melee),
                    fx_kind: *fx_kind,
                    impact_scale: profile.impact_scale,
                },
                FogHideable::Vfx,
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(vfx.projectile_material.clone()),
                Transform::from_translation(atk_tf.translation + Vec3::Y * 0.5)
                    .with_scale(Vec3::splat(profile.projectile_scale.max(0.12))),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            spawn_combat_flash(
                &mut commands,
                &vfx,
                atk_tf.translation + Vec3::Y * 0.7,
                *fx_kind,
                profile.projectile_scale.max(0.16),
                profile.projectile_scale.max(0.32),
                0.18,
                0.4,
            );
        } else {
            // Melee: apply damage directly with multiplier + flash VFX
            let charge_mult = opt_charge.map(|c| c.damage_mult).unwrap_or(1.0);
            health.current -= damage.0 * multiplier * charge_mult;
            // Consume charge bonus after use
            if opt_charge.is_some() {
                commands.entity(entity).remove::<ChargeBonus>();
            }
            spawn_combat_flash(
                &mut commands,
                &vfx,
                target_tf.translation,
                *fx_kind,
                0.2,
                profile.impact_scale,
                0.15,
                0.8,
            );
            spawn_combat_dust(&mut commands, &vfx, target_tf.translation, profile.impact_scale);
        }

        commands.entity(entity).remove::<AttackWindup>();
        commands.entity(entity).insert(AttackRecovery {
            remaining_secs: profile.recovery_secs,
        });
    }
}

fn tick_attack_recovery(
    mut commands: Commands,
    time: Res<Time>,
    mut recoveries: Query<(Entity, &mut AttackRecovery)>,
) {
    for (entity, mut recovery) in &mut recoveries {
        recovery.remaining_secs -= time.delta_secs();
        if recovery.remaining_secs <= 0.0 {
            commands.entity(entity).remove::<AttackRecovery>();
        }
    }
}

fn spawn_combat_flash(
    commands: &mut Commands,
    vfx: &VfxAssets,
    pos: Vec3,
    fx_kind: CombatFxKind,
    start_scale: f32,
    end_scale: f32,
    lifetime: f32,
    rise_speed: f32,
) {
    let material = match fx_kind {
        CombatFxKind::Slash | CombatFxKind::Shadow => vfx.melee_material.clone(),
        CombatFxKind::Pierce | CombatFxKind::Arcane | CombatFxKind::Siege => {
            vfx.impact_material.clone()
        }
    };
    commands.spawn((
        VfxFlash {
            timer: Timer::from_seconds(lifetime, TimerMode::Once),
            start_scale,
            end_scale,
            rise_speed,
        },
        FogHideable::Vfx,
        Mesh3d(vfx.sphere_mesh.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(pos).with_scale(Vec3::splat(start_scale)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn spawn_combat_dust(commands: &mut Commands, vfx: &VfxAssets, pos: Vec3, intensity: f32) {
    for (offset, vel_scale) in [
        (Vec3::new(0.2, 0.0, 0.1), 0.8),
        (Vec3::new(-0.15, 0.0, -0.05), 1.0),
    ] {
        commands.spawn((
            CombatDust {
                timer: Timer::from_seconds(0.35 + intensity * 0.08, TimerMode::Once),
                velocity: Vec3::new(offset.x * 2.5, 1.1 * vel_scale, offset.z * 2.5),
                start_scale: 0.08 + intensity * 0.04,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.dust_material.clone()),
            Transform::from_translation(pos + offset).with_scale(Vec3::splat(0.08)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
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
    ), Without<Dying>>,
    mut attackers_with_target: Query<(Entity, &AttackTarget, Option<&mut PatrolState>), Without<Dying>>,
    mut experience_q: Query<&mut Experience>,
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

        if *is_building {
            // Buildings despawn immediately
            commands.entity(*dead_entity).despawn();
        } else {
            // Units play death animation before despawning
            let scale = opt_transform
                .map(|t| t.scale)
                .unwrap_or(Vec3::ONE);
            // Find killer entity and faction for XP granting
            let killer_entity = attackers_with_target
                .iter()
                .find(|(_, at, _)| at.0 == *dead_entity)
                .map(|(e, _, _)| e);
            let killer_faction = killer_entity
                .and_then(|e| attacker_factions.get(e).ok())
                .copied();

            // Grant XP to killer
            if let Some(killer_e) = killer_entity {
                let dead_max_hp = dead
                    .iter()
                    .find(|(e, ..)| *e == *dead_entity)
                    .map(|(_, h, ..)| h.max)
                    .unwrap_or(50.0);
                let xp = (dead_max_hp / 5.0) as u32;
                if let Ok(mut exp) = experience_q.get_mut(killer_e) {
                    exp.current += xp;
                    // Check for level-up
                    if let Some((next_level, threshold)) = exp.level.next() {
                        if exp.current >= threshold {
                            exp.level = next_level;
                        }
                    }
                }
            }

            commands
                .entity(*dead_entity)
                .remove::<Unit>()
                .remove::<AttackTarget>()
                .remove::<MoveTarget>()
                .remove::<AttackWindup>()
                .remove::<AttackRecovery>()
                .remove::<AttackCooldown>()
                .insert(Dying {
                    timer: Timer::from_seconds(1.5, TimerMode::Once),
                    killed_by: killer_faction,
                    original_scale: scale,
                });
        }
    }
}

fn tick_dying(
    mut commands: Commands,
    time: Res<Time>,
    mut dying: Query<(Entity, &mut Dying, &mut Transform)>,
) {
    for (entity, mut dying, mut tf) in &mut dying {
        dying.timer.tick(time.delta());

        // Shrink during the last 0.4 seconds
        let remaining = dying.timer.remaining_secs();
        if remaining < 0.4 {
            let shrink_frac = (remaining / 0.4).max(0.0);
            tf.scale = dying.original_scale * shrink_frac;
        }

        if dying.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
