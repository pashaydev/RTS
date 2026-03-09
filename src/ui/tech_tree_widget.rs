use bevy::prelude::*;

use crate::blueprints::BlueprintRegistry;
use crate::components::*;
use crate::theme;

#[derive(Component)]
pub struct TechTreeContent;

pub fn update_tech_tree(
    mut commands: Commands,
    active_player: Res<ActivePlayer>,
    icons: Res<IconAssets>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    existing: Query<Entity, With<TechTreeContent>>,
    all_completed: Res<AllCompletedBuildings>,
    blueprint_reg: Res<BlueprintRegistry>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::TechTree) {
        return;
    }

    // Find content entity
    let mut content_entity = None;
    for (widget, widget_children) in &widget_q {
        if widget.id == WidgetId::TechTree {
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

    let completed = all_completed.completed_for(&active_player.0);
    let building_kinds = blueprint_reg.building_kinds();

    let container = commands
        .spawn((
            TechTreeContent,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(container);

    // Build dependency tree: Base is root, others depend on their prerequisite
    // Group by depth level
    for kind in &building_kinds {
        let bp = blueprint_reg.get(*kind);
        let building_data = bp.building.as_ref();

        let is_built = completed.contains(kind);
        let prereq = building_data.and_then(|b| b.prerequisite);
        let prereq_met = prereq.map_or(true, |p| completed.contains(&p));

        let (border_color, text_color) = if is_built {
            (theme::SUCCESS, theme::TEXT_PRIMARY)
        } else if prereq_met {
            (theme::WARNING, theme::TEXT_PRIMARY)
        } else {
            (theme::TEXT_DISABLED, theme::TEXT_DISABLED)
        };

        // Indent based on prerequisite depth
        let indent = if prereq.is_none() { 0.0 } else { 16.0 };

        let row = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                margin: UiRect::left(Val::Px(indent)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            })
            .id();
        commands.entity(container).add_child(row);

        // Status indicator
        let status_dot = commands
            .spawn((
                Node {
                    width: Val::Px(6.0),
                    height: Val::Px(6.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(border_color),
            ))
            .id();
        commands.entity(row).add_child(status_dot);

        // Icon
        let icon = commands
            .spawn((
                ImageNode::new(icons.entity_icon(*kind)),
                Node {
                    width: Val::Px(20.0),
                    height: Val::Px(20.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(row).add_child(icon);

        // Name
        let name = commands
            .spawn((
                Text::new(kind.display_name()),
                TextFont { font_size: 9.0, ..default() },
                TextColor(text_color),
            ))
            .id();
        commands.entity(row).add_child(name);

        // Show what it trains
        if let Some(bd) = building_data {
            if !bd.trains.is_empty() {
                let trains_row = commands
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(1.0),
                        margin: UiRect::left(Val::Px(4.0)),
                        ..default()
                    })
                    .id();
                commands.entity(row).add_child(trains_row);

                for train_kind in &bd.trains {
                    let train_icon = commands
                        .spawn((
                            ImageNode::new(icons.entity_icon(*train_kind)),
                            Node {
                                width: Val::Px(12.0),
                                height: Val::Px(12.0),
                                ..default()
                            },
                        ))
                        .id();
                    commands.entity(trains_row).add_child(train_icon);
                }
            }
        }
    }

    // Legend
    let legend = commands
        .spawn((
            TechTreeContent,
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                margin: UiRect::top(Val::Px(4.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(legend);

    for (color, label) in [
        (theme::SUCCESS, "Built"),
        (theme::WARNING, "Available"),
        (theme::TEXT_DISABLED, "Locked"),
    ] {
        let item = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                ..default()
            })
            .id();
        commands.entity(legend).add_child(item);

        let dot = commands
            .spawn((
                Node {
                    width: Val::Px(6.0),
                    height: Val::Px(6.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(color),
            ))
            .id();
        commands.entity(item).add_child(dot);

        let text = commands
            .spawn((
                Text::new(label),
                TextFont { font_size: 8.0, ..default() },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(item).add_child(text);
    }
}
