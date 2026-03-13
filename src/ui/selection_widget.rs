use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme;
use super::shared::spawn_hp_bar;

pub fn rebuild_selection_panel(
    mut commands: Commands,
    ui_mode: Res<UiMode>,
    inspected: Res<InspectedEnemy>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    icons: Res<IconAssets>,
    panel_q: Query<Entity, With<SelectionInfoPanel>>,
    children_q: Query<&Children>,
    selected_units: Query<
        (Entity, &EntityKind, &Health, &AttackDamage, &AttackRange, &UnitSpeed),
        (With<Unit>, With<Selected>),
    >,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingState, &Health),
        (With<Building>, With<Selected>),
    >,
    mob_query: Query<
        (&EntityKind, &Health, &AttackDamage, &AttackRange, &UnitSpeed, &AggroRange, Has<Boss>),
        With<Mob>,
    >,
    faction_q: Query<&Faction>,
    inspected_unit_q: Query<(&EntityKind, &Health, &AttackDamage, &AttackRange, &UnitSpeed), With<Unit>>,
    inspected_building_q: Query<(&EntityKind, &BuildingState, &Health), With<Building>>,
) {
    let Ok(panel_entity) = panel_q.single() else {
        return;
    };

    let has_inspected = inspected.entity.map_or(false, |e| {
        mob_query.get(e).is_ok() || inspected_unit_q.get(e).is_ok() || inspected_building_q.get(e).is_ok()
    });

    let should_show = matches!(*ui_mode, UiMode::SelectedUnits(_) | UiMode::SelectedBuilding(_)) || has_inspected;

    if !should_show {
        if let Ok(children) = children_q.get(panel_entity) {
            for child in children.iter() {
                commands.entity(child).try_despawn();
            }
        }
        return;
    }

    if !ui_mode.is_changed() && !inspected.is_changed() {
        return;
    }

    if let Ok(children) = children_q.get(panel_entity) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    let has_selection = matches!(*ui_mode, UiMode::SelectedUnits(_) | UiMode::SelectedBuilding(_));

    match &*ui_mode {
        UiMode::SelectedUnits(entities) if entities.len() == 1 => {
            if let Some((entity, kind, health, dmg, rng, spd)) = selected_units.iter().next() {
                spawn_friendly_detail_card(&mut commands, panel_entity, entity, *kind, health, dmg, rng, spd, &icons);
            }
        }
        UiMode::SelectedBuilding(_) => {
            if let Some((entity, kind, state, health)) = selected_buildings.iter().next() {
                spawn_building_detail_card(&mut commands, panel_entity, entity, *kind, *state, health, &icons);
            }
        }
        UiMode::SelectedUnits(entities) if entities.len() > 1 => {
            let grid_container = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    max_width: Val::Px(480.0),
                    overflow: Overflow::scroll_y(),
                    max_height: Val::Px(140.0),
                    ..default()
                })
                .id();
            commands.entity(panel_entity).add_child(grid_container);

            let mut unit_groups: Vec<(EntityKind, Vec<(Entity, &Health)>)> = Vec::new();
            for (entity, kind, health, _, _, _) in &selected_units {
                if let Some(group) = unit_groups.iter_mut().find(|(k, _)| *k == *kind) {
                    group.1.push((entity, health));
                } else {
                    unit_groups.push((*kind, vec![(entity, health)]));
                }
            }
            let mut building_groups: Vec<(EntityKind, Vec<(Entity, &Health)>)> = Vec::new();
            for (entity, kind, _state, health) in &selected_buildings {
                if let Some(group) = building_groups.iter_mut().find(|(k, _)| *k == *kind) {
                    group.1.push((entity, health));
                } else {
                    building_groups.push((*kind, vec![(entity, health)]));
                }
            }

            for (kind, entities) in &unit_groups {
                let header = commands.spawn((
                    Text::new(format!("{} ({})", kind.display_name(), entities.len())),
                    TextFont { font_size: theme::FONT_SMALL, ..default() },
                    TextColor(theme::TEXT_SECONDARY),
                    Node { margin: UiRect::bottom(Val::Px(1.0)), ..default() },
                )).id();
                commands.entity(grid_container).add_child(header);

                let grid = commands
                    .spawn((
                        UnitCardGrid,
                        Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(3.0),
                            row_gap: Val::Px(3.0),
                            ..default()
                        },
                    ))
                    .id();
                commands.entity(grid_container).add_child(grid);

                for (entity, health) in entities {
                    spawn_unit_mini_card(&mut commands, grid, *entity, *kind, health, &icons);
                }
            }

            for (kind, entities) in &building_groups {
                let header = commands.spawn((
                    Text::new(format!("{} ({})", kind.display_name(), entities.len())),
                    TextFont { font_size: theme::FONT_SMALL, ..default() },
                    TextColor(theme::TEXT_SECONDARY),
                    Node { margin: UiRect::bottom(Val::Px(1.0)), ..default() },
                )).id();
                commands.entity(grid_container).add_child(header);

                let grid = commands
                    .spawn((
                        UnitCardGrid,
                        Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(3.0),
                            row_gap: Val::Px(3.0),
                            ..default()
                        },
                    ))
                    .id();
                commands.entity(grid_container).add_child(grid);

                for (entity, health) in entities {
                    spawn_unit_mini_card(&mut commands, grid, *entity, *kind, health, &icons);
                }
            }
        }
        _ => {}
    }

    // Inspect section (mobs, enemy/allied player entities)
    if let Some(inspected_entity) = inspected.entity {
        let relationship = faction_q.get(inspected_entity).map(|f| {
            if teams.is_allied(&active_player.0, f) { "Allied" } else { "Enemy" }
        }).unwrap_or("Neutral");
        let relationship_color = if relationship == "Allied" {
            Color::srgb(0.3, 0.8, 0.3)
        } else {
            Color::srgb(1.0, 0.3, 0.3)
        };

        if let Ok((kind, health, dmg, rng, spd, aggro, is_boss)) = mob_query.get(inspected_entity) {
            if has_selection {
                let divider = commands
                    .spawn((
                        Node {
                            width: Val::Px(1.0),
                            height: Val::Px(50.0),
                            margin: UiRect::axes(Val::Px(6.0), Val::Px(0.0)),
                            ..default()
                        },
                        BackgroundColor(theme::SEPARATOR),
                    ))
                    .id();
                commands.entity(panel_entity).add_child(divider);
            }

            spawn_enemy_detail_card(
                &mut commands, panel_entity, inspected_entity,
                *kind, is_boss, health, dmg, rng, spd, aggro, &icons,
            );
        } else if let Ok((kind, health, dmg, rng, spd)) = inspected_unit_q.get(inspected_entity) {
            if has_selection {
                let divider = commands.spawn((
                    Node { width: Val::Px(1.0), height: Val::Px(50.0), margin: UiRect::axes(Val::Px(6.0), Val::Px(0.0)), ..default() },
                    BackgroundColor(theme::SEPARATOR),
                )).id();
                commands.entity(panel_entity).add_child(divider);
            }
            spawn_friendly_detail_card(&mut commands, panel_entity, inspected_entity, *kind, health, dmg, rng, spd, &icons);
            let label = commands.spawn((
                Text::new(relationship),
                TextFont { font_size: theme::FONT_BODY, ..default() },
                TextColor(relationship_color),
            )).id();
            commands.entity(panel_entity).add_child(label);
        } else if let Ok((kind, state, health)) = inspected_building_q.get(inspected_entity) {
            if has_selection {
                let divider = commands.spawn((
                    Node { width: Val::Px(1.0), height: Val::Px(50.0), margin: UiRect::axes(Val::Px(6.0), Val::Px(0.0)), ..default() },
                    BackgroundColor(theme::SEPARATOR),
                )).id();
                commands.entity(panel_entity).add_child(divider);
            }
            spawn_building_detail_card(&mut commands, panel_entity, inspected_entity, *kind, *state, health, &icons);
            let label = commands.spawn((
                Text::new(relationship),
                TextFont { font_size: theme::FONT_BODY, ..default() },
                TextColor(relationship_color),
            )).id();
            commands.entity(panel_entity).add_child(label);
        }
    }
}

