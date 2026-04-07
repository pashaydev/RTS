use bevy::prelude::*;

use super::actions_widget::BuildGridButton;
use super::core::constants::*;
use super::core::shared::{format_cost, widget_content_stack, widget_wrap_row};
use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::types::*;
use crate::ui::theme::{self, Theme};

pub(super) fn spawn_building_action_bar(
    commands: &mut Commands,
    parent: Entity,
    kind: EntityKind,
    level: u8,
    upgrade_progress: Option<&UpgradeProgress>,
    training_queue: Option<&TrainingQueue>,
    storage_inventory: Option<&StorageInventory>,
    health: Option<&Health>,
    auto_attack: Option<&TowerAutoAttackEnabled>,
    processor: Option<&ResourceProcessor>,
    production: Option<&ProductionState>,
    worker_info: &[(Entity, AssignedPhase)],
    is_paused: bool,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    unit_cap: UnitCapStats,
    rally_mode: &RallyPointMode,
    layout_bucket: u8,
    theme: &Theme,
) {
    let is_upgrading = upgrade_progress.is_some();
    let bp = registry.get(kind);

    let container = commands
        .spawn((widget_content_stack(), Interaction::None))
        .id();
    commands.entity(parent).add_child(container);

    // Name row
    let name_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::bottom(Val::Px(6.0)),
            ..default()
        })
        .id();
    commands.entity(container).add_child(name_row);

    let name_child = commands
        .spawn((
            Text::new(kind.display_name()),
            TextFont {
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ))
        .id();
    commands.entity(name_row).add_child(name_child);

    let level_pill = commands
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                // border_radius: RADIUS_XL,
                ..default()
            },
            BackgroundColor(theme.colors.bg_elevated),
        ))
        .with_children(|pill| {
            pill.spawn((
                Text::new(format!("Lv {}", level)),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ));
        })
        .id();
    commands.entity(name_row).add_child(level_pill);

    // HP row
    if let Some(hp) = health {
        let hp_fraction = hp.current / hp.max;
        let hp_color = if hp_fraction > 0.6 {
            theme.colors.hp_high()
        } else if hp_fraction > 0.3 {
            theme.colors.hp_mid()
        } else {
            theme.colors.hp_low()
        };

        let hp_row = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::bottom(Val::Px(6.0)),
                ..default()
            })
            .id();
        commands.entity(container).add_child(hp_row);

        let hp_bar_bg = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    max_width: Val::Px(240.0),
                    height: Val::Px(4.0),
                    // border_radius: RADIUS_XS,
                    ..default()
                },
                BackgroundColor(theme.colors.hp_bar_bg),
            ))
            .with_children(|bg| {
                bg.spawn((
                    BuildingHpBarFill,
                    Node {
                        width: Val::Percent(hp_fraction * 100.0),
                        height: Val::Percent(100.0),
                        // border_radius: RADIUS_XS,
                        ..default()
                    },
                    BackgroundColor(hp_color),
                ));
            })
            .id();
        commands.entity(hp_row).add_child(hp_bar_bg);

        let hp_text = commands
            .spawn((
                Text::new(format!("{}/{}", hp.current as u32, hp.max as u32)),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();
        commands.entity(hp_row).add_child(hp_text);
    }

    // Separator
    spawn_separator(commands, container, theme);

    // Storage inventory display
    if let Some(inv) = storage_inventory {
        let total = inv.total();
        let total_cap = inv.total_capacity();
        let capacity_color = if total >= total_cap {
            theme.colors.destructive
        } else if total as f32 >= total_cap as f32 * 0.8 {
            theme.colors.warning
        } else {
            theme.colors.text_secondary
        };

        let storage_row = commands
            .spawn(Node {
                padding: UiRect::axes(Val::Px(0.0), Val::Px(2.0)),
                ..widget_wrap_row(10.0, 4.0)
            })
            .id();
        commands.entity(container).add_child(storage_row);

        let cap_text = commands
            .spawn((
                Text::new(format!("Storage: {}/{}", total, total_cap)),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(capacity_color),
            ))
            .id();
        commands.entity(storage_row).add_child(cap_text);

        // Show per-resource amounts with their individual caps
        for rt in ResourceType::ALL {
            let amount = inv.amounts[rt.index()];
            let cap = inv.cap_for(rt);
            if cap == 0 {
                continue;
            } // skip resource types this building doesn't accept
            let color = rt.carry_color();
            let entry = commands
                .spawn((
                    Text::new(format!("{}: {}/{}", rt.display_name(), amount, cap)),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(color),
                ))
                .id();
            commands.entity(storage_row).add_child(entry);
        }

        spawn_separator(commands, container, theme);
    }

    // Processor info section
    if let Some(proc) = processor {
        let worker_count = worker_info.len();
        let proc_row = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            })
            .id();
        commands.entity(container).add_child(proc_row);

        let rt_names: Vec<&str> = proc
            .resource_types
            .iter()
            .map(|rt| rt.display_name())
            .collect();
        let effective_rate =
            proc.harvest_rate + (worker_count as f32 * proc.harvest_rate * proc.worker_rate_bonus);
        let status_suffix = if is_paused { " [PAUSED]" } else { "" };
        let harvest_label = commands
            .spawn((
                Text::new(format!(
                    "Harvesting: {} ({:.1}/s){}",
                    rt_names.join(", "),
                    if is_paused { 0.0 } else { effective_rate },
                    status_suffix
                )),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(if is_paused {
                    theme.colors.warning
                } else {
                    theme.colors.text_secondary
                }),
            ))
            .id();
        commands.entity(proc_row).add_child(harvest_label);

        if proc.max_workers > 0 {
            // Worker slots row: interactive clickable slots
            let slot_row = commands
                .spawn(Node {
                    ..widget_wrap_row(4.0, 2.0)
                })
                .id();
            commands.entity(proc_row).add_child(slot_row);

            for i in 0..proc.max_workers as usize {
                if i < worker_count {
                    // Filled slot — clickable, shows phase letter
                    let (worker_entity, phase) = &worker_info[i];
                    let phase_letter = match phase {
                        AssignedPhase::SeekingNode => "S",
                        AssignedPhase::MovingToNode(_) => "M",
                        AssignedPhase::Harvesting { .. } => "H",
                        AssignedPhase::ReturningToBuilding => "R",
                        AssignedPhase::Depositing { .. } => "D",
                    };
                    let slot = commands
                        .spawn((
                            Button,
                            UnassignSpecificWorkerButton(*worker_entity),
                            Node {
                                width: Val::Px(20.0),
                                height: Val::Px(20.0),
                                // border_radius: RADIUS_MD,
                                border: BORDER_1,
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BorderColor::all(theme.colors.accent),
                            BackgroundColor(theme.colors.accent.with_alpha(0.7)),
                            Interaction::None,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new(phase_letter),
                                TextFont {
                                    font_size: 10.0,
                                    ..default()
                                },
                                TextColor(theme::TEXT_PRIMARY),
                            ));
                        })
                        .id();
                    commands.entity(slot_row).add_child(slot);
                } else {
                    // Empty slot — non-interactive placeholder
                    let slot = commands
                        .spawn((
                            Node {
                                width: Val::Px(20.0),
                                height: Val::Px(20.0),
                                // border_radius: RADIUS_MD,
                                border: BORDER_1,
                                ..default()
                            },
                            BorderColor::all(theme.colors.accent.with_alpha(0.3)),
                            BackgroundColor(theme::BG_RECESSED.with_alpha(0.2)),
                        ))
                        .id();
                    commands.entity(slot_row).add_child(slot);
                }
            }

            // Button row: [ - ] Workers: X/Y [ + ]   [Pause/Resume]  [Unassign All]
            let btn_row = commands
                .spawn(Node {
                    ..widget_wrap_row(4.0, 4.0)
                })
                .id();
            commands.entity(proc_row).add_child(btn_row);

            let rest_bg = [0.14, 0.14, 0.14, 0.94];

            // "-" button (unassign one)
            if worker_count > 0 {
                let minus_btn = commands
                    .spawn((
                        Button,
                        UnassignOneWorkerButton,
                        ButtonAnimState::new(rest_bg),
                        ButtonStyle::Destructive,
                        Node {
                            width: Val::Px(28.0),
                            height: Val::Px(24.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: BORDER_1,
                            // border_radius: RADIUS_MD,
                            ..default()
                        },
                        BorderColor::all(theme.colors.destructive.with_alpha(0.3)),
                        BackgroundColor(theme.colors.bg_elevated),
                        Interaction::None,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("-"),
                            TextFont {
                                font_size: theme.typography.body,
                                ..default()
                            },
                            TextColor(theme.colors.destructive),
                        ));
                    })
                    .id();
                commands.entity(btn_row).add_child(minus_btn);
            }

            // Workers label
            let workers_label = commands
                .spawn((
                    Text::new(format!("Workers: {}/{}", worker_count, proc.max_workers)),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                    Node {
                        margin: UiRect::axes(Val::Px(4.0), Val::ZERO),
                        ..default()
                    },
                ))
                .id();
            commands.entity(btn_row).add_child(workers_label);

            // "+" button (assign one)
            if worker_count < proc.max_workers as usize {
                let plus_btn = commands
                    .spawn((
                        Button,
                        AssignWorkerButton,
                        ButtonAnimState::new(rest_bg),
                        ButtonStyle::Ghost,
                        Node {
                            width: Val::Px(28.0),
                            height: Val::Px(24.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: BORDER_1,
                            // border_radius: RADIUS_MD,
                            ..default()
                        },
                        BorderColor::all(theme.colors.accent.with_alpha(0.3)),
                        BackgroundColor(theme.colors.bg_elevated),
                        Interaction::None,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("+"),
                            TextFont {
                                font_size: theme.typography.body,
                                ..default()
                            },
                            TextColor(theme.colors.accent),
                        ));
                    })
                    .id();
                commands.entity(btn_row).add_child(plus_btn);
            }

            // Pause/Resume toggle button
            let pause_label = if is_paused { "Resume" } else { "Pause" };
            let pause_color = if is_paused {
                theme.colors.accent
            } else {
                theme.colors.warning
            };
            let pause_btn = commands
                .spawn((
                    Button,
                    PauseBuildingButton,
                    ButtonAnimState::new(rest_bg),
                    ButtonStyle::Ghost,
                    Node {
                        padding: PAD_COMPACT,
                        border: BORDER_1,
                        // border_radius: RADIUS_MD,
                        margin: UiRect::left(Val::Px(8.0)),
                        ..default()
                    },
                    BorderColor::all(pause_color.with_alpha(0.3)),
                    BackgroundColor(theme.colors.bg_elevated),
                    Interaction::None,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(pause_label),
                        TextFont {
                            font_size: theme.typography.small,
                            ..default()
                        },
                        TextColor(pause_color),
                    ));
                })
                .id();
            commands.entity(btn_row).add_child(pause_btn);

            // "Unassign All" small button (only when >1 worker)
            if worker_count > 1 {
                let unassign_all_btn = commands
                    .spawn((
                        Button,
                        UnassignWorkerButton,
                        ButtonAnimState::new(rest_bg),
                        ButtonStyle::Destructive,
                        Node {
                            padding: PAD_COMPACT,
                            border: BORDER_1,
                            // border_radius: RADIUS_MD,
                            margin: UiRect::left(Val::Px(4.0)),
                            ..default()
                        },
                        BorderColor::all(theme.colors.destructive.with_alpha(0.3)),
                        BackgroundColor(theme.colors.bg_elevated),
                        Interaction::None,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("Unassign All"),
                            TextFont {
                                font_size: theme.typography.small,
                                ..default()
                            },
                            TextColor(theme.colors.destructive),
                        ));
                    })
                    .id();
                commands.entity(btn_row).add_child(unassign_all_btn);
            }
        } else {
            let auto_badge = commands
                .spawn((
                    Text::new("Automated (no workers needed)"),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.accent),
                ))
                .id();
            commands.entity(proc_row).add_child(auto_badge);

            // Pause/Resume for automated buildings too
            let rest_bg = [0.14, 0.14, 0.14, 0.94];
            let pause_label = if is_paused { "Resume" } else { "Pause" };
            let pause_color = if is_paused {
                theme.colors.accent
            } else {
                theme.colors.warning
            };
            let pause_btn = commands
                .spawn((
                    Button,
                    PauseBuildingButton,
                    ButtonAnimState::new(rest_bg),
                    ButtonStyle::Ghost,
                    Node {
                        padding: PAD_COMPACT,
                        border: BORDER_1,
                        // border_radius: RADIUS_MD,
                        margin: UiRect::top(Val::Px(4.0)),
                        ..default()
                    },
                    BorderColor::all(pause_color.with_alpha(0.3)),
                    BackgroundColor(theme.colors.bg_elevated),
                    Interaction::None,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(pause_label),
                        TextFont {
                            font_size: theme.typography.small,
                            ..default()
                        },
                        TextColor(pause_color),
                    ));
                })
                .id();
            commands.entity(proc_row).add_child(pause_btn);
        }

        spawn_separator(commands, container, theme);
    }

    // Production state section
    if let Some(prod) = production {
        let prod_col = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            })
            .id();
        commands.entity(container).add_child(prod_col);

        // Section label
        let section_label = commands
            .spawn((
                Text::new("Production"),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();
        commands.entity(prod_col).add_child(section_label);

        for (idx, recipe) in prod.recipes.iter().enumerate() {
            let is_active = prod.active_recipe == Some(idx);
            let is_locked = recipe.requires_level > level;

            if is_locked {
                let locked_row = commands
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        padding: PAD_SM,
                        ..default()
                    })
                    .id();
                commands.entity(prod_col).add_child(locked_row);
                let locked_text = commands
                    .spawn((
                        Text::new(format!(
                            "\u{1f512} {}  (Requires L{})",
                            recipe.name, recipe.requires_level
                        )),
                        TextFont {
                            font_size: theme.typography.body,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary.with_alpha(0.5)),
                    ))
                    .id();
                commands.entity(locked_row).add_child(locked_text);
            } else {
                // Clickable recipe row — click to select, click active to deselect
                let rest_bg = [0.14, 0.14, 0.14, 0.94];
                let recipe_row = commands
                    .spawn((
                        Button,
                        SelectRecipeButton(idx),
                        ButtonAnimState::new(rest_bg),
                        ButtonStyle::Ghost,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            padding: PAD_SM,
                            border: UiRect::left(Val::Px(if is_active { 3.0 } else { 0.0 })),
                            // border_radius: RADIUS_XS,
                            ..default()
                        },
                        BorderColor::all(if is_active {
                            theme.colors.accent
                        } else {
                            Color::NONE
                        }),
                        BackgroundColor(if is_active {
                            theme.colors.accent.with_alpha(0.1)
                        } else {
                            Color::NONE
                        }),
                        Interaction::None,
                    ))
                    .id();
                commands.entity(prod_col).add_child(recipe_row);

                // Recipe name + cycle time
                let status = if is_active {
                    "Active"
                } else {
                    "Click to start"
                };
                let header_text =
                    format!("{}  ({:.0}s) [{}]", recipe.name, recipe.cycle_secs, status);
                let header = commands
                    .spawn((
                        Text::new(header_text),
                        TextFont {
                            font_size: theme.typography.body,
                            ..default()
                        },
                        TextColor(if is_active {
                            theme.colors.accent
                        } else {
                            theme.colors.text_primary
                        }),
                    ))
                    .id();
                commands.entity(recipe_row).add_child(header);

                // Inputs
                if !recipe.inputs.is_empty() {
                    let inputs_str: Vec<String> = recipe
                        .inputs
                        .iter()
                        .map(|(rt, qty)| format!("{} {}", qty, rt.display_name()))
                        .collect();
                    let inputs_label = commands
                        .spawn((
                            Text::new(format!("  In: {}", inputs_str.join(", "))),
                            TextFont {
                                font_size: theme.typography.body,
                                ..default()
                            },
                            TextColor(theme.colors.text_secondary),
                        ))
                        .id();
                    commands.entity(recipe_row).add_child(inputs_label);
                }

                // Outputs
                if !recipe.outputs.is_empty() {
                    let outputs_str: Vec<String> = recipe
                        .outputs
                        .iter()
                        .map(|(rt, qty)| format!("{} {}", qty, rt.display_name()))
                        .collect();
                    let outputs_label = commands
                        .spawn((
                            Text::new(format!("  Out: {}", outputs_str.join(", "))),
                            TextFont {
                                font_size: theme.typography.body,
                                ..default()
                            },
                            TextColor(theme.colors.text_secondary),
                        ))
                        .id();
                    commands.entity(recipe_row).add_child(outputs_label);
                }

                // Visual progress bar for active recipe
                if is_active {
                    let elapsed = prod.progress_timer.elapsed_secs();
                    let duration = prod.progress_timer.duration().as_secs_f32();
                    let pct = if duration > 0.0 {
                        (elapsed / duration).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    // Progress bar container
                    let bar_row = commands
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            margin: UiRect::top(Val::Px(2.0)),
                            ..default()
                        })
                        .id();
                    commands.entity(recipe_row).add_child(bar_row);

                    // Outer bar background
                    let bar_bg = commands
                        .spawn((
                            Node {
                                width: Val::Percent(80.0),
                                height: Val::Px(6.0),
                                // border_radius: RADIUS_SM,
                                overflow: Overflow::clip(),
                                ..default()
                            },
                            BackgroundColor(theme::BG_RECESSED.with_alpha(0.8)),
                        ))
                        .id();
                    commands.entity(bar_row).add_child(bar_bg);

                    // Inner bar fill
                    let bar_fill = commands
                        .spawn((
                            Node {
                                width: Val::Percent(pct * 100.0),
                                height: Val::Percent(100.0),
                                // border_radius: RADIUS_SM,
                                ..default()
                            },
                            BackgroundColor(theme.colors.accent),
                        ))
                        .id();
                    commands.entity(bar_bg).add_child(bar_fill);

                    // Percentage text
                    let pct_label = commands
                        .spawn((
                            Text::new(format!("{:.0}%", pct * 100.0)),
                            TextFont {
                                font_size: theme.typography.small,
                                ..default()
                            },
                            TextColor(theme.colors.accent),
                        ))
                        .id();
                    commands.entity(bar_row).add_child(pct_label);
                }
            }
        }

        spawn_separator(commands, container, theme);
    }

    // Train buttons row
    if let Some(ref bd) = bp.building {
        let mut all_trainable: Vec<EntityKind> = bd.trains.clone();
        for (idx, upgrade_data) in bd.level_upgrades.iter().enumerate() {
            let required_level = (idx + 2) as u8;
            if level >= required_level {
                if let crate::blueprints::LevelBonus::UnlocksTraining(ref kinds) =
                    upgrade_data.bonus
                {
                    for k in kinds {
                        if !all_trainable.contains(k) {
                            all_trainable.push(*k);
                        }
                    }
                }
            }
        }

        if !all_trainable.is_empty() {
            let train_row = commands
                .spawn(Node {
                    ..widget_wrap_row(4.0, 4.0)
                })
                .id();
            commands.entity(container).add_child(train_row);

            for unit_kind in &all_trainable {
                spawn_train_button(
                    commands,
                    train_row,
                    *unit_kind,
                    icons,
                    registry,
                    player_res,
                    unit_cap,
                    layout_bucket,
                    theme,
                );
            }

            spawn_separator(commands, container, theme);
        }
    }

    // Upgrade + Rally ghost buttons row
    let actions_row = commands
        .spawn(Node {
            ..widget_wrap_row(6.0, 4.0)
        })
        .id();
    commands.entity(container).add_child(actions_row);

    // Upgrade button
    if let Some(ref bd) = bp.building {
        if level < 3 && !bd.level_upgrades.is_empty() {
            let upgrade_index = (level - 1) as usize;
            if upgrade_index < bd.level_upgrades.len() {
                let upgrade_data = &bd.level_upgrades[upgrade_index];
                let can_afford = upgrade_data.cost.can_afford(player_res);

                if is_upgrading {
                    let fraction = upgrade_progress.map_or(0.0, |up| up.timer.fraction());
                    let remaining = upgrade_progress.map_or(0.0, |up| up.timer.remaining_secs());
                    let target_lvl = upgrade_progress.map_or(level + 1, |up| up.target_level);

                    let upgrade_container = commands
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            max_width: Val::Px(280.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(2.0),
                            padding: PAD_BUTTON,
                            // border_radius: RADIUS_MD,
                            ..default()
                        })
                        .insert(BackgroundColor(theme.colors.bg_surface))
                        .with_children(|c| {
                            c.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(6.0),
                                align_items: AlignItems::Center,
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((
                                    Text::new(format!("Upgrading L{}", target_lvl)),
                                    TextFont {
                                        font_size: theme.typography.body,
                                        ..default()
                                    },
                                    TextColor(theme.colors.accent),
                                ));
                                row.spawn((
                                    Text::new(format!("{:.0}s", remaining)),
                                    TextFont {
                                        font_size: theme.typography.body,
                                        ..default()
                                    },
                                    TextColor(theme.colors.warning),
                                ));
                            });
                            c.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(4.0),
                                ..default()
                            })
                            .with_children(|bar_row| {
                                bar_row
                                    .spawn(Node {
                                        width: Val::Percent(100.0),
                                        max_width: Val::Px(160.0),
                                        height: Val::Px(6.0),
                                        // border_radius: RADIUS_SM,
                                        ..default()
                                    })
                                    .insert(BackgroundColor(theme.colors.hp_bar_bg))
                                    .with_children(|bg| {
                                        bg.spawn((
                                            UpgradeProgressBar,
                                            Node {
                                                width: Val::Percent(fraction * 100.0),
                                                height: Val::Percent(100.0),
                                                // border_radius: RADIUS_SM,
                                                ..default()
                                            },
                                            BackgroundColor(theme.colors.accent),
                                            // BoxShadow::new(
                                            //     Color::srgba(0.29, 0.62, 1.0, 0.4),
                                            //     Val::Px(0.0),
                                            //     Val::Px(0.0),
                                            //     Val::Px(0.0),
                                            //     Val::Px(3.0),
                                            // ),
                                        ));
                                    });
                                bar_row.spawn((
                                    Text::new(format!("{}%", (fraction * 100.0) as u32)),
                                    TextFont {
                                        font_size: theme.typography.small,
                                        ..default()
                                    },
                                    TextColor(theme.colors.text_secondary),
                                ));
                            });
                        })
                        .id();
                    commands.entity(actions_row).add_child(upgrade_container);
                } else {
                    let cost_str = format_cost(&upgrade_data.cost);
                    let text_color = if can_afford {
                        theme.colors.text_primary
                    } else {
                        theme.colors.destructive
                    };

                    let upgrade_opacity = if can_afford { 1.0 } else { 0.5 };
                    let btn = commands
                        .spawn((
                            Button,
                            UpgradeButton,
                            ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                            ButtonStyle::Ghost,
                            Node {
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                min_width: Val::Px(120.0),
                                padding: PAD_BUTTON,
                                border: BORDER_1,
                                // border_radius: RADIUS_MD,
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            BorderColor::all(theme.colors.border_subtle),
                            Transform::from_scale(Vec3::splat(upgrade_opacity)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new(format!("Upgrade L{}", level + 1)),
                                TextFont {
                                    font_size: theme.typography.body,
                                    ..default()
                                },
                                TextColor(text_color),
                            ));
                            btn.spawn((
                                Text::new(cost_str),
                                TextFont {
                                    font_size: theme.typography.caption,
                                    ..default()
                                },
                                TextColor(theme.colors.text_secondary),
                            ));
                        })
                        .id();
                    commands.entity(actions_row).add_child(btn);
                }
            }
        } else if level >= 3 {
            let max_label = commands
                .spawn((
                    Node {
                        min_width: Val::Px(72.0),
                        padding: PAD_BUTTON,
                        border: BORDER_1,
                        // border_radius: RADIUS_MD,
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(theme::SEPARATOR),
                ))
                .with_children(|pill| {
                    pill.spawn((
                        Text::new("MAX"),
                        TextFont {
                            font_size: theme.typography.body,
                            ..default()
                        },
                        TextColor(theme.colors.text_disabled),
                    ));
                })
                .id();
            commands.entity(actions_row).add_child(max_label);
        }
    }

    // Rally point button
    if let Some(ref bd) = bp.building {
        if !bd.trains.is_empty() {
            let is_rally_active = rally_mode.0;
            let rally_border = if is_rally_active {
                theme.colors.accent
            } else {
                theme.colors.border_subtle
            };
            let rally_text = if is_rally_active {
                "Click Ground..."
            } else {
                "Set Rally"
            };
            let rally_text_color = if is_rally_active {
                theme.colors.accent
            } else {
                theme.colors.text_secondary
            };
            let rally_bg = if is_rally_active {
                theme::HIGHLIGHT_SUBTLE
            } else {
                Color::NONE
            };
            let rally_btn = commands
                .spawn((
                    Button,
                    RallyPointButton,
                    ButtonAnimState::new(if is_rally_active {
                        theme::HIGHLIGHT_SUBTLE.to_srgba().to_f32_array()
                    } else {
                        [0.0, 0.0, 0.0, 0.0]
                    }),
                    ButtonStyle::Ghost,
                    ActionTooltipTrigger {
                        text: "Set rally point\nNew units will move here after training"
                            .to_string(),
                    },
                    Node {
                        min_width: Val::Px(108.0),
                        padding: PAD_BUTTON,
                        border: BORDER_1,
                        // border_radius: RADIUS_MD,
                        ..default()
                    },
                    BackgroundColor(rally_bg),
                    BorderColor::all(rally_border),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(rally_text),
                        TextFont {
                            font_size: theme.typography.body,
                            ..default()
                        },
                        TextColor(rally_text_color),
                    ));
                })
                .id();
            commands.entity(actions_row).add_child(rally_btn);
        }
    }

    // Tower auto-attack toggle
    if kind.uses_tower_auto_attack() {
        let is_enabled = auto_attack.map_or(true, |a| a.0);
        let toggle_bg = if is_enabled {
            theme::SUCCESS.with_alpha(0.15)
        } else {
            theme::DESTRUCTIVE.with_alpha(0.15)
        };
        let toggle_text = if is_enabled {
            "Auto-Attack: ON"
        } else {
            "Auto-Attack: OFF"
        };
        let toggle_color = if is_enabled {
            theme.colors.success
        } else {
            theme.colors.destructive
        };
        let toggle_btn = commands
            .spawn((
                Button,
                ToggleAutoAttackButton,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
                    // border_radius: RADIUS_2XL,
                    margin: UiRect::top(Val::Px(2.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
                BackgroundColor(toggle_bg),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(toggle_text),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(toggle_color),
                ));
            })
            .id();
        commands.entity(container).add_child(toggle_btn);
    }

    // Training queue section
    if let Some(queue) = training_queue {
        if !queue.queue.is_empty() || queue.timer.is_some() {
            spawn_separator(commands, container, theme);
            spawn_training_queue_ui(
                commands,
                container,
                queue,
                icons,
                registry,
                layout_bucket,
                theme,
            );
        }
    }

    // Demolish section
    spawn_separator(commands, container, theme);

    let refund_pct = 50;
    let demolish_tooltip = format!("Demolish building\nRefunds {}% of cost", refund_pct);
    let demolish_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            ..default()
        })
        .id();
    commands.entity(container).add_child(demolish_row);

    let demolish_btn = commands
        .spawn((
            Button,
            DemolishButton,
            ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
            ButtonStyle::Destructive,
            ActionTooltipTrigger {
                text: demolish_tooltip,
            },
            Node {
                padding: PAD_COMPACT,
                // border_radius: RADIUS_SM,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Demolish"),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme.colors.destructive),
            ));
        })
        .id();
    commands.entity(demolish_row).add_child(demolish_btn);
}

