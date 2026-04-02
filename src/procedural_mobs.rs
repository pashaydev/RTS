use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

use crate::components::*;
use crate::ground::HeightMap;

pub struct ProceduralMobsPlugin;

impl Plugin for ProceduralMobsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                trigger_mob_attack_anim,
                animate_procedural_mobs,
                update_demon_pulse_rings,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Attack animation trigger ──

fn trigger_mob_attack_anim(
    mut mobs: Query<(&mut ProceduralMob, &AttackWindup), (With<Mob>, Added<AttackWindup>)>,
) {
    for (mut proc_mob, _windup) in &mut mobs {
        if proc_mob.attack_timer.is_some() {
            continue;
        }
        let duration = match proc_mob.visual_kind {
            MobVisualKind::Goblin => 0.25,
            MobVisualKind::Skeleton => 0.3,
            MobVisualKind::Orc => 0.35,
            MobVisualKind::Demon => 0.4,
        };
        proc_mob.attack_timer = Some(Timer::from_seconds(duration, TimerMode::Once));
        proc_mob.pulse_ring_spawned = false;
    }
}

// ── Main procedural animation system ──

#[derive(Clone, Copy, PartialEq)]
enum MobAnimPhase {
    Idle,
    Moving,
    Attacking,
    Dying,
}

fn animate_procedural_mobs(
    mut commands: Commands,
    time: Res<Time>,
    height_map: Res<HeightMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut mobs: Query<
        (
            Entity,
            &mut Transform,
            &mut ProceduralMob,
            Option<&PatrolState>,
            Option<&MoveTarget>,
            Option<&AttackWindup>,
            Option<&mut Dying>,
            Has<SpawnAnimation>,
        ),
        With<Mob>,
    >,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    for (entity, mut tf, mut proc_mob, patrol, move_target, windup, mut dying, has_spawn_anim) in
        &mut mobs
    {
        if has_spawn_anim {
            continue;
        }

        // Initialize base values from actual Transform on first tick
        if !proc_mob.initialized {
            proc_mob.base_scale = tf.scale;
            proc_mob.base_translation = tf.translation;
            proc_mob.initialized = true;
        }

        // Determine animation phase
        let phase = if dying.is_some() {
            MobAnimPhase::Dying
        } else if windup.is_some() || proc_mob.attack_timer.is_some() {
            MobAnimPhase::Attacking
        } else if move_target.is_some()
            || patrol
                .map(|p| {
                    matches!(
                        p.state,
                        PatrolStateKind::Patrolling
                            | PatrolStateKind::Chasing
                            | PatrolStateKind::Returning
                    )
                })
                .unwrap_or(false)
        {
            MobAnimPhase::Moving
        } else {
            MobAnimPhase::Idle
        };

        // Advance phase accumulator
        proc_mob.phase += dt * 6.0;
        if proc_mob.phase > TAU * 100.0 {
            proc_mob.phase -= TAU * 100.0;
        }

        // Tick attack timer
        if let Some(ref mut timer) = proc_mob.attack_timer {
            timer.tick(time.delta());
            if timer.is_finished() {
                proc_mob.attack_timer = None;
            }
        }

        // Tick dying progress (frame-rate independent)
        if let Some(ref mut dying) = dying {
            dying.timer.tick(time.delta());
        }

        if phase == MobAnimPhase::Dying {
            proc_mob.dying_progress = (proc_mob.dying_progress + dt * 1.5).min(1.0);
        }

        // Track base position from movement systems (they change XZ externally)
        // Sample terrain height so mobs stay grounded to the terrain
        if phase != MobAnimPhase::Dying {
            proc_mob.base_translation.x = tf.translation.x;
            proc_mob.base_translation.z = tf.translation.z;
            proc_mob.base_translation.y = height_map
                .sample(tf.translation.x, tf.translation.z)
                + proc_mob.base_y_offset;
        }

        let base_scale = proc_mob.base_scale;
        let base_y = proc_mob.base_translation.y;
        let p = proc_mob.phase;

        // Reset scale to base before applying animation (prevents accumulation)
        tf.scale = base_scale;

        match proc_mob.visual_kind {
            MobVisualKind::Goblin => {
                animate_goblin(&mut tf, phase, base_scale, base_y, p, &proc_mob);
            }
            MobVisualKind::Skeleton => {
                animate_skeleton(&mut tf, phase, base_scale, base_y, p, t, &proc_mob);
            }
            MobVisualKind::Orc => {
                animate_orc(&mut tf, phase, base_scale, base_y, p, dt, &proc_mob);
            }
            MobVisualKind::Demon => {
                animate_demon(
                    &mut commands,
                    &mut tf,
                    phase,
                    base_scale,
                    base_y,
                    p,
                    dt,
                    &mut proc_mob,
                    &mut meshes,
                    &mut materials,
                );
            }
        }

        // Despawn when dying timer is complete (single despawn point)
        if phase == MobAnimPhase::Dying {
            if let Some(ref d) = dying {
                if d.timer.is_finished() {
                    commands.entity(entity).try_despawn();
                }
            }
        }
    }
}

