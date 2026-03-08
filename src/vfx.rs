use bevy::prelude::*;

use crate::components::*;

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_vfx_assets)
            .add_systems(Update, (
                update_projectiles,
                update_vfx_flashes,
                gather_particle_spawner,
                update_gather_particles,
                footstep_dust_spawner,
                update_footstep_dust,
            ));
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
    for rt in [ResourceType::Wood, ResourceType::Copper, ResourceType::Iron, ResourceType::Gold, ResourceType::Oil] {
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
    mut projectiles: Query<(Entity, &mut Transform, &Projectile)>,
    mut targets: Query<(&Transform, &mut Health), Without<Projectile>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (proj_entity, mut proj_tf, projectile) in &mut projectiles {
        let Ok((target_tf, mut health)) = targets.get_mut(projectile.target) else {
            // Target gone, despawn projectile
            commands.entity(proj_entity).despawn();
            continue;
        };

        let target_pos = target_tf.translation;
        let dir = target_pos - proj_tf.translation;
        let dist = dir.length();

        if dist < 0.5 {
            // Hit! Apply damage
            health.current -= projectile.damage;

            // Spawn impact flash
            commands.spawn((
                VfxFlash {
                    timer: Timer::from_seconds(0.2, TimerMode::Once),
                    start_scale: 0.2,
                    end_scale: 0.6,
                },
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(vfx.impact_material.clone()),
                Transform::from_translation(target_pos)
                    .with_scale(Vec3::splat(0.2)),
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

        if flash.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

// ── Gather Particles ──

fn gather_particle_spawner(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<
        (&Transform, &WorkerTask, &mut GatherParticleTimer),
        With<Unit>,
    >,
    nodes: Query<&ResourceNode>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (tf, task, mut particle_timer) in &mut workers {
        let WorkerTask::Gathering(node) = task else {
            continue;
        };
        particle_timer.0.tick(time.delta());
        if !particle_timer.0.just_finished() {
            continue;
        }

        let rt = nodes.get(*node).ok()
            .map(|n| n.resource_type)
            .unwrap_or(ResourceType::Wood);

        let mat = vfx.resource_particle_materials.get(&rt)
            .cloned()
            .unwrap_or(vfx.impact_material.clone());

        let mesh = match rt {
            ResourceType::Wood => vfx.cube_mesh.clone(),
            _ => vfx.sphere_mesh.clone(),
        };

        // Spawn 2-3 particles
        let count = 2 + (time.elapsed_secs() as u32 % 2);
        for i in 0..count {
            let angle = std::f32::consts::TAU * (i as f32 / count as f32) + time.elapsed_secs() * 3.0;
            let vel = Vec3::new(
                angle.cos() * 2.0,
                1.5 + (i as f32 * 0.3),
                angle.sin() * 2.0,
            );
            let scale = match rt {
                ResourceType::Wood => 0.8,
                ResourceType::Oil => 0.5,
                _ => 0.6,
            };
            commands.spawn((
                GatherParticle {
                    timer: Timer::from_seconds(0.5, TimerMode::Once),
                    velocity: vel,
                },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(tf.translation + Vec3::Y * 0.5)
                    .with_scale(Vec3::splat(scale)),
            ));
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
        tf.scale = Vec3::splat(frac.max(0.01) * 0.8);

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
    mut workers: Query<
        (&Transform, &mut FootstepTimer, Option<&Carrying>, Option<&CarryCapacity>),
        (With<Unit>, With<MoveTarget>),
    >,
) {
    let Some(vfx) = vfx_assets else { return };

    for (tf, mut timer, carrying, capacity) in &mut workers {
        timer.0.tick(time.delta());
        if !timer.0.just_finished() {
            continue;
        }

        // Adjust interval when encumbered (slower steps = longer interval)
        let base_interval = 0.4;
        let interval = if let (Some(carry), Some(cap)) = (carrying, capacity) {
            if cap.0 > 0.0 && carry.weight > 0.0 {
                base_interval * (1.0 + 0.5 * (carry.weight / cap.0).min(1.0))
            } else {
                base_interval
            }
        } else {
            base_interval
        };
        timer.0.set_duration(std::time::Duration::from_secs_f32(interval));

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
