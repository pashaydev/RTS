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
    units: Query<(&EntityKind, &Faction, Option<&UnitState>), With<Unit>>,
    training_queues: Query<(&Faction, &TrainingQueue), With<Building>>,
    buildings: Query<(&Faction, &EntityKind, &BuildingState, &BuildingLevel), With<Building>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::ArmyOverview) {
        return;
    }

    let Some(content) =
        super::widget_framework::find_widget_content(WidgetId::ArmyOverview, &widget_q, &content_q)
    else {
        return;
    };

    // Clear existing
    for entity in &existing {
        commands.entity(entity).try_despawn();
    }

    // Count units by type
    let mut counts: Vec<(EntityKind, u32, u32)> = Vec::new(); // kind, total, idle_workers
    for (kind, faction, unit_state) in &units {
        if *faction != active_player.0 {
            continue;
        }
        if let Some(entry) = counts.iter_mut().find(|(k, _, _)| *k == *kind) {
            entry.1 += 1;
            if *kind == EntityKind::Worker {
                if let Some(state) = unit_state {
                    if *state == UnitState::Idle {
                        entry.2 += 1;
                    }
                }
            }
        } else {
            let idle = if *kind == EntityKind::Worker {
                unit_state.map_or(0, |s| if *s == UnitState::Idle { 1 } else { 0 })
            } else {
                0
            };
            counts.push((*kind, 1, idle));
        }
    }

    let total: u32 = counts.iter().map(|(_, c, _)| c).sum();
    let unit_cap = faction_unit_cap_stats(
        active_player.0,
        units.iter().map(|(_, faction, _)| faction),
        training_queues.iter(),
        buildings.iter(),
    );

    let container = commands
        .spawn((
            ArmyOverviewContent,
            Node {
                width: Val::Percent(100.0),
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
                TextFont {
                    font_size: theme::FONT_CAPTION,
                    ..default()
                },
                TextColor(theme::TEXT_PRIMARY),
            ))
            .id();
        commands.entity(entry).add_child(count_text);

        if *idle > 0 {
            let idle_badge = commands
                .spawn((
                    Text::new(format!("({})", idle)),
                    TextFont {
                        font_size: theme::FONT_TINY,
                        ..default()
                    },
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
            Text::new(format!("Total: {} / {}", total, unit_cap.cap)),
            TextFont {
                font_size: theme::FONT_CAPTION,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::top(Val::Px(2.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(total_text);
}
