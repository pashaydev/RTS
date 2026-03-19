use bevy::prelude::*;
use std::collections::VecDeque;

use crate::components::{ActivePlayer, Faction, TeamConfig};
use crate::theme;

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

    pub fn color(self) -> Color {
        match self {
            LogLevel::Info => Color::srgb(0.4, 0.75, 1.0),
            LogLevel::Warning => theme::WARNING,
            LogLevel::Error => theme::HP_LOW,
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
    pub fn color(self) -> Color {
        match self {
            EventCategory::Combat => theme::HP_LOW,
            EventCategory::Construction => theme::ACCENT,
            EventCategory::Training => theme::SUCCESS,
            EventCategory::Alert => theme::WARNING,
            EventCategory::Resource => Color::srgb(0.4, 0.75, 1.0),
            EventCategory::Upgrade => Color::srgb(0.9, 0.7, 0.2),
            EventCategory::Demolish => Color::srgb(0.8, 0.3, 0.3),
            EventCategory::Network => Color::srgb(0.2, 0.8, 0.6),
            EventCategory::Sync => Color::srgb(0.6, 0.5, 0.9),
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

/// Filter pill button — stores which log level it toggles.
#[derive(Component)]
pub struct LogLevelPill(pub LogLevel);

/// Tracks which log levels are currently visible.
#[derive(Resource)]
pub struct EventLogFilter {
    pub visible: [bool; 3], // Info, Warning, Error
    pub revision: u64,
}

impl Default for EventLogFilter {
    fn default() -> Self {
        Self {
            visible: [true, true, true],
            revision: 0,
        }
    }
}

impl EventLogFilter {
    fn index(level: LogLevel) -> usize {
        match level {
            LogLevel::Info => 0,
            LogLevel::Warning => 1,
            LogLevel::Error => 2,
        }
    }

    pub fn is_visible(&self, level: LogLevel) -> bool {
        self.visible[Self::index(level)]
    }

    pub fn toggle(&mut self, level: LogLevel) {
        let i = Self::index(level);
        self.visible[i] = !self.visible[i];
        self.revision += 1;
    }
}

/// Tracks the last rendered revision so we only rebuild when data changes.
#[derive(Resource, Default)]
pub struct EventLogRenderState {
    pub last_revision: u64,
    pub last_filter_revision: u64,
}

/// How many entries to render per "page" visible in the scrollable area.
const PAGE_SIZE: usize = 50;

pub fn handle_log_level_pills(
    interactions: Query<(&Interaction, &LogLevelPill), Changed<Interaction>>,
    mut filter: ResMut<EventLogFilter>,
) {
    for (interaction, pill) in &interactions {
        if *interaction == Interaction::Pressed {
            filter.toggle(pill.0);
        }
    }
}

pub fn update_event_log(
    mut commands: Commands,
    event_log: Res<GameEventLog>,
    mut render_state: ResMut<EventLogRenderState>,
    filter: Res<EventLogFilter>,
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

    let data_changed = render_state.last_revision != event_log.revision;
    let filter_changed = render_state.last_filter_revision != filter.revision;

    if !data_changed && !filter_changed {
        return;
    }
    render_state.last_revision = event_log.revision;
    render_state.last_filter_revision = filter.revision;

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

    // ── Filter pills row ──
    let pills_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(3.0),
            padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
            margin: UiRect::bottom(Val::Px(2.0)),
            ..default()
        })
        .id();
    commands.entity(container).add_child(pills_row);

    for &level in LogLevel::ALL {
        let active = filter.is_visible(level);
        let bg = if active {
            level.color().with_alpha(0.25)
        } else {
            Color::srgba(0.2, 0.2, 0.2, 0.3)
        };
        let text_col = if active {
            level.color()
        } else {
            theme::TEXT_DISABLED
        };

        let pill = commands
            .spawn((
                LogLevelPill(level),
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(bg),
            ))
            .id();
        commands.entity(pills_row).add_child(pill);

        let label = commands
            .spawn((
                Text::new(level.label()),
                TextFont {
                    font_size: theme::FONT_MICRO,
                    ..default()
                },
                TextColor(text_col),
            ))
            .id();
        commands.entity(pill).add_child(label);
    }

    // Filter: only show events from our faction or allied factions (or global events with no faction),
    // and matching the selected log levels.
    let allied_events: Vec<&GameEvent> = event_log
        .entries
        .iter()
        .filter(|e| {
            if !filter.is_visible(e.level) {
                return false;
            }
            match e.faction {
                None => true,
                Some(ref f) => teams.is_allied(&active_player.0, f),
            }
        })
        .collect();

    if allied_events.is_empty() {
        let empty = commands
            .spawn((
                Text::new("No events yet"),
                TextFont {
                    font_size: theme::FONT_CAPTION,
                    ..default()
                },
                TextColor(theme::TEXT_DISABLED),
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
                font_size: theme::FONT_MICRO,
                ..default()
            },
            TextColor(theme::TEXT_DISABLED),
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
                    border_radius: BorderRadius::all(Val::Px(2.0)),
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
                    font_size: theme::FONT_MICRO,
                    ..default()
                },
                TextColor(event.level.color()),
            ))
            .id();
        commands.entity(row).add_child(level_dot);

        // Category prefix
        let prefix = commands
            .spawn((
                Text::new(event.category.prefix()),
                TextFont {
                    font_size: theme::FONT_TINY,
                    ..default()
                },
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
                TextFont {
                    font_size: theme::FONT_MICRO,
                    ..default()
                },
                TextColor(theme::TEXT_DISABLED),
            ))
            .id();
        commands.entity(row).add_child(time_text);

        // Player name tag (colored by faction)
        if let Some(ref faction) = event.faction {
            let tag = commands
                .spawn((
                    Text::new(faction.display_name()),
                    TextFont {
                        font_size: theme::FONT_MICRO,
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
                    font_size: theme::FONT_TINY,
                    ..default()
                },
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
                let offset = cam_transform.translation - cam_transform.forward() * 100.0;
                cam_transform.translation = Vec3::new(pos.x, cam_transform.translation.y, pos.z)
                    + (cam_transform.translation - offset).normalize_or_zero() * 0.0;
                cam_transform.translation.x = pos.x;
                cam_transform.translation.z = pos.z + 80.0;
            }
        }
    }
}
