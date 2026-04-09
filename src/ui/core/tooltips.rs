use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::types::*;
use crate::ui::theme::{self, Theme};

pub fn show_action_tooltips(
    mut commands: Commands,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    theme: Res<Theme>,
    triggers: Query<(Entity, &Interaction, &ActionTooltipTrigger), Changed<Interaction>>,
    existing_tooltips: Query<(Entity, &ActionTooltip)>,
) {
    for (entity, interaction, trigger) in &triggers {
        match interaction {
            Interaction::Hovered => {
                // Check if tooltip already exists for this trigger
                let has_tooltip = existing_tooltips.iter().any(|(_, tt)| tt.owner == entity);
                if has_tooltip {
                    continue;
                }

                // Estimate height based on line count for positioning
                let line_count = trigger.text.split('\n').filter(|l| !l.is_empty()).count();
                let estimated_h = 20.0 + line_count as f32 * 18.0;
                let (left, top) = tooltip_anchor_under_cursor(
                    windows.single().ok(),
                    ui_scale.0,
                    176.0,
                    estimated_h,
                );

                commands
                    .spawn((
                        ActionTooltip { owner: entity },
                        Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(left),
                            top: Val::Px(top),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(6.0)),
                            row_gap: Val::Px(1.0),
                            // border_radius: BorderRadius::all(Val::Px(5.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            width: Val::Px(176.0),
                            ..default()
                        },
                        BackgroundColor(theme::TOOLTIP_BG),
                        BorderColor::all(theme::TOOLTIP_BORDER),
                        BoxShadow::new(
                            theme::OVERLAY,
                            Val::Px(0.0),
                            Val::Px(2.0),
                            Val::Px(0.0),
                            Val::Px(8.0),
                        ),
                        GlobalZIndex(100),
                    ))
                    .with_children(|tt| {
                        spawn_tooltip_content(tt, &trigger.text, &theme);
                    });
            }
            _ => {
                // Remove tooltip owned by this trigger
                for (tooltip_entity, tt) in &existing_tooltips {
                    if tt.owner == entity {
                        commands.entity(tooltip_entity).try_despawn();
                    }
                }
            }
        }
    }
}

pub fn update_action_tooltip_positions(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut tooltips: Query<(&mut Node, &ComputedNode), With<ActionTooltip>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if tooltips.is_empty() {
        return;
    }

    for (mut node, computed) in &mut tooltips {
        let actual_h = computed.size().y / ui_scale.0.max(0.001);
        let h = if actual_h > 1.0 { actual_h } else { 160.0 };
        let (left, top) = tooltip_anchor_under_cursor(Some(window), ui_scale.0, 176.0, h);
        node.left = Val::Px(left);
        node.top = Val::Px(top);
    }
}

/// Clean up orphaned tooltips whose owner trigger no longer exists or is no longer hovered.
pub fn cleanup_action_tooltips(
    mut commands: Commands,
    tooltips: Query<(Entity, &ActionTooltip)>,
    triggers: Query<&Interaction, With<ActionTooltipTrigger>>,
) {
    for (tooltip_entity, tt) in &tooltips {
        let should_remove = match triggers.get(tt.owner) {
            Ok(interaction) => *interaction != Interaction::Hovered,
            Err(_) => true, // owner despawned
        };
        if should_remove {
            commands.entity(tooltip_entity).try_despawn();
        }
    }
}

fn tooltip_anchor_under_cursor(
    window: Option<&Window>,
    ui_scale: f32,
    tooltip_w: f32,
    tooltip_h: f32,
) -> (f32, f32) {
    let scale = ui_scale.max(0.001);
    let Some(window) = window else {
        return (6.0, 6.0);
    };
    let Some(cursor) = window.cursor_position() else {
        return (6.0, 6.0);
    };

    let ui_w = window.width() / scale;
    let ui_h = window.height() / scale;
    let cx = cursor.x / scale;
    let cy = cursor.y / scale;
    let screen_padding = 6.0;
    let cursor_gap = 16.0;

    let left = (cx - tooltip_w * 0.5)
        .clamp(screen_padding, (ui_w - tooltip_w - screen_padding).max(screen_padding));

    // Keep the tooltip off the cursor. If there's no room below, flip it above instead of
    // clamping it into the cursor position, which causes hover/despawn flicker on tall tooltips.
    let below_top = cy + cursor_gap;
    let below_fits = below_top + tooltip_h <= ui_h - screen_padding;
    let top = if below_fits {
        below_top
    } else {
        (cy - cursor_gap - tooltip_h).max(screen_padding)
    };

    (left, top)
}

fn spawn_tooltip_content(tt: &mut ChildSpawnerCommands, text: &str, theme: &Theme) {
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }

        // First line = title
        if i == 0 {
            tt.spawn((
                Text::new(*line),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_primary),
            ));
            tt.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::axes(Val::Px(0.0), Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ));
            continue;
        }

        let (color, font_size) = if line.starts_with("Not enough") {
            (theme.colors.destructive, theme.typography.caption)
        } else if line.starts_with("Requires:") {
            (theme.colors.warning, theme.typography.caption)
        } else if line.starts_with("Cost:") {
            (theme.colors.text_secondary, theme.typography.caption)
        } else if line.starts_with("HP:") || line.starts_with("DMG:") {
            (theme.colors.stat_dmg, theme.typography.caption)
        } else if *line == "Click to place"
            || *line == "Click to train"
            || *line == "Click ground to place"
        {
            tt.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::axes(Val::Px(0.0), Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ));
            (theme::HIGHLIGHT.with_alpha(0.7), theme.typography.tiny)
        } else if line.starts_with("Build time:") || line.starts_with("Train:") {
            (theme.colors.text_secondary, theme.typography.caption)
        } else {
            (
                theme::TEXT_DISABLED.with_alpha(0.9),
                theme.typography.caption,
            )
        };

        tt.spawn((
            Text::new(*line),
            TextFont {
                font_size,
                ..default()
            },
            TextColor(color),
        ));
    }
}