pub fn spawn_friendly_detail_card(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    kind: EntityKind,
    health: &Health,
    damage: &AttackDamage,
    range: &AttackRange,
    speed: &UnitSpeed,
    icons: &IconAssets,
) {
    let card = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect { left: Val::Px(12.0), right: Val::Px(10.0), top: Val::Px(8.0), bottom: Val::Px(8.0) },
                column_gap: Val::Px(10.0),
                border: UiRect { left: Val::Px(3.0), ..default() },
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BorderColor::all(theme::PANEL_ACCENT_FRIENDLY),
        ))
        .id();
    commands.entity(parent).add_child(card);

    let icon_frame = commands
        .spawn((
            Node {
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(theme::ICON_FRAME_BG),
        ))
        .with_children(|frame| {
            frame.spawn((
                ImageNode::new(icons.entity_icon(kind)),
                Node {
                    width: Val::Px(44.0),
                    height: Val::Px(44.0),
                    ..default()
                },
            ));
        })
        .id();
    commands.entity(card).add_child(icon_frame);

    let info = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .id();
    commands.entity(card).add_child(info);

    let name = commands
        .spawn((
            Text::new(kind.display_name()),
            TextFont { font_size: theme::FONT_LARGE, ..default() },
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 160.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: theme::FONT_SMALL, ..default() },
            TextColor(theme::TEXT_SECONDARY),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    let stats = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(10.0),
            ..default()
        })
        .id();
    commands.entity(info).add_child(stats);

    let stat_colors = [theme::STAT_DMG, theme::STAT_RNG, theme::STAT_SPD];
    for ((label, value), color) in [
        ("DMG", format!("{:.0}", damage.0)),
        ("RNG", format!("{:.1}", range.0)),
        ("SPD", format!("{:.1}", speed.0)),
    ].iter().zip(stat_colors.iter()) {
        let stat = commands
            .spawn((
                Text::new(format!("{}: {}", label, value)),
                TextFont { font_size: theme::FONT_BODY, ..default() },
                TextColor(*color),
            ))
            .id();
        commands.entity(stats).add_child(stat);
    }
}

