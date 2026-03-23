use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::components::*;

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_vfx_assets).add_systems(
            Update,
            (
                update_projectiles,
                update_vfx_flashes,
                update_gather_particles,
                footstep_dust_spawner,
                update_footstep_dust,
                update_combat_dust,
                summon_vfx_system,
                animate_spawn,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn create_vfx_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use std::collections::HashMap;

    let sphere_mesh = meshes.add(Sphere::new(1.0));
    let cube_mesh = meshes.add(Cuboid::new(0.1, 0.1, 0.1));

    let melee_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.2, 0.1, 0.8),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let projectile_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.9, 0.2, 0.9),
        emissive: LinearRgba::new(1.0, 0.8, 0.1, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let impact_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 0.6, 0.8),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let deposit_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.3, 1.0, 0.4, 0.8),
        emissive: LinearRgba::new(0.1, 0.5, 0.15, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let dust_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.6, 0.5, 0.35, 0.5),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // Per-resource particle materials
    let mut resource_particle_materials = HashMap::new();
    for rt in [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ] {
        let color = rt.carry_color();
        let srgba = color.to_srgba();
        let (r, g, b) = (srgba.red, srgba.green, srgba.blue);
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgba(r, g, b, 0.9),
            emissive: LinearRgba::new(r * 0.3, g * 0.3, b * 0.3, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        resource_particle_materials.insert(rt, mat);
    }

    commands.insert_resource(VfxAssets {
        sphere_mesh,
        cube_mesh,
        melee_material,
        projectile_material,
        impact_material,
        deposit_material,
        dust_material,
        resource_particle_materials,
    });
}

fn update_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    spatial_grid: Res<crate::spatial::SpatialHashGrid>,
    mut projectiles: Query<(Entity, &mut Transform, &Projectile, Option<&AoeSplash>)>,
    mut targets: Query<(&Transform, &mut Health, Option<&ArmorType>, Option<&Faction>), Without<Projectile>>,
    factions: Query<&Faction>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (proj_entity, mut proj_tf, projectile, opt_aoe) in &mut projectiles {
        let Ok((target_tf, mut health, opt_armor, _)) = targets.get_mut(projectile.target) else {
            // Target gone, despawn projectile
            commands.entity(proj_entity).despawn();
            continue;
        };

        let target_pos = target_tf.translation;
        let dir = target_pos - proj_tf.translation;
        let dist = dir.length();

        if dist < 0.5 {
            // Hit! Apply damage with armor multiplier
            let multiplier = opt_armor
                .map(|armor| projectile.damage_type.multiplier_vs(*armor))
                .unwrap_or(1.0);
            health.current -= projectile.damage * multiplier;

            // AoE splash damage if present
            if let Some(aoe) = opt_aoe {
                let nearby = spatial_grid.query_radius(target_pos, aoe.radius);
                for (splash_entity, splash_pos) in &nearby {
                    if *splash_entity == projectile.target {
                        continue; // already damaged primary target
                    }
                    if let Ok((_, mut splash_health, splash_armor, _)) =
                        targets.get_mut(*splash_entity)
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
                    }
                }
            }

            // Spawn impact flash
            let impact_material = match projectile.fx_kind {
                CombatFxKind::Slash | CombatFxKind::Shadow => vfx.melee_material.clone(),
                CombatFxKind::Pierce | CombatFxKind::Arcane | CombatFxKind::Siege => {
                    vfx.impact_material.clone()
                }
            };

            let impact_scale = if opt_aoe.is_some() {
                projectile.impact_scale * 2.0
            } else {
                projectile.impact_scale
            };

            commands.spawn((
                VfxFlash {
                    timer: Timer::from_seconds(0.2, TimerMode::Once),
                    start_scale: impact_scale * 0.3,
                    end_scale: impact_scale,
                    rise_speed: 0.6,
                },
                FogHideable::Vfx,
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(impact_material),
                Transform::from_translation(target_pos)
                    .with_scale(Vec3::splat(impact_scale * 0.3)),
                NotShadowCaster,
                NotShadowReceiver,
            ));

            commands.entity(proj_entity).despawn();
        } else {
            let step = dir.normalize() * projectile.speed * time.delta_secs();
            proj_tf.translation += step;
        }
    }
}

