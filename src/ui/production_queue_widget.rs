use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme;

#[derive(Component)]
pub struct GlobalQueueRow(pub Entity);

pub fn update_production_queue(
    mut commands: Commands,
    active_player: Res<ActivePlayer>,
    icons: Res<IconAssets>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    buildings: Query<
        (Entity, &EntityKind, &TrainingQueue, &Faction),
        With<Building>,
    >,
    existing_rows: Query<Entity, With<GlobalQueueRow>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::ProductionQueue) {
        return;
    }

    let Some(content) = super::widget_framework::find_widget_content(WidgetId::ProductionQueue, &widget_q, &content_q) else { return; };

    // Clear existing rows
    for row in &existing_rows {
        commands.entity(row).try_despawn();
    }

    // Find all buildings with active queues for this player
    let mut queue_buildings: Vec<(Entity, EntityKind, &TrainingQueue)> = Vec::new();
    for (entity, kind, queue, faction) in &buildings {
        if *faction != active_player.0 {
            continue;
        }
        if queue.queue.is_empty() && queue.timer.is_none() {
            continue;
        }
        queue_buildings.push((entity, *kind, queue));
    }

    if queue_buildings.is_empty() {
        let empty_label = commands
            .spawn((
                GlobalQueueRow(Entity::PLACEHOLDER),
                Text::new("No active queues"),
                TextFont { font_size: theme::FONT_SMALL, ..default() },
                TextColor(theme::TEXT_DISABLED),
            ))
            .id();
        commands.entity(content).add_child(empty_label);
        return;
    }

    for (building_entity, kind, queue) in &queue_buildings {
        let row = commands
            .spawn((
                GlobalQueueRow(*building_entity),
                Button,
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .id();
        commands.entity(content).add_child(row);

        // Building icon
        let icon = commands
            .spawn((
                ImageNode::new(icons.entity_icon(*kind)),
                Node {
                    width: Val::Px(24.0),
                    height: Val::Px(24.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(row).add_child(icon);

        // Queued unit icons
        let icons_row = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(2.0),
                ..default()
            })
            .id();
        commands.entity(row).add_child(icons_row);

        for (i, unit_kind) in queue.queue.iter().enumerate().take(5) {
            let size = if i == 0 { 20.0 } else { 16.0 };
            let unit_icon = commands
                .spawn((
                    ImageNode::new(icons.entity_icon(*unit_kind)),
                    Node {
                        width: Val::Px(size),
                        height: Val::Px(size),
                        ..default()
                    },
                ))
                .id();
            commands.entity(icons_row).add_child(unit_icon);
        }
        if queue.queue.len() > 5 {
            let more = commands
                .spawn((
                    Text::new(format!("+{}", queue.queue.len() - 5)),
                    TextFont { font_size: theme::FONT_CAPTION, ..default() },
                    TextColor(theme::TEXT_SECONDARY),
                ))
                .id();
            commands.entity(icons_row).add_child(more);
        }

        // Progress bar
        if let Some(timer) = &queue.timer {
            let fraction = timer.fraction();
            let remaining = timer.remaining_secs();

            let bar_bg = commands
                .spawn((
                    Node {
                        width: Val::Px(50.0),
                        height: Val::Px(4.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(theme::HP_BAR_BG),
                ))
                .with_children(|bg| {
                    bg.spawn((
                        Node {
                            width: Val::Percent(fraction * 100.0),
                            height: Val::Percent(100.0),
                            border_radius: BorderRadius::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(theme::ACCENT),
                    ));
                })
                .id();
            commands.entity(row).add_child(bar_bg);

            let time_text = commands
                .spawn((
                    Text::new(format!("{:.0}s", remaining)),
                    TextFont { font_size: theme::FONT_CAPTION, ..default() },
                    TextColor(theme::TEXT_SECONDARY),
                ))
                .id();
            commands.entity(row).add_child(time_text);
        }
    }
}

pub fn handle_queue_row_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &GlobalQueueRow), Changed<Interaction>>,
    selected: Query<Entity, With<Selected>>,
    buildings: Query<Entity, With<Building>>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, row) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if row.0 == Entity::PLACEHOLDER {
            continue;
        }
        if buildings.get(row.0).is_err() {
            continue;
        }
        ui_press.0 = true;
        for entity in &selected {
            commands.entity(entity).remove::<Selected>();
        }
        commands.entity(row.0).insert(Selected);
    }
}
