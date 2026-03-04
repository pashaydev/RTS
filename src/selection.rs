use bevy::prelude::*;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::picking::mesh_picking::ray_cast::MeshRayCast;
use bevy::window::PrimaryWindow;

use crate::components::*;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .init_resource::<DragState>()
            .add_systems(Startup, spawn_selection_box)
            .add_systems(
                Update,
                (
                    track_drag,
                    update_selection_box_visual,
                    handle_click_select,
                    handle_right_click_move,
                )
                    .chain(),
            );
    }
}

fn spawn_selection_box(mut commands: Commands) {
    commands.spawn((
        SelectionBox,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Px(0.0),
            height: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.2, 0.4, 1.0, 0.2)),
        BorderColor(Color::srgba(0.3, 0.5, 1.0, 0.8)),
        Visibility::Hidden,
        GlobalTransform::default(),
    ));
}

fn track_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut drag: ResMut<DragState>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        drag.start = Some(cursor);
        drag.current = Some(cursor);
        drag.dragging = false;
    }

    if mouse.pressed(MouseButton::Left) {
        drag.current = Some(cursor);
        if let Some(start) = drag.start {
            if (cursor - start).length() > 5.0 {
                drag.dragging = true;
            }
        }
    }
}

fn update_selection_box_visual(
    drag: Res<DragState>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut query: Query<(&mut Node, &mut Visibility), With<SelectionBox>>,
) {
    let Ok((mut node, mut vis)) = query.get_single_mut() else {
        return;
    };

    if drag.dragging && mouse.pressed(MouseButton::Left) {
        if let (Some(start), Some(current)) = (drag.start, drag.current) {
            let min_x = start.x.min(current.x);
            let min_y = start.y.min(current.y);
            let w = (current.x - start.x).abs();
            let h = (current.y - start.y).abs();

            node.left = Val::Px(min_x);
            node.top = Val::Px(min_y);
            node.width = Val::Px(w);
            node.height = Val::Px(h);
            *vis = Visibility::Visible;
        }
    } else {
        *vis = Visibility::Hidden;
    }
}

fn handle_click_select(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut drag: ResMut<DragState>,
    placement: Res<BuildingPlacementState>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ray_cast: MeshRayCast,
    units: Query<Entity, With<Unit>>,
    buildings: Query<Entity, With<Building>>,
    selected: Query<Entity, With<Selected>>,
    unit_transforms: Query<&GlobalTransform, With<Unit>>,
) {
    if !mouse.just_released(MouseButton::Left) {
        return;
    }

    // Don't select while placing a building
    if placement.mode != PlacementMode::None {
        return;
    }

    let was_dragging = drag.dragging;
    let drag_start = drag.start;
    let drag_end = drag.current;

    // Reset drag state
    drag.start = None;
    drag.current = None;
    drag.dragging = false;

    let Ok(window) = windows.get_single() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.get_single() else {
        return;
    };

    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    if was_dragging {
        // Box select
        if let (Some(start), Some(end)) = (drag_start, drag_end) {
            let min_x = start.x.min(end.x);
            let max_x = start.x.max(end.x);
            let min_y = start.y.min(end.y);
            let max_y = start.y.max(end.y);

            if !shift {
                for entity in &selected {
                    commands.entity(entity).remove::<Selected>();
                }
            }

            for entity in &units {
                if let Ok(gt) = unit_transforms.get(entity) {
                    if let Ok(screen_pos) = camera.world_to_viewport(cam_gt, gt.translation()) {
                        if screen_pos.x >= min_x
                            && screen_pos.x <= max_x
                            && screen_pos.y >= min_y
                            && screen_pos.y <= max_y
                        {
                            commands.entity(entity).insert(Selected);
                        }
                    }
                }
            }
        }
    } else {
        // Click select
        let Some(cursor) = window.cursor_position() else {
            return;
        };
        let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
            return;
        };

        let hits = ray_cast.cast_ray(ray, &default());

        let mut hit_unit = None;
        let mut hit_building = None;
        for (entity, _) in hits {
            if units.contains(*entity) {
                hit_unit = Some(*entity);
                break;
            }
            if buildings.contains(*entity) {
                hit_building = Some(*entity);
                break;
            }
        }

        if !shift {
            for entity in &selected {
                commands.entity(entity).remove::<Selected>();
            }
        }

        if let Some(entity) = hit_unit {
            if shift && selected.contains(entity) {
                commands.entity(entity).remove::<Selected>();
            } else {
                commands.entity(entity).insert(Selected);
            }
        } else if let Some(entity) = hit_building {
            if shift && selected.contains(entity) {
                commands.entity(entity).remove::<Selected>();
            } else {
                commands.entity(entity).insert(Selected);
            }
        }
    }
}

fn handle_right_click_move(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    selected_units: Query<Entity, (With<Unit>, With<Selected>)>,
    mut ray_cast: MeshRayCast,
    mobs: Query<Entity, With<Mob>>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.get_single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
        return;
    };

    let entities: Vec<Entity> = selected_units.iter().collect();
    if entities.is_empty() {
        return;
    }

    // Check if we clicked on a mob
    let hits = ray_cast.cast_ray(ray, &default());
    let mut hit_mob = None;
    for (entity, _) in hits {
        if mobs.contains(*entity) {
            hit_mob = Some(*entity);
            break;
        }
    }

    if let Some(mob_entity) = hit_mob {
        // Attack the mob
        for entity in &entities {
            commands
                .entity(*entity)
                .remove::<GatherTarget>()
                .remove::<MoveTarget>()
                .insert(AttackTarget(mob_entity));
        }
    } else {
        // Move to ground — intersect with terrain approximation (Y=0 plane, then adjust)
        if let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) {
            let point = ray.get_point(dist);
            let n = entities.len();

            if n == 1 {
                commands
                    .entity(entities[0])
                    .remove::<GatherTarget>()
                    .remove::<AttackTarget>()
                    .insert(MoveTarget(point));
            } else if n > 1 {
                let spacing = 1.5;
                let radius = (spacing * n as f32 / std::f32::consts::TAU).max(1.0);
                for (i, entity) in entities.iter().enumerate() {
                    let angle = i as f32 / n as f32 * std::f32::consts::TAU;
                    let offset =
                        Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                    commands
                        .entity(*entity)
                        .remove::<GatherTarget>()
                        .remove::<AttackTarget>()
                        .insert(MoveTarget(point + offset));
                }
            }
        }
    }
}