pub fn spawn_building_detail_card(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    kind: EntityKind,
    state: BuildingState,
    health: &Health,
    icons: &IconAssets,
) {
    let accent_color = match state {
        BuildingState::UnderConstruction => theme::PANEL_ACCENT_CONSTRUCTION,
        BuildingState::Complete => theme::PANEL_ACCENT_FRIENDLY,
    };
    let card = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect { left: Val::Px(12.0), right: Val::Px(10.0), top: Val::Px(8.0), bottom: Val::Px(8.0) },
                column_gap: Val::Px(10.0),
                border: UiRect { left: Val::Px(3.0), ..default() },
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BorderColor::all(accent_color),
        ))
        .id();
    commands.entity(parent).add_child(card);

    let icon_frame = commands
        .spawn((
            Node {
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(theme::ICON_FRAME_BG),
        ))
        .with_children(|frame| {
            frame.spawn((
                ImageNode::new(icons.entity_icon(kind)),
                Node {
                    width: Val::Px(44.0),
                    height: Val::Px(44.0),
                    ..default()
                },
            ));
        })
        .id();
    commands.entity(card).add_child(icon_frame);

    let info = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .id();
    commands.entity(card).add_child(info);

    let state_str = match state {
        BuildingState::UnderConstruction => " (building...)",
        BuildingState::Complete => "",
    };
    let name_color = match state {
        BuildingState::UnderConstruction => theme::WARNING,
        BuildingState::Complete => theme::TEXT_PRIMARY,
    };
    let name = commands
        .spawn((
            Text::new(format!("{}{}", kind.display_name(), state_str)),
            TextFont { font_size: theme::FONT_LARGE, ..default() },
            TextColor(name_color),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 160.0);
}

fn spawn_enemy_detail_card(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    kind: EntityKind,
    is_boss: bool,
    health: &Health,
    damage: &AttackDamage,
    range: &AttackRange,
    speed: &UnitSpeed,
    aggro: &AggroRange,
    icons: &IconAssets,
) {
    let card = commands
        .spawn((
            EnemyInspectPanel,
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect { left: Val::Px(12.0), right: Val::Px(10.0), top: Val::Px(8.0), bottom: Val::Px(8.0) },
                column_gap: Val::Px(10.0),
                border: UiRect { left: Val::Px(3.0), top: Val::Px(1.0), right: Val::Px(1.0), bottom: Val::Px(1.0) },
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BorderColor::all(theme::PANEL_ACCENT_ENEMY),
        ))
        .id();
    commands.entity(parent).add_child(card);

    let icon_frame = commands
        .spawn((
            Node {
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(theme::ICON_FRAME_BG),
        ))
        .with_children(|frame| {
            frame.spawn((
                ImageNode::new(icons.entity_icon(kind)),
                Node {
                    width: Val::Px(44.0),
                    height: Val::Px(44.0),
                    ..default()
                },
            ));
        })
        .id();
    commands.entity(card).add_child(icon_frame);

    let info = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .id();
    commands.entity(card).add_child(info);

    let name_str = if is_boss {
        format!("{} Boss", kind.display_name())
    } else {
        kind.display_name().to_string()
    };
    let name = commands
        .spawn((
            Text::new(name_str),
            TextFont { font_size: theme::FONT_LARGE, ..default() },
            TextColor(theme::WARNING),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 160.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: theme::FONT_SMALL, ..default() },
            TextColor(theme::TEXT_SECONDARY),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    let stats = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(10.0),
            ..default()
        })
        .id();
    commands.entity(info).add_child(stats);

    let stat_data = [
        ("DMG", format!("{:.0}", damage.0), theme::STAT_DMG),
        ("RNG", format!("{:.1}", range.0), theme::STAT_RNG),
        ("AGR", format!("{:.0}", aggro.0), theme::WARNING),
        ("SPD", format!("{:.1}", speed.0), theme::STAT_SPD),
    ];
    for (label, value, color) in &stat_data {
        let stat = commands
            .spawn((
                Text::new(format!("{}: {}", label, value)),
                TextFont { font_size: theme::FONT_BODY, ..default() },
                TextColor(*color),
            ))
            .id();
        commands.entity(stats).add_child(stat);
    }
}

fn spawn_unit_mini_card(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    kind: EntityKind,
    health: &Health,
    icons: &IconAssets,
) {
    let card = commands
        .spawn((
            UnitCardRef(entity),
            Button,
            StandardButton,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(4.0)),
                row_gap: Val::Px(2.0),
                width: Val::Px(62.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(theme::BG_SURFACE),
            BorderColor::all(Color::NONE),
        ))
        .id();
    commands.entity(parent).add_child(card);

    let icon = commands
        .spawn((
            ImageNode::new(icons.entity_icon(kind)),
            Node {
                width: Val::Px(30.0),
                height: Val::Px(30.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

    spawn_hp_bar(commands, card, entity, health, 54.0);
}
