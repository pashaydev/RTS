use bevy::prelude::*;

use super::core::fonts::UiFonts;
use super::core::framework::{spawn_widget_frame, WidgetId, WidgetRegistry};
use super::core::hud::MainHudRoot;
use super::core::shared::{hp_color, spawn_hp_bar};
use super::group_hotkeys_widget::{group_color, ControlGroups};
use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme;

pub struct SelectionWidgetPlugin;

impl Plugin for SelectionWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_selection_widget
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(
            Update,
            (
                rebuild_selection_panel,
                update_hp_bars,
                handle_unit_card_click,
                clear_stale_inspected,
            )
                .after(super::core::hud::compute_ui_mode)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn spawn_selection_widget(
    mut commands: Commands,
    registry: Res<WidgetRegistry>,
    fonts: Res<UiFonts>,
    root_q: Query<Entity, Added<MainHudRoot>>,
) {
    let Ok(hud_root) = root_q.single() else {
        return;
    };
    let selection_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Selection,
        registry.slots.get(&WidgetId::Selection).unwrap(),
        registry.is_visible(WidgetId::Selection),
        &fonts,
    );
    commands
        .entity(selection_content)
        .insert(SelectionInfoPanel);
}

pub fn handle_unit_card_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &UnitCardRef), (Changed<Interaction>, With<Button>)>,
    selected: Query<Entity, With<Selected>>,
    mut ui_press: ResMut<UiPressActive>,
    keys: Res<ButtonInput<KeyCode>>,
    entity_kinds: Query<&EntityKind, With<Unit>>,
) {
    for (interaction, card_ref) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_press.0 = true;

        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

        if ctrl {
            if let Ok(clicked_kind) = entity_kinds.get(card_ref.0) {
                let target_kind = *clicked_kind;
                let mut to_deselect = Vec::new();
                let mut has_target = false;
                for entity in &selected {
                    if let Ok(kind) = entity_kinds.get(entity) {
                        if *kind == target_kind {
                            has_target = true;
                        } else {
                            to_deselect.push(entity);
                        }
                    }
                }
                if has_target {
                    for entity in to_deselect {
                        commands.entity(entity).remove::<Selected>();
                    }
                }
            }
        } else {
            for entity in &selected {
                commands.entity(entity).remove::<Selected>();
            }
            commands.entity(card_ref.0).try_insert(Selected);
        }
    }
}

pub fn update_hp_bars(
    mut hp_fills: Query<(&HpBarFill, &mut Node, &mut BackgroundColor)>,
    healths: Query<&Health>,
) {
    for (hp_bar, mut node, mut bg) in &mut hp_fills {
        if let Ok(health) = healths.get(hp_bar.0) {
            let pct = (health.current / health.max).clamp(0.0, 1.0) * 100.0;
            node.width = Val::Percent(pct);
            *bg = BackgroundColor(hp_color(health.current, health.max));
        }
    }
}

