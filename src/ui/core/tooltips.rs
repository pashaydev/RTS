use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::types::*;
use crate::ui::theme::{self, Theme};

pub fn show_action_tooltips(
    mut commands: Commands,
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

                // Spawn hidden; the position system places it next frame once
                // its real laid-out size is known, then reveals it. This avoids
                // the "appear at estimated spot, then jump" flicker.
                commands
                    .spawn((
                        ActionTooltip { owner: entity },
                        Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(-9999.0),
                            top: Val::Px(-9999.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(6.0)),
                            row_gap: Val::Px(1.0),
                            border: UiRect::all(Val::Px(1.0)),
                            width: Val::Px(176.0),
                            ..default()
                        },
                        Visibility::Hidden,
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
    triggers: Query<(&ComputedNode, &UiGlobalTransform), With<ActionTooltipTrigger>>,
    mut tooltips: Query<(&mut Node, &ComputedNode, &mut Visibility, &ActionTooltip)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if tooltips.is_empty() {
        return;
    }

    let scale_factor = window.scale_factor();
    let ui_scale_v = ui_scale.0.max(0.001);
    // physical-px → Val::Px (logical UI) units
    let to_ui = 1.0 / (scale_factor * ui_scale_v);
    let ui_w = window.width() / ui_scale_v;
    let ui_h = window.height() / ui_scale_v;
    let screen_padding = 6.0;
    let gap = 8.0;

    for (mut node, tt_computed, mut vis, tt) in &mut tooltips {
        let Ok((trig_computed, trig_tf)) = triggers.get(tt.owner) else {
            continue;
        };

        let tt_size_phys = tt_computed.size();
        if tt_size_phys.x < 1.0 || tt_size_phys.y < 1.0 {
            // Layout hasn't measured the tooltip yet — keep it hidden one more frame.
            *vis = Visibility::Hidden;
            continue;
        }
        let tt_w = tt_size_phys.x * to_ui;
        let tt_h = tt_size_phys.y * to_ui;

        let trig_size = trig_computed.size() * to_ui;
        let trig_center_x = trig_tf.translation.x * to_ui;
        let trig_center_y = trig_tf.translation.y * to_ui;
        let trig_top = trig_center_y - trig_size.y * 0.5;
        let trig_bottom = trig_center_y + trig_size.y * 0.5;

        // Anchor centered horizontally on the trigger card, clamped to viewport.
        let left = (trig_center_x - tt_w * 0.5).clamp(
            screen_padding,
            (ui_w - tt_w - screen_padding).max(screen_padding),
        );

        // Prefer above the card (action bar lives at the bottom of the screen),
        // fall back below only if there isn't enough room.
        let above_top = trig_top - gap - tt_h;
        let top = if above_top >= screen_padding {
            above_top
        } else {
            (trig_bottom + gap).min((ui_h - tt_h - screen_padding).max(screen_padding))
        };

        node.left = Val::Px(left);
        node.top = Val::Px(top);
        *vis = Visibility::Inherited;
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
