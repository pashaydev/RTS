use bevy::prelude::*;

use crate::components::*;

pub struct AttentionPlugin;

impl Plugin for AttentionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                track_health_changes,
                update_damage_popups,
                manage_attention_icons,
                position_overlays,
                cleanup_orphaned_icons,
            ),
        );
    }
}

// ── Constants ──

const POPUP_LIFETIME: f32 = 0.9;
const POPUP_RISE_SPEED: f32 = 40.0; // pixels per second
const POPUP_FONT_SIZE: f32 = 15.0;
const UNDER_ATTACK_DURATION: f32 = 2.5;
const ICON_OFFSET_Y_WORLD: f32 = 1.8;
const POPUP_OFFSET_Y_WORLD: f32 = 2.2;
const ICON_SIZE: f32 = 20.0;

// ── System 1: Detect HP changes, spawn popups, manage UnderAttackTimer ──

fn track_health_changes(
    mut commands: Commands,
    time: Res<Time>,
    new_units: Query<(Entity, &Health), Without<PreviousHealth>>,
    mut tracked: Query<(
        Entity,
        &Transform,
        &Health,
        &mut PreviousHealth,
        Option<&mut UnderAttackTimer>,
    )>,
) {
    // Insert PreviousHealth on new entities
    for (entity, health) in &new_units {
        commands
            .entity(entity)
            .insert(PreviousHealth(health.current));
    }

    for (_entity, tf, health, mut prev, mut opt_timer) in &mut tracked {
        // Tick under-attack timer every frame
        if let Some(ref mut timer) = opt_timer {
            timer.0.tick(time.delta());
        }

        let delta = health.current - prev.0;

        if delta.abs() > 0.01 {
            let is_damage = delta < 0.0;

            // Random-ish horizontal scatter based on time
            let scatter = (time.elapsed_secs() * 17.3).sin() * 20.0;

            // Spawn damage popup UI node
            commands.spawn((
                DamagePopup {
                    timer: Timer::from_seconds(POPUP_LIFETIME, TimerMode::Once),
                    amount: delta.abs(),
                    is_damage,
                    world_pos: tf.translation + Vec3::Y * POPUP_OFFSET_Y_WORLD,
                    offset_x: scatter,
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-1000.0),
                    top: Val::Px(-1000.0),
                    ..default()
                },
                Text::new(if is_damage {
                    format!("-{}", delta.abs() as u32)
                } else {
                    format!("+{}", delta.abs() as u32)
                }),
                TextFont {
                    font_size: POPUP_FONT_SIZE,
                    ..default()
                },
                TextColor(if is_damage {
                    Color::srgb(1.0, 0.3, 0.25)
                } else {
                    Color::srgb(0.3, 1.0, 0.4)
                }),
                TextLayout::new_with_justify(Justify::Center),
                Pickable::IGNORE,
            ));

            // Mark as under attack
            if is_damage {
                if let Some(ref mut timer) = opt_timer {
                    timer.0.reset();
                } else {
                    commands.entity(_entity).insert(UnderAttackTimer(
                        Timer::from_seconds(UNDER_ATTACK_DURATION, TimerMode::Once),
                    ));
                }
            }
        }

        prev.0 = health.current;
    }
}

// ── System 2: Animate and despawn damage popups ──