// ── Per-mob animation functions ──
// All animations SET absolute values instead of accumulating via +=

fn animate_goblin(
    tf: &mut Transform,
    phase: MobAnimPhase,
    base_scale: Vec3,
    base_y: f32,
    p: f32,
    proc_mob: &ProceduralMob,
) {
    match phase {
        MobAnimPhase::Idle => {
            let bounce = 0.02 * (p * 3.0).sin();
            tf.translation.y = base_y + bounce;
        }
        MobAnimPhase::Moving => {
            let hop = 0.08 * (p * 8.0).sin().abs();
            tf.translation.y = base_y + hop;
            let hop_phase = (p * 8.0).sin();
            if hop_phase < 0.2 {
                tf.scale.y = base_scale.y * 0.95;
                tf.scale.x = base_scale.x * 1.02;
                tf.scale.z = base_scale.z * 1.02;
            }
        }
        MobAnimPhase::Attacking => {
            tf.translation.y = base_y;
            if let Some(ref timer) = proc_mob.attack_timer {
                let t = timer.fraction();
                if t < 0.5 {
                    let squash = t * 2.0;
                    tf.scale.y = base_scale.y * (1.0 - 0.15 * squash);
                    tf.scale.x = base_scale.x * (1.0 + 0.15 * squash);
                    tf.scale.z = base_scale.z * (1.0 + 0.15 * squash);
                } else {
                    let recover = (t - 0.5) * 2.0;
                    tf.scale.y = base_scale.y * (0.85 + 0.15 * recover);
                    tf.scale.x = base_scale.x * (1.15 - 0.15 * recover);
                    tf.scale.z = base_scale.z * (1.15 - 0.15 * recover);
                }
            }
        }
        MobAnimPhase::Dying => {
            let progress = proc_mob.dying_progress;
            let shrink = (1.0 - progress).max(0.0);
            tf.scale = base_scale * shrink;
            tf.rotation *= Quat::from_rotation_y(progress * PI * 1.5);
            tf.translation.y = base_y - progress * 0.2;
        }
    }
}

fn animate_skeleton(
    tf: &mut Transform,
    phase: MobAnimPhase,
    base_scale: Vec3,
    base_y: f32,
    p: f32,
    time: f32,
    proc_mob: &ProceduralMob,
) {
    let jitter_x = 0.01 * (time * 15.0 + p * 3.7).sin();
    let jitter_z = 0.01 * (time * 13.0 + p * 5.1).cos();

    match phase {
        MobAnimPhase::Idle => {
            tf.translation.y = base_y;
            tf.translation.x += jitter_x;
            tf.translation.z += jitter_z;
        }
        MobAnimPhase::Moving => {
            tf.translation.y = base_y;
            tf.translation.x += jitter_x * 1.5;
            tf.translation.z += jitter_z * 1.5;
            tf.rotation *= Quat::from_rotation_z(0.015 * (p * 12.0).sin());
        }
        MobAnimPhase::Attacking => {
            tf.translation.y = base_y;
            if let Some(ref timer) = proc_mob.attack_timer {
                let t = timer.fraction();
                let osc = (t * PI * 2.0).sin();
                tf.scale.y = base_scale.y * (1.0 + 0.25 * osc);
                tf.scale.x = base_scale.x * (1.0 + 0.1 * osc.abs());
                tf.scale.z = base_scale.z * (1.0 + 0.1 * osc.abs());
                tf.translation.x += jitter_x * 2.0;
                tf.translation.z += jitter_z * 2.0;
            }
        }
        MobAnimPhase::Dying => {
            let progress = proc_mob.dying_progress;
            let y_factor = (1.0 - progress).max(0.02);
            tf.scale.y = base_scale.y * y_factor;
            let spread = 1.0 + (1.0 - y_factor) * 0.5;
            tf.scale.x = base_scale.x * spread;
            tf.scale.z = base_scale.z * spread;
            tf.translation.y = base_y - progress * 0.5;
        }
    }
}