pub fn clear_stale_inspected(
    mut inspected: ResMut<InspectedEnemy>,
    mob_query: Query<Entity, With<Mob>>,
    unit_query: Query<Entity, With<Unit>>,
    building_query: Query<Entity, With<Building>>,
) {
    if let Some(e) = inspected.entity {
        let exists =
            mob_query.get(e).is_ok() || unit_query.get(e).is_ok() || building_query.get(e).is_ok();
        if !exists {
            inspected.entity = None;
        }
    }
}

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
        (
            Entity,
            &EntityKind,
            Option<&UnitDisplayName>,
            &Health,
            &AttackDamage,
            &AttackRange,
            &UnitSpeed,
            Option<&UnitStance>,
        ),
        (With<Unit>, With<Selected>),
    >,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingState, &Health),
        (With<Building>, With<Selected>),
    >,
    mob_query: Query<
        (
            &EntityKind,
            &Health,
            &AttackDamage,
            &AttackRange,
            &UnitSpeed,
            &AggroRange,
            Has<Boss>,
        ),
        With<Mob>,
    >,
    faction_q: Query<&Faction>,
    inspected_unit_q: Query<
        (
            &EntityKind,
            Option<&UnitDisplayName>,
            &Health,
            &AttackDamage,
            &AttackRange,
            &UnitSpeed,
        ),
        With<Unit>,
    >,
    inspected_building_q: Query<(&EntityKind, &BuildingState, &Health), With<Building>>,
    control_groups: Res<ControlGroups>,
) {
    let Ok(panel_entity) = panel_q.single() else {
        return;
    };

    let has_inspected = inspected.entity.map_or(false, |e| {
        mob_query.get(e).is_ok()
            || inspected_unit_q.get(e).is_ok()
            || inspected_building_q.get(e).is_ok()
    });

    let should_show = matches!(
        *ui_mode,
        UiMode::SelectedUnits(_) | UiMode::SelectedBuilding(_)
    ) || has_inspected;

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

    let has_selection = matches!(
        *ui_mode,
        UiMode::SelectedUnits(_) | UiMode::SelectedBuilding(_)
    );

    match &*ui_mode {
        UiMode::SelectedUnits(entities) if entities.len() == 1 => {
            if let Some((entity, kind, display_name, health, dmg, rng, spd, stance)) =
                selected_units.iter().next()
            {
                spawn_friendly_detail_card(
                    &mut commands,
                    panel_entity,
                    entity,
                    display_name.map(|name| name.0.as_str()),
                    *kind,
                    health,
                    dmg,
                    rng,
                    spd,
                    stance.copied(),
                    &icons,
                );
            }
        }
        UiMode::SelectedBuilding(_) => {
            if let Some((entity, kind, state, health)) = selected_buildings.iter().next() {
                spawn_building_detail_card(
                    &mut commands,
                    panel_entity,
                    entity,
                    *kind,
                    *state,
                    health,
                    &icons,
                );
            }
        }
        UiMode::SelectedUnits(entities) if entities.len() > 1 => {
            let grid_container = commands
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    overflow: Overflow::scroll_y(),
                    max_height: Val::Percent(100.0),
                    ..default()
                })
                .id();
            commands.entity(panel_entity).add_child(grid_container);

            let mut unit_groups: Vec<(
                EntityKind,
                Vec<(Entity, Option<&UnitDisplayName>, &Health)>,
            )> = Vec::new();
            for (entity, kind, display_name, health, _, _, _, _) in &selected_units {
                if let Some(group) = unit_groups.iter_mut().find(|(k, _)| *k == *kind) {
                    group.1.push((entity, display_name, health));
                } else {
                    unit_groups.push((*kind, vec![(entity, display_name, health)]));
                }
            }
            for (_, entities) in &mut unit_groups {
                entities.sort_by(|a, b| {
                    let a_name = a.1.map_or("", |name| name.0.as_str());
                    let b_name = b.1.map_or("", |name| name.0.as_str());
                    a_name.cmp(&b_name)
                });
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
                let header = commands
                    .spawn((
                        Text::new(format!("{} ({})", kind.display_name(), entities.len())),
                        TextFont {
                            font_size: theme::FONT_SMALL,
                            ..default()
                        },
                        TextColor(theme::TEXT_SECONDARY),
                        Node {
                            margin: UiRect::bottom(Val::Px(1.0)),
                            ..default()
                        },
                    ))
                    .id();
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

                for (entity, display_name, health) in entities {
                    spawn_unit_mini_card(
                        &mut commands,
                        grid,
                        *entity,
                        display_name.map(|name| name.0.as_str()),
                        *kind,
                        health,
                        &icons,
                        &control_groups,
                    );
                }
            }

            for (kind, entities) in &building_groups {
                let header = commands
                    .spawn((
                        Text::new(format!("{} ({})", kind.display_name(), entities.len())),
                        TextFont {
                            font_size: theme::FONT_SMALL,
                            ..default()
                        },
                        TextColor(theme::TEXT_SECONDARY),
                        Node {
                            margin: UiRect::bottom(Val::Px(1.0)),
                            ..default()
                        },
                    ))
                    .id();
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
                    spawn_unit_mini_card(
                        &mut commands,
                        grid,
                        *entity,
                        None,
                        *kind,
                        health,
                        &icons,
                        &control_groups,
                    );
                }
            }
        }
        _ => {}
    }

    // Inspect section (mobs, enemy/allied player entities)
    if let Some(inspected_entity) = inspected.entity {
        let relationship = faction_q
            .get(inspected_entity)
            .map(|f| {
                if teams.is_allied(&active_player.0, f) {
                    "Allied"
                } else {
                    "Enemy"
                }
            })
            .unwrap_or("Neutral");
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
                &mut commands,
                panel_entity,
                inspected_entity,
                *kind,
                is_boss,
                health,
                dmg,
                rng,
                spd,
                aggro,
                &icons,
            );
        } else if let Ok((kind, display_name, health, dmg, rng, spd)) =
            inspected_unit_q.get(inspected_entity)
        {
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
            spawn_friendly_detail_card(
                &mut commands,
                panel_entity,
                inspected_entity,
                display_name.map(|name| name.0.as_str()),
                *kind,
                health,
                dmg,
                rng,
                spd,
                None,
                &icons,
            );
            let label = commands
                .spawn((
                    Text::new(relationship),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(relationship_color),
                ))
                .id();
            commands.entity(panel_entity).add_child(label);
        } else if let Ok((kind, state, health)) = inspected_building_q.get(inspected_entity) {
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
            spawn_building_detail_card(
                &mut commands,
                panel_entity,
                inspected_entity,
                *kind,
                *state,
                health,
                &icons,
            );
            let label = commands
                .spawn((
                    Text::new(relationship),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(relationship_color),
                ))
                .id();
            commands.entity(panel_entity).add_child(label);
        }
    }
}

