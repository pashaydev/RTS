use bevy::prelude::*;
use bevy::time::Fixed;

use crate::types::*;
use crate::world::spatial::SpatialHashGrid;

use super::damage::apply_damage;

pub struct CombatProjectilesPlugin;

impl Plugin for CombatProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProjectileImpactEvent>()
            .add_message::<DamageApplied>()
            .add_systems(
                FixedUpdate,
                tick_projectiles
                    .in_set(SimSet::Combat)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn tick_projectiles(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    spatial_grid: Res<SpatialHashGrid>,
    mut impacts: MessageWriter<ProjectileImpactEvent>,
    mut damage_events: MessageWriter<DamageApplied>,
    mut projectiles: Query<(Entity, &mut Transform, &mut Projectile, Option<&AoeSplash>)>,
    mut targets: Query<
        (
            &Transform,
            &mut Health,
            Option<&ArmorType>,
            Option<&mut ReservedIncomingDamage>,
        ),
        Without<Projectile>,
    >,
) {
    for (proj_entity, mut proj_tf, mut projectile, opt_aoe) in &mut projectiles {
        projectile.lifetime_secs -= time.delta_secs();
        if projectile.lifetime_secs <= 0.0 {
            commands.entity(proj_entity).try_despawn();
            continue;
        }

        let Ok((target_tf, _, _, _)) = targets.get(projectile.target) else {
            commands.entity(proj_entity).try_despawn();
            continue;
        };

        let target_pos = target_tf.translation;
        let dir = target_pos - proj_tf.translation;
        let dist = dir.length();

        if dist < 0.5 {
            let direction = dir.normalize_or_zero();
            let now = time.elapsed_secs_f64();
            let mem = tuning.damage_memory_secs;
            let applied_damage;

            {
                let Ok((_, mut health, opt_armor, opt_reserved)) =
                    targets.get_mut(projectile.target)
                else {
                    commands.entity(proj_entity).try_despawn();
                    continue;
                };
                applied_damage = apply_damage(
                    &mut commands,
                    projectile.target,
                    Some(projectile.source),
                    projectile.damage,
                    projectile.damage_type,
                    &mut health,
                    opt_armor.copied(),
                    opt_reserved.map(|r| r.into_inner()),
                    now,
                    mem,
                );
                damage_events.write(DamageApplied {
                    target: projectile.target,
                    source: Some(projectile.source),
                    amount: applied_damage,
                    damage_type: projectile.damage_type,
                    now_secs: now,
                });
            }

            if let Some(aoe) = opt_aoe {
                let nearby = spatial_grid.query_radius(target_pos, aoe.radius);
                for (splash_entity, splash_pos) in &nearby {
                    if *splash_entity == projectile.target {
                        continue;
                    }
                    if let Ok((_, mut splash_health, splash_armor, splash_reserved)) =
                        targets.get_mut(*splash_entity)
                    {
                        let splash_dist = (target_pos - *splash_pos).length();
                        let dmg_mult = if aoe.falloff {
                            (1.0 - splash_dist / aoe.radius).max(0.3)
                        } else {
                            1.0
                        };
                        let splash_dealt = apply_damage(
                            &mut commands,
                            *splash_entity,
                            Some(projectile.source),
                            projectile.damage * dmg_mult,
                            projectile.damage_type,
                            &mut splash_health,
                            splash_armor.copied(),
                            splash_reserved.map(|r| r.into_inner()),
                            now,
                            mem,
                        );
                        damage_events.write(DamageApplied {
                            target: *splash_entity,
                            source: Some(projectile.source),
                            amount: splash_dealt,
                            damage_type: projectile.damage_type,
                            now_secs: now,
                        });
                    }
                }
            }

            impacts.write(ProjectileImpactEvent {
                position: target_pos,
                target: projectile.target,
                damage: applied_damage,
                fx_kind: projectile.fx_kind,
                impact_scale: projectile.impact_scale,
                is_aoe: opt_aoe.is_some(),
                direction,
            });

            commands.entity(proj_entity).try_despawn();
        } else {
            let step = projectile.velocity * time.delta_secs();
            proj_tf.translation += step;
            if projectile.orient_to_velocity {
                let forward = projectile.velocity.normalize_or_zero();
                if forward.length_squared() > 0.0 {
                    proj_tf.rotation = Quat::from_rotation_arc(Vec3::Z, forward);
                }
            }
        }
    }
}
