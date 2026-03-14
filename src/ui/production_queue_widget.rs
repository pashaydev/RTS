use bevy::prelude::*;
use std::collections::BTreeMap;

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme;

#[derive(Component)]
pub struct QueuePanelItem;

#[derive(Component)]
pub struct QueueFocusRow(pub Entity);

struct CommandQueueGroup {
    representative: Entity,
    kind: EntityKind,
    count: usize,
    active_label: String,
    active_task_id: Option<u64>,
    queued_labels: Vec<(String, u64)>,
}

pub fn update_production_queue(
    mut commands: Commands,
    active_player: Res<ActivePlayer>,
    icons: Res<IconAssets>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    selected_units: Query<
        (Entity, &EntityKind, &Faction, &UnitState, &TaskQueue),
        (With<Unit>, With<Selected>),
    >,
    selected_buildings: Query<
        (Entity, &EntityKind, &Faction, &TrainingQueue),
        (With<Building>, With<Selected>),
    >,
    buildings: Query<(Entity, &EntityKind, &TrainingQueue, &Faction), With<Building>>,
    resource_nodes: Query<&ResourceNode>,
    kind_lookup: Query<&EntityKind>,
    existing_items: Query<Entity, With<QueuePanelItem>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::ProductionQueue) {
        return;
    }

    let Some(content) = super::widget_framework::find_widget_content(
        WidgetId::ProductionQueue,
        &widget_q,
        &content_q,
    ) else {
        return;
    };

    for item in &existing_items {
        commands.entity(item).try_despawn();
    }

    let selected_units: Vec<_> = selected_units
        .iter()
        .filter(|(_, _, faction, _, _)| **faction == active_player.0)
        .collect();
    let selected_buildings: Vec<_> = selected_buildings
        .iter()
        .filter(|(_, _, faction, _)| **faction == active_player.0)
        .collect();

    let has_commands = !selected_units.is_empty();
    let has_selected_production = !selected_buildings.is_empty();

    if has_commands {
        spawn_section_header(
            &mut commands,
            content,
            format!("Commands ({})", selected_units.len()),
        );

        for group in group_command_queues(&selected_units, &kind_lookup, &resource_nodes) {
            let row = spawn_focus_row(&mut commands, content, group.representative);
            let title = if group.count > 1 {
                format!("{}x {}", group.count, group.kind.display_name())
            } else {
                group.kind.display_name().to_string()
            };
            spawn_row_header(
                &mut commands,
                row,
                icons.entity_icon(group.kind),
                title.as_str(),
            );
            spawn_command_line(
                &mut commands,
                row,
                group.active_label.as_str(),
                if group.count == 1 {
                    Some(group.representative)
                } else {
                    None
                },
                group.active_task_id,
                true,
                group.count == 1 && group.active_task_id.is_some(),
            );

            if group.queued_labels.is_empty() {
                spawn_secondary_text(&mut commands, row, "No queued tasks");
            } else {
                for (label, task_id) in &group.queued_labels {
                    spawn_command_line(
                        &mut commands,
                        row,
                        label.as_str(),
                        if group.count == 1 {
                            Some(group.representative)
                        } else {
                            None
                        },
                        if group.count == 1 { Some(*task_id) } else { None },
                        false,
                        group.count == 1,
                    );
                }
            }
        }
    }

    if has_selected_production {
        spawn_section_header(
            &mut commands,
            content,
            format!("Production ({})", selected_buildings.len()),
        );

        for (entity, kind, _faction, queue) in selected_buildings {
            spawn_building_queue_card(&mut commands, content, entity, *kind, queue, &icons, true);
        }
    } else {
        let active_buildings: Vec<_> = buildings
            .iter()
            .filter(|(_, _, queue, faction)| {
                **faction == active_player.0 && (!queue.queue.is_empty() || queue.timer.is_some())
            })
            .collect();

        spawn_section_header(
            &mut commands,
            content,
            format!("Production ({})", active_buildings.len()),
        );

        if active_buildings.is_empty() && !has_commands {
            spawn_secondary_text(&mut commands, content, "No active queues");
        } else if active_buildings.is_empty() {
            spawn_secondary_text(&mut commands, content, "No active production");
        } else {
            for (entity, kind, queue, _faction) in active_buildings {
                spawn_building_queue_card(
                    &mut commands,
                    content,
                    entity,
                    *kind,
                    queue,
                    &icons,
                    false,
                );
            }
        }
    }
}

