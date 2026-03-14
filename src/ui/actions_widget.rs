use bevy::prelude::*;

use super::shared::{format_cost, widget_content_stack, widget_wrap_row};
use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::components::*;
use crate::theme;

#[derive(Resource)]
pub struct ActionBarLayoutRevision {
    pub revision: u64,
    pub bucket: u8,
}

impl Default for ActionBarLayoutRevision {
    fn default() -> Self {
        Self {
            revision: 0,
            bucket: u8::MAX,
        }
    }
}

pub fn track_action_bar_layout(
    mut layout: ResMut<ActionBarLayoutRevision>,
    action_bar: Query<&ComputedNode, With<ActionBarInner>>,
) {
    let Ok(node) = action_bar.single() else {
        return;
    };
    let logical_width = node.size().x * node.inverse_scale_factor();
    let bucket = if logical_width < 300.0 {
        0
    } else if logical_width < 420.0 {
        1
    } else {
        2
    };
    if bucket != layout.bucket {
        layout.bucket = bucket;
        layout.revision = layout.revision.saturating_add(1);
    }
}

pub fn update_action_bar(
    mut commands: Commands,
    ui_mode: Res<UiMode>,
    selected_units: Query<
        (
            &EntityKind,
            Option<&Carrying>,
            Option<&CarryCapacity>,
            Option<&UnitState>,
        ),
        (With<Unit>, With<Selected>),
    >,
    selected_buildings: Query<
        (
            Entity,
            &EntityKind,
            &BuildingState,
            &BuildingLevel,
            Option<&UpgradeProgress>,
            Option<&ConstructionProgress>,
            Option<&TrainingQueue>,
            Option<&StorageInventory>,
            Option<&Health>,
            Option<&TowerAutoAttackEnabled>,
            Option<&ResourceProcessor>,
        ),
        (With<Building>, With<Selected>),
    >,
    assigned_workers_q: Query<&AssignedWorkers>,
    player_state: (
        Res<AllCompletedBuildings>,
        Res<FactionBaseState>,
        Res<ActivePlayer>,
        Res<AllPlayerResources>,
    ),
    registry: Res<BlueprintRegistry>,
    action_bar: Query<(Entity, Option<&Children>), With<ActionBarInner>>,
    changed_buildings: Query<
        Entity,
        Or<(
            Changed<BuildingState>,
            Changed<BuildingLevel>,
            Changed<UpgradeProgress>,
            Changed<TowerAutoAttackEnabled>,
            Changed<AssignedWorkers>,
        )>,
    >,
    mut last_queue_len: Local<usize>,
    ui_state: (Res<IconAssets>, Res<RallyPointMode>),
    existing_cards: Query<Entity, With<BuildGridButton>>,
    confirm_panels: Query<Entity, With<DemolishConfirmPanel>>,
    children_q_readonly: Query<&Children>,
    layout_revision: Res<ActionBarLayoutRevision>,
) {
    let (all_completed, base_state, active_player, all_resources) = player_state;
    let (icons, rally_mode) = ui_state;

    if !confirm_panels.is_empty() {
        return;
    }

    if matches!(*ui_mode, UiMode::PlacingBuilding(_)) {
        return;
    }

    let mode_changed = ui_mode.is_changed();
    let has_building_change = !changed_buildings.is_empty();
    let completed_changed = all_completed.is_changed();
    let founded_changed = base_state.is_changed();
    let rally_changed = rally_mode.is_changed();
    let resources_changed = all_resources.is_changed();
    let layout_changed = layout_revision.is_changed();

    let current_queue_len = selected_buildings
        .iter()
        .next()
        .and_then(|(_, _, _, _, _, _, q, _, _, _, _)| q.map(|q| q.queue.len()))
        .unwrap_or(0);
    let queue_changed = current_queue_len != *last_queue_len;
    *last_queue_len = current_queue_len;

    if !mode_changed
        && !has_building_change
        && !completed_changed
        && !founded_changed
        && !queue_changed
        && !rally_changed
        && !resources_changed
        && !layout_changed
    {
        return;
    }

    let Ok((bar_entity, bar_children)) = action_bar.single() else {
        return;
    };

    if !mode_changed
        && *ui_mode == UiMode::Idle
        && !existing_cards.is_empty()
        && !completed_changed
        && !founded_changed
        && !resources_changed
        && !layout_changed
    {
        return;
    }

    // Clear existing children — despawn immediately to avoid duplicates
    if let Some(children) = bar_children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    let layout_bucket = layout_revision.bucket;

    let is_building_grid;
    match &*ui_mode {
        UiMode::SelectedBuilding(_) => {
            is_building_grid = false;
            if let Ok((
                building_entity,
                kind,
                state,
                level,
                upgrade_progress,
                construction,
                training_queue,
                storage_inv,
                health,
                auto_attack,
                proc_opt,
            )) = selected_buildings.single()
            {
                if *state == BuildingState::Complete {
                    let player_res = all_resources.get(&active_player.0);
                    let worker_count = assigned_workers_q
                        .get(building_entity)
                        .map(|aw| aw.workers.len())
                        .unwrap_or(0);
                    spawn_building_action_bar(
                        &mut commands,
                        bar_entity,
                        *kind,
                        level.0,
                        upgrade_progress,
                        training_queue,
                        storage_inv,
                        health,
                        auto_attack,
                        proc_opt,
                        worker_count,
                        &icons,
                        &registry,
                        player_res,
                        &rally_mode,
                        layout_bucket,
                    );
                } else {
                    spawn_construction_action_bar(
                        &mut commands,
                        bar_entity,
                        *kind,
                        construction,
                        &registry,
                        layout_bucket,
                    );
                }
            }
        }
        UiMode::SelectedUnits(_) => {
            let founded = base_state.is_founded(&active_player.0);
            let has_workers = selected_units.iter().any(|(k, ..)| *k == EntityKind::Worker);
            if !founded && has_workers {
                is_building_grid = true;
                let player_res = all_resources.get(&active_player.0);
                spawn_found_base_panel(
                    &mut commands,
                    bar_entity,
                    &icons,
                    &registry,
                    player_res,
                    layout_bucket,
                );
            } else {
                is_building_grid = false;
                spawn_units_action_bar(
                    &mut commands,
                    bar_entity,
                    &selected_units,
                    layout_bucket,
                );
            }
        }
        _ => {
            is_building_grid = true;
            let player_res = all_resources.get(&active_player.0);
            let founded = base_state.is_founded(&active_player.0);
            if founded {
                let completed = all_completed.completed_for(&active_player.0);
                spawn_building_grid(
                    &mut commands,
                    bar_entity,
                    completed,
                    founded,
                    &icons,
                    &registry,
                    player_res,
                    layout_bucket,
                );
            } else {
                spawn_found_base_panel(
                    &mut commands,
                    bar_entity,
                    &icons,
                    &registry,
                    player_res,
                    layout_bucket,
                );
            }
        }
    }

    if !is_building_grid {
        if let Ok(children) = children_q_readonly.get(bar_entity) {
            for child in children.iter() {
                commands.entity(child).try_insert((
                    ActionBarFadeIn {
                        timer: Timer::from_seconds(0.2, TimerMode::Once),
                        delay: Timer::from_seconds(0.1, TimerMode::Once),
                        started: false,
                    },
                    Visibility::Hidden,
                ));
            }
        }
    }
}