fn update_damage_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut popups: Query<(Entity, &mut DamagePopup, &mut TextColor, &mut TextFont)>,
) {
    for (entity, mut popup, mut color, mut font) in &mut popups {
        popup.timer.tick(time.delta());

        popup.offset_x *= 0.98;

        let frac = popup.timer.fraction();

        // Scale: pop in then settle
        let scale = if frac < 0.15 {
            0.5 + (frac / 0.15) * 0.7
        } else if frac < 0.3 {
            1.2 - ((frac - 0.15) / 0.15) * 0.2
        } else {
            1.0
        };
        font.font_size = POPUP_FONT_SIZE * scale;

        // Fade out in the last 40%
        let alpha = if frac > 0.6 {
            1.0 - (frac - 0.6) / 0.4
        } else {
            1.0
        };
        let base = color.0.to_srgba();
        color.0 = Color::srgba(base.red, base.green, base.blue, alpha);

        // Rise in world Y
        popup.world_pos.y += POPUP_RISE_SPEED * time.delta_secs() * 0.02;

        if popup.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

// ── System 3: Manage attention icons based on unit state ──

fn determine_attention_kind(
    worker_task: Option<&WorkerTask>,
    has_attack_target: bool,
    under_attack: Option<&UnderAttackTimer>,
) -> Option<AttentionKind> {
    // Priority: UnderAttack > Attacking > Gathering > Building
    if let Some(timer) = under_attack {
        if !timer.0.is_finished() {
            return Some(AttentionKind::UnderAttack);
        }
    }
    if has_attack_target {
        return Some(AttentionKind::Attacking);
    }
    if let Some(task) = worker_task {
        match task {
            WorkerTask::Gathering(_) | WorkerTask::MovingToResource(_) => {
                return Some(AttentionKind::Gathering);
            }
            WorkerTask::Building(_) | WorkerTask::MovingToBuild(_) => {
                return Some(AttentionKind::Building);
            }
            _ => {}
        }
    }
    None
}

fn manage_attention_icons(
    mut commands: Commands,
    attention_assets: Option<Res<AttentionIconAssets>>,
    units: Query<
        (
            Entity,
            Option<&WorkerTask>,
            Option<&AttackTarget>,
            Option<&UnderAttackTimer>,
        ),
        With<Unit>,
    >,
    existing_icons: Query<(Entity, &AttentionIcon)>,
) {
    let Some(assets) = attention_assets else {
        return;
    };

    // Build map of current icons by owner
    let mut icon_map: std::collections::HashMap<Entity, (Entity, AttentionKind)> =
        std::collections::HashMap::new();
    for (icon_entity, icon) in &existing_icons {
        icon_map.insert(icon.owner, (icon_entity, icon.kind));
    }

    for (unit_entity, worker_task, attack_target, under_attack) in &units {
        let desired =
            determine_attention_kind(worker_task, attack_target.is_some(), under_attack);

        match (icon_map.remove(&unit_entity), desired) {
            (Some((_icon_e, existing_kind)), Some(desired_kind))
                if existing_kind == desired_kind => {}
            (Some((icon_e, _)), Some(desired_kind)) => {
                commands.entity(icon_e).despawn();
                spawn_attention_icon(&mut commands, &assets, unit_entity, desired_kind);
            }
            (None, Some(desired_kind)) => {
                spawn_attention_icon(&mut commands, &assets, unit_entity, desired_kind);
            }
            (Some((icon_e, _)), None) => {
                commands.entity(icon_e).despawn();
            }
            (None, None) => {}
        }
    }

    // Remove icons for entities that no longer exist in the unit query
    for (_owner, (icon_e, _)) in icon_map {
        commands.entity(icon_e).despawn();
    }
}

fn spawn_attention_icon(
    commands: &mut Commands,
    assets: &AttentionIconAssets,
    owner: Entity,
    kind: AttentionKind,
) {
    let (image, tint) = match kind {
        AttentionKind::UnderAttack => (
            assets.under_attack.clone(),
            Color::srgb(1.0, 0.25, 0.2),
        ),
        AttentionKind::Gathering => (
            assets.gathering.clone(),
            Color::srgb(0.95, 0.75, 0.3),
        ),
        AttentionKind::Attacking => (
            assets.attacking.clone(),
            Color::srgb(1.0, 0.4, 0.35),
        ),
        AttentionKind::Building => (
            assets.building.clone(),
            Color::srgb(0.5, 0.75, 1.0),
        ),
    };

    commands.spawn((
        AttentionIcon { owner, kind },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-1000.0),
            top: Val::Px(-1000.0),
            width: Val::Px(ICON_SIZE),
            height: Val::Px(ICON_SIZE),
            ..default()
        },
        ImageNode {
            image,
            color: tint,
            ..default()
        },
        Pickable::IGNORE,
    ));
}

// ── System 4: Project world positions to screen for all overlays ──

fn position_overlays(
    time: Res<Time>,
    camera_q: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    fog_map: Option<Res<FogOfWarMap>>,
    mut popups: Query<(&DamagePopup, &mut Node, &mut Visibility), Without<AttentionIcon>>,
    mut icons: Query<
        (&AttentionIcon, &mut Node, &mut Visibility),
        Without<DamagePopup>,
    >,
    transforms: Query<&Transform>,
) {
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };

    // Position damage popups
    for (popup, mut node, mut vis) in &mut popups {
        let rise = popup.timer.fraction() * POPUP_RISE_SPEED;
        if let Ok(vp) = camera.world_to_viewport(cam_gt, popup.world_pos) {
            let fog_visible = fog_map
                .as_ref()
                .map(|f| f.get_visible(popup.world_pos.x, popup.world_pos.z) > 0.2)
                .unwrap_or(true);
            if fog_visible {
                node.left = Val::Px(vp.x + popup.offset_x);
                node.top = Val::Px(vp.y - rise);
                *vis = Visibility::Inherited;
            } else {
                *vis = Visibility::Hidden;
            }
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // Position attention icons
    for (icon, mut node, mut vis) in &mut icons {
        let Ok(owner_tf) = transforms.get(icon.owner) else {
            *vis = Visibility::Hidden;
            continue;
        };

        let world_pos = owner_tf.translation + Vec3::Y * ICON_OFFSET_Y_WORLD;

        let fog_visible = fog_map
            .as_ref()
            .map(|f| f.get_visible(world_pos.x, world_pos.z) > 0.2)
            .unwrap_or(true);

        if !fog_visible {
            *vis = Visibility::Hidden;
            continue;
        }

        if let Ok(vp) = camera.world_to_viewport(cam_gt, world_pos) {
            // Micro-animation: pulsing scale for under-attack, gentle bob for others
            let size = if icon.kind == AttentionKind::UnderAttack {
                ICON_SIZE * (1.0 + 0.2 * (time.elapsed_secs() * 6.0).sin().abs())
            } else {
                ICON_SIZE
            };

            let bob = match icon.kind {
                AttentionKind::UnderAttack => (time.elapsed_secs() * 5.0).sin() * 3.0,
                _ => (time.elapsed_secs() * 2.5).sin() * 2.0,
            };

            node.width = Val::Px(size);
            node.height = Val::Px(size);
            node.left = Val::Px(vp.x - size * 0.5);
            node.top = Val::Px(vp.y - size * 0.5 + bob);
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

// ── System 5: Cleanup orphaned attention icons ──

fn cleanup_orphaned_icons(
    mut commands: Commands,
    icons: Query<(Entity, &AttentionIcon)>,
    existing: Query<Entity, With<Unit>>,
) {
    for (icon_entity, icon) in &icons {
        if existing.get(icon.owner).is_err() {
            commands.entity(icon_entity).despawn();
        }
    }
}
