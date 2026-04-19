use bevy::prelude::*;
use bevy::time::Fixed;

use crate::blueprints::EntityKind;
use crate::presentation::model_assets::{
    projectile_visual_for, ProjectileModelAssets, ProjectileVisualKind,
};
use crate::types::*;
use crate::world::spatial::SpatialHashGrid;

use super::ability::PendingEffect;
use super::brain::CombatSet;
use super::damage::apply_damage;
use super::effect::{EffectTarget, ProjectileOnImpact};

pub struct CombatProjectilesPlugin;

impl Plugin for CombatProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProjectileImpactEvent>()
            .add_message::<DamageApplied>()
            .add_systems(
                FixedUpdate,
                tick_projectiles
                    .in_set(CombatSet::Projectiles)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                attach_projectile_visuals.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Attaches a SceneRoot child to projectiles spawned by the new pipeline (e.g.
/// `Effect::SpawnProjectile`), picking the visual from the source entity's kind.
fn attach_projectile_visuals(
    mut commands: Commands,
    projectile_assets: Option<Res<ProjectileModelAssets>>,
    new_projectiles: Query<(Entity, &Projectile), Added<Projectile>>,
    sources: Query<&EntityKind>,
) {
    let Some(assets) = projectile_assets else {
        return;
    };
    for (entity, projectile) in &new_projectiles {
        let Ok(kind) = sources.get(projectile.source) else {
            continue;
        };
        let Some(visual_kind) = projectile_visual_for(*kind) else {
            continue;
        };
        let scene = assets.scene_for(visual_kind, entity.to_bits() as usize);
        let proj_scale = match visual_kind {
            ProjectileVisualKind::Arrow => 0.35,
            ProjectileVisualKind::Bolt => 0.4,
            ProjectileVisualKind::CatapultRock => 0.5,
        };
        let rotation = if projectile.orient_to_velocity {
            let forward = projectile.velocity.normalize_or_zero();
            if forward.length_squared() > 0.0 {
                Quat::from_rotation_arc(Vec3::Z, forward)
            } else {
                Quat::IDENTITY
            }
        } else {
            Quat::IDENTITY
        };
        let child = commands
            .spawn((
                SceneRoot(scene),
                Transform::from_scale(Vec3::splat(proj_scale)).with_rotation(rotation),
            ))
            .id();
        commands.entity(entity).add_child(child);
        commands
            .entity(entity)
            .insert(Visibility::Visible);
    }
}

fn tick_projectiles(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    spatial_grid: Res<SpatialHashGrid>,
    mut impacts: MessageWriter<ProjectileImpactEvent>,
    mut damage_events: MessageWriter<DamageApplied>,
    mut pending: MessageWriter<PendingEffect>,
    faction_q: Query<&crate::types::app::Faction>,
    mut projectiles: Query<(
        Entity,
        &mut Transform,
        &mut Projectile,
        Option<&AoeSplash>,
        Option<&ProjectileOnImpact>,
    )>,
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
    for (proj_entity, mut proj_tf, mut projectile, opt_aoe, opt_on_impact) in &mut projectiles {
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

            if let Some(on_impact) = opt_on_impact {
                // New pipeline: route impact through PendingEffect resolver.
                let caster_faction = faction_q.get(projectile.source).ok().copied();
                pending.write(PendingEffect {
                    caster: projectile.source,
                    caster_faction,
                    target: EffectTarget::Entity(projectile.target),
                    effect: on_impact.0.clone(),
                });
                impacts.write(ProjectileImpactEvent {
                    position: target_pos,
                    target: projectile.target,
                    damage: 0.0,
                    fx_kind: projectile.fx_kind,
                    impact_scale: projectile.impact_scale,
                    is_aoe: matches!(on_impact.0, super::effect::Effect::AreaSearch { .. }),
                    direction,
                });
                commands.entity(proj_entity).try_despawn();
                continue;
            }

            // Legacy path: damage field + optional AoeSplash. Removed once legacy abilities.rs is gone.
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
