//! Unit/tech production queue widget: shows active and queued items
//! with ETA bars for the currently selected building.

use bevy::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use super::core::framework::WidgetId;
use super::core::hud::MainHudRoot;
use crate::blueprints::EntityKind;
use crate::simulation::combat::clear_combat_intent;
use crate::types::*;
use crate::ui::theme::Theme;

pub struct ProductionQueueWidgetPlugin;

impl Plugin for ProductionQueueWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_production_queue_widget
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(
            Update,
            (
                update_production_queue,
                handle_queue_row_click,
                handle_queue_cancel_buttons,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

widget_spawn_system!(spawn_production_queue_widget, WidgetId::ProductionQueue);

#[derive(Component)]
pub struct QueuePanelItem;

#[derive(Component)]
pub struct QueueFocusRow(pub Entity);

#[derive(Component, Default)]
pub(crate) struct QueuePanelState {
    signature: u64,
}

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
    theme: Res<Theme>,
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
    panel_state_q: Query<&QueuePanelState>,
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

    let selected_units: Vec<_> = selected_units
        .iter()
        .filter(|(_, _, faction, _, _)| **faction == active_player.0)
        .collect();
    let selected_buildings: Vec<_> = selected_buildings
        .iter()
        .filter(|(_, _, faction, _)| **faction == active_player.0)
        .collect();

    let active_buildings: Vec<_> = if selected_buildings.is_empty() {
        buildings
            .iter()
            .filter(|(_, _, queue, faction)| {
                **faction == active_player.0 && (!queue.queue.is_empty() || queue.timer.is_some())
            })
            .collect()
    } else {
        Vec::new()
    };

    let signature = compute_panel_signature(
        &selected_units,
        &selected_buildings,
        &active_buildings,
        active_player.0,
        registry.is_visible(WidgetId::ProductionQueue),
    );
    if panel_state_q
        .get(content)
        .is_ok_and(|state| state.signature == signature)
    {
        return;
    }
    commands
        .entity(content)
        .insert(QueuePanelState { signature });

    for item in &existing_items {
        commands.entity(item).try_despawn();
    }

    let has_commands = !selected_units.is_empty();
    let has_selected_production = !selected_buildings.is_empty();

    if has_commands {
        spawn_section_header(
            &mut commands,
            content,
            format!("Commands ({})", selected_units.len()),
            &theme,
        );

        for group in group_command_queues(&selected_units, &kind_lookup, &resource_nodes) {
            let row = spawn_focus_row(&mut commands, content, group.representative, &theme);
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
                &theme,
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
                &theme,
            );

            if group.queued_labels.is_empty() {
                spawn_secondary_text(&mut commands, row, "No queued tasks", &theme);
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
                        if group.count == 1 {
                            Some(*task_id)
                        } else {
                            None
                        },
                        false,
                        group.count == 1,
                        &theme,
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
            &theme,
        );

        for (entity, kind, _faction, queue) in selected_buildings {
            spawn_building_queue_card(
                &mut commands,
                content,
                entity,
                *kind,
                queue,
                &icons,
                true,
                &theme,
            );
        }
    } else {
        spawn_section_header(
            &mut commands,
            content,
            format!("Production ({})", active_buildings.len()),
            &theme,
        );

        if active_buildings.is_empty() && !has_commands {
            spawn_secondary_text(&mut commands, content, "No active queues", &theme);
        } else if active_buildings.is_empty() {
            spawn_secondary_text(&mut commands, content, "No active production", &theme);
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
                    &theme,
                );
            }
        }
    }
}

fn compute_panel_signature(
    selected_units: &[(Entity, &EntityKind, &Faction, &UnitState, &TaskQueue)],
    selected_buildings: &[(Entity, &EntityKind, &Faction, &TrainingQueue)],
    active_buildings: &[(Entity, &EntityKind, &TrainingQueue, &Faction)],
    active_player: Faction,
    widget_visible: bool,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    widget_visible.hash(&mut hasher);
    active_player.hash(&mut hasher);
    selected_units.len().hash(&mut hasher);
    selected_buildings.len().hash(&mut hasher);
    active_buildings.len().hash(&mut hasher);

    for (entity, kind, faction, state, queue) in selected_units {
        entity.to_bits().hash(&mut hasher);
        kind.hash(&mut hasher);
        faction.hash(&mut hasher);
        hash_unit_state(state, &mut hasher);
        hash_task_queue(queue, &mut hasher);
    }

    for (entity, kind, faction, queue) in selected_buildings {
        entity.to_bits().hash(&mut hasher);
        kind.hash(&mut hasher);
        faction.hash(&mut hasher);
        hash_training_queue(queue, &mut hasher);
    }

    for (entity, kind, queue, faction) in active_buildings {
        entity.to_bits().hash(&mut hasher);
        kind.hash(&mut hasher);
        faction.hash(&mut hasher);
        hash_training_queue(queue, &mut hasher);
    }

    hasher.finish()
}

