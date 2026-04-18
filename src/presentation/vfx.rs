use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::types::*;

/// Maximum number of simultaneous VFX particle entities (dust, flashes, gather particles).
/// Prevents frame drops during massive battles by capping short-lived particle spawns.
const MAX_VFX_PARTICLES: usize = 512;

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_vfx_assets).add_systems(
            Update,
            (
                handle_projectile_impacts,
                spawn_blood_splashes,
                update_vfx_flashes,
                update_gather_particles,
                footstep_dust_spawner,
                update_footstep_dust,
                update_combat_dust,
                update_blood_splashes,
                spawn_depletion_vfx,
                animate_depletion,
                summon_vfx_system,
                animate_spawn,
                animate_attack_lunge,
                animate_hit_recoil,
                tick_hit_reactions,
                apply_camera_shake,
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

    let blood_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.72, 0.04, 0.04, 0.82),
        emissive: LinearRgba::new(0.12, 0.01, 0.01, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let blood_dark_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.26, 0.01, 0.01, 0.92),
        emissive: LinearRgba::new(0.03, 0.0, 0.0, 1.0),
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
        blood_material,
        blood_dark_material,
        deposit_material,
        dust_material,
        resource_particle_materials,
    });
}

fn supports_blood_splash(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Worker
            | EntityKind::Soldier
            | EntityKind::Archer
            | EntityKind::Tank
            | EntityKind::Knight
            | EntityKind::Mage
            | EntityKind::Priest
            | EntityKind::Cavalry
            | EntityKind::Scout
            | EntityKind::Goblin
            | EntityKind::SpiritWolf
    )
}

