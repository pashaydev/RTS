use bevy::prelude::*;

use crate::components::*;
use crate::theme;
use crate::ui::fonts::{self, UiFonts};

pub fn hp_color(current: f32, max: f32) -> Color {
    let pct = (current / max).clamp(0.0, 1.0);
    if pct > 0.6 {
        theme::HP_HIGH
    } else if pct > 0.3 {
        theme::HP_MID
    } else {
        theme::HP_LOW
    }
}

pub fn spawn_hp_bar(
    commands: &mut Commands,
    parent: Entity,
    tracked_entity: Entity,
    health: &Health,
    width: f32,
) {
    let pct = (health.current / health.max).clamp(0.0, 1.0) * 100.0;
    let bar_color = hp_color(health.current, health.max);

    let bg = commands
        .spawn((
            Node {
                width: Val::Px(width),
                height: Val::Px(6.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(theme::HP_BAR_BG),
        ))
        .id();
    commands.entity(parent).add_child(bg);

    let fill = commands
        .spawn((
            HpBarFill(tracked_entity),
            Node {
                width: Val::Percent(pct),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(bar_color),
            BoxShadow::new(
                bar_color.with_alpha(0.4),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(4.0),
            ),
        ))
        .id();
    commands.entity(bg).add_child(fill);
}

/// Spawn a section divider: a label above a horizontal separator line.
pub fn spawn_section_divider(
    commands: &mut Commands,
    parent: Entity,
    label: &str,
    fonts: &UiFonts,
) {
    let section = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            margin: UiRect::new(Val::ZERO, Val::ZERO, Val::Px(16.0), Val::Px(8.0)),
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                Text::new(label),
                fonts::heading(fonts, theme::FONT_MEDIUM),
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
            ));
            p.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ));
        })
        .id();
    commands.entity(parent).add_child(section);
}

pub fn format_cost(cost: &crate::blueprints::ResourceCost) -> String {
    cost.cost_entries()
        .iter()
        .map(|(rt, amt)| format!("{}:{}", rt.abbreviation(), amt))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Base node for widget content to keep internals visually integrated with the outer widget tile.
pub fn widget_content_stack() -> Node {
    Node {
        width: Val::Percent(100.0),
        min_width: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Stretch,
        row_gap: Val::Px(6.0),
        padding: UiRect::all(Val::Px(4.0)),
        ..default()
    }
}

/// Utility row for dense inline controls and chips.
pub fn widget_wrap_row(column_gap: f32, row_gap: f32) -> Node {
    Node {
        width: Val::Percent(100.0),
        min_width: Val::Px(0.0),
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(column_gap),
        row_gap: Val::Px(row_gap),
        align_items: AlignItems::Center,
        ..default()
    }
}