fn animate_orc(
    tf: &mut Transform,
    phase: MobAnimPhase,
    base_scale: Vec3,
    base_y: f32,
    p: f32,
    _dt: f32,
    proc_mob: &ProceduralMob,
) {
    match phase {
        MobAnimPhase::Idle => {
            let pulse = 1.0 + 0.03 * (p * 1.5).sin();
            tf.scale = base_scale * pulse;
            tf.translation.y = base_y;
        }
        MobAnimPhase::Moving => {
            let stomp = 0.08 * (p * 4.0).sin().abs();
            tf.translation.y = base_y + stomp;
            tf.rotation *= Quat::from_rotation_z(0.02 * (p * 2.0).sin());
        }
        MobAnimPhase::Attacking => {
            tf.translation.y = base_y;
            if let Some(ref timer) = proc_mob.attack_timer {
                let t = timer.fraction();
                if t < 0.33 {
                    let rise = t / 0.33;
                    tf.scale.y = base_scale.y * (1.0 + 0.15 * rise);
                    tf.scale.x = base_scale.x * (1.0 - 0.05 * rise);
                    tf.scale.z = base_scale.z * (1.0 - 0.05 * rise);
                } else if t < 0.66 {
                    let slam = (t - 0.33) / 0.33;
                    tf.scale.y = base_scale.y * (1.15 - 0.35 * slam);
                    tf.scale.x = base_scale.x * (0.95 + 0.25 * slam);
                    tf.scale.z = base_scale.z * (0.95 + 0.25 * slam);
                } else {
                    let recover = (t - 0.66) / 0.34;
                    tf.scale.y = base_scale.y * (0.8 + 0.2 * recover);
                    tf.scale.x = base_scale.x * (1.2 - 0.2 * recover);
                    tf.scale.z = base_scale.z * (1.2 - 0.2 * recover);
                }
            }
        }
        MobAnimPhase::Dying => {
            let progress = proc_mob.dying_progress;
            let topple_angle = (progress * PI * 0.45).min(PI * 0.45);
            tf.rotation = Quat::from_rotation_x(topple_angle);
            if progress > 0.5 {
                let shrink = 1.0 - (progress - 0.5) * 2.0;
                tf.scale = base_scale * shrink.max(0.0);
            }
            tf.translation.y = base_y;
        }
    }
}

fn animate_demon(
    commands: &mut Commands,
    tf: &mut Transform,
    phase: MobAnimPhase,
    base_scale: Vec3,
    base_y: f32,
    p: f32,
    dt: f32,
    proc_mob: &mut ProceduralMob,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let hover_offset = 0.3 + 0.1 * (p * 2.0).sin();

    match phase {
        MobAnimPhase::Idle => {
            tf.translation.y = base_y + hover_offset;
            tf.rotation *= Quat::from_rotation_y(dt * 0.5);
        }
        MobAnimPhase::Moving => {
            tf.translation.y = base_y + hover_offset;
            tf.rotation *= Quat::from_rotation_y(dt * 1.5);
        }
        MobAnimPhase::Attacking => {
            tf.translation.y = base_y + hover_offset;
            if let Some(ref timer) = proc_mob.attack_timer {
                let t = timer.fraction();
                let pulse = if t < 0.5 {
                    1.0 + 0.25 * (t * 2.0)
                } else {
                    1.25 - 0.25 * ((t - 0.5) * 2.0)
                };
                tf.scale = base_scale * pulse;

                // Spawn pulse ring exactly once per attack
                if t > 0.45 && !proc_mob.pulse_ring_spawned {
                    proc_mob.pulse_ring_spawned = true;
                    spawn_demon_pulse_ring(commands, tf.translation, meshes, materials);
                }
            }
            tf.rotation *= Quat::from_rotation_y(dt * 2.0);
        }
        MobAnimPhase::Dying => {
            let progress = proc_mob.dying_progress;
            let scale_factor = if progress < 0.2 {
                1.0 + progress * 1.5
            } else {
                let shrink_t = (progress - 0.2) / 0.8;
                (1.3 * (1.0 - shrink_t)).max(0.0)
            };
            tf.scale = base_scale * scale_factor;
            tf.translation.y = base_y + hover_offset * (1.0 - progress);
            let spin_speed = 8.0 + progress * 12.0;
            tf.rotation *= Quat::from_rotation_y(dt * spin_speed);
        }
    }
}

// ── Demon pulse ring VFX ──

fn spawn_demon_pulse_ring(
    commands: &mut Commands,
    position: Vec3,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = meshes.add(Cylinder::new(1.0, 0.05));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.8, 0.1, 0.1, 0.6),
        emissive: LinearRgba::new(1.5, 0.2, 0.1, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(position).with_scale(Vec3::new(0.5, 1.0, 0.5)),
        DemonPulseRing {
            timer: Timer::from_seconds(0.4, TimerMode::Once),
        },
        GameWorld,
    ));
}

fn update_demon_pulse_rings(
    mut commands: Commands,
    time: Res<Time>,
    mut rings: Query<(Entity, &mut DemonPulseRing, &mut Transform)>,
) {
    for (entity, mut ring, mut tf) in &mut rings {
        ring.timer.tick(time.delta());
        let t = ring.timer.fraction();

        let radius = 0.5 + 3.5 * t;
        tf.scale.x = radius;
        tf.scale.z = radius;
        tf.scale.y = (1.0 - t).max(0.0);

        if ring.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}
