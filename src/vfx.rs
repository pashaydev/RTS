use bevy::prelude::*;

use crate::components::*;

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_vfx_assets)
            .add_systems(Update, (update_projectiles, update_vfx_flashes));
    }
}

fn create_vfx_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sphere_mesh = meshes.add(Sphere::new(1.0));

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

    commands.insert_resource(VfxAssets {
        sphere_mesh,
        melee_material,
        projectile_material,
        impact_material,
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

        if flash.timer.finished() {
            commands.entity(entity).despawn();
        }
    }
}