pub fn spawn_friendly_detail_card(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    display_name: Option<&str>,
    kind: EntityKind,
    health: &Health,
    damage: &AttackDamage,
    range: &AttackRange,
    speed: &UnitSpeed,
    stance: Option<UnitStance>,
    icons: &IconAssets,
) {
    let card = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                min_width: Val::Px(0.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                padding: UiRect {
                    left: Val::Px(12.0),
                    right: Val::Px(10.0),
                    top: Val::Px(8.0),
                    bottom: Val::Px(8.0),
                },
                column_gap: Val::Px(10.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
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
            Text::new(display_name.unwrap_or(kind.display_name())),
            TextFont {
                font_size: theme::FONT_LARGE,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(info).add_child(name);

    if display_name.is_some() {
        let unit_type = commands
            .spawn((
                Text::new(kind.display_name()),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(info).add_child(unit_type);
    }

    spawn_hp_bar(commands, info, entity, health, 160.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    let stats = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(10.0),
            row_gap: Val::Px(2.0),
            ..default()
        })
        .id();
    commands.entity(info).add_child(stats);

    let stat_colors = [theme::STAT_DMG, theme::STAT_RNG, theme::STAT_SPD];
    for ((label, value), color) in [
        ("DMG", format!("{:.0}", damage.0)),
        ("RNG", format!("{:.1}", range.0)),
        ("SPD", format!("{:.1}", speed.0)),
    ]
    .iter()
    .zip(stat_colors.iter())
    {
        let stat = commands
            .spawn((
                Text::new(format!("{}: {}", label, value)),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(*color),
            ))
            .id();
        commands.entity(stats).add_child(stat);
    }

    // Stance indicator
    if let Some(stance) = stance {
        let (stance_text, stance_color) = match stance {
            UnitStance::Passive => ("Passive", Color::srgb(0.5, 0.5, 0.8)),
            UnitStance::Defensive => ("Defensive", Color::srgb(0.3, 0.7, 0.3)),
            UnitStance::Aggressive => ("Aggressive", Color::srgb(0.9, 0.3, 0.2)),
        };
        let stance_label = commands
            .spawn((
                Text::new(format!("[{}] (V)", stance_text)),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(stance_color),
            ))
            .id();
        commands.entity(info).add_child(stance_label);
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
                width: Val::Percent(100.0),
                min_width: Val::Px(0.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                padding: UiRect {
                    left: Val::Px(12.0),
                    right: Val::Px(10.0),
                    top: Val::Px(8.0),
                    bottom: Val::Px(8.0),
                },
                column_gap: Val::Px(10.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
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
            TextFont {
                font_size: theme::FONT_LARGE,
                ..default()
            },
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
                width: Val::Percent(100.0),
                min_width: Val::Px(0.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                padding: UiRect {
                    left: Val::Px(12.0),
                    right: Val::Px(10.0),
                    top: Val::Px(8.0),
                    bottom: Val::Px(8.0),
                },
                column_gap: Val::Px(10.0),
                border: UiRect {
                    left: Val::Px(1.0),
                    top: Val::Px(1.0),
                    right: Val::Px(1.0),
                    bottom: Val::Px(1.0),
                },
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
            TextFont {
                font_size: theme::FONT_LARGE,
                ..default()
            },
            TextColor(theme::WARNING),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 160.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    let stats = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(10.0),
            row_gap: Val::Px(2.0),
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
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
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
    display_name: Option<&str>,
    kind: EntityKind,
    health: &Health,
    icons: &IconAssets,
    control_groups: &ControlGroups,
) {
    let groups = control_groups.groups_for_entity(entity);

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
                min_width: Val::Px(56.0),
                flex_grow: 1.0,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                position_type: PositionType::Relative,
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

    let name = commands
        .spawn((
            Text::new(display_name.unwrap_or(kind.display_name())),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
            Node {
                max_width: Val::Px(72.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(name);

    if display_name.is_some() {
        let unit_type = commands
            .spawn((
                Text::new(kind.display_name()),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(card).add_child(unit_type);
    }

    // Group membership badge(s) — small colored numbers in top-right corner
    if !groups.is_empty() {
        let badge_row = commands
            .spawn(Node {
                position_type: PositionType::Absolute,
                top: Val::Px(-1.0),
                right: Val::Px(-1.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(1.0),
                ..default()
            })
            .id();
        commands.entity(card).add_child(badge_row);

        for &gi in &groups {
            let badge = commands
                .spawn((
                    Text::new(format!("{}", gi + 1)),
                    TextFont {
                        font_size: 7.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.0, 0.0, 0.0)),
                    Node {
                        padding: UiRect::axes(Val::Px(2.0), Val::Px(0.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(group_color(gi)),
                ))
                .id();
            commands.entity(badge_row).add_child(badge);
        }
    }
}
