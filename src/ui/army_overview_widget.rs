use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme;

#[derive(Component)]
pub struct ArmyOverviewContent;

pub fn update_army_overview(
    mut commands: Commands,
    active_player: Res<ActivePlayer>,
    icons: Res<IconAssets>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    existing: Query<Entity, With<ArmyOverviewContent>>,
    units: Query<(&EntityKind, &Faction, Option<&WorkerTask>), With<Unit>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::ArmyOverview) {
        return;
    }

    // Find the army overview content entity
    let mut content_entity = None;
    for (widget, widget_children) in &widget_q {
        if widget.id == WidgetId::ArmyOverview {
            for wchild in widget_children.iter() {
                if content_q.get(wchild).is_ok() {
                    content_entity = Some(wchild);
                }
            }
        }
    }
    let Some(content) = content_entity else { return; };

    // Clear existing
    for entity in &existing {
        commands.entity(entity).try_despawn();
    }

    // Count units by type
    let mut counts: Vec<(EntityKind, u32, u32)> = Vec::new(); // kind, total, idle_workers
    for (kind, faction, worker_task) in &units {
        if *faction != active_player.0 {
            continue;
        }
        if let Some(entry) = counts.iter_mut().find(|(k, _, _)| *k == *kind) {
            entry.1 += 1;
            if *kind == EntityKind::Worker {
                if let Some(task) = worker_task {
                    if *task == WorkerTask::Idle {
                        entry.2 += 1;
                    }
                }
            }
        } else {
            let idle = if *kind == EntityKind::Worker {
                worker_task.map_or(0, |t| if *t == WorkerTask::Idle { 1 } else { 0 })
            } else {
                0
            };
            counts.push((*kind, 1, idle));
        }
    }

    let total: u32 = counts.iter().map(|(_, c, _)| c).sum();

    let container = commands
        .spawn((
            ArmyOverviewContent,
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(container);

    for (kind, count, idle) in &counts {
        let entry = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                ..default()
            })
            .id();
        commands.entity(container).add_child(entry);

        let icon = commands
            .spawn((
                ImageNode::new(icons.entity_icon(*kind)),
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(entry).add_child(icon);

        let count_text = commands
            .spawn((
                Text::new(format!("x{}", count)),
                TextFont { font_size: 9.0, ..default() },
                TextColor(theme::TEXT_PRIMARY),
            ))
            .id();
        commands.entity(entry).add_child(count_text);

        if *idle > 0 {
            let idle_badge = commands
                .spawn((
                    Text::new(format!("({})", idle)),
                    TextFont { font_size: 8.0, ..default() },
                    TextColor(theme::WARNING),
                ))
                .id();
            commands.entity(entry).add_child(idle_badge);
        }
    }

    // Total row
    let total_text = commands
        .spawn((
            ArmyOverviewContent,
            Text::new(format!("Total: {}", total)),
            TextFont { font_size: 9.0, ..default() },
            TextColor(theme::TEXT_SECONDARY),
            Node { margin: UiRect::top(Val::Px(2.0)), ..default() },
        ))
        .id();
    commands.entity(content).add_child(total_text);
}