fn hash_task_queue(queue: &TaskQueue, hasher: &mut DefaultHasher) {
    match &queue.current {
        Some(entry) => {
            true.hash(hasher);
            entry.id.hash(hasher);
            hash_queued_task(&entry.task, hasher);
        }
        None => false.hash(hasher),
    }

    queue.queue.len().hash(hasher);
    for entry in &queue.queue {
        entry.id.hash(hasher);
        hash_queued_task(&entry.task, hasher);
    }
}

fn hash_training_queue(queue: &TrainingQueue, hasher: &mut DefaultHasher) {
    queue.queue.len().hash(hasher);
    for kind in &queue.queue {
        kind.hash(hasher);
    }
    queue.total_trained.hash(hasher);

    match &queue.timer {
        Some(timer) => {
            true.hash(hasher);
            ((timer.fraction() * 100.0).round() as u32).hash(hasher);
            (timer.remaining_secs().round() as u32).hash(hasher);
        }
        None => false.hash(hasher),
    }
}

fn hash_unit_state(state: &UnitState, hasher: &mut DefaultHasher) {
    match state {
        UnitState::Idle => 0u8.hash(hasher),
        UnitState::Moving(pos) => {
            1u8.hash(hasher);
            hash_vec3(*pos, hasher);
        }
        UnitState::Attacking(entity) => {
            2u8.hash(hasher);
            entity.to_bits().hash(hasher);
        }
        UnitState::Gathering(entity) => {
            3u8.hash(hasher);
            entity.to_bits().hash(hasher);
        }
        UnitState::ReturningToDeposit { depot, gather_node } => {
            4u8.hash(hasher);
            depot.to_bits().hash(hasher);
            gather_node.map(Entity::to_bits).hash(hasher);
        }
        UnitState::Depositing { depot, gather_node } => {
            5u8.hash(hasher);
            depot.to_bits().hash(hasher);
            gather_node.map(Entity::to_bits).hash(hasher);
        }
        UnitState::WaitingForStorage { depot, gather_node } => {
            6u8.hash(hasher);
            depot.to_bits().hash(hasher);
            gather_node.map(Entity::to_bits).hash(hasher);
        }
        UnitState::WaitingForDepot { gather_node } => {
            7u8.hash(hasher);
            gather_node.map(Entity::to_bits).hash(hasher);
        }
        UnitState::MovingToPlot(pos) => {
            8u8.hash(hasher);
            hash_vec3(*pos, hasher);
        }
        UnitState::MovingToBuild(entity) => {
            9u8.hash(hasher);
            entity.to_bits().hash(hasher);
        }
        UnitState::Building(entity) => {
            10u8.hash(hasher);
            entity.to_bits().hash(hasher);
        }
        UnitState::AssignedGathering { building, phase } => {
            11u8.hash(hasher);
            building.to_bits().hash(hasher);
            std::mem::discriminant(phase).hash(hasher);
        }
        UnitState::Patrolling { target, origin } => {
            12u8.hash(hasher);
            hash_vec3(*target, hasher);
            hash_vec3(*origin, hasher);
        }
        UnitState::AttackMoving(pos) => {
            13u8.hash(hasher);
            hash_vec3(*pos, hasher);
        }
        UnitState::HoldPosition => 14u8.hash(hasher),
    }
}

fn hash_queued_task(task: &QueuedTask, hasher: &mut DefaultHasher) {
    match task {
        QueuedTask::Move(pos) => {
            0u8.hash(hasher);
            hash_vec3(*pos, hasher);
        }
        QueuedTask::AttackMove(pos) => {
            1u8.hash(hasher);
            hash_vec3(*pos, hasher);
        }
        QueuedTask::Attack(entity) => {
            2u8.hash(hasher);
            entity.to_bits().hash(hasher);
        }
        QueuedTask::Gather(entity) => {
            3u8.hash(hasher);
            entity.to_bits().hash(hasher);
        }
        QueuedTask::Build(entity) => {
            4u8.hash(hasher);
            entity.to_bits().hash(hasher);
        }
        QueuedTask::Patrol(pos) => {
            5u8.hash(hasher);
            hash_vec3(*pos, hasher);
        }
        QueuedTask::HoldPosition => 6u8.hash(hasher),
    }
}

fn hash_vec3(value: Vec3, hasher: &mut DefaultHasher) {
    value.x.to_bits().hash(hasher);
    value.y.to_bits().hash(hasher);
    value.z.to_bits().hash(hasher);
}