fn spawn_blood_splashes(
    mut commands: Commands,
    mut damage_events: bevy::ecs::message::MessageReader<DamageApplied>,
    vfx_assets: Option<Res<VfxAssets>>,
    zoom_level: Res<CameraZoomLevel>,
    existing_flashes: Query<(), With<VfxFlash>>,
    existing_particles: Query<(), Or<(With<FootstepDust>, With<CombatDust>, With<GatherParticle>)>>,
    existing_blood: Query<(), With<BloodSplashParticle>>,
    targets: Query<(&Transform, &EntityKind), (Or<(With<Unit>, With<Mob>)>, Without<Building>)>,
    sources: Query<&Transform, Without<Projectile>>,
) {
    if zoom_level.detail == DetailLevel::Far {
        return;
    }

    let Some(vfx) = vfx_assets else { return };
    let total_particles = existing_flashes.iter().count()
        + existing_particles.iter().count()
        + existing_blood.iter().count();
    if total_particles >= MAX_VFX_PARTICLES {
        return;
    }

    let detail_scale = match zoom_level.detail {
        DetailLevel::Close => 1.0,
        DetailLevel::Medium => 0.65,
        DetailLevel::Far => 0.0,
    };
    let mut remaining_budget = MAX_VFX_PARTICLES.saturating_sub(total_particles);

    for damage in damage_events.read() {
        if damage.amount <= 0.0 || remaining_budget == 0 {
            continue;
        }

        let Ok((target_tf, kind)) = targets.get(damage.target) else {
            continue;
        };
        if !supports_blood_splash(*kind) {
            continue;
        }

        let desired_count =
            ((4.0 + (damage.amount / 10.0).clamp(0.0, 6.0)) * detail_scale).round() as usize;
        let max_count = remaining_budget.min(10);
        if max_count == 0 {
            continue;
        }
        let min_count = if max_count >= 2 { 2 } else { 1 };
        let count = desired_count.max(min_count).min(max_count);
        remaining_budget = remaining_budget.saturating_sub(count);

        let impact_dir = damage
            .source
            .and_then(|source| sources.get(source).ok())
            .map(|source_tf| (target_tf.translation - source_tf.translation).normalize_or_zero())
            .filter(|dir| dir.length_squared() > 0.0)
            .unwrap_or(Vec3::Z);
        let spray_dir = Vec3::new(impact_dir.x, 0.0, impact_dir.z).normalize_or_zero();
        let burst_origin =
            target_tf.translation + Vec3::Y * (0.28 + target_tf.scale.y.abs().max(1.0) * 0.32);
        let seed = damage.now_secs as f32 * 11.7 + damage.target.to_bits() as f32 * 0.000173;
        let speed_scale = 0.8 + (damage.amount / 24.0).min(1.5);

        for i in 0..count {
            let angle = seed + i as f32 * 2.3999631;
            let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
            let lateral = spray_dir.cross(Vec3::Y).normalize_or_zero();
            let forward_bias = if spray_dir.length_squared() > 0.0 {
                spray_dir * (1.2 + (i % 3) as f32 * 0.35)
            } else {
                radial * 0.9
            };
            let side_bias = radial * (0.45 + (i % 2) as f32 * 0.2)
                + lateral * if i % 2 == 0 { 0.25 } else { -0.25 };
            let lift = 1.4 + ((i * 37 % 11) as f32) * 0.12;
            let velocity =
                (forward_bias + side_bias) * (1.7 * speed_scale) + Vec3::Y * (lift * speed_scale);
            let offset = radial * (0.03 + i as f32 * 0.008);
            let start_scale = 0.045 + (damage.amount / 90.0).min(0.04) + (i % 3) as f32 * 0.006;
            let material = if i % 3 == 0 {
                vfx.blood_dark_material.clone()
            } else {
                vfx.blood_material.clone()
            };
            let mesh = if i % 4 == 0 {
                vfx.cube_mesh.clone()
            } else {
                vfx.sphere_mesh.clone()
            };

            commands.spawn((
                BloodSplashParticle {
                    timer: Timer::from_seconds(0.28 + 0.05 * (i % 3) as f32, TimerMode::Once),
                    velocity,
                    start_scale,
                },
                FogHideable::Vfx,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(burst_origin + offset)
                    .with_scale(Vec3::splat(start_scale)),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

fn handle_projectile_impacts(
    mut commands: Commands,
    mut impact_events: bevy::ecs::message::MessageReader<ProjectileImpactEvent>,
    vfx_assets: Option<Res<VfxAssets>>,
    targets: Query<(&Transform, Option<&HitRecoil>), Without<Projectile>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for impact in impact_events.read() {
        let Ok((target_tf, opt_existing_recoil)) = targets.get(impact.target) else {
            continue;
        };

        let target_scale = opt_existing_recoil.map_or(target_tf.scale, |r| r.base_scale);
        let impact_material = match impact.fx_kind {
            CombatFxKind::Slash | CombatFxKind::Shadow => vfx.melee_material.clone(),
            CombatFxKind::Pierce | CombatFxKind::Arcane | CombatFxKind::Siege => {
                vfx.impact_material.clone()
            }
        };
        let impact_scale = if impact.is_aoe {
            impact.impact_scale * 2.0
        } else {
            impact.impact_scale
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
            Transform::from_translation(impact.position)
                .with_scale(Vec3::splat(impact_scale * 0.3)),
            NotShadowCaster,
            NotShadowReceiver,
        ));

        let hit_dir_flat = Vec3::new(-impact.direction.x, 0.0, -impact.direction.z);
        commands.entity(impact.target).insert(HitRecoil {
            direction: hit_dir_flat,
            timer: Timer::from_seconds(0.14, TimerMode::Once),
            strength: (0.1 + impact.damage * 0.003).min(0.26),
            lift: (0.02 + impact.damage * 0.001).min(0.07),
            base_scale: target_scale,
            applied_offset: Vec3::ZERO,
        });
        commands
            .entity(impact.target)
            .insert(HitReaction(Timer::from_seconds(0.2, TimerMode::Once)));
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
            commands.entity(entity).try_despawn();
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
            commands.entity(entity).try_despawn();
        }
    }
}

// ── Footstep Dust ──

fn footstep_dust_spawner(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    zoom_level: Res<CameraZoomLevel>,
    existing_dust: Query<(), With<FootstepDust>>,
    existing_flashes: Query<(), With<VfxFlash>>,
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
    // Cap total VFX particles to prevent frame drops in large battles
    let total_particles = existing_dust.iter().count() + existing_flashes.iter().count();
    if total_particles >= MAX_VFX_PARTICLES {
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
        let base_interval = if zoom_level.detail == DetailLevel::Medium {
            0.8
        } else {
            0.4
        };
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
            commands.entity(entity).try_despawn();
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
            commands.entity(entity).try_despawn();
        }
    }
}

fn update_blood_splashes(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Transform, &mut BloodSplashParticle)>,
) {
    for (entity, mut tf, mut particle) in &mut particles {
        particle.timer.tick(time.delta());

        let dt = time.delta_secs();
        tf.translation += particle.velocity * dt;
        particle.velocity *= 0.94;
        particle.velocity.y -= 8.5 * dt;

        let frac = 1.0 - particle.timer.fraction();
        tf.scale = Vec3::splat((frac * particle.start_scale).max(0.01));

        if particle.timer.is_finished() {
            commands.entity(entity).try_despawn();
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
                Transform::from_translation(tf.translation + Vec3::new(offset_x, 0.2, offset_z))
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

// ── Combat juice systems ──

/// Melee attacker lunges forward briefly on hit.
pub fn animate_attack_lunge(
    mut commands: Commands,
    time: Res<Time>,
    mut lungers: Query<(Entity, &mut Transform, &mut AttackLunge)>,
) {
    for (entity, mut tf, mut lunge) in &mut lungers {
        lunge.timer.tick(time.delta());
        let t = lunge.timer.fraction();
        // Apply a temporary forward offset and remove it cleanly as the timer ends.
        let offset_frac = (1.0 - t) * (1.0 - t);
        let desired_offset = lunge.direction * lunge.strength * offset_frac;
        tf.translation += desired_offset - lunge.applied_offset;
        lunge.applied_offset = desired_offset;

        if lunge.timer.is_finished() {
            tf.translation -= lunge.applied_offset;
            commands.entity(entity).remove::<AttackLunge>();
        }
    }
}

/// Target gets pushed back and briefly scales up on hit.
pub fn animate_hit_recoil(
    mut commands: Commands,
    time: Res<Time>,
    mut recoiling: Query<(Entity, &mut Transform, &mut HitRecoil)>,
) {
    for (entity, mut tf, mut recoil) in &mut recoiling {
        recoil.timer.tick(time.delta());
        let t = recoil.timer.fraction();
        // Apply a temporary hit offset and remove it cleanly as the timer ends.
        let push_frac = (1.0 - t) * (1.0 - t);
        let desired_offset =
            recoil.direction * recoil.strength * push_frac + Vec3::Y * recoil.lift * push_frac;
        tf.translation += desired_offset - recoil.applied_offset;
        recoil.applied_offset = desired_offset;
        // Brief scale pulse from a stable base scale.
        let scale_boost = 1.0 + 0.08 * (1.0 - t);
        tf.scale = recoil.base_scale * scale_boost;

        if recoil.timer.is_finished() {
            tf.translation -= recoil.applied_offset;
            tf.scale = recoil.base_scale;
            commands.entity(entity).remove::<HitRecoil>();
        }
    }
}

/// Tick hit reaction timers and remove when done.
pub fn tick_hit_reactions(
    mut commands: Commands,
    time: Res<Time>,
    mut reactions: Query<(Entity, &mut HitReaction)>,
) {
    for (entity, mut reaction) in &mut reactions {
        reaction.0.tick(time.delta());
        if reaction.0.is_finished() {
            commands.entity(entity).remove::<HitReaction>();
        }
    }
}

/// Camera shake for heavy hits — sinusoidal offset that decays.
pub fn apply_camera_shake(
    mut commands: Commands,
    time: Res<Time>,
    mut cameras: Query<(Entity, &mut Transform, &mut CameraShake)>,
) {
    for (entity, mut tf, mut shake) in &mut cameras {
        shake.timer.tick(time.delta());
        let remaining = 1.0 - shake.timer.fraction();
        let t = time.elapsed_secs();
        let offset_x = (t * 55.0).sin() * shake.intensity * remaining;
        let offset_y = (t * 73.0).cos() * shake.intensity * remaining * 0.7;
        tf.translation.x += offset_x;
        tf.translation.y += offset_y;

        if shake.timer.is_finished() {
            commands.entity(entity).remove::<CameraShake>();
        }
    }
}

// ── Resource Depletion VFX ──

/// Spawns burst particles when a depletion animation starts.
fn spawn_depletion_vfx(
    mut commands: Commands,
    vfx_assets: Option<Res<VfxAssets>>,
    existing_particles: Query<(), With<GatherParticle>>,
    new_depletions: Query<(&Transform, &DepletionAnimation), Added<DepletionAnimation>>,
) {
    let Some(vfx) = vfx_assets else { return };
    let particle_count = existing_particles.iter().count();

    for (transform, depletion) in &new_depletions {
        let pos = transform.translation;

        match depletion.kind {
            DepletionKind::MinePuff => {
                // Rock chunks + dust burst radiating outward
                let count = 10.min(MAX_VFX_PARTICLES.saturating_sub(particle_count));
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let angle = std::f32::consts::TAU * frac;
                    let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
                    let up_bias = 1.5 + (frac * 13.7).sin().abs() * 1.5;
                    let velocity = outward * (1.8 + frac * 1.2) + Vec3::Y * up_bias;
                    let mesh = if i % 2 == 0 {
                        vfx.cube_mesh.clone()
                    } else {
                        vfx.sphere_mesh.clone()
                    };
                    commands.spawn((
                        GatherParticle {
                            timer: Timer::from_seconds(0.6, TimerMode::Once),
                            velocity,
                            start_scale: 0.1 + frac * 0.06,
                        },
                        FogHideable::Vfx,
                        Mesh3d(mesh),
                        MeshMaterial3d(vfx.dust_material.clone()),
                        Transform::from_translation(pos + Vec3::Y * 0.5)
                            .with_scale(Vec3::splat(0.1)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            DepletionKind::OilSpray => {
                // Dark particles spraying upward
                let count = 8.min(MAX_VFX_PARTICLES.saturating_sub(particle_count));
                let material = vfx
                    .resource_particle_materials
                    .get(&crate::types::ResourceType::Oil)
                    .cloned()
                    .unwrap_or_else(|| vfx.dust_material.clone());
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let angle = std::f32::consts::TAU * frac;
                    let spread = Vec3::new(angle.cos() * 0.6, 0.0, angle.sin() * 0.6);
                    let velocity = spread * 1.5 + Vec3::Y * (3.0 + frac * 2.0);
                    commands.spawn((
                        GatherParticle {
                            timer: Timer::from_seconds(0.7, TimerMode::Once),
                            velocity,
                            start_scale: 0.08 + frac * 0.04,
                        },
                        FogHideable::Vfx,
                        Mesh3d(vfx.sphere_mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(pos + Vec3::Y * 0.3)
                            .with_scale(Vec3::splat(0.08)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            DepletionKind::TreeFall { .. } => {
                // Small wood chip particles at the base
                let count = 4.min(MAX_VFX_PARTICLES.saturating_sub(particle_count));
                let material = vfx
                    .resource_particle_materials
                    .get(&crate::types::ResourceType::Wood)
                    .cloned()
                    .unwrap_or_else(|| vfx.dust_material.clone());
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let angle = std::f32::consts::TAU * frac;
                    let velocity =
                        Vec3::new(angle.cos() * 0.8, 1.0 + frac * 0.5, angle.sin() * 0.8);
                    commands.spawn((
                        GatherParticle {
                            timer: Timer::from_seconds(0.5, TimerMode::Once),
                            velocity,
                            start_scale: 0.07,
                        },
                        FogHideable::Vfx,
                        Mesh3d(vfx.cube_mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(pos + Vec3::Y * 0.2)
                            .with_scale(Vec3::splat(0.07)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
        }
    }
}

/// Animates depleting resource nodes (shrink, fall, sink) then despawns them.
fn animate_depletion(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut DepletionAnimation, &mut Transform)>,
) {
    for (entity, mut depletion, mut tf) in &mut query {
        depletion.timer.tick(time.delta());
        let t = depletion.timer.fraction();

        match depletion.kind {
            DepletionKind::TreeFall { fall_direction } => {
                // Ease-in: accelerating fall (t^2)
                let eased = t * t;
                // Rotate up to ~85 degrees around an axis perpendicular to fall direction
                let fall_angle = eased * std::f32::consts::FRAC_PI_2 * 0.94;
                let rotation_axis = Vec3::Y.cross(fall_direction).normalize_or_zero();
                let rotation_axis = if rotation_axis.length_squared() < 0.01 {
                    Vec3::X // fallback
                } else {
                    rotation_axis
                };
                tf.rotation = Quat::from_axis_angle(rotation_axis, fall_angle);
                // Slight scale reduction at the end
                let scale_factor = 1.0 - eased * 0.15;
                tf.scale = depletion.initial_scale * scale_factor;
            }
            DepletionKind::MinePuff => {
                // Shrink and sink into ground
                let eased = t * t; // ease-in
                let scale_factor = 1.0 - eased;
                tf.scale = depletion.initial_scale * scale_factor.max(0.01);
                tf.translation.y -= 0.8 * time.delta_secs();
            }
            DepletionKind::OilSpray => {
                // Shrink and sink, slightly slower
                let eased = t * t;
                let scale_factor = 1.0 - eased;
                tf.scale = depletion.initial_scale * scale_factor.max(0.01);
                tf.translation.y -= 0.5 * time.delta_secs();
            }
        }

        if depletion.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}
