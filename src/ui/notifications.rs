use bevy::prelude::*;

use crate::components::*;
use crate::theme;

#[derive(Component)]
pub struct AllyNotificationToast {
    pub spawn_time: f32,
    pub world_pos: Option<Vec3>,
}

#[derive(Component)]
pub struct AllyNotificationContainer;

pub fn spawn_notification_container(commands: &mut Commands, parent: Entity) {
    let container = commands
        .spawn((
            AllyNotificationContainer,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(50.0),
                left: Val::Percent(50.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(container);
}

pub fn update_ally_notifications(
    mut commands: Commands,
    time: Res<Time>,
    mut notifications: ResMut<AllyNotifications>,
    container_q: Query<Entity, With<AllyNotificationContainer>>,
    existing_toasts: Query<(Entity, &AllyNotificationToast)>,
) {
    let elapsed = time.elapsed_secs();

    for (entity, toast) in existing_toasts.iter() {
        if elapsed - toast.spawn_time > 5.0 {
            commands.entity(entity).despawn();
        }
    }

    let visible_count = existing_toasts.iter().filter(|(_, t)| elapsed - t.spawn_time < 5.0).count();

    notifications.active.retain(|n| elapsed - n.timestamp < 5.0);

    let Ok(container) = container_q.single() else { return; };

    for notif in &notifications.active {
        let already_spawned = existing_toasts.iter().any(|(_, t)| {
            (t.spawn_time - notif.timestamp).abs() < 0.01
        });
        if already_spawned || visible_count >= 3 {
            continue;
        }

        let color = notif.kind.color();
        let bg_color = Color::srgba(0.1, 0.1, 0.15, 0.9);

        commands.entity(container).with_children(|parent| {
            parent.spawn((
                AllyNotificationToast {
                    spawn_time: notif.timestamp,
                    world_pos: notif.world_pos,
                },
                Node {
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    margin: UiRect::left(Val::Px(-150.0)),
                    min_width: Val::Px(200.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(bg_color),
                BorderColor::all(color),
            ))
            .with_children(|toast| {
                toast.spawn((
                    Text::new(notif.message.clone()),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(color),
                ));
            });
        });
    }
}

pub fn handle_notification_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    toasts_q: Query<(Entity, &AllyNotificationToast, &Node, &GlobalTransform)>,
    mut camera_q: Query<&mut RtsCamera>,
    windows: Query<&Window>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else { return; };
    let Some(cursor_pos) = window.cursor_position() else { return; };

    for (entity, toast, _node, _gtf) in toasts_q.iter() {
        if let Some(world_pos) = toast.world_pos {
            let toast_y_range = 50.0..130.0;
            let toast_x_range = (window.width() / 2.0 - 150.0)..(window.width() / 2.0 + 150.0);

            if toast_x_range.contains(&cursor_pos.x) && toast_y_range.contains(&cursor_pos.y) {
                if let Ok(mut cam) = camera_q.single_mut() {
                    cam.target_pivot = Vec3::new(world_pos.x, 0.0, world_pos.z);
                }
                commands.entity(entity).despawn();
                return;
            }
        }
    }
}
