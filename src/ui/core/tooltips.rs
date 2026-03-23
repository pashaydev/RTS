use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::components::*;
use crate::theme;

pub fn show_action_tooltips(
    mut commands: Commands,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    ui_scale: Res<UiScale>,
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

                let (left, top) =
                    tooltip_anchor_under_cursor(windows.single().ok(), ui_scale.0, 176.0, 110.0);

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
                            border_radius: BorderRadius::all(Val::Px(5.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            max_width: Val::Px(176.0),
                            min_width: Val::Px(116.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.96)),
                        BorderColor::all(Color::srgba(0.25, 0.25, 0.30, 0.6)),
                        BoxShadow::new(
                            Color::srgba(0.0, 0.0, 0.0, 0.6),
                            Val::Px(0.0),
                            Val::Px(2.0),
                            Val::Px(0.0),
                            Val::Px(8.0),
                        ),
                        GlobalZIndex(100),
                    ))
                    .with_children(|tt| {
                        spawn_tooltip_content(tt, &trigger.text);
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
    mut tooltips: Query<&mut Node, With<ActionTooltip>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if tooltips.is_empty() {
        return;
    }

    let (left, top) = tooltip_anchor_under_cursor(Some(window), ui_scale.0, 176.0, 110.0);
    for mut node in &mut tooltips {
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
    let left = (cx - tooltip_w * 0.5).clamp(6.0, (ui_w - tooltip_w - 6.0).max(6.0));
    let top = (cy + 16.0).clamp(6.0, (ui_h - tooltip_h - 6.0).max(6.0));
    (left, top)
}

fn spawn_tooltip_content(tt: &mut ChildSpawnerCommands, text: &str) {
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
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_PRIMARY),
            ));
            tt.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::axes(Val::Px(0.0), Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.30, 0.30, 0.35, 0.4)),
            ));
            continue;
        }

        let (color, font_size) = if line.starts_with("Not enough") {
            (theme::DESTRUCTIVE, theme::FONT_CAPTION)
        } else if line.starts_with("Requires:") {
            (theme::WARNING, theme::FONT_CAPTION)
        } else if line.starts_with("Cost:") {
            (theme::TEXT_SECONDARY, theme::FONT_CAPTION)
        } else if line.starts_with("HP:") || line.starts_with("DMG:") {
            (theme::STAT_DMG, theme::FONT_CAPTION)
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
                BackgroundColor(Color::srgba(0.30, 0.30, 0.35, 0.4)),
            ));
            (Color::srgba(0.45, 0.65, 1.0, 0.7), theme::FONT_TINY)
        } else if line.starts_with("Build time:") || line.starts_with("Train:") {
            (theme::TEXT_SECONDARY, theme::FONT_CAPTION)
        } else {
            (Color::srgba(0.65, 0.65, 0.65, 0.9), theme::FONT_CAPTION)
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
