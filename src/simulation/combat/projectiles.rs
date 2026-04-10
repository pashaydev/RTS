use bevy::prelude::*;

use crate::infrastructure::multiplayer::NetRole;
use crate::types::*;
use crate::world::spatial::SpatialHashGrid;

pub struct CombatProjectilesPlugin;

impl Plugin for CombatProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProjectileImpactEvent>().add_systems(
            FixedUpdate,
            tick_projectiles
                .in_set(GameFlowSet::Simulation)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn tick_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<CombatTuning>,
    net_role: Res<NetRole>,
    spatial_grid: Res<SpatialHashGrid>,
    mut impacts: MessageWriter<ProjectileImpactEvent>,
    mut projectiles: Query<(Entity, &mut Transform, &Projectile, Option<&AoeSplash>)>,
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
    if *net_role == NetRole::Client {
        return;
    }

    for (proj_entity, mut proj_tf, projectile, opt_aoe) in &mut projectiles {
        let Ok((target_tf, _, _, _)) = targets.get(projectile.target) else {
            commands.entity(proj_entity).try_despawn();
            continue;
        };

        let target_pos = target_tf.translation;
        let dir = target_pos - proj_tf.translation;
        let dist = dir.length();

        if dist < 0.5 {
            let direction = dir.normalize_or_zero();
            let mut applied_damage = 0.0;

            {
                let Ok((_, mut health, opt_armor, opt_reserved)) = targets.get_mut(projectile.target)
                else {
                    commands.entity(proj_entity).try_despawn();
                    continue;
                };

                if let Some(mut reserved) = opt_reserved {
                    let src = projectile.source;
                    reserved.reservations.retain(|(s, _, _)| *s != src);
                }

                let multiplier = opt_armor
                    .map(|armor| projectile.damage_type.multiplier_vs(*armor))
                    .unwrap_or(1.0);
                applied_damage = projectile.damage * multiplier;
                health.current -= applied_damage;
            }
            commands.entity(projectile.target).insert(RecentCombatDamage {
                attacker: projectile.source,
                observed_at: time.elapsed_secs_f64(),
                expires_at: time.elapsed_secs_f64() + tuning.damage_memory_secs as f64,
            });

            if let Some(aoe) = opt_aoe {
                let nearby = spatial_grid.query_radius(target_pos, aoe.radius);
                for (splash_entity, splash_pos) in &nearby {
                    if *splash_entity == projectile.target {
                        continue;
                    }
                    if let Ok((_, mut splash_health, splash_armor, _)) = targets.get_mut(*splash_entity)
                    {
                        let splash_dist = (target_pos - *splash_pos).length();
                        let dmg_mult = if aoe.falloff {
                            (1.0 - splash_dist / aoe.radius).max(0.3)
                        } else {
                            1.0
                        };
                        let armor_mult = splash_armor
                            .map(|a| projectile.damage_type.multiplier_vs(*a))
                            .unwrap_or(1.0);
                        splash_health.current -= projectile.damage * dmg_mult * armor_mult;
                        commands.entity(*splash_entity).insert(RecentCombatDamage {
                            attacker: projectile.source,
                            observed_at: time.elapsed_secs_f64(),
                            expires_at: time.elapsed_secs_f64() + tuning.damage_memory_secs as f64,
                        });
                    }
                }
            }

            impacts.write(ProjectileImpactEvent {
                position: target_pos,
                target: projectile.target,
                damage: applied_damage.max(projectile.damage),
                fx_kind: projectile.fx_kind,
                impact_scale: projectile.impact_scale,
                is_aoe: opt_aoe.is_some(),
                direction,
            });

            commands.entity(proj_entity).try_despawn();
        } else {
            let forward = dir.normalize();
            let step = forward * projectile.speed * time.delta_secs();
            proj_tf.translation += step;
            if projectile.orient_to_velocity {
                proj_tf.rotation = Quat::from_rotation_arc(Vec3::Z, forward);
            }
        }
    }
}
