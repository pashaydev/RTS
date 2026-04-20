//! Scrolling event log widget: kills, structures lost/built, tech
//! unlocks, and milestone callouts for the current match.

use bevy::prelude::*;
use std::collections::VecDeque;

use super::core::framework::WidgetId;
use super::core::hud::MainHudRoot;
use crate::types::{ActivePlayer, AppState, Faction, RtsCamera, TeamConfig};
use crate::ui::theme::{self, Theme};

pub struct EventLogWidgetPlugin;

impl Plugin for EventLogWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameEventLog>()
            .init_resource::<EventLogRenderState>()
            .add_systems(
                Update,
                spawn_event_log_widget
                    .run_if(in_state(AppState::InGame))
                    .run_if(any_with_component::<MainHudRoot>),
            )
            .add_systems(
                Update,
                (update_event_log, handle_event_log_click).run_if(in_state(AppState::InGame)),
            );
    }
}

widget_spawn_system!(spawn_event_log_widget, WidgetId::EventLog);

// ── Log Level ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

impl LogLevel {
    pub const ALL: &'static [LogLevel] = &[LogLevel::Info, LogLevel::Warning, LogLevel::Error];

    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Info => "Info",
            LogLevel::Warning => "Warn",
            LogLevel::Error => "Error",
        }
    }

    pub fn color(self, theme: &Theme) -> Color {
        match self {
            LogLevel::Info => theme::LOG_INFO,
            LogLevel::Warning => theme.colors.warning,
            LogLevel::Error => theme.colors.hp_low(),
        }
    }
}

// ── Event Log Resource ──

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventCategory {
    Combat,
    Construction,
    Training,
    Alert,
    Resource,
    Upgrade,
    Demolish,
    Network,
    Sync,
}

impl EventCategory {
    pub fn color(self, theme: &Theme) -> Color {
        match self {
            EventCategory::Combat => theme.colors.hp_low(),
            EventCategory::Construction => theme.colors.accent,
            EventCategory::Training => theme.colors.success,
            EventCategory::Alert => theme.colors.warning,
            EventCategory::Resource => theme::LOG_RESOURCE,
            EventCategory::Upgrade => theme::LOG_UPGRADE,
            EventCategory::Demolish => theme::LOG_DEMOLISH,
            EventCategory::Network => theme::LOG_NETWORK,
            EventCategory::Sync => theme::LOG_SYNC,
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            EventCategory::Combat => "!",
            EventCategory::Construction => "+",
            EventCategory::Training => "*",
            EventCategory::Alert => "?",
            EventCategory::Resource => "$",
            EventCategory::Upgrade => "^",
            EventCategory::Demolish => "-",
            EventCategory::Network => "@",
            EventCategory::Sync => "~",
        }
    }
}

#[derive(Clone)]
pub struct GameEvent {
    pub time: f32,
    pub message: String,
    pub category: EventCategory,
    pub level: LogLevel,
    pub world_pos: Option<Vec3>,
    /// Which faction triggered the event. `None` for global/neutral events.
    pub faction: Option<Faction>,
}

/// Stores the full history of game events (no cap).
/// The widget renders a virtual window into this list.
#[derive(Resource, Default)]
pub struct GameEventLog {
    pub entries: VecDeque<GameEvent>,
    pub revision: u64,
}

impl GameEventLog {
    pub fn push(
        &mut self,
        time: f32,
        message: String,
        category: EventCategory,
        world_pos: Option<Vec3>,
        faction: Option<Faction>,
    ) {
        self.entries.push_back(GameEvent {
            time,
            message,
            category,
            level: LogLevel::Info,
            world_pos,
            faction,
        });
        self.revision += 1;
    }

    pub fn push_with_level(
        &mut self,
        time: f32,
        message: String,
        category: EventCategory,
        level: LogLevel,
        world_pos: Option<Vec3>,
        faction: Option<Faction>,
    ) {
        self.entries.push_back(GameEvent {
            time,
            message,
            category,
            level,
            world_pos,
            faction,
        });
        self.revision += 1;
    }
}

// ── Widget UI ──

#[derive(Component)]
pub struct EventLogContent;

#[derive(Component)]
pub struct EventLogEntry(pub Option<Vec3>);