fn spawn_units_action_bar(
    commands: &mut Commands,
    parent: Entity,
    selected_units: &Query<
        (
            &EntityKind,
            Option<&Carrying>,
            Option<&CarryCapacity>,
            Option<&UnitState>,
        ),
        (With<Unit>, With<Selected>),
    >,
    layout_bucket: u8,
) {
    let container = commands
        .spawn((widget_content_stack(), Interaction::None))
        .id();
    commands.entity(parent).add_child(container);

    let unit_count = selected_units.iter().count();
    let worker_count = selected_units
        .iter()
        .filter(|(k, ..)| **k == EntityKind::Worker)
        .count();

    let label_text = if worker_count == unit_count && worker_count > 0 {
        format!(
            "{} Worker{}",
            worker_count,
            if worker_count > 1 { "s" } else { "" }
        )
    } else {
        format!(
            "{} unit{} selected",
            unit_count,
            if unit_count > 1 { "s" } else { "" }
        )
    };

    let label = commands
        .spawn((
            Text::new(label_text),
            TextFont {
                font_size: theme::FONT_LARGE,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(container).add_child(label);

    if unit_count == 1 {
        if let Some((kind, carrying, capacity, worker_state)) = selected_units.iter().next() {
            if *kind == EntityKind::Worker {
                if let (Some(carry), Some(cap)) = (carrying, capacity) {
                    if carry.amount > 0 {
                        let rt_name = carry
                            .resource_type
                            .map(|rt| rt.display_name())
                            .unwrap_or("Nothing");
                        let carry_text =
                            format!("Carrying: {:.1}/{:.0} {}", carry.weight, cap.0, rt_name);
                        let carry_label = commands
                            .spawn((
                                Text::new(carry_text),
                                TextFont {
                                    font_size: theme::FONT_MEDIUM,
                                    ..default()
                                },
                                TextColor(theme::WARNING),
                            ))
                            .id();
                        commands.entity(container).add_child(carry_label);

                        let bar_bg = commands
                            .spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    max_width: Val::Px(220.0),
                                    height: Val::Px(6.0),
                                    border_radius: BorderRadius::all(Val::Px(3.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 0.8)),
                            ))
                            .id();
                        commands.entity(container).add_child(bar_bg);

                        let fill_frac = (carry.weight / cap.0).min(1.0);
                        let fill = commands
                            .spawn((
                                Node {
                                    width: Val::Percent(fill_frac * 100.0),
                                    height: Val::Percent(100.0),
                                    border_radius: BorderRadius::all(Val::Px(3.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.8, 0.65, 0.2)),
                            ))
                            .id();
                        commands.entity(bar_bg).add_child(fill);
                    }

                    if let Some(state) = worker_state {
                        let state_text = match state {
                            UnitState::Idle => "Idle",
                            UnitState::Moving(_) => "Moving",
                            UnitState::Gathering(_) => "Gathering",
                            UnitState::ReturningToDeposit { .. } => "Returning to depot",
                            UnitState::Depositing { .. } => "Depositing",
                            UnitState::MovingToPlot(_) => "Going to plot building",
                            UnitState::MovingToBuild(_) => "Moving to build",
                            UnitState::Building(_) => "Building",
                            UnitState::WaitingForStorage { .. } => "Storage full!",
                            UnitState::InsideProcessor(_) => "Working at building",
                            UnitState::MovingToProcessor(_) => "Going to building",
                            UnitState::Attacking(_) => "Attacking",
                            UnitState::AttackMoving(_) => "Attack moving",
                            UnitState::Patrolling { .. } => "Patrolling",
                            UnitState::HoldPosition => "Holding position",
                        };
                        let state_label = commands
                            .spawn((
                                Text::new(state_text),
                                TextFont {
                                    font_size: theme::FONT_BODY,
                                    ..default()
                                },
                                TextColor(theme::TEXT_SECONDARY),
                            ))
                            .id();
                        commands.entity(container).add_child(state_label);
                    }
                }
            }
        }
    }

    // --- Command buttons row (for all units) ---
    let cmd_row = commands
        .spawn(Node {
            margin: UiRect::top(Val::Px(6.0)),
            ..widget_wrap_row(4.0, 4.0)
        })
        .id();
    commands.entity(container).add_child(cmd_row);

    let cmd_min_width = match layout_bucket {
        0 => Val::Percent(48.0),
        1 => Val::Px(116.0),
        _ => Val::Px(128.0),
    };

    struct CmdBtn {
        label: &'static str,
        tooltip: &'static str,
    }
    let cmd_defs = [
        CmdBtn {
            label: "Attack (F)",
            tooltip: "Attack-Move (F)\nClick a location to move while engaging enemies",
        },
        CmdBtn {
            label: "Patrol (P)",
            tooltip: "Patrol (P)\nClick a location to patrol between current position and target",
        },
        CmdBtn {
            label: "Hold (H)",
            tooltip: "Hold Position (H)\nStop and hold current position",
        },
        CmdBtn {
            label: "Stop (X)",
            tooltip: "Stop (X)\nClear all orders",
        },
        CmdBtn {
            label: "Stance (V)",
            tooltip: "Cycle Stance (V)\nCycle between Passive / Defensive / Aggressive",
        },
    ];

    for (i, def) in cmd_defs.iter().enumerate() {
        let mut btn = commands.spawn((
            Button,
            ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
            ButtonStyle::Filled,
            ActionTooltipTrigger {
                text: def.tooltip.to_string(),
            },
            Node {
                min_width: cmd_min_width,
                flex_grow: if layout_bucket == 0 { 1.0 } else { 0.0 },
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ));
        match i {
            0 => {
                btn.insert(AttackMoveButton);
            }
            1 => {
                btn.insert(PatrolButton);
            }
            2 => {
                btn.insert(HoldPositionButton);
            }
            3 => {
                btn.insert(StopButton);
            }
            4 => {
                btn.insert(CycleStanceButton);
            }
            _ => {}
        }
        let btn_id = btn
            .with_children(|b| {
                b.spawn((
                    Text::new(def.label),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(theme::TEXT_PRIMARY),
                ));
            })
            .id();
        commands.entity(cmd_row).add_child(btn_id);
    }

    if worker_count > 0 && worker_count == unit_count {
        let scuttle_btn = commands
            .spawn((
                Button,
                ScuttleUnitButton,
                ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                ButtonStyle::Destructive,
                ActionTooltipTrigger {
                    text: "Scuttle selected worker(s)\nDestroys the unit and loses any carried resources".to_string(),
                },
                Node {
                    margin: UiRect::top(Val::Px(6.0)),
                    align_self: AlignSelf::FlexStart,
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("Scuttle Worker"),
                    TextFont { font_size: theme::FONT_BODY, ..default() },
                    TextColor(theme::DESTRUCTIVE),
                ));
            })
            .id();
        commands.entity(container).add_child(scuttle_btn);
    }
}

fn spawn_building_action_bar(
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
    worker_count: usize,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    rally_mode: &RallyPointMode,
    layout_bucket: u8,
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
                font_size: theme::FONT_LARGE,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(name_row).add_child(name_child);

    let level_pill = commands
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(theme::BG_ELEVATED),
        ))
        .with_children(|pill| {
            pill.spawn((
                Text::new(format!("Lv {}", level)),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
            ));
        })
        .id();
    commands.entity(name_row).add_child(level_pill);

    // HP row
    if let Some(hp) = health {
        let hp_fraction = hp.current / hp.max;
        let hp_color = if hp_fraction > 0.6 {
            theme::HP_HIGH
        } else if hp_fraction > 0.3 {
            theme::HP_MID
        } else {
            theme::HP_LOW
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
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(theme::HP_BAR_BG),
            ))
            .with_children(|bg| {
                bg.spawn((
                    BuildingHpBarFill,
                    Node {
                        width: Val::Percent(hp_fraction * 100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
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
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(hp_row).add_child(hp_text);
    }

    // Separator
    spawn_separator(commands, container);

    // Storage inventory display
    if let Some(inv) = storage_inventory {
        let total = inv.total();
        let total_cap = inv.total_capacity();
        let capacity_color = if total >= total_cap {
            theme::DESTRUCTIVE
        } else if total as f32 >= total_cap as f32 * 0.8 {
            theme::WARNING
        } else {
            theme::TEXT_SECONDARY
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
                    font_size: theme::FONT_BODY,
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
                        font_size: theme::FONT_SMALL,
                        ..default()
                    },
                    TextColor(color),
                ))
                .id();
            commands.entity(storage_row).add_child(entry);
        }

        spawn_separator(commands, container);
    }

    // Processor info section
    if let Some(proc) = processor {
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
        let harvest_label = commands
            .spawn((
                Text::new(format!(
                    "Harvesting: {} ({:.1}/s)",
                    rt_names.join(", "),
                    effective_rate
                )),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(proc_row).add_child(harvest_label);

        if proc.max_workers > 0 {
            let slot_row = commands
                .spawn(Node {
                    ..widget_wrap_row(4.0, 2.0)
                })
                .id();
            commands.entity(proc_row).add_child(slot_row);

            let workers_label = commands
                .spawn((
                    Text::new(format!("Workers: {}/{}", worker_count, proc.max_workers)),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(theme::TEXT_SECONDARY),
                ))
                .id();
            commands.entity(slot_row).add_child(workers_label);

            for i in 0..proc.max_workers {
                let is_filled = (i as usize) < worker_count;
                let circle = commands
                    .spawn((
                        Node {
                            width: Val::Px(10.0),
                            height: Val::Px(10.0),
                            border_radius: BorderRadius::all(Val::Px(5.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(theme::ACCENT),
                        BackgroundColor(if is_filled {
                            theme::ACCENT
                        } else {
                            Color::srgba(0.0, 0.0, 0.0, 0.2)
                        }),
                    ))
                    .id();
                commands.entity(slot_row).add_child(circle);
            }

            let btn_row = commands
                .spawn(Node {
                    ..widget_wrap_row(4.0, 4.0)
                })
                .id();
            commands.entity(proc_row).add_child(btn_row);

            if worker_count < proc.max_workers as usize {
                let rest_bg = [0.14, 0.14, 0.14, 0.94];
                let assign_btn = commands
                    .spawn((
                        Button,
                        AssignWorkerButton,
                        ButtonAnimState::new(rest_bg),
                        ButtonStyle::Ghost,
                        Node {
                            min_width: Val::Px(92.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BorderColor::all(theme::ACCENT.with_alpha(0.3)),
                        BackgroundColor(theme::BG_ELEVATED),
                        Interaction::None,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("+ Assign"),
                            TextFont {
                                font_size: theme::FONT_SMALL,
                                ..default()
                            },
                            TextColor(theme::ACCENT),
                        ));
                    })
                    .id();
                commands.entity(btn_row).add_child(assign_btn);
            }

            if worker_count > 0 {
                let rest_bg = [0.14, 0.14, 0.14, 0.94];
                let unassign_btn = commands
                    .spawn((
                        Button,
                        UnassignWorkerButton,
                        ButtonAnimState::new(rest_bg),
                        ButtonStyle::Destructive,
                        Node {
                            min_width: Val::Px(92.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BorderColor::all(theme::DESTRUCTIVE.with_alpha(0.3)),
                        BackgroundColor(theme::BG_ELEVATED),
                        Interaction::None,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("- Unassign"),
                            TextFont {
                                font_size: theme::FONT_SMALL,
                                ..default()
                            },
                            TextColor(theme::DESTRUCTIVE),
                        ));
                    })
                    .id();
                commands.entity(btn_row).add_child(unassign_btn);
            }
        } else {
            let auto_badge = commands
                .spawn((
                    Text::new("Automated (no workers needed)"),
                    TextFont {
                        font_size: theme::FONT_SMALL,
                        ..default()
                    },
                    TextColor(theme::ACCENT),
                ))
                .id();
            commands.entity(proc_row).add_child(auto_badge);
        }

        spawn_separator(commands, container);
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
                    layout_bucket,
                );
            }

            spawn_separator(commands, container);
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
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        })
                        .insert(BackgroundColor(theme::BG_SURFACE))
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
                                        font_size: theme::FONT_BODY,
                                        ..default()
                                    },
                                    TextColor(theme::ACCENT),
                                ));
                                row.spawn((
                                    Text::new(format!("{:.0}s", remaining)),
                                    TextFont {
                                        font_size: theme::FONT_BODY,
                                        ..default()
                                    },
                                    TextColor(theme::WARNING),
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
                                        border_radius: BorderRadius::all(Val::Px(3.0)),
                                        ..default()
                                    })
                                    .insert(BackgroundColor(theme::HP_BAR_BG))
                                    .with_children(|bg| {
                                        bg.spawn((
                                            UpgradeProgressBar,
                                            Node {
                                                width: Val::Percent(fraction * 100.0),
                                                height: Val::Percent(100.0),
                                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                                ..default()
                                            },
                                            BackgroundColor(theme::ACCENT),
                                            BoxShadow::new(
                                                Color::srgba(0.29, 0.62, 1.0, 0.4),
                                                Val::Px(0.0),
                                                Val::Px(0.0),
                                                Val::Px(0.0),
                                                Val::Px(3.0),
                                            ),
                                        ));
                                    });
                                bar_row.spawn((
                                    Text::new(format!("{}%", (fraction * 100.0) as u32)),
                                    TextFont {
                                        font_size: theme::FONT_SMALL,
                                        ..default()
                                    },
                                    TextColor(theme::TEXT_SECONDARY),
                                ));
                            });
                        })
                        .id();
                    commands.entity(actions_row).add_child(upgrade_container);
                } else {
                    let cost_str = format_cost(&upgrade_data.cost);
                    let text_color = if can_afford {
                        theme::TEXT_PRIMARY
                    } else {
                        theme::DESTRUCTIVE
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
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            BorderColor::all(theme::BORDER_SUBTLE),
                            Transform::from_scale(Vec3::splat(upgrade_opacity)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new(format!("Upgrade L{}", level + 1)),
                                TextFont {
                                    font_size: theme::FONT_BODY,
                                    ..default()
                                },
                                TextColor(text_color),
                            ));
                            btn.spawn((
                                Text::new(cost_str),
                                TextFont {
                                    font_size: theme::FONT_CAPTION,
                                    ..default()
                                },
                                TextColor(theme::TEXT_SECONDARY),
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
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(Color::srgba(0.33, 0.33, 0.33, 0.4)),
                ))
                .with_children(|pill| {
                    pill.spawn((
                        Text::new("MAX"),
                        TextFont {
                            font_size: theme::FONT_BODY,
                            ..default()
                        },
                        TextColor(theme::TEXT_DISABLED),
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
                theme::ACCENT
            } else {
                theme::BORDER_SUBTLE
            };
            let rally_text = if is_rally_active {
                "Click Ground..."
            } else {
                "Set Rally"
            };
            let rally_text_color = if is_rally_active {
                theme::ACCENT
            } else {
                theme::TEXT_SECONDARY
            };
            let rally_bg = if is_rally_active {
                Color::srgba(0.29, 0.62, 1.0, 0.1)
            } else {
                Color::NONE
            };
            let rally_btn = commands
                .spawn((
                    Button,
                    RallyPointButton,
                    ButtonAnimState::new(if is_rally_active {
                        [0.29, 0.62, 1.0, 0.1]
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
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(rally_bg),
                    BorderColor::all(rally_border),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(rally_text),
                        TextFont {
                            font_size: theme::FONT_BODY,
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
            Color::srgba(0.30, 0.69, 0.31, 0.15)
        } else {
            Color::srgba(0.80, 0.27, 0.27, 0.15)
        };
        let toggle_text = if is_enabled {
            "Auto-Attack: ON"
        } else {
            "Auto-Attack: OFF"
        };
        let toggle_color = if is_enabled {
            theme::SUCCESS
        } else {
            theme::DESTRUCTIVE
        };
        let toggle_btn = commands
            .spawn((
                Button,
                ToggleAutoAttackButton,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
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
                        font_size: theme::FONT_BODY,
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
            spawn_separator(commands, container);
            spawn_training_queue_ui(commands, container, queue, icons, registry, layout_bucket);
        }
    }

    // Demolish section
    spawn_separator(commands, container);

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
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Demolish"),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(theme::DESTRUCTIVE),
            ));
        })
        .id();
    commands.entity(demolish_row).add_child(demolish_btn);
}

fn spawn_training_queue_ui(
    commands: &mut Commands,
    parent: Entity,
    queue: &TrainingQueue,
    icons: &IconAssets,
    _registry: &BlueprintRegistry,
    layout_bucket: u8,
) {
    let header = commands
        .spawn((
            Text::new(format!("Queue ({})", queue.queue.len())),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
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
            BackgroundColor(theme::BG_TRANSPARENT),
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
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(theme::BG_SURFACE),
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
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    })
                    .insert(BackgroundColor(theme::HP_BAR_BG))
                    .with_children(|bg| {
                        let fraction = queue.timer.as_ref().map_or(0.0, |t| t.fraction());
                        bg.spawn((
                            TrainingProgressBar,
                            Node {
                                width: Val::Percent(fraction * 100.0),
                                height: Val::Percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(theme::ACCENT),
                            BoxShadow::new(
                                Color::srgba(0.29, 0.62, 1.0, 0.4),
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
                            theme::FONT_SMALL
                        } else {
                            theme::FONT_TINY
                        },
                        ..default()
                    },
                    TextColor(Color::srgba(0.80, 0.27, 0.27, 0.4)),
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

fn spawn_construction_action_bar(
    commands: &mut Commands,
    parent: Entity,
    kind: EntityKind,
    construction: Option<&ConstructionProgress>,
    _registry: &BlueprintRegistry,
    _layout_bucket: u8,
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
                font_size: theme::FONT_LARGE,
                ..default()
            },
            TextColor(theme::WARNING),
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
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(theme::HP_BAR_BG),
            ))
            .with_children(|bg| {
                bg.spawn((
                    ConstructionProgressBar,
                    Node {
                        width: Val::Percent(fraction * 100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(theme::WARNING),
                ));
            })
            .id();
        commands.entity(container).add_child(bar_bg);

        let pct = commands
            .spawn((
                Text::new(pct_text),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(container).add_child(pct);

        let worker_text = commands
            .spawn((
                ConstructionWorkerCountText,
                Text::new("Waiting for workers..."),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.7, 0.9)),
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
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Cancel"),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(theme::DESTRUCTIVE),
            ));
        })
        .id();
    commands.entity(container).add_child(cancel_btn);
}

// ── Building Grid (replaces card hand) ──

/// New component replacing BuildCard for the grid-based building buttons
#[derive(Component)]
pub struct BuildGridButton(pub EntityKind);

fn spawn_found_base_panel(
    commands: &mut Commands,
    parent: Entity,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    layout_bucket: u8,
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
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
        ));
        panel.spawn((
            Text::new("Found a Base to unlock construction and unit production."),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
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
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(if can_afford {
                theme::BG_SURFACE
            } else {
                Color::srgba(0.08, 0.08, 0.08, 0.7)
            }),
            BorderColor::all(if can_afford {
                Color::srgba(0.25, 0.25, 0.30, 0.4)
            } else {
                Color::srgba(0.80, 0.27, 0.27, 0.25)
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
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(if can_afford {
                        theme::TEXT_PRIMARY
                    } else {
                        theme::TEXT_DISABLED
                    }),
                ));
                text_col.spawn((
                    Text::new(cost_str),
                    TextFont {
                        font_size: theme::FONT_SMALL,
                        ..default()
                    },
                    TextColor(if can_afford {
                        theme::TEXT_SECONDARY
                    } else {
                        theme::DESTRUCTIVE
                    }),
                ));
            });
        })
        .id();
    commands.entity(container).add_child(btn);
}

fn spawn_building_grid(
    commands: &mut Commands,
    parent: Entity,
    completed: &[EntityKind],
    founded: bool,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    layout_bucket: u8,
) {
    let building_kinds = registry.building_kinds();
    let available: Vec<EntityKind> = building_kinds
        .iter()
        .copied()
        .filter(|kind| {
            if founded && *kind == EntityKind::Base {
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
                    | EntityKind::Sawmill
                    | EntityKind::Mine
                    | EntityKind::OilRig
                    | EntityKind::Storage
            )
        })
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
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
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
                Color::srgba(0.25, 0.25, 0.30, 0.4)
            } else {
                Color::srgba(0.80, 0.27, 0.27, 0.25)
            };
            let name_color = if can_afford {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_DISABLED
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
                        padding: UiRect::all(Val::Px(4.0)),
                        row_gap: Val::Px(2.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(if can_afford {
                        theme::BG_SURFACE
                    } else {
                        Color::srgba(0.08, 0.08, 0.08, 0.7)
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
                            font_size: theme::FONT_TINY,
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

fn spawn_train_button(
    commands: &mut Commands,
    parent: Entity,
    kind: EntityKind,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    layout_bucket: u8,
) {
    let label = kind.display_name();
    let bp = registry.get(kind);
    let cost_str = format_cost_from_blueprint(bp);
    let can_afford = bp.cost.can_afford(player_res);

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
    if !can_afford {
        tooltip_lines.push("Not enough resources!".to_string());
    }
    tooltip_lines.push("Click to train".to_string());

    let border_color = if can_afford {
        Color::srgba(0.25, 0.25, 0.30, 0.4)
    } else {
        Color::srgba(0.80, 0.27, 0.27, 0.25)
    };
    let name_color = if can_afford {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_DISABLED
    };

    let child = commands
        .spawn((
            TrainButton(kind),
            Button,
            ButtonAnimState::new(if can_afford {
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
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(if can_afford {
                theme::BTN_PRIMARY
            } else {
                Color::srgba(0.08, 0.08, 0.08, 0.7)
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
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(theme::ICON_FRAME_BG),
            ))
            .with_children(|frame| {
                frame.spawn((
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
                        width: Val::Percent(82.0),
                        height: Val::Percent(82.0),
                        ..default()
                    },
                ));
            });
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(name_color),
            ));
            btn.spawn((
                TrainCostText { kind },
                Text::new(cost_str),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(if can_afford {
                    theme::TEXT_SECONDARY
                } else {
                    theme::DESTRUCTIVE
                }),
            ));
        })
        .id();

    commands.entity(parent).add_child(child);
}

fn format_cost_from_blueprint(bp: &crate::blueprints::Blueprint) -> String {
    format_cost(&bp.cost)
}

fn spawn_separator(commands: &mut Commands, parent: Entity) {
    let sep = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::SEPARATOR),
        ))
        .id();
    commands.entity(parent).add_child(sep);
}
