use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind, IsRanged};
use crate::components::*;
use crate::ground::HeightMap;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                player_auto_acquire_target,
                approach_attack_target,
                execute_melee_attacks,
                execute_ranged_attacks,
                handle_death,
            )
                .chain(),
        );
    }
}

fn player_auto_acquire_target(
    mut commands: Commands,
    teams: Res<TeamConfig>,
    idle_units: Query<
        (Entity, &Transform, &AttackRange, &Faction, Option<&WorkerTask>),
        (With<Unit>, Without<MoveTarget>, Without<AttackTarget>),
    >,
    potential_targets: Query<(Entity, &Transform, &Faction), Or<(With<Mob>, With<Unit>)>>,
    buildings_with_faction: Query<(Entity, &Transform, &Faction), With<Building>>,
) {
    for (unit_entity, unit_tf, range, faction, worker_task) in &idle_units {
        // Skip workers that are busy (not idle)
        if let Some(task) = worker_task {
            if !matches!(task, WorkerTask::Idle) {
                continue;
            }
        }
        let scan_range = range.0 * 2.0;
        let mut closest_dist = f32::MAX;
        let mut closest_target = None;

        // Check units and mobs
        for (target_entity, target_tf, target_faction) in &potential_targets {
            if target_entity == unit_entity {
                continue;
            }
            if !teams.is_hostile(faction, target_faction) {
                continue;
            }
            let dist = unit_tf.translation.distance(target_tf.translation);
            if dist < scan_range && dist < closest_dist {
                closest_dist = dist;
                closest_target = Some(target_entity);
            }
        }

        // Also check hostile buildings
        for (target_entity, target_tf, target_faction) in &buildings_with_faction {
            if !teams.is_hostile(faction, target_faction) {
                continue;
            }
            let dist = unit_tf.translation.distance(target_tf.translation);
            if dist < scan_range && dist < closest_dist {
                closest_dist = dist;
                closest_target = Some(target_entity);
            }
        }

        if let Some(target) = closest_target {
            commands.entity(unit_entity).insert(AttackTarget(target));
        }
    }
}

fn approach_attack_target(
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    height_map: Res<HeightMap>,
    mut attackers: Query<
        (&mut Transform, &AttackTarget, &UnitSpeed, &AttackRange, Option<&EntityKind>),
    >,
    targets: Query<&Transform, Without<AttackTarget>>,
) {
    for (mut tf, attack_target, speed, range, opt_kind) in &mut attackers {
        let Ok(target_tf) = targets.get(attack_target.0) else {
            continue;
        };

        let dir = Vec3::new(
            target_tf.translation.x - tf.translation.x,
            0.0,
            target_tf.translation.z - tf.translation.z,
        );
        let dist = dir.length();

        if dist > range.0 {
            let step = dir.normalize() * speed.0 * time.delta_secs();
            tf.translation += step;

            let y_off = if let Some(kind) = opt_kind {
                registry.get(*kind).movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8)
            } else {
                0.8
            };
            tf.translation.y = height_map.sample(tf.translation.x, tf.translation.z) + y_off;
        }
    }
}

fn execute_melee_attacks(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut attackers: Query<
        (&Transform, &AttackTarget, &mut AttackCooldown, &AttackDamage, &AttackRange),
        Without<IsRanged>,
    >,
    mut healths: Query<(&Transform, &mut Health)>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (atk_tf, attack_target, mut cooldown, damage, range) in &mut attackers {
        cooldown.timer.tick(time.delta());

        if !cooldown.timer.just_finished() {
            continue;
        }

        let Ok((target_tf, mut health)) = healths.get_mut(attack_target.0) else {
            continue;
        };

        let dist = atk_tf.translation.distance(target_tf.translation);
        if dist > range.0 * 1.2 {
            continue;
        }

        health.current -= damage.0;

        commands.spawn((
            VfxFlash {
                timer: Timer::from_seconds(0.15, TimerMode::Once),
                start_scale: 0.3,
                end_scale: 0.8,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.melee_material.clone()),
            Transform::from_translation(target_tf.translation)
                .with_scale(Vec3::splat(0.3)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

fn execute_ranged_attacks(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut archers: Query<
        (&Transform, &AttackTarget, &mut AttackCooldown, &AttackDamage, &AttackRange),
        (With<Unit>, With<IsRanged>),
    >,
    targets: Query<&Transform, Without<Unit>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (atk_tf, attack_target, mut cooldown, damage, range) in &mut archers {
        cooldown.timer.tick(time.delta());

        if !cooldown.timer.just_finished() {
            continue;
        }

        let Ok(target_tf) = targets.get(attack_target.0) else {
            continue;
        };

        let dist = atk_tf.translation.distance(target_tf.translation);
        if dist > range.0 * 1.2 {
            continue;
        }

        commands.spawn((
            Projectile {
                target: attack_target.0,
                speed: 15.0,
                damage: damage.0,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.projectile_material.clone()),
            Transform::from_translation(atk_tf.translation + Vec3::Y * 0.5)
                .with_scale(Vec3::splat(0.15)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

fn handle_death(
    mut commands: Commands,
    dead: Query<(Entity, &Health, Option<&Building>, Option<&Selected>, Option<&EntityKind>, Option<&Transform>)>,
    mut attackers_with_target: Query<(Entity, &AttackTarget, Option<&mut PatrolState>)>,
    time: Res<Time>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
) {
    for (dead_entity, health, _opt_building, opt_selected, opt_kind, opt_transform) in &dead {
        if health.current > 0.0 {
            continue;
        }

        for (attacker_entity, attack_target, opt_patrol) in &mut attackers_with_target {
            if attack_target.0 == dead_entity {
                commands.entity(attacker_entity).remove::<AttackTarget>();
                if let Some(mut patrol) = opt_patrol {
                    patrol.state = PatrolStateKind::Returning;
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
        );

        // Clear selection if selected
        if opt_selected.is_some() {
            commands.entity(dead_entity).remove::<Selected>();
        }

        commands.entity(dead_entity).despawn();
    }
}