fn spawn_section_header(commands: &mut Commands, parent: Entity, label: String, theme: &Theme) {
    let header = commands
        .spawn((
            QueuePanelItem,
            Text::new(label),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
            Node {
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(header);
}

fn spawn_focus_row(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    theme: &Theme,
) -> Entity {
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
                // border_radius: RADIUS_LG,
                ..default()
            },
            BackgroundColor(theme.colors.bg_surface),
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
    theme: &Theme,
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
                font_size: theme.typography.body,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ))
        .id();
    commands.entity(header).add_child(text);
}

fn spawn_secondary_text(commands: &mut Commands, parent: Entity, label: &str, theme: &Theme) {
    let text = commands
        .spawn((
            QueuePanelItem,
            Text::new(label),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_disabled),
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
    theme: &Theme,
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
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(if is_current {
                theme.colors.text_primary
            } else {
                theme.colors.text_secondary
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
                    // border_radius: RADIUS_MD,
                    ..default()
                },
                BackgroundColor(crate::ui::theme::DESTRUCTIVE.with_alpha(0.12)),
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new("x"),
                    TextFont {
                        font_size: theme.typography.caption,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
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
    theme: &Theme,
) {
    let row = spawn_focus_row(commands, parent, building, theme);
    spawn_row_header(
        commands,
        row,
        icons.entity_icon(kind),
        kind.display_name(),
        theme,
    );

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
                Text::new(format!(
                    "Training {}  {:.0}s",
                    current.display_name(),
                    remaining
                )),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_primary),
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
                    // border_radius: RADIUS_SM,
                    ..default()
                },
                BackgroundColor(theme.colors.hp_bar_bg),
            ))
            .with_children(|bg| {
                bg.spawn((
                    QueuePanelItem,
                    Node {
                        width: Val::Percent(
                            queue
                                .timer
                                .as_ref()
                                .map_or(0.0, |timer| timer.fraction() * 100.0),
                        ),
                        height: Val::Percent(100.0),
                        // border_radius: RADIUS_SM,
                        ..default()
                    },
                    BackgroundColor(theme.colors.accent),
                ));
            })
            .id();
        commands.entity(current_row).add_child(progress_bg);
    } else {
        spawn_secondary_text(commands, row, "Idle", theme);
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
                        // border_radius: RADIUS_MD,
                        ..default()
                    },
                    BackgroundColor(theme.colors.bg_panel),
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
                            font_size: theme.typography.caption,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
                    ));
                    chip.spawn((
                        Text::new("x"),
                        TextFont {
                            font_size: theme.typography.tiny,
                            ..default()
                        },
                        TextColor(theme.colors.text_disabled),
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
                        font_size: theme.typography.caption,
                        ..default()
                    },
                    TextColor(theme.colors.text_disabled),
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
            format!(
                "Attack {}",
                format_target(target, kind_lookup, resource_nodes)
            )
        }
        UnitState::Gathering(target) => {
            format!(
                "Gather {}",
                format_target(target, kind_lookup, resource_nodes)
            )
        }
        UnitState::ReturningToDeposit { .. } => "Return to deposit".to_string(),
        UnitState::Depositing { .. } => "Deposit resources".to_string(),
        UnitState::WaitingForStorage { .. } => "Waiting for storage".to_string(),
        UnitState::WaitingForDepot { .. } => "Waiting for depot".to_string(),
        UnitState::MovingToPlot(pos) => format!("Plot building at {}", format_position(pos)),
        UnitState::MovingToBuild(target) => {
            format!(
                "Move to build {}",
                format_target(target, kind_lookup, resource_nodes)
            )
        }
        UnitState::Building(target) => {
            format!(
                "Build {}",
                format_target(target, kind_lookup, resource_nodes)
            )
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
            format!(
                "Attack {}",
                format_target(*target, kind_lookup, resource_nodes)
            )
        }
        QueuedTask::Gather(target) => {
            format!(
                "Gather {}",
                format_target(*target, kind_lookup, resource_nodes)
            )
        }
        QueuedTask::Build(target) => {
            format!(
                "Build {}",
                format_target(*target, kind_lookup, resource_nodes)
            )
        }
        QueuedTask::Patrol(pos) => format!("Patrol {}", format_position(*pos)),
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
            .map(|entry| {
                (
                    format_task(&entry.task, kind_lookup, resource_nodes),
                    entry.id,
                )
            })
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
        commands.entity(row.0).try_insert(Selected);
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
    time: Res<Time>,
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
                clear_combat_intent(&mut commands, button.unit, time.elapsed_secs_f64());
                commands
                    .entity(button.unit)
                    .remove::<MoveTarget>()
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