fn spawn_section_header(commands: &mut Commands, parent: Entity, label: String) {
    let header = commands
        .spawn((
            QueuePanelItem,
            Text::new(label),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(header);
}

fn spawn_focus_row(commands: &mut Commands, parent: Entity, entity: Entity) -> Entity {
    let row = commands
        .spawn((
            QueuePanelItem,
            QueueFocusRow(entity),
            Button,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(6.0)),
                margin: UiRect::top(Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(theme::BG_SURFACE),
        ))
        .id();
    commands.entity(parent).add_child(row);
    row
}

fn spawn_row_header(
    commands: &mut Commands,
    parent: Entity,
    icon_handle: Handle<Image>,
    label: &str,
) {
    let header = commands
        .spawn((
            QueuePanelItem,
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(header);

    let icon = commands
        .spawn((
            QueuePanelItem,
            ImageNode::new(icon_handle),
            Node {
                width: Val::Px(22.0),
                height: Val::Px(22.0),
                ..default()
            },
        ))
        .id();
    commands.entity(header).add_child(icon);

    let text = commands
        .spawn((
            QueuePanelItem,
            Text::new(label),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(header).add_child(text);
}

fn spawn_secondary_text(commands: &mut Commands, parent: Entity, label: &str) {
    let text = commands
        .spawn((
            QueuePanelItem,
            Text::new(label),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_DISABLED),
        ))
        .id();
    commands.entity(parent).add_child(text);
}

fn spawn_command_line(
    commands: &mut Commands,
    parent: Entity,
    label: &str,
    unit: Option<Entity>,
    task_id: Option<u64>,
    is_current: bool,
    show_cancel: bool,
) {
    let row = commands
        .spawn((
            QueuePanelItem,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(row);

    let prefix = if is_current { "Now" } else { "Queue" };
    let text = commands
        .spawn((
            QueuePanelItem,
            Text::new(format!("{}  {}", prefix, label)),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(if is_current {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            }),
            Node {
                flex_grow: 1.0,
                ..default()
            },
        ))
        .id();
    commands.entity(row).add_child(text);

    if show_cancel {
        let cancel = commands
            .spawn((
                QueuePanelItem,
                CancelUnitTaskButton {
                    unit: unit.expect("cancel button requires a unit"),
                    task_id,
                    is_current,
                },
                Button,
                Node {
                    min_width: Val::Px(22.0),
                    min_height: Val::Px(18.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.75, 0.22, 0.22, 0.12)),
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new("x"),
                    TextFont {
                        font_size: theme::FONT_CAPTION,
                        ..default()
                    },
                    TextColor(theme::TEXT_SECONDARY),
                ));
            })
            .id();
        commands.entity(row).add_child(cancel);
    }
}

fn spawn_building_queue_card(
    commands: &mut Commands,
    parent: Entity,
    building: Entity,
    kind: EntityKind,
    queue: &TrainingQueue,
    icons: &IconAssets,
    show_full_queue: bool,
) {
    let row = spawn_focus_row(commands, parent, building);
    spawn_row_header(commands, row, icons.entity_icon(kind), kind.display_name());

    if let Some(current) = queue.queue.first() {
        let remaining = queue.timer.as_ref().map_or(0.0, Timer::remaining_secs);
        let current_row = commands
            .spawn((
                QueuePanelItem,
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(row).add_child(current_row);

        let current_icon = commands
            .spawn((
                QueuePanelItem,
                ImageNode::new(icons.entity_icon(*current)),
                Node {
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(current_row).add_child(current_icon);

        let current_text = commands
            .spawn((
                QueuePanelItem,
                Text::new(format!("Training {}  {:.0}s", current.display_name(), remaining)),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_PRIMARY),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ))
            .id();
        commands.entity(current_row).add_child(current_text);

        let progress_bg = commands
            .spawn((
                QueuePanelItem,
                Node {
                    width: Val::Px(72.0),
                    height: Val::Px(5.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(theme::HP_BAR_BG),
            ))
            .with_children(|bg| {
                bg.spawn((
                    QueuePanelItem,
                    Node {
                        width: Val::Percent(queue.timer.as_ref().map_or(0.0, |timer| {
                            timer.fraction() * 100.0
                        })),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(theme::ACCENT),
                ));
            })
            .id();
        commands.entity(current_row).add_child(progress_bg);
    } else {
        spawn_secondary_text(commands, row, "Idle");
    }

    let queue_items = if show_full_queue {
        queue.queue.len()
    } else {
        queue.queue.len().min(5)
    };

    if queue_items > 0 {
        let queue_row = commands
            .spawn((
                QueuePanelItem,
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(4.0),
                    row_gap: Val::Px(4.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(row).add_child(queue_row);

        for (index, unit_kind) in queue.queue.iter().enumerate().take(queue_items) {
            let chip = commands
                .spawn((
                    QueuePanelItem,
                    CancelTrainQueueItemButton { building, index },
                    Button,
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.0),
                        padding: UiRect::axes(Val::Px(5.0), Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(theme::BG_PANEL),
                ))
                .with_children(|chip| {
                    chip.spawn((
                        ImageNode::new(icons.entity_icon(*unit_kind)),
                        Node {
                            width: Val::Px(14.0),
                            height: Val::Px(14.0),
                            ..default()
                        },
                    ));
                    chip.spawn((
                        Text::new(unit_kind.display_name()),
                        TextFont {
                            font_size: theme::FONT_CAPTION,
                            ..default()
                        },
                        TextColor(theme::TEXT_SECONDARY),
                    ));
                    chip.spawn((
                        Text::new("x"),
                        TextFont {
                            font_size: theme::FONT_TINY,
                            ..default()
                        },
                        TextColor(theme::TEXT_DISABLED),
                    ));
                })
                .id();
            commands.entity(queue_row).add_child(chip);
        }

        if !show_full_queue && queue.queue.len() > queue_items {
            let more = commands
                .spawn((
                    QueuePanelItem,
                    Text::new(format!("+{}", queue.queue.len() - queue_items)),
                    TextFont {
                        font_size: theme::FONT_CAPTION,
                        ..default()
                    },
                    TextColor(theme::TEXT_DISABLED),
                ))
                .id();
            commands.entity(queue_row).add_child(more);
        }
    }
}

fn format_active_state(
    state: UnitState,
    kind_lookup: &Query<&EntityKind>,
    resource_nodes: &Query<&ResourceNode>,
) -> String {
    match state {
        UnitState::Idle => "Idle".to_string(),
        UnitState::Moving(pos) => format!("Move to {}", format_position(pos)),
        UnitState::Attacking(target) => {
            format!("Attack {}", format_target(target, kind_lookup, resource_nodes))
        }
        UnitState::Gathering(target) => {
            format!("Gather {}", format_target(target, kind_lookup, resource_nodes))
        }
        UnitState::ReturningToDeposit { .. } => "Return to deposit".to_string(),
        UnitState::Depositing { .. } => "Deposit resources".to_string(),
        UnitState::WaitingForStorage { .. } => "Waiting for storage".to_string(),
        UnitState::MovingToPlot(pos) => format!("Plot building at {}", format_position(pos)),
        UnitState::MovingToBuild(target) => {
            format!("Move to build {}", format_target(target, kind_lookup, resource_nodes))
        }
        UnitState::Building(target) => {
            format!("Build {}", format_target(target, kind_lookup, resource_nodes))
        }
        UnitState::AssignedGathering { building, .. } => {
            format!(
                "Assigned to {}",
                format_target(building, kind_lookup, resource_nodes)
            )
        }
        UnitState::Patrolling { target, .. } => format!("Patrol {}", format_position(target)),
        UnitState::AttackMoving(pos) => format!("Attack-move {}", format_position(pos)),
        UnitState::HoldPosition => "Hold position".to_string(),
    }
}

fn format_task(
    task: &QueuedTask,
    kind_lookup: &Query<&EntityKind>,
    resource_nodes: &Query<&ResourceNode>,
) -> String {
    match task {
        QueuedTask::Move(pos) => format!("Move to {}", format_position(*pos)),
        QueuedTask::AttackMove(pos) => format!("Attack-move {}", format_position(*pos)),
        QueuedTask::Attack(target) => {
            format!("Attack {}", format_target(*target, kind_lookup, resource_nodes))
        }
        QueuedTask::Gather(target) => {
            format!("Gather {}", format_target(*target, kind_lookup, resource_nodes))
        }
        QueuedTask::Build(target) => {
            format!("Build {}", format_target(*target, kind_lookup, resource_nodes))
        }
        QueuedTask::Patrol(pos) => format!("Patrol {}", format_position(*pos)),
        QueuedTask::AssignToProcessor(target) => format!(
            "Assign to {}",
            format_target(*target, kind_lookup, resource_nodes)
        ),
        QueuedTask::HoldPosition => "Hold position".to_string(),
    }
}

fn format_target(
    entity: Entity,
    kind_lookup: &Query<&EntityKind>,
    resource_nodes: &Query<&ResourceNode>,
) -> String {
    if let Ok(kind) = kind_lookup.get(entity) {
        kind.display_name().to_string()
    } else if let Ok(node) = resource_nodes.get(entity) {
        node.resource_type.display_name().to_string()
    } else {
        format!("Entity {}", entity.index())
    }
}

fn format_position(pos: Vec3) -> String {
    format!("{:.0}, {:.0}", pos.x, pos.z)
}

fn group_command_queues(
    selected_units: &[(Entity, &EntityKind, &Faction, &UnitState, &TaskQueue)],
    kind_lookup: &Query<&EntityKind>,
    resource_nodes: &Query<&ResourceNode>,
) -> Vec<CommandQueueGroup> {
    let mut groups: BTreeMap<String, CommandQueueGroup> = BTreeMap::new();

    for (entity, kind, _faction, state, queue) in selected_units {
        let active_label = queue
            .current
            .as_ref()
            .map(|entry| format_task(&entry.task, kind_lookup, resource_nodes))
            .unwrap_or_else(|| format_active_state(**state, kind_lookup, resource_nodes));
        let active_task_id = queue.current.as_ref().map(|entry| entry.id);
        let queued_labels: Vec<(String, u64)> = queue
            .queue
            .iter()
            .map(|entry| (format_task(&entry.task, kind_lookup, resource_nodes), entry.id))
            .collect();

        let mut key = format!("{}|{}", kind.display_name(), active_label);
        for (label, _) in &queued_labels {
            key.push('|');
            key.push_str(label);
        }

        groups
            .entry(key)
            .and_modify(|group| {
                group.count += 1;
            })
            .or_insert_with(|| CommandQueueGroup {
                representative: *entity,
                kind: **kind,
                count: 1,
                active_label,
                active_task_id,
                queued_labels,
            });
    }

    groups.into_values().collect()
}

pub fn handle_queue_row_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &QueueFocusRow), Changed<Interaction>>,
    selected: Query<Entity, With<Selected>>,
    units: Query<Entity, With<Unit>>,
    buildings: Query<Entity, With<Building>>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, row) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if units.get(row.0).is_err() && buildings.get(row.0).is_err() {
            continue;
        }
        ui_press.0 = true;
        for entity in &selected {
            commands.entity(entity).remove::<Selected>();
        }
        commands.entity(row.0).insert(Selected);
    }
}

pub fn handle_queue_cancel_buttons(
    mut commands: Commands,
    unit_cancel_buttons: Query<(&Interaction, &CancelUnitTaskButton), Changed<Interaction>>,
    building_cancel_buttons: Query<
        (&Interaction, &CancelTrainQueueItemButton),
        Changed<Interaction>,
    >,
    mut unit_states: Query<(&mut UnitState, &mut TaskSource, &mut TaskQueue), With<Unit>>,
    mut training_queues: Query<&mut TrainingQueue, With<Building>>,
    registry: Res<crate::blueprints::BlueprintRegistry>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, button) in &unit_cancel_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if let Ok((mut state, mut source, mut queue)) = unit_states.get_mut(button.unit) {
            if button.is_current {
                queue.current = None;
                *state = UnitState::Idle;
                *source = TaskSource::Auto;
                commands
                    .entity(button.unit)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>()
                    .remove::<LeashOrigin>();
            } else if let Some(task_id) = button.task_id {
                queue.remove_by_id(task_id);
            }
        }
    }

    for (interaction, button) in &building_cancel_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if let Ok(mut queue) = training_queues.get_mut(button.building) {
            if button.index < queue.queue.len() {
                let removed_kind = queue.queue.remove(button.index);
                let bp = registry.get(removed_kind);
                let player_res = all_resources.get_mut(&active_player.0);
                for (i, &amt) in bp.cost.amounts.iter().enumerate() {
                    player_res.amounts[i] += amt;
                }
                if button.index == 0 {
                    queue.timer = None;
                }
            }
        }
    }
}