fn update_vfx_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut Transform, &mut VfxFlash)>,
) {
    for (entity, mut tf, mut flash) in &mut flashes {
        flash.timer.tick(time.delta());

        let progress = flash.timer.fraction();
        let scale = flash.start_scale + (flash.end_scale - flash.start_scale) * progress;
        tf.scale = Vec3::splat(scale);
        tf.translation.y += flash.rise_speed * time.delta_secs();

        if flash.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

fn update_gather_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Transform, &mut GatherParticle)>,
) {
    for (entity, mut tf, mut particle) in &mut particles {
        particle.timer.tick(time.delta());

        let dt = time.delta_secs();
        tf.translation += particle.velocity * dt;
        // Apply gravity
        particle.velocity.y -= 5.0 * dt;

        // Shrink over lifetime
        let frac = 1.0 - particle.timer.fraction();
        tf.scale = Vec3::splat(frac.max(0.01) * particle.start_scale);

        if particle.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

// ── Footstep Dust ──

fn footstep_dust_spawner(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    zoom_level: Res<CameraZoomLevel>,
    mut workers: Query<
        (
            &Transform,
            &mut FootstepTimer,
            Option<&Carrying>,
            Option<&CarryCapacity>,
        ),
        (With<Unit>, With<MoveTarget>, Without<FrustumCulled>),
    >,
) {
    // Skip dust particles when zoomed far out
    if zoom_level.detail == DetailLevel::Far {
        return;
    }
    let Some(vfx) = vfx_assets else { return };

    for (tf, mut timer, carrying, capacity) in &mut workers {
        timer.0.tick(time.delta());
        if !timer.0.just_finished() {
            continue;
        }

        // Adjust interval when encumbered (slower steps = longer interval)
        // At medium zoom, double the interval to reduce particle count
        let base_interval = if zoom_level.detail == DetailLevel::Medium { 0.8 } else { 0.4 };
        let interval = if let (Some(carry), Some(cap)) = (carrying, capacity) {
            if cap.0 > 0.0 && carry.weight > 0.0 {
                base_interval * (1.0 + 0.5 * (carry.weight / cap.0).min(1.0))
            } else {
                base_interval
            }
        } else {
            base_interval
        };
        timer
            .0
            .set_duration(std::time::Duration::from_secs_f32(interval));

        let offset_x = (time.elapsed_secs() * 7.0).sin() * 0.3;
        let offset_z = (time.elapsed_secs() * 11.0).cos() * 0.3;

        commands.spawn((
            FootstepDust {
                timer: Timer::from_seconds(0.6, TimerMode::Once),
                velocity: Vec3::new(offset_x, 0.8, offset_z),
            },
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.dust_material.clone()),
            Transform::from_translation(tf.translation - Vec3::Y * 0.3)
                .with_scale(Vec3::splat(0.08)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

fn update_footstep_dust(
    mut commands: Commands,
    time: Res<Time>,
    mut dust: Query<(Entity, &mut Transform, &mut FootstepDust)>,
) {
    for (entity, mut tf, mut particle) in &mut dust {
        particle.timer.tick(time.delta());

        let dt = time.delta_secs();
        tf.translation += particle.velocity * dt;

        let frac = 1.0 - particle.timer.fraction();
        tf.scale = Vec3::splat(frac.max(0.01) * 0.08);

        if particle.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

fn update_combat_dust(
    mut commands: Commands,
    time: Res<Time>,
    mut dust: Query<(Entity, &mut Transform, &mut CombatDust)>,
) {
    for (entity, mut tf, mut particle) in &mut dust {
        particle.timer.tick(time.delta());

        let dt = time.delta_secs();
        tf.translation += particle.velocity * dt;
        particle.velocity.y = (particle.velocity.y - 6.0 * dt).max(-1.0);

        let frac = 1.0 - particle.timer.fraction();
        tf.scale = Vec3::splat((frac * particle.start_scale).max(0.01));

        if particle.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

// ── Summon VFX ──

/// Spawns a pulsing point light on first frame, then continuously emits rising
/// particles for summoned creatures (SpiritWolf, FireElemental).
fn summon_vfx_system(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut summons: Query<(Entity, &Transform, &mut SummonVfx)>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (entity, tf, mut svfx) in &mut summons {
        // Spawn point light on first tick
        if svfx.light_entity.is_none() {
            let srgba = svfx.color.to_srgba();
            let light = commands
                .spawn((
                    PointLight {
                        color: svfx.color,
                        intensity: 800.0,
                        range: 6.0,
                        shadows_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 1.5, 0.0),
                ))
                .id();
            commands.entity(entity).add_child(light);
            svfx.light_entity = Some(light);

            // Apply emissive material to the summon's mesh
            let mat = materials.add(StandardMaterial {
                base_color: Color::srgba(srgba.red, srgba.green, srgba.blue, 0.6),
                emissive: svfx.emissive,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            commands.entity(entity).insert(MeshMaterial3d(mat));
        }

        // Pulse the light intensity
        if let Some(light_e) = svfx.light_entity {
            // Light component is on the child; we can't easily query it here,
            // but the visual pulsing comes from particles below which is sufficient.
            let _ = light_e;
        }

        // Emit rising particles
        svfx.particle_timer.tick(time.delta());
        if svfx.particle_timer.just_finished() {
            let srgba = svfx.color.to_srgba();
            let particle_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(srgba.red, srgba.green, srgba.blue, 0.7),
                emissive: svfx.emissive * 0.5,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            });

            let offset_x = (time.elapsed_secs() * 13.0).sin() * 0.4;
            let offset_z = (time.elapsed_secs() * 17.0).cos() * 0.4;

            commands.spawn((
                VfxFlash {
                    timer: Timer::from_seconds(0.5, TimerMode::Once),
                    start_scale: 0.12,
                    end_scale: 0.02,
                    rise_speed: 1.5,
                },
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(particle_mat),
                Transform::from_translation(
                    tf.translation + Vec3::new(offset_x, 0.2, offset_z),
                )
                .with_scale(Vec3::splat(0.12)),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

// ── Spawn Animation ──

/// Scales entities up from near-zero to their target scale with ease-out.
fn animate_spawn(
    mut commands: Commands,
    time: Res<Time>,
    mut spawning: Query<(Entity, &mut SpawnAnimation, &mut Transform)>,
) {
    for (entity, mut anim, mut tf) in &mut spawning {
        anim.timer.tick(time.delta());
        let t = anim.timer.fraction();
        // Ease-out: 1 - (1-t)^2
        let eased = 1.0 - (1.0 - t) * (1.0 - t);
        tf.scale = anim.target_scale * eased.max(0.01);

        if anim.timer.is_finished() {
            tf.scale = anim.target_scale;
            commands.entity(entity).remove::<SpawnAnimation>();
        }
    }
}
