use bevy::prelude::*;

use super::constants::*;
use crate::components::*;
use crate::theme::Theme;

pub fn hp_color(theme: &Theme, current: f32, max: f32) -> Color {
    let pct = (current / max).clamp(0.0, 1.0);
    if pct > 0.6 {
        theme.colors.hp_high()
    } else if pct > 0.3 {
        theme.colors.hp_mid()
    } else {
        theme.colors.hp_low()
    }
}

pub fn spawn_hp_bar(
    commands: &mut Commands,
    parent: Entity,
    tracked_entity: Entity,
    health: &Health,
    width: f32,
    theme: &Theme,
) {
    let pct = (health.current / health.max).clamp(0.0, 1.0) * 100.0;
    let bar_color = hp_color(theme, health.current, health.max);

    let bg = commands
        .spawn((
            progress_track(Val::Px(width), 6.0),
            BackgroundColor(theme.colors.hp_bar_bg),
        ))
        .id();
    commands.entity(parent).add_child(bg);

    let fill = commands
        .spawn((
            HpBarFill(tracked_entity),
            progress_fill(pct),
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

pub fn format_cost(cost: &crate::blueprints::ResourceCost) -> String {
    cost.cost_entries()
        .iter()
        .map(|(rt, amt)| format!("{}:{}", rt.abbreviation(), amt))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Base node for widget content to keep internals visually integrated with the outer widget tile.
pub fn widget_content_stack() -> Node {
    let mut n = flex_col(GAP_LG);
    n.min_width = Val::Px(0.0);
    n.padding = PAD_SM;
    n
}

/// Utility row for dense inline controls and chips.
pub fn widget_wrap_row(col_gap: f32, row_gap: f32) -> Node {
    let mut n = flex_wrap(Val::Px(col_gap), Val::Px(row_gap));
    n.min_width = Val::Px(0.0);
    n
}
