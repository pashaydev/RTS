use bevy::prelude::*;

use crate::components::*;
use crate::ground::terrain_height;

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
    idle_units: Query<
        (Entity, &Transform, &AttackRange, &Faction),
        (With<Unit>, Without<MoveTarget>, Without<AttackTarget>, Without<GatherTarget>),
    >,
    mobs: Query<(Entity, &Transform), With<Mob>>,
) {
    for (unit_entity, unit_tf, range, faction) in &idle_units {
        if *faction != Faction::Player {
            continue;
        }
        let scan_range = range.0 * 2.0;
        let mut closest_dist = f32::MAX;
        let mut closest_mob = None;

        for (mob_entity, mob_tf) in &mobs {
            let dist = unit_tf.translation.distance(mob_tf.translation);
            if dist < scan_range && dist < closest_dist {
                closest_dist = dist;
                closest_mob = Some(mob_entity);
            }
        }

        if let Some(target) = closest_mob {
            commands.entity(unit_entity).insert(AttackTarget(target));
        }
    }
}

fn approach_attack_target(
    time: Res<Time>,
    mut attackers: Query<
        (&mut Transform, &AttackTarget, &UnitSpeed, &AttackRange, Option<&UnitType>, Option<&MobType>),
    >,
    targets: Query<&Transform, Without<AttackTarget>>,
) {
    for (mut tf, attack_target, speed, range, opt_ut, opt_mt) in &mut attackers {
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

            // Snap Y to terrain
            let y_off = if let Some(ut) = opt_ut {
                crate::units::y_offset(*ut)
            } else if let Some(mt) = opt_mt {
                match mt {
                    MobType::Goblin => 0.65,
                    MobType::Skeleton => 0.78,
                    MobType::Orc => 1.05,
                    MobType::Demon => 1.15,
                }
            } else {
                0.8
            };
            tf.translation.y = terrain_height(tf.translation.x, tf.translation.z) + y_off;
        }
    }
}

fn execute_melee_attacks(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut attackers: Query<
        (&Transform, &AttackTarget, &mut AttackCooldown, &AttackDamage, &AttackRange, Option<&UnitType>),
    >,
    mut healths: Query<(&Transform, &mut Health)>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (atk_tf, attack_target, mut cooldown, damage, range, opt_ut) in &mut attackers {
        // Skip archers (ranged)
        if let Some(ut) = opt_ut {
            if *ut == UnitType::Archer {
                continue;
            }
        }

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

        // Apply damage
        health.current -= damage.0;

        // Spawn melee flash VFX
        commands.spawn((
            VfxFlash {
                timer: Timer::from_seconds(0.15, TimerMode::Once),
                start_scale: 0.3,
                end_scale: 0.8,
            },
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.melee_material.clone()),
            Transform::from_translation(target_tf.translation)
                .with_scale(Vec3::splat(0.3)),
        ));
    }
}

fn execute_ranged_attacks(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut archers: Query<
        (&Transform, &AttackTarget, &mut AttackCooldown, &AttackDamage, &AttackRange, &UnitType),
        With<Unit>,
    >,
    targets: Query<&Transform, Without<Unit>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (atk_tf, attack_target, mut cooldown, damage, range, ut) in &mut archers {
        if *ut != UnitType::Archer {
            continue;
        }

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

        // Spawn projectile
        commands.spawn((
            Projectile {
                target: attack_target.0,
                speed: 15.0,
                damage: damage.0,
            },
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.projectile_material.clone()),
            Transform::from_translation(atk_tf.translation + Vec3::Y * 0.5)
                .with_scale(Vec3::splat(0.15)),
        ));
    }
}

fn handle_death(
    mut commands: Commands,
    dead: Query<(Entity, &Health)>,
    mut attackers_with_target: Query<(Entity, &AttackTarget, Option<&mut PatrolState>)>,
) {
    for (dead_entity, health) in &dead {
        if health.current > 0.0 {
            continue;
        }

        // Clean up AttackTarget references
        for (attacker_entity, attack_target, opt_patrol) in &mut attackers_with_target {
            if attack_target.0 == dead_entity {
                commands.entity(attacker_entity).remove::<AttackTarget>();
                if let Some(mut patrol) = opt_patrol {
                    patrol.state = PatrolStateKind::Returning;
                }
            }
        }

        commands.entity(dead_entity).despawn();
    }
}
