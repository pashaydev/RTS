use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::blueprints::{EntityKind, IsRanged};
use crate::components::*;
use crate::multiplayer::NetRole;
use crate::spatial::SpatialHashGrid;

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                tick_ability_cooldowns,
                process_ability_casts,
                apply_status_effects,
                cleanup_expired_effects,
                tick_charge_bonus,
                apply_catapult_aoe_splash,
                apply_veterancy_bonuses,
                spawn_veterancy_indicators,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Tick down all ability cooldowns each frame.
fn tick_ability_cooldowns(time: Res<Time>, mut units: Query<&mut UnitAbilities>) {
    let dt = time.delta_secs();
    for mut abilities in &mut units {
        for cd in abilities.cooldowns.values_mut() {
            if *cd > 0.0 {
                *cd = (*cd - dt).max(0.0);
            }
        }
    }
}

/// Process abilities that are currently being cast.
fn process_ability_casts(
    mut commands: Commands,
    time: Res<Time>,
    spatial_grid: Res<SpatialHashGrid>,
    teams: Res<TeamConfig>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut casters: Query<(
        Entity,
        &mut CastingAbility,
        &Transform,
        &Faction,
        &EntityKind,
    )>,
    mut targets_health: Query<(&Transform, &mut Health, Option<&ArmorType>, Option<&EntityKind>), Without<CastingAbility>>,
    factions: Query<&Faction>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (caster_entity, mut casting, caster_tf, faction, caster_kind) in &mut casters {
        casting.cast_timer.tick(time.delta());
        if !casting.cast_timer.is_finished() {
            continue;
        }

        let ability = casting.ability;
        let target_pos = casting.target_pos;
        let target_entity = casting.target_entity;

        match ability {
            AbilityId::KnightCharge => {
                // Dash toward target position
                if let Some(target) = target_pos {
                    let dir = (target - caster_tf.translation).normalize_or_zero();
                    let dash_target = caster_tf.translation + dir * 8.0;
                    commands.entity(caster_entity).insert((
                        MoveTarget(dash_target),
                        ChargeBonus {
                            timer: Timer::from_seconds(2.0, TimerMode::Once),
                            damage_mult: 2.0,
                        },
                    ));
                }
            }

            AbilityId::MageFireball => {
                // Spawn AoE projectile toward target position
                if let Some(target) = target_pos {
                    let fireball_mat = materials.add(StandardMaterial {
                        base_color: Color::srgba(1.0, 0.3, 0.05, 0.9),
                        emissive: LinearRgba::new(2.0, 0.5, 0.05, 1.0),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        ..default()
                    });

                    // Find closest enemy at target for projectile tracking
                    let nearby = spatial_grid.query_radius(target, 4.0);
                    let projectile_target = nearby
                        .iter()
                        .filter(|(e, _)| {
                            factions
                                .get(*e)
                                .ok()
                                .map_or(false, |f| teams.is_hostile(faction, f))
                        })
                        .min_by(|a, b| {
                            let da = (a.1 - target).length_squared();
                            let db = (b.1 - target).length_squared();
                            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(e, _)| *e);

                    if let Some(target_e) = projectile_target {
                        commands.spawn((
                            Projectile {
                                target: target_e,
                                speed: 12.0,
                                damage: 20.0,
                                damage_type: DamageType::Magic,
                                fx_kind: CombatFxKind::Arcane,
                                impact_scale: 1.2,
                            },
                            AoeSplash {
                                radius: 4.0,
                                falloff: true,
                            },
                            Mesh3d(vfx.sphere_mesh.clone()),
                            MeshMaterial3d(fireball_mat),
                            Transform::from_translation(
                                caster_tf.translation + Vec3::Y * 1.5,
                            )
                            .with_scale(Vec3::splat(0.35)),
                            NotShadowCaster,
                            NotShadowReceiver,
                        ));
                    } else {
                        // No target found — do AoE at target point directly
                        aoe_damage_at(
                            &mut commands,
                            &spatial_grid,
                            &teams,
                            faction,
                            target,
                            4.0,
                            20.0,
                            DamageType::Magic,
                            true,
                            &mut targets_health,
                            &factions,
                            &vfx,
                            &mut materials,
                        );
                    }
                }
            }

            AbilityId::MageFrostNova => {
                // AoE damage + slow around self
                let center = caster_tf.translation;
                let nearby = spatial_grid.query_radius(center, 5.0);
                for (target_e, target_pos) in &nearby {
                    if *target_e == caster_entity {
                        continue;
                    }
                    let Some(target_f) = factions.get(*target_e).ok() else {
                        continue;
                    };
                    if !teams.is_hostile(faction, target_f) {
                        continue;
                    }
                    if let Ok((_, mut health, opt_armor, _)) = targets_health.get_mut(*target_e) {
                        let multiplier = opt_armor
                            .map(|a| DamageType::Magic.multiplier_vs(*a))
                            .unwrap_or(1.0);
                        health.current -= 15.0 * multiplier;
                    }
                    // Apply slow
                    commands.entity(*target_e).insert(StatusEffects {
                        effects: vec![ActiveStatusEffect {
                            kind: StatusEffectKind::Slow,
                            remaining: 4.0,
                            strength: 0.5,
                        }],
                    });
                }

                // Spawn frost ring VFX
                let frost_mat = materials.add(StandardMaterial {
                    base_color: Color::srgba(0.4, 0.7, 1.0, 0.5),
                    emissive: LinearRgba::new(0.2, 0.5, 1.5, 1.0),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                });
                commands.spawn((
                    VfxFlash {
                        timer: Timer::from_seconds(0.6, TimerMode::Once),
                        start_scale: 0.5,
                        end_scale: 5.0,
                        rise_speed: 0.0,
                    },
                    Mesh3d(vfx.sphere_mesh.clone()),
                    MeshMaterial3d(frost_mat),
                    Transform::from_translation(center).with_scale(Vec3::splat(0.5)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }

            AbilityId::PriestHeal => {
                // Heal a friendly unit
                if let Some(target_e) = target_entity {
                    if let Ok((target_tf, mut health, _, _)) = targets_health.get_mut(target_e) {
                        health.current = (health.current + 40.0).min(health.max);

                        // Green heal VFX
                        let heal_mat = materials.add(StandardMaterial {
                            base_color: Color::srgba(0.2, 1.0, 0.3, 0.7),
                            emissive: LinearRgba::new(0.1, 1.0, 0.2, 1.0),
                            alpha_mode: AlphaMode::Blend,
                            unlit: true,
                            ..default()
                        });
                        commands.spawn((
                            VfxFlash {
                                timer: Timer::from_seconds(0.4, TimerMode::Once),
                                start_scale: 0.3,
                                end_scale: 1.0,
                                rise_speed: 1.5,
                            },
                            Mesh3d(vfx.sphere_mesh.clone()),
                            MeshMaterial3d(heal_mat),
                            Transform::from_translation(target_tf.translation + Vec3::Y * 0.5)
                                .with_scale(Vec3::splat(0.3)),
                            NotShadowCaster,
                            NotShadowReceiver,
                        ));
                    }
                }
            }

            AbilityId::PriestHolySmite => {
                // Direct damage, bonus to undead
                if let Some(target_e) = target_entity {
                    if let Ok((target_tf, mut health, opt_armor, opt_kind)) =
                        targets_health.get_mut(target_e)
                    {
                        let is_undead = opt_kind.map_or(false, |k| {
                            matches!(k, EntityKind::Skeleton | EntityKind::SkeletonMinion)
                        });
                        let base_damage = if is_undead { 50.0 } else { 25.0 };
                        let multiplier = opt_armor
                            .map(|a| DamageType::Magic.multiplier_vs(*a))
                            .unwrap_or(1.0);
                        health.current -= base_damage * multiplier;

                        // Golden smite VFX
                        let smite_mat = materials.add(StandardMaterial {
                            base_color: Color::srgba(1.0, 0.9, 0.3, 0.8),
                            emissive: LinearRgba::new(1.5, 1.2, 0.3, 1.0),
                            alpha_mode: AlphaMode::Blend,
                            unlit: true,
                            ..default()
                        });
                        commands.spawn((
                            VfxFlash {
                                timer: Timer::from_seconds(0.3, TimerMode::Once),
                                start_scale: 0.2,
                                end_scale: 1.5,
                                rise_speed: 2.0,
                            },
                            Mesh3d(vfx.sphere_mesh.clone()),
                            MeshMaterial3d(smite_mat),
                            Transform::from_translation(target_tf.translation + Vec3::Y * 2.0)
                                .with_scale(Vec3::splat(0.2)),
                            NotShadowCaster,
                            NotShadowReceiver,
                        ));
                    }
                }
            }

            AbilityId::CatapultAoeBoulder => {
                // Handled passively via apply_catapult_aoe_splash system
            }
        }

        commands.entity(caster_entity).remove::<CastingAbility>();
    }
}

/// Helper: deal AoE damage at a world position.
fn aoe_damage_at(
    commands: &mut Commands,
    spatial_grid: &SpatialHashGrid,
    teams: &TeamConfig,
    caster_faction: &Faction,
    center: Vec3,
    radius: f32,
    damage: f32,
    damage_type: DamageType,
    falloff: bool,
    targets: &mut Query<(&Transform, &mut Health, Option<&ArmorType>, Option<&EntityKind>), Without<CastingAbility>>,
    factions: &Query<&Faction>,
    vfx: &VfxAssets,
    materials: &mut Assets<StandardMaterial>,
) {
    let nearby = spatial_grid.query_radius(center, radius);
    for (target_e, target_pos) in &nearby {
        let Some(target_f) = factions.get(*target_e).ok() else {
            continue;
        };
        if !teams.is_hostile(caster_faction, target_f) {
            continue;
        }
        if let Ok((_, mut health, opt_armor, _)) = targets.get_mut(*target_e) {
            let dist = (center - *target_pos).length();
            let dmg_mult = if falloff {
                (1.0 - dist / radius).max(0.3)
            } else {
                1.0
            };
            let armor_mult = opt_armor
                .map(|a| damage_type.multiplier_vs(*a))
                .unwrap_or(1.0);
            health.current -= damage * dmg_mult * armor_mult;
        }
    }

    // Spawn explosion VFX at center
    let explosion_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.5, 0.1, 0.8),
        emissive: LinearRgba::new(2.0, 0.8, 0.1, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.spawn((
        VfxFlash {
            timer: Timer::from_seconds(0.4, TimerMode::Once),
            start_scale: 0.5,
            end_scale: radius * 0.6,
            rise_speed: 0.3,
        },
        Mesh3d(vfx.sphere_mesh.clone()),
        MeshMaterial3d(explosion_mat),
        Transform::from_translation(center).with_scale(Vec3::splat(0.5)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Apply status effects: slow modifies speed, stun blocks actions, burning does DoT.
fn apply_status_effects(
    time: Res<Time>,
    mut units: Query<(&mut StatusEffects, Option<&mut Health>)>,
) {
    let dt = time.delta_secs();
    for (mut effects, mut opt_health) in &mut units {
        for effect in &mut effects.effects {
            effect.remaining -= dt;

            // Burning DoT
            if effect.kind == StatusEffectKind::Burning {
                if let Some(ref mut health) = opt_health {
                    health.current -= effect.strength * dt;
                }
            }
        }
    }
}

/// Remove expired status effects.
fn cleanup_expired_effects(mut units: Query<&mut StatusEffects>) {
    for mut effects in &mut units {
        effects.effects.retain(|e| e.remaining > 0.0);
    }
}

/// Tick charge bonus timer and remove when expired.
fn tick_charge_bonus(
    mut commands: Commands,
    time: Res<Time>,
    mut charged: Query<(Entity, &mut ChargeBonus)>,
) {
    for (entity, mut bonus) in &mut charged {
        bonus.timer.tick(time.delta());
        if bonus.timer.is_finished() {
            commands.entity(entity).remove::<ChargeBonus>();
        }
    }
}

/// Auto-add AoeSplash to catapult projectiles.
fn apply_catapult_aoe_splash(
    mut commands: Commands,
    new_projectiles: Query<(Entity, &Projectile), (Added<Projectile>, Without<AoeSplash>)>,
) {
    for (entity, proj) in &new_projectiles {
        if proj.damage_type == DamageType::SiegeDmg {
            commands.entity(entity).insert(AoeSplash {
                radius: 3.0,
                falloff: true,
            });
        }
    }
}

// ── Veterancy Systems ──

/// Apply stat bonuses when a unit levels up.
fn apply_veterancy_bonuses(
    mut units: Query<(
        &Experience,
        &mut VeterancyApplied,
        &mut Health,
        &mut AttackDamage,
        Option<&mut UnitSpeed>,
    )>,
) {
    for (exp, mut applied, mut health, mut damage, opt_speed) in &mut units {
        if exp.level == applied.0 {
            continue;
        }

        // Calculate ratio of old multipliers to apply delta
        let old_hp_mult = applied.0.hp_mult();
        let new_hp_mult = exp.level.hp_mult();
        let old_dmg_mult = applied.0.damage_mult();
        let new_dmg_mult = exp.level.damage_mult();

        // Scale max HP and heal proportionally
        let hp_ratio = new_hp_mult / old_hp_mult;
        health.max *= hp_ratio;
        health.current = (health.current * hp_ratio).min(health.max);

        // Scale damage
        let dmg_ratio = new_dmg_mult / old_dmg_mult;
        damage.0 *= dmg_ratio;

        // Scale speed
        if let Some(mut speed) = opt_speed {
            let old_spd = applied.0.speed_mult();
            let new_spd = exp.level.speed_mult();
            speed.0 *= new_spd / old_spd;
        }

        applied.0 = exp.level;
    }
}

/// Spawn golden star indicators above veteran/elite units.
fn spawn_veterancy_indicators(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    units: Query<
        (Entity, &Experience, &Children),
        Changed<Experience>,
    >,
    indicator_q: Query<Entity, With<VeterancyIndicator>>,
    children_q: Query<&Children>,
) {
    for (entity, exp, children) in &units {
        if exp.level.star_count() == 0 {
            continue;
        }

        // Remove existing indicators
        for child in children.iter() {
            if indicator_q.get(child).is_ok() {
                commands.entity(child).despawn();
            }
            // Also check grandchildren
            if let Ok(grandchildren) = children_q.get(child) {
                for gc in grandchildren.iter() {
                    if indicator_q.get(gc).is_ok() {
                        commands.entity(gc).despawn();
                    }
                }
            }
        }

        let star_mesh = meshes.add(Sphere::new(0.08));
        let star_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.2),
            emissive: LinearRgba::new(2.0, 1.5, 0.3, 1.0),
            ..default()
        });

        let star_count = exp.level.star_count();
        for i in 0..star_count {
            let x_offset = (i as f32 - (star_count as f32 - 1.0) / 2.0) * 0.25;
            let star = commands
                .spawn((
                    VeterancyIndicator,
                    Mesh3d(star_mesh.clone()),
                    MeshMaterial3d(star_mat.clone()),
                    Transform::from_xyz(x_offset, 2.5, 0.0),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();
            commands.entity(entity).add_child(star);
        }
    }
}