pub(super) fn spawn_training_queue_ui(
    commands: &mut Commands,
    parent: Entity,
    queue: &TrainingQueue,
    icons: &IconAssets,
    _registry: &BlueprintRegistry,
    layout_bucket: u8,
    theme: &Theme,
) {
    let header = commands
        .spawn((
            Text::new(format!("Queue ({})", queue.queue.len())),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
            Node {
                margin: UiRect::bottom(Val::Px(2.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(header);

    let queue_row = commands
        .spawn((
            TrainingQueueDisplay,
            Node {
                padding: UiRect::all(Val::Px(2.0)),
                ..widget_wrap_row(3.0, 3.0)
            },
            BackgroundColor(theme.colors.bg_transparent),
        ))
        .id();
    commands.entity(parent).add_child(queue_row);

    for (i, unit_kind) in queue.queue.iter().enumerate() {
        let is_first = i == 0;
        let (first_size, other_size) = match layout_bucket {
            0 => (30.0, 22.0),
            1 => (34.0, 24.0),
            _ => (38.0, 28.0),
        };
        let icon_size = if is_first { first_size } else { other_size };

        let item = commands
            .spawn((
                Button,
                CancelTrainButton(i),
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    min_width: Val::Px(icon_size + 10.0),
                    padding: UiRect::all(Val::Px(3.0)),
                    // border_radius: RADIUS_MD,
                    ..default()
                },
                BackgroundColor(theme.colors.bg_surface),
            ))
            .with_children(|item| {
                item.spawn(Node {
                    width: Val::Px(icon_size),
                    height: Val::Px(icon_size),
                    ..default()
                })
                .with_children(|icon_container| {
                    icon_container.spawn((
                        ImageNode::new(icons.entity_icon(*unit_kind)),
                        Node {
                            width: Val::Px(icon_size),
                            height: Val::Px(icon_size),
                            ..default()
                        },
                    ));
                });

                if is_first {
                    item.spawn(Node {
                        width: Val::Px(icon_size),
                        height: Val::Px(6.0),
                        margin: UiRect::top(Val::Px(2.0)),
                        // border_radius: RADIUS_SM,
                        ..default()
                    })
                    .insert(BackgroundColor(theme.colors.hp_bar_bg))
                    .with_children(|bg| {
                        let fraction = queue.timer.as_ref().map_or(0.0, |t| t.fraction());
                        bg.spawn((
                            TrainingProgressBar,
                            Node {
                                width: Val::Percent(fraction * 100.0),
                                height: Val::Percent(100.0),
                                // border_radius: RADIUS_SM,
                                ..default()
                            },
                            BackgroundColor(theme.colors.accent),
                            BoxShadow::new(
                                theme::HIGHLIGHT,
                                Val::Px(0.0),
                                Val::Px(0.0),
                                Val::Px(0.0),
                                Val::Px(3.0),
                            ),
                        ));
                    });
                }

                item.spawn((
                    Text::new("X"),
                    TextFont {
                        font_size: if is_first {
                            theme.typography.small
                        } else {
                            theme.typography.tiny
                        },
                        ..default()
                    },
                    TextColor(theme::DESTRUCTIVE.with_alpha(0.4)),
                    Node {
                        margin: UiRect::top(Val::Px(1.0)),
                        ..default()
                    },
                ));
            })
            .id();
        commands.entity(queue_row).add_child(item);
    }
}

pub(super) fn spawn_construction_action_bar(
    commands: &mut Commands,
    parent: Entity,
    kind: EntityKind,
    construction: Option<&ConstructionProgress>,
    _registry: &BlueprintRegistry,
    _layout_bucket: u8,
    theme: &Theme,
) {
    let mut root = widget_content_stack();
    root.align_items = AlignItems::Center;
    root.row_gap = Val::Px(6.0);

    let container = commands.spawn((root, Interaction::None)).id();
    commands.entity(parent).add_child(container);

    let name = commands
        .spawn((
            Text::new(format!("Building {}", kind.display_name())),
            TextFont {
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(theme.colors.warning),
        ))
        .id();
    commands.entity(container).add_child(name);

    if let Some(cp) = construction {
        let fraction = cp.timer.fraction();
        let pct_text = format!("{}%", (fraction * 100.0) as u32);

        let bar_bg = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    max_width: Val::Px(280.0),
                    height: Val::Px(8.0),
                    // border_radius: RADIUS_SM,
                    ..default()
                },
                BackgroundColor(theme.colors.hp_bar_bg),
            ))
            .with_children(|bg| {
                bg.spawn((
                    ConstructionProgressBar,
                    Node {
                        width: Val::Percent(fraction * 100.0),
                        height: Val::Percent(100.0),
                        // border_radius: RADIUS_SM,
                        ..default()
                    },
                    BackgroundColor(theme.colors.warning),
                ));
            })
            .id();
        commands.entity(container).add_child(bar_bg);

        let pct = commands
            .spawn((
                Text::new(pct_text),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();
        commands.entity(container).add_child(pct);

        let worker_text = commands
            .spawn((
                ConstructionWorkerCountText,
                Text::new("Waiting for workers..."),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(container).add_child(worker_text);
    }

    let cancel_btn = commands
        .spawn((
            Button,
            DemolishButton,
            ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
            ButtonStyle::Destructive,
            Node {
                padding: PAD_BUTTON,
                // border_radius: RADIUS_MD,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Cancel"),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme.colors.destructive),
            ));
        })
        .id();
    commands.entity(container).add_child(cancel_btn);
}

pub(super) fn spawn_found_base_panel(
    commands: &mut Commands,
    parent: Entity,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    layout_bucket: u8,
    theme: &Theme,
) {
    let kind = EntityKind::Base;
    let bp = registry.get(kind);
    let can_afford = bp.cost.can_afford(player_res);
    let cost_str = format_cost(&bp.cost);

    let container = commands
        .spawn((
            Node {
                max_width: Val::Px(match layout_bucket {
                    0 => 240.0,
                    1 => 320.0,
                    _ => 380.0,
                }),
                ..widget_content_stack()
            },
            Interaction::None,
        ))
        .id();
    commands.entity(parent).add_child(container);

    commands.entity(container).with_children(|panel| {
        panel.spawn((
            Text::new("Settlement"),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
        ));
        panel.spawn((
            Text::new("Found a Base to unlock construction and unit production."),
            TextFont {
                font_size: theme.typography.body,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ));
    });

    let mut tooltip_lines = vec![
        "Found Base".to_string(),
        "Establish your headquarters.".to_string(),
        format!("Cost: {}", cost_str),
    ];
    if let Some(ref bd) = bp.building {
        tooltip_lines.push(format!("Build time: {:.0}s", bd.construction_time_secs));
    }
    if !can_afford {
        tooltip_lines.push("Not enough resources!".to_string());
    }
    tooltip_lines.push("Click to place".to_string());

    let btn = commands
        .spawn((
            BuildGridButton(kind),
            BuildButton(kind),
            Button,
            ButtonAnimState::new(if can_afford {
                [0.12, 0.12, 0.12, 0.94]
            } else {
                [0.06, 0.06, 0.06, 0.94]
            }),
            ButtonStyle::Filled,
            ActionTooltipTrigger {
                text: tooltip_lines.join("\n"),
            },
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(10.0),
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: BORDER_1,
                // border_radius: RADIUS_XL,
                ..default()
            },
            BackgroundColor(if can_afford {
                theme.colors.bg_surface
            } else {
                theme::BG_PANEL.with_alpha(0.7)
            }),
            BorderColor::all(if can_afford {
                theme::TOOLTIP_BORDER
            } else {
                theme::DESTRUCTIVE.with_alpha(0.25)
            }),
        ))
        .with_children(|btn| {
            btn.spawn((
                ImageNode {
                    image: icons.entity_icon(kind),
                    color: if can_afford {
                        Color::WHITE
                    } else {
                        Color::srgba(1.0, 1.0, 1.0, 0.35)
                    },
                    ..default()
                },
                Node {
                    width: Val::Px(48.0),
                    height: Val::Px(48.0),
                    ..default()
                },
            ));

            btn.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|text_col| {
                text_col.spawn((
                    Text::new("Found Base"),
                    TextFont {
                        font_size: theme.typography.medium,
                        ..default()
                    },
                    TextColor(if can_afford {
                        theme.colors.text_primary
                    } else {
                        theme.colors.text_disabled
                    }),
                ));
                text_col.spawn((
                    Text::new(cost_str),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(if can_afford {
                        theme.colors.text_secondary
                    } else {
                        theme.colors.destructive
                    }),
                ));
            });
        })
        .id();
    commands.entity(container).add_child(btn);
}

pub(super) fn spawn_building_grid(
    commands: &mut Commands,
    parent: Entity,
    completed: &[EntityKind],
    founded: bool,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    layout_bucket: u8,
    theme: &Theme,
    current_age: crate::simulation::ages::Age,
) {
    let building_kinds = registry.building_kinds();
    let available: Vec<EntityKind> = building_kinds
        .iter()
        .copied()
        .filter(|kind| {
            if founded && *kind == EntityKind::Base {
                return false;
            }
            // Filter by age requirement
            let required_age = crate::simulation::ages::required_age_for_building(*kind);
            if current_age < required_age {
                return false;
            }
            let bp = registry.get(*kind);
            let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
            match prereq {
                None => true,
                Some(prereq_kind) => {
                    if prereq_kind == EntityKind::Base {
                        founded || completed.contains(&prereq_kind)
                    } else {
                        completed.contains(&prereq_kind)
                    }
                }
            }
        })
        .collect();

    // Categorize buildings
    let economy: Vec<EntityKind> = available
        .iter()
        .copied()
        .filter(|k| {
            matches!(
                k,
                EntityKind::Base
                    | EntityKind::Floor
                    | EntityKind::House
                    | EntityKind::Sawmill
                    | EntityKind::Mine
                    | EntityKind::OilRig
                    | EntityKind::Storage
            )
        })
        .collect();
    let production: Vec<EntityKind> = available
        .iter()
        .copied()
        .filter(|k| matches!(k, EntityKind::Smelter | EntityKind::Alchemist))
        .collect();
    let military: Vec<EntityKind> = available
        .iter()
        .copied()
        .filter(|k| {
            matches!(
                k,
                EntityKind::Barracks
                    | EntityKind::Stable
                    | EntityKind::SiegeWorks
                    | EntityKind::Workshop
                    | EntityKind::MageTower
                    | EntityKind::Temple
            )
        })
        .collect();
    let defense: Vec<EntityKind> = available
        .iter()
        .copied()
        .filter(|k| {
            matches!(
                k,
                EntityKind::WatchTower
                    | EntityKind::GuardTower
                    | EntityKind::BallistaTower
                    | EntityKind::BombardTower
                    | EntityKind::Outpost
                    | EntityKind::WallSegment
                    | EntityKind::Gatehouse
            )
        })
        .collect();

    let container = commands
        .spawn((widget_content_stack(), Interaction::None))
        .id();
    commands.entity(parent).add_child(container);

    let categories = [
        ("Economy", &economy),
        ("Production", &production),
        ("Military", &military),
        ("Defense", &defense),
    ];
    for (cat_name, kinds) in &categories {
        if kinds.is_empty() {
            continue;
        }

        let cat_label = commands
            .spawn((
                Text::new(*cat_name),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();
        commands.entity(container).add_child(cat_label);

        let row = commands
            .spawn(Node {
                ..widget_wrap_row(4.0, 4.0)
            })
            .id();
        commands.entity(container).add_child(row);

        for kind in *kinds {
            let bp = registry.get(*kind);
            let can_afford = bp.cost.can_afford(player_res);
            let cost_str = format_cost(&bp.cost);

            // Build rich tooltip
            let mut tooltip_lines = vec![kind.display_name().to_string()];
            tooltip_lines.push(kind.description().to_string());
            if let Some(ref bd) = bp.building {
                if let Some(prereq) = bd.prerequisite {
                    tooltip_lines.push(format!("Requires: {}", prereq.display_name()));
                }
                tooltip_lines.push(format!("Build time: {:.0}s", bd.construction_time_secs));
            }
            tooltip_lines.push(format!("Cost: {}", cost_str));
            if !can_afford {
                tooltip_lines.push("Not enough resources!".to_string());
            }
            tooltip_lines.push("Click to place".to_string());

            let border_color = if can_afford {
                theme::TOOLTIP_BORDER
            } else {
                theme::DESTRUCTIVE.with_alpha(0.25)
            };
            let name_color = if can_afford {
                theme.colors.text_primary
            } else {
                theme.colors.text_disabled
            };

            let btn = commands
                .spawn((
                    BuildGridButton(*kind),
                    BuildButton(*kind),
                    Button,
                    ButtonAnimState::new(if can_afford {
                        [0.12, 0.12, 0.12, 0.94]
                    } else {
                        [0.06, 0.06, 0.06, 0.94]
                    }),
                    ButtonStyle::Filled,
                    ActionTooltipTrigger {
                        text: tooltip_lines.join("\n"),
                    },
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        min_width: Val::Px(match layout_bucket {
                            0 => 48.0,
                            1 => 56.0,
                            _ => 64.0,
                        }),
                        min_height: Val::Px(match layout_bucket {
                            0 => 58.0,
                            1 => 64.0,
                            _ => 70.0,
                        }),
                        flex_grow: if layout_bucket == 0 { 1.0 } else { 0.0 },
                        padding: PAD_SM,
                        row_gap: Val::Px(2.0),
                        border: BORDER_1,
                        // border_radius: RADIUS_LG,
                        ..default()
                    },
                    BackgroundColor(if can_afford {
                        theme.colors.bg_surface
                    } else {
                        theme::BG_PANEL.with_alpha(0.7)
                    }),
                    BorderColor::all(border_color),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        ImageNode {
                            image: icons.entity_icon(*kind),
                            color: if can_afford {
                                Color::WHITE
                            } else {
                                Color::srgba(1.0, 1.0, 1.0, 0.35)
                            },
                            ..default()
                        },
                        Node {
                            width: Val::Px(match layout_bucket {
                                0 => 32.0,
                                1 => 36.0,
                                _ => 40.0,
                            }),
                            height: Val::Px(match layout_bucket {
                                0 => 32.0,
                                1 => 36.0,
                                _ => 40.0,
                            }),
                            ..default()
                        },
                    ));
                    btn.spawn((
                        Text::new(kind.display_name()),
                        TextFont {
                            font_size: theme.typography.tiny,
                            ..default()
                        },
                        TextColor(name_color),
                    ));
                })
                .id();
            commands.entity(row).add_child(btn);
        }
    }
}

pub(super) fn spawn_train_button(
    commands: &mut Commands,
    parent: Entity,
    kind: EntityKind,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    unit_cap: UnitCapStats,
    layout_bucket: u8,
    theme: &Theme,
) {
    let label = kind.display_name();
    let bp = registry.get(kind);
    let cost_str = format_cost_from_blueprint(bp);
    let can_afford = bp.cost.can_afford(player_res);
    let has_capacity = unit_cap.has_room(1);
    let can_train = can_afford && has_capacity;

    // Build rich tooltip
    let mut tooltip_lines = vec![label.to_string()];
    tooltip_lines.push(kind.description().to_string());
    if let Some(ref combat) = bp.combat {
        tooltip_lines.push(format!(
            "HP: {} | DMG: {} | Range: {:.0}",
            combat.hp as u32, combat.damage as u32, combat.attack_range,
        ));
    }
    tooltip_lines.push(format!(
        "Cost: {} | Train: {:.0}s",
        cost_str, bp.train_time_secs
    ));
    tooltip_lines.push(format!(
        "Units: {} active + {} queued / {}",
        unit_cap.used, unit_cap.queued, unit_cap.cap
    ));
    if !can_afford {
        tooltip_lines.push("Not enough resources!".to_string());
    }
    if !has_capacity {
        tooltip_lines.push("Population cap reached. Build or upgrade Houses.".to_string());
    } else {
        tooltip_lines.push("Click to train".to_string());
    }

    let border_color = if can_train {
        theme::TOOLTIP_BORDER
    } else {
        theme::DESTRUCTIVE.with_alpha(0.25)
    };
    let name_color = if can_train {
        theme.colors.text_primary
    } else {
        theme.colors.text_disabled
    };

    let child = commands
        .spawn((
            TrainButton(kind),
            Button,
            ButtonAnimState::new(if can_train {
                [0.17, 0.17, 0.17, 0.94]
            } else {
                [0.08, 0.08, 0.08, 0.94]
            }),
            ButtonStyle::Filled,
            ActionTooltipTrigger {
                text: tooltip_lines.join("\n"),
            },
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                min_width: Val::Px(match layout_bucket {
                    0 => 82.0,
                    1 => 96.0,
                    _ => 110.0,
                }),
                flex_grow: if layout_bucket == 0 { 1.0 } else { 0.0 },
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                row_gap: Val::Px(3.0),
                border: BORDER_1,
                // border_radius: RADIUS_LG,
                ..default()
            },
            BackgroundColor(if can_train {
                theme.colors.btn_primary
            } else {
                theme::BG_PANEL.with_alpha(0.7)
            }),
            BorderColor::all(border_color),
        ))
        .with_children(|btn| {
            btn.spawn((
                Node {
                    width: Val::Px(match layout_bucket {
                        0 => 36.0,
                        1 => 40.0,
                        _ => 44.0,
                    }),
                    height: Val::Px(match layout_bucket {
                        0 => 36.0,
                        1 => 40.0,
                        _ => 44.0,
                    }),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    // border_radius: RADIUS_MD,
                    ..default()
                },
                BackgroundColor(theme.colors.icon_frame_bg),
            ))
            .with_children(|frame| {
                frame.spawn((
                    ImageNode {
                        image: icons.entity_icon(kind),
                        color: if can_train {
                            Color::WHITE
                        } else {
                            Color::srgba(1.0, 1.0, 1.0, 0.35)
                        },
                        ..default()
                    },
                    Node {
                        width: Val::Percent(82.0),
                        height: Val::Percent(82.0),
                        ..default()
                    },
                ));
            });
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(name_color),
            ));
            btn.spawn((
                TrainCostText { kind },
                Text::new(cost_str),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(if can_train {
                    theme.colors.text_secondary
                } else {
                    theme.colors.destructive
                }),
            ));
        })
        .id();

    commands.entity(parent).add_child(child);
}

pub(super) fn format_cost_from_blueprint(bp: &crate::blueprints::Blueprint) -> String {
    format_cost(&bp.cost)
}

pub(super) fn spawn_separator(commands: &mut Commands, parent: Entity, theme: &Theme) {
    let sep = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme.colors.separator),
        ))
        .id();
    commands.entity(parent).add_child(sep);
}
