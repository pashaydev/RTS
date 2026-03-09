use bevy::prelude::*;
use std::collections::VecDeque;

use crate::theme;

// ── Event Log Resource ──

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventCategory {
    Combat,
    Construction,
    Training,
    Alert,
}

impl EventCategory {
    pub fn color(self) -> Color {
        match self {
            EventCategory::Combat => theme::HP_LOW,
            EventCategory::Construction => theme::ACCENT,
            EventCategory::Training => theme::SUCCESS,
            EventCategory::Alert => theme::WARNING,
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            EventCategory::Combat => "!",
            EventCategory::Construction => "+",
            EventCategory::Training => "*",
            EventCategory::Alert => "?",
        }
    }
}

#[derive(Clone)]
pub struct GameEvent {
    pub time: f32,
    pub message: String,
    pub category: EventCategory,
    pub world_pos: Option<Vec3>,
}

#[derive(Resource, Default)]
pub struct GameEventLog {
    pub entries: VecDeque<GameEvent>,
}

impl GameEventLog {
    pub fn push(&mut self, time: f32, message: String, category: EventCategory, world_pos: Option<Vec3>) {
        self.entries.push_back(GameEvent { time, message, category, world_pos });
        if self.entries.len() > 50 {
            self.entries.pop_front();
        }
    }
}

// ── Widget UI ──

#[derive(Component)]
pub struct EventLogContent;

#[derive(Component)]
pub struct EventLogEntry(pub Option<Vec3>);

pub fn update_event_log(
    mut commands: Commands,
    event_log: Res<GameEventLog>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    existing: Query<Entity, With<EventLogContent>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::EventLog) {
        return;
    }

    if !event_log.is_changed() {
        return;
    }

    // Find content entity
    let mut content_entity = None;
    for (widget, widget_children) in &widget_q {
        if widget.id == WidgetId::EventLog {
            for wchild in widget_children.iter() {
                if content_q.get(wchild).is_ok() {
                    content_entity = Some(wchild);
                }
            }
        }
    }
    let Some(content) = content_entity else { return; };

    // Clear existing
    for entity in &existing {
        commands.entity(entity).try_despawn();
    }

    let container = commands
        .spawn((
            EventLogContent,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(1.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(container);

    if event_log.entries.is_empty() {
        let empty = commands
            .spawn((
                Text::new("No events yet"),
                TextFont { font_size: 9.0, ..default() },
                TextColor(theme::TEXT_DISABLED),
            ))
            .id();
        commands.entity(container).add_child(empty);
        return;
    }

    // Show most recent events first (reversed)
    for event in event_log.entries.iter().rev().take(20) {
        let row = commands
            .spawn((
                EventLogEntry(event.world_pos),
                Button,
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(3.0),
                    padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .id();
        commands.entity(container).add_child(row);

        // Category prefix
        let prefix = commands
            .spawn((
                Text::new(event.category.prefix()),
                TextFont { font_size: 8.0, ..default() },
                TextColor(event.category.color()),
            ))
            .id();
        commands.entity(row).add_child(prefix);

        // Time
        let mins = (event.time / 60.0) as u32;
        let secs = (event.time % 60.0) as u32;
        let time_str = format!("{:02}:{:02}", mins, secs);
        let time_text = commands
            .spawn((
                Text::new(time_str),
                TextFont { font_size: 7.0, ..default() },
                TextColor(theme::TEXT_DISABLED),
            ))
            .id();
        commands.entity(row).add_child(time_text);

        // Message
        let msg = commands
            .spawn((
                Text::new(&event.message),
                TextFont { font_size: 8.0, ..default() },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(row).add_child(msg);
    }
}

pub fn handle_event_log_click(
    interactions: Query<(&Interaction, &EventLogEntry), Changed<Interaction>>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    for (interaction, entry) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(pos) = entry.0 {
            if let Ok(mut cam_transform) = camera.single_mut() {
                // Move camera to look at the event position
                let offset = cam_transform.translation - cam_transform.forward() * 100.0;
                cam_transform.translation = Vec3::new(pos.x, cam_transform.translation.y, pos.z)
                    + (cam_transform.translation - offset).normalize_or_zero() * 0.0;
                cam_transform.translation.x = pos.x;
                cam_transform.translation.z = pos.z + 80.0;
            }
        }
    }
}