/// Tracks the last rendered revision so we only rebuild when data changes.
#[derive(Resource, Default)]
pub struct EventLogRenderState {
    pub last_revision: u64,
}

/// How many entries to render per "page" visible in the scrollable area.
const PAGE_SIZE: usize = 50;

pub fn update_event_log(
    mut commands: Commands,
    event_log: Res<GameEventLog>,
    theme: Res<Theme>,
    mut render_state: ResMut<EventLogRenderState>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    existing: Query<Entity, With<EventLogContent>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::EventLog) {
        return;
    }

    if render_state.last_revision == event_log.revision {
        return;
    }
    render_state.last_revision = event_log.revision;

    let Some(content) =
        super::widget_framework::find_widget_content(WidgetId::EventLog, &widget_q, &content_q)
    else {
        return;
    };

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

    // Filter: only show events from our faction or allied factions (or global events with no faction).
    let allied_events: Vec<&GameEvent> = event_log
        .entries
        .iter()
        .filter(|e| match e.faction {
            None => true,
            Some(ref f) => teams.is_allied(&active_player.0, f),
        })
        .collect();

    if allied_events.is_empty() {
        let empty = commands
            .spawn((
                Text::new("No events yet"),
                TextFont {
                    font_size: theme.typography.caption,
                    ..default()
                },
                TextColor(theme.colors.text_disabled),
            ))
            .id();
        commands.entity(container).add_child(empty);
        return;
    }

    // Header with total count
    let total = allied_events.len();
    let showing = total.min(PAGE_SIZE);
    let header = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                margin: UiRect::bottom(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    commands.entity(container).add_child(header);

    let header_text = commands
        .spawn((
            Text::new(format!("Events: {} (showing {})", total, showing)),
            TextFont {
                font_size: theme.typography.micro,
                ..default()
            },
            TextColor(theme.colors.text_disabled),
        ))
        .id();
    commands.entity(header).add_child(header_text);

    // Show most recent events first (reversed), render up to PAGE_SIZE
    for event in allied_events.iter().rev().take(PAGE_SIZE) {
        let row = commands
            .spawn((
                EventLogEntry(event.world_pos),
                Button,
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(3.0),
                    padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                    // border_radius: RADIUS_XS,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .id();
        commands.entity(container).add_child(row);

        // Log level indicator dot
        let level_dot = commands
            .spawn((
                Text::new("\u{2022}"),
                TextFont {
                    font_size: theme.typography.micro,
                    ..default()
                },
                TextColor(event.level.color(&theme)),
            ))
            .id();
        commands.entity(row).add_child(level_dot);

        // Category prefix
        let prefix = commands
            .spawn((
                Text::new(event.category.prefix()),
                TextFont {
                    font_size: theme.typography.tiny,
                    ..default()
                },
                TextColor(event.category.color(&theme)),
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
                TextFont {
                    font_size: theme.typography.micro,
                    ..default()
                },
                TextColor(theme.colors.text_disabled),
            ))
            .id();
        commands.entity(row).add_child(time_text);

        // Player name tag (colored by faction)
        if let Some(ref faction) = event.faction {
            let tag = commands
                .spawn((
                    Text::new(faction.display_name()),
                    TextFont {
                        font_size: theme.typography.micro,
                        ..default()
                    },
                    TextColor(faction.color()),
                ))
                .id();
            commands.entity(row).add_child(tag);
        }

        // Message
        let msg = commands
            .spawn((
                Text::new(&event.message),
                TextFont {
                    font_size: theme.typography.tiny,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();
        commands.entity(row).add_child(msg);
    }
}

pub fn handle_event_log_click(
    interactions: Query<(&Interaction, &EventLogEntry), Changed<Interaction>>,
    mut camera: Query<&mut Transform, With<RtsCamera>>,
) {
    for (interaction, entry) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(pos) = entry.0 {
            if let Ok(mut cam_transform) = camera.single_mut() {
                let offset = cam_transform.translation - cam_transform.forward() * 100.0;
                cam_transform.translation = Vec3::new(pos.x, cam_transform.translation.y, pos.z)
                    + (cam_transform.translation - offset).normalize_or_zero() * 0.0;
                cam_transform.translation.x = pos.x;
                cam_transform.translation.z = pos.z + 80.0;
            }
        }
    }
}
