use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::buildings;
use crate::components::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RallyPointMode>()
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (
                    update_resource_texts,
                    rebuild_selection_panel,
                    update_hp_bars,
                    handle_unit_card_click,
                    clear_stale_inspected,
                    update_action_bar,
                    handle_build_buttons,
                    handle_train_buttons,
                ),
            )
            .add_systems(
                Update,
                (
                    handle_upgrade_button,
                    handle_demolish_button,
                    handle_demolish_confirm,
                    handle_rally_point_button,
                    handle_toggle_auto_attack,
                    handle_cancel_train,
                    update_training_queue_display,
                    update_construction_progress_display,
                ),
            )
            .add_systems(
                Update,
                (
                    update_upgrade_progress_display,
                    button_hover_visual,
                    show_action_tooltips,
                ),
            )
            .add_systems(
                Update,
                (
                    card_deal_in_system,
                    card_hover_system,
                    card_play_out_system,
                    card_placement_mode_system,
                    card_spring_back_system,
                    card_anim_lerp_system,
                    update_card_states,
                    card_tooltip_system,
                    card_glow_system,
                ),
            );
    }
}

fn spawn_hud(mut commands: Commands, icons: Res<IconAssets>) {
    // ── Top-left widget — resource list ──
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(10.0),
            width: Val::Px(170.0),
            padding: UiRect::all(Val::Px(10.0)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: Val::Px(6.0),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        })
        .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)))
        .with_children(|panel| {
            let resource_types = [
                ResourceType::Wood,
                ResourceType::Copper,
                ResourceType::Iron,
                ResourceType::Gold,
                ResourceType::Oil,
            ];
            for rt in resource_types {
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|entry| {
                        entry.spawn((
                            ImageNode::new(icons.resource_icon(rt)),
                            Node {
                                width: Val::Px(18.0),
                                height: Val::Px(18.0),
                                ..default()
                            },
                        ));
                        entry.spawn((
                            ResourceText(rt),
                            Text::new(format!("{:?}: 0", rt)),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
            }
        });

    // ── Bottom panel — selection info bar ──
    commands.spawn((
        SelectionInfoPanel,
        Interaction::None,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            left: Val::Px(10.0),
            padding: UiRect::all(Val::Px(10.0)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            min_height: Val::Px(60.0),
            max_width: Val::Px(800.0),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.1, 0.85)),
        Visibility::Hidden,
    ));

    // ── Bottom panel — context-sensitive action bar ──
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|wrapper| {
            wrapper.spawn((
                ActionBarInner,
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexEnd,
                    padding: UiRect::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ));
        });
}

fn update_resource_texts(
    player_res: Res<PlayerResources>,
    mut text_q: Query<(&mut Text, &ResourceText)>,
) {
    for (mut text, rt_marker) in &mut text_q {
        let rt = rt_marker.0;
        let val = player_res.get(rt);
        **text = format!("{:?}: {}", rt, val);
    }
}

fn hp_color(current: f32, max: f32) -> Color {
    let pct = (current / max).clamp(0.0, 1.0);
    if pct > 0.6 {
        Color::srgb(0.2, 0.8, 0.2)
    } else if pct > 0.3 {
        Color::srgb(0.9, 0.8, 0.1)
    } else {
        Color::srgb(0.9, 0.15, 0.1)
    }
}

fn spawn_hp_bar(commands: &mut Commands, parent: Entity, tracked_entity: Entity, health: &Health, width: f32) {
    let pct = (health.current / health.max).clamp(0.0, 1.0) * 100.0;

    let bg = commands
        .spawn((
            Node {
                width: Val::Px(width),
                height: Val::Px(6.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
        ))
        .id();
    commands.entity(parent).add_child(bg);

    let fill = commands
        .spawn((
            HpBarFill(tracked_entity),
            Node {
                width: Val::Percent(pct),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(hp_color(health.current, health.max)),
        ))
        .id();
    commands.entity(bg).add_child(fill);
}

fn spawn_friendly_detail_card(
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
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(8.0)),
            column_gap: Val::Px(10.0),
            ..default()
        })
        .id();
    commands.entity(parent).add_child(card);

    let icon = commands
        .spawn((
            ImageNode::new(icons.entity_icon(kind)),
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

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
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 120.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: 10.0, ..default() },
            TextColor(Color::srgb(0.6, 0.6, 0.6)),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    let stats = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .id();
    commands.entity(info).add_child(stats);

    for (label, value) in [
        ("DMG", format!("{:.0}", damage.0)),
        ("RNG", format!("{:.1}", range.0)),
        ("SPD", format!("{:.1}", speed.0)),
    ] {
        let stat = commands
            .spawn((
                Text::new(format!("{}: {}", label, value)),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.65, 0.65, 0.65)),
            ))
            .id();
        commands.entity(stats).add_child(stat);
    }
}

fn spawn_building_detail_card(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    kind: EntityKind,
    state: BuildingState,
    health: &Health,
    icons: &IconAssets,
) {
    let card = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(8.0)),
            column_gap: Val::Px(10.0),
            ..default()
        })
        .id();
    commands.entity(parent).add_child(card);

    let icon = commands
        .spawn((
            ImageNode::new(icons.entity_icon(kind)),
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

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
        BuildingState::UnderConstruction => Color::srgb(0.8, 0.7, 0.3),
        BuildingState::Complete => Color::WHITE,
    };
    let name = commands
        .spawn((
            Text::new(format!("{}{}", kind.display_name(), state_str)),
            TextFont { font_size: 15.0, ..default() },
            TextColor(name_color),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 120.0);
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
                padding: UiRect::all(Val::Px(8.0)),
                column_gap: Val::Px(10.0),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.8, 0.3, 0.1)),
        ))
        .id();
    commands.entity(parent).add_child(card);

    let icon = commands
        .spawn((
            ImageNode::new(icons.entity_icon(kind)),
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

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
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::srgb(1.0, 0.6, 0.3)),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 120.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: 10.0, ..default() },
            TextColor(Color::srgb(0.6, 0.6, 0.6)),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    let stats = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .id();
    commands.entity(info).add_child(stats);

    for (label, value) in [
        ("DMG", format!("{:.0}", damage.0)),
        ("RNG", format!("{:.1}", range.0)),
        ("AGR", format!("{:.0}", aggro.0)),
        ("SPD", format!("{:.1}", speed.0)),
    ] {
        let stat = commands
            .spawn((
                Text::new(format!("{}: {}", label, value)),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.65, 0.65, 0.65)),
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
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(4.0)),
                row_gap: Val::Px(2.0),
                width: Val::Px(56.0),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.2, 0.8)),
        ))
        .id();
    commands.entity(parent).add_child(card);

    let icon = commands
        .spawn((
            ImageNode::new(icons.entity_icon(kind)),
            Node {
                width: Val::Px(26.0),
                height: Val::Px(26.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

    spawn_hp_bar(commands, card, entity, health, 48.0);

    let label = commands
        .spawn((
            Text::new(kind.display_name()),
            TextFont { font_size: 9.0, ..default() },
            TextColor(Color::srgb(0.75, 0.75, 0.75)),
        ))
        .id();
    commands.entity(card).add_child(label);
}

fn rebuild_selection_panel(
    mut commands: Commands,
    inspected: Res<InspectedEnemy>,
    icons: Res<IconAssets>,
    panel_q: Query<Entity, With<SelectionInfoPanel>>,
    children_q: Query<&Children>,
    added_selected: Query<Entity, Added<Selected>>,
    mut removed_selected: RemovedComponents<Selected>,
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
) {
    let has_new = !added_selected.is_empty();
    let has_removed = removed_selected.read().count() > 0;
    let inspected_changed = inspected.is_changed();

    if !has_new && !has_removed && !inspected_changed {
        return;
    }

    let Ok(panel_entity) = panel_q.single() else {
        return;
    };

    if let Ok(children) = children_q.get(panel_entity) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    let unit_count = selected_units.iter().count();
    let building_count = selected_buildings.iter().count();
    let has_inspected = inspected.entity.and_then(|e| mob_query.get(e).ok()).is_some();

    if unit_count == 0 && building_count == 0 && !has_inspected {
        commands.entity(panel_entity).insert(Visibility::Hidden);
        return;
    }
    commands.entity(panel_entity).insert(Visibility::Visible);

    if unit_count == 1 && building_count == 0 {
        let (entity, kind, health, dmg, rng, spd) = selected_units.iter().next().unwrap();
        spawn_friendly_detail_card(&mut commands, panel_entity, entity, *kind, health, dmg, rng, spd, &icons);
    } else if unit_count == 0 && building_count == 1 {
        let (entity, kind, state, health) = selected_buildings.iter().next().unwrap();
        spawn_building_detail_card(&mut commands, panel_entity, entity, *kind, *state, health, &icons);
    } else if unit_count + building_count > 1 {
        let grid = commands
            .spawn((
                UnitCardGrid,
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(4.0),
                    row_gap: Val::Px(4.0),
                    max_width: Val::Px(420.0),
                    overflow: Overflow::scroll_y(),
                    max_height: Val::Px(120.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(panel_entity).add_child(grid);

        for (entity, kind, health, _, _, _) in &selected_units {
            spawn_unit_mini_card(&mut commands, grid, entity, *kind, health, &icons);
        }
        for (entity, kind, _state, health) in &selected_buildings {
            let card = commands
                .spawn((
                    UnitCardRef(entity),
                    Button,
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(4.0)),
                        row_gap: Val::Px(2.0),
                        width: Val::Px(56.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.15, 0.15, 0.2, 0.8)),
                ))
                .id();
            commands.entity(grid).add_child(card);

            let icon = commands
                .spawn((
                    ImageNode::new(icons.entity_icon(*kind)),
                    Node {
                        width: Val::Px(26.0),
                        height: Val::Px(26.0),
                        ..default()
                    },
                ))
                .id();
            commands.entity(card).add_child(icon);

            spawn_hp_bar(&mut commands, card, entity, health, 48.0);

            let label = commands
                .spawn((
                    Text::new(kind.display_name()),
                    TextFont { font_size: 9.0, ..default() },
                    TextColor(Color::srgb(0.75, 0.75, 0.75)),
                ))
                .id();
            commands.entity(card).add_child(label);
        }
    }

    // Enemy inspect section
    if let Some(enemy_entity) = inspected.entity {
        if let Ok((kind, health, dmg, rng, spd, aggro, is_boss)) = mob_query.get(enemy_entity) {
            if unit_count + building_count > 0 {
                let divider = commands
                    .spawn((
                        Node {
                            width: Val::Px(2.0),
                            height: Val::Px(50.0),
                            margin: UiRect::axes(Val::Px(6.0), Val::Px(0.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.5, 0.5, 0.5, 0.4)),
                    ))
                    .id();
                commands.entity(panel_entity).add_child(divider);
            }

            spawn_enemy_detail_card(
                &mut commands, panel_entity, enemy_entity,
                *kind, is_boss, health, dmg, rng, spd, aggro, &icons,
            );
        }
    }
}

fn update_hp_bars(
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

fn handle_unit_card_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &UnitCardRef), (Changed<Interaction>, With<Button>)>,
    selected: Query<Entity, With<Selected>>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, card_ref) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_press.0 = true;
        for entity in &selected {
            commands.entity(entity).remove::<Selected>();
        }
        commands.entity(card_ref.0).insert(Selected);
    }
}

fn clear_stale_inspected(
    mut inspected: ResMut<InspectedEnemy>,
    mob_query: Query<Entity, With<Mob>>,
) {
    if let Some(e) = inspected.entity {
        if mob_query.get(e).is_err() {
            inspected.entity = None;
        }
    }
}

fn update_action_bar(
    mut commands: Commands,
    selected_units: Query<(&EntityKind, Option<&Carrying>, Option<&CarryCapacity>, Option<&WorkerTask>), (With<Unit>, With<Selected>)>,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingState, &BuildingLevel, Option<&UpgradeProgress>, Option<&ConstructionProgress>, Option<&TrainingQueue>, Option<&StorageInventory>, Option<&Health>, Option<&TowerAutoAttackEnabled>),
        (With<Building>, With<Selected>),
    >,
    completed: Res<CompletedBuildings>,
    registry: Res<BlueprintRegistry>,
    player_res: Res<PlayerResources>,
    action_bar: Query<(Entity, Option<&Children>), With<ActionBarInner>>,
    added_selected: Query<Entity, Added<Selected>>,
    mut removed_selected: RemovedComponents<Selected>,
    changed_buildings: Query<Entity, Or<(Changed<BuildingState>, Changed<BuildingLevel>, Changed<UpgradeProgress>, Changed<TowerAutoAttackEnabled>)>>,
    mut last_queue_len: Local<usize>,
    icons: Res<IconAssets>,
    placement: Res<BuildingPlacementState>,
    existing_cards: Query<Entity, With<BuildCard>>,
    confirm_panels: Query<Entity, With<DemolishConfirmPanel>>,
    rally_mode: Res<RallyPointMode>,
) {
    // Don't rebuild while a demolish confirm panel is open
    if !confirm_panels.is_empty() {
        return;
    }

    let has_new = !added_selected.is_empty();
    let has_removed = removed_selected.read().count() > 0;
    let has_building_change = !changed_buildings.is_empty();
    let completed_changed = completed.is_changed();
    let rally_changed = rally_mode.is_changed();

    // Detect queue length changes (timer ticks don't count — those are handled by update_training_queue_display)
    let current_queue_len = selected_buildings.iter().next()
        .and_then(|(_, _, _, _, _, _, q, _, _, _)| q.map(|q| q.queue.len()))
        .unwrap_or(0);
    let queue_changed = current_queue_len != *last_queue_len;
    *last_queue_len = current_queue_len;

    if !has_new && !has_removed && !has_building_change && !completed_changed && !queue_changed && !rally_changed {
        return;
    }

    if placement.mode != PlacementMode::None {
        return;
    }

    let Ok((bar_entity, bar_children)) = action_bar.single() else {
        return;
    };

    let has_selected_building = selected_buildings.iter().next().is_some();
    let has_selected_units = selected_units.iter().count() > 0;
    let need_cards = !has_selected_building && !has_selected_units;

    if need_cards && !existing_cards.is_empty() {
        return;
    }

    if let Some(children) = bar_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    if let Ok((_entity, kind, state, level, upgrade_progress, construction, training_queue, storage_inv, health, auto_attack)) = selected_buildings.single() {
        if *state == BuildingState::Complete {
            spawn_building_action_bar(
                &mut commands, bar_entity, *kind, level.0, upgrade_progress,
                training_queue, storage_inv, health, auto_attack,
                &icons, &registry, &player_res, &rally_mode,
            );
        } else {
            // Under construction — show progress + name + demolish (cancel)
            spawn_construction_action_bar(&mut commands, bar_entity, *kind, construction, &registry);
        }
    } else if selected_units.iter().count() > 0 {
        spawn_units_action_bar(&mut commands, bar_entity, &selected_units);
    } else {
        spawn_card_hand(&mut commands, bar_entity, &completed, &icons, &registry);
    }
}

// ── Building action bar (complete building) ──

fn spawn_units_action_bar(
    commands: &mut Commands,
    parent: Entity,
    selected_units: &Query<(&EntityKind, Option<&Carrying>, Option<&CarryCapacity>, Option<&WorkerTask>), (With<Unit>, With<Selected>)>,
) {
    let container = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(4.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        })
        .insert(Interaction::None)
        .id();
    commands.entity(parent).add_child(container);

    let unit_count = selected_units.iter().count();
    let worker_count = selected_units.iter().filter(|(k, ..)| **k == EntityKind::Worker).count();

    let label_text = if worker_count == unit_count && worker_count > 0 {
        format!("{} Worker{}", worker_count, if worker_count > 1 { "s" } else { "" })
    } else {
        format!("{} unit{} selected", unit_count, if unit_count > 1 { "s" } else { "" })
    };

    let label = commands
        .spawn((
            Text::new(label_text),
            TextFont { font_size: 16.0, ..default() },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(container).add_child(label);

    // Show carrying info for single selected worker
    if unit_count == 1 {
        if let Some((kind, carrying, capacity, worker_state)) = selected_units.iter().next() {
            if *kind == EntityKind::Worker {
                if let (Some(carry), Some(cap)) = (carrying, capacity) {
                    if carry.amount > 0 {
                        let rt_name = carry.resource_type
                            .map(|rt| rt.display_name())
                            .unwrap_or("Nothing");
                        let carry_text = format!("Carrying: {:.1}/{:.0} {}", carry.weight, cap.0, rt_name);
                        let carry_label = commands
                            .spawn((
                                Text::new(carry_text),
                                TextFont { font_size: 13.0, ..default() },
                                TextColor(Color::srgb(0.8, 0.75, 0.5)),
                            ))
                            .id();
                        commands.entity(container).add_child(carry_label);

                        // Progress bar
                        let bar_bg = commands
                            .spawn((
                                Node {
                                    width: Val::Px(120.0),
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
                            WorkerTask::Idle => "Idle",
                            WorkerTask::MovingToResource(_) => "Moving to resource",
                            WorkerTask::Gathering(_) => "Gathering",
                            WorkerTask::ReturningToDeposit { .. } => "Returning to depot",
                            WorkerTask::Depositing { .. } => "Depositing",
                            WorkerTask::MovingToBuild(_) => "Moving to build",
                            WorkerTask::Building(_) => "Building",
                        };
                        let state_label = commands
                            .spawn((
                                Text::new(state_text),
                                TextFont { font_size: 12.0, ..default() },
                                TextColor(Color::srgba(0.6, 0.6, 0.7, 0.9)),
                            ))
                            .id();
                        commands.entity(container).add_child(state_label);
                    }
                }
            }
        }
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
    icons: &IconAssets,
    registry: &BlueprintRegistry,
    player_res: &PlayerResources,
    rally_mode: &RallyPointMode,
) {
    let is_upgrading = upgrade_progress.is_some();
    let bp = registry.get(kind);

    // Main container
    let container = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(8.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        })
        .insert(BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.9)))
        .insert(Interaction::None)
        .id();
    commands.entity(parent).add_child(container);

    // Building name + level
    let level_str = format!("{} (Lv {})", kind.display_name(), level);
    let name_child = commands
        .spawn((
            Text::new(level_str),
            TextFont { font_size: 16.0, ..default() },
            TextColor(Color::srgb(0.9, 0.85, 0.7)),
        ))
        .id();
    commands.entity(container).add_child(name_child);

    // HP bar
    if let Some(hp) = health {
        let hp_fraction = hp.current / hp.max;
        let hp_color = if hp_fraction > 0.6 {
            Color::srgb(0.3, 0.8, 0.3)
        } else if hp_fraction > 0.3 {
            Color::srgb(0.9, 0.7, 0.2)
        } else {
            Color::srgb(0.9, 0.3, 0.2)
        };
        let hp_bar_bg = commands
            .spawn((
                Node {
                    width: Val::Px(160.0),
                    height: Val::Px(6.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
            ))
            .with_children(|bg| {
                bg.spawn((
                    BuildingHpBarFill,
                    Node {
                        width: Val::Percent(hp_fraction * 100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(hp_color),
                ));
            })
            .id();
        commands.entity(container).add_child(hp_bar_bg);

        let hp_text = commands
            .spawn((
                Text::new(format!("{}/{}", hp.current as u32, hp.max as u32)),
                TextFont { font_size: 10.0, ..default() },
                TextColor(Color::srgba(0.7, 0.7, 0.7, 0.8)),
            ))
            .id();
        commands.entity(container).add_child(hp_text);
    }

    // Storage inventory display for Base/Storage
    if let Some(inv) = storage_inventory {
        // Always show capacity header
        let total = inv.total();
        let capacity_color = if total >= inv.capacity {
            Color::srgb(1.0, 0.3, 0.3)
        } else if total as f32 >= inv.capacity as f32 * 0.8 {
            Color::srgb(1.0, 0.8, 0.3)
        } else {
            Color::srgb(0.7, 0.7, 0.65)
        };
        let cap_text = commands
            .spawn((
                Text::new(format!("Storage: {} / {}", total, inv.capacity)),
                TextFont { font_size: 12.0, ..default() },
                TextColor(capacity_color),
            ))
            .id();
        commands.entity(container).add_child(cap_text);

        if total > 0 {
            let inv_row = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .id();
            commands.entity(container).add_child(inv_row);

            for (rt, amount) in [
                (ResourceType::Wood, inv.wood),
                (ResourceType::Copper, inv.copper),
                (ResourceType::Iron, inv.iron),
                (ResourceType::Gold, inv.gold),
                (ResourceType::Oil, inv.oil),
            ] {
                if amount == 0 { continue; }
                let text = format!("{}: {}", rt.display_name(), amount);
                let color = match rt {
                    ResourceType::Wood => Color::srgb(0.55, 0.35, 0.15),
                    ResourceType::Copper => Color::srgb(0.72, 0.45, 0.2),
                    ResourceType::Iron => Color::srgb(0.7, 0.7, 0.73),
                    ResourceType::Gold => Color::srgb(0.95, 0.8, 0.2),
                    ResourceType::Oil => Color::srgb(0.4, 0.4, 0.45),
                };
                let entry = commands
                    .spawn((
                        Text::new(text),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(color),
                    ))
                    .id();
                commands.entity(inv_row).add_child(entry);
            }
        }
    }

    // Top row: train buttons (left) + action buttons (right)
    let top_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .id();
    commands.entity(container).add_child(top_row);

    // Train buttons
    if let Some(ref bd) = bp.building {
        if !bd.trains.is_empty() {
            let train_section = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexEnd,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .id();
            commands.entity(top_row).add_child(train_section);

            for unit_kind in &bd.trains {
                spawn_train_button(commands, train_section, *unit_kind, icons, registry);
            }
        }
    }

    // Separator
    let sep = commands
        .spawn((
            Node {
                width: Val::Px(1.0),
                height: Val::Px(50.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.4, 0.4, 0.5, 0.5)),
        ))
        .id();
    commands.entity(top_row).add_child(sep);

    // Action buttons column (upgrade, demolish, rally)
    let actions = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(4.0),
            ..default()
        })
        .id();
    commands.entity(top_row).add_child(actions);

    // Upgrade button
    if let Some(ref bd) = bp.building {
        if level < 3 && !bd.level_upgrades.is_empty() {
            let upgrade_index = (level - 1) as usize;
            if upgrade_index < bd.level_upgrades.len() {
                let upgrade_data = &bd.level_upgrades[upgrade_index];
                let can_afford = upgrade_data.cost.can_afford(player_res);

                if is_upgrading {
                    // Show upgrading progress bar
                    let fraction = upgrade_progress.map_or(0.0, |up| up.timer.fraction());
                    let remaining = upgrade_progress.map_or(0.0, |up| up.timer.remaining_secs());
                    let target_lvl = upgrade_progress.map_or(level + 1, |up| up.target_level);

                    let upgrade_container = commands
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(2.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        })
                        .insert(BackgroundColor(Color::srgba(0.2, 0.2, 0.1, 0.9)))
                        .with_children(|c| {
                            c.spawn((
                                Text::new(format!("Upgrading L{} — {:.0}s", target_lvl, remaining)),
                                TextFont { font_size: 11.0, ..default() },
                                TextColor(Color::srgb(0.9, 0.8, 0.3)),
                            ));
                            // Progress bar
                            c.spawn(Node {
                                width: Val::Px(100.0),
                                height: Val::Px(5.0),
                                border_radius: BorderRadius::all(Val::Px(2.0)),
                                ..default()
                            })
                            .insert(BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)))
                            .with_children(|bg| {
                                bg.spawn((
                                    UpgradeProgressBar,
                                    Node {
                                        width: Val::Percent(fraction * 100.0),
                                        height: Val::Percent(100.0),
                                        border_radius: BorderRadius::all(Val::Px(2.0)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.9, 0.75, 0.2)),
                                ));
                            });
                        })
                        .id();
                    commands.entity(actions).add_child(upgrade_container);
                } else {
                    let cost_str = format_cost(&upgrade_data.cost);
                    let text_color = if can_afford {
                        Color::WHITE
                    } else {
                        Color::srgb(0.8, 0.3, 0.3)
                    };
                    let bg_color = if can_afford {
                        Color::srgba(0.15, 0.3, 0.15, 0.9)
                    } else {
                        Color::srgba(0.2, 0.2, 0.2, 0.7)
                    };

                    let btn = commands
                        .spawn((
                            Button,
                            UpgradeButton,
                            Node {
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(bg_color),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new(format!("Upgrade L{}", level + 1)),
                                TextFont { font_size: 12.0, ..default() },
                                TextColor(text_color),
                            ));
                            btn.spawn((
                                Text::new(cost_str),
                                TextFont { font_size: 9.0, ..default() },
                                TextColor(Color::srgb(0.6, 0.6, 0.5)),
                            ));
                        })
                        .id();
                    commands.entity(actions).add_child(btn);
                }
            }
        } else if level >= 3 {
            let btn = commands
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.7)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("MAX"),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(Color::srgba(0.5, 0.5, 0.5, 0.7)),
                    ));
                })
                .id();
            commands.entity(actions).add_child(btn);
        }
    }

    // Demolish button
    let refund_pct = 50;
    let demolish_tooltip = format!(
        "Demolish building\nRefunds {}% of cost",
        refund_pct,
    );
    let demolish_btn = commands
        .spawn((
            Button,
            DemolishButton,
            ActionTooltipTrigger { text: demolish_tooltip },
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.4, 0.15, 0.1, 0.9)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Demolish"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgb(0.9, 0.6, 0.5)),
            ));
        })
        .id();
    commands.entity(actions).add_child(demolish_btn);

    // Rally point button (only for buildings that train units)
    if let Some(ref bd) = bp.building {
        if !bd.trains.is_empty() {
            let is_rally_active = rally_mode.0;
            let rally_bg = if is_rally_active {
                Color::srgba(0.2, 0.5, 0.7, 0.9)
            } else {
                Color::srgba(0.15, 0.25, 0.35, 0.9)
            };
            let rally_text = if is_rally_active { "Click Ground..." } else { "Set Rally" };
            let rally_text_color = if is_rally_active {
                Color::WHITE
            } else {
                Color::srgb(0.6, 0.8, 0.9)
            };
            let rally_btn = commands
                .spawn((
                    Button,
                    RallyPointButton,
                    ActionTooltipTrigger { text: "Set rally point\nNew units will move here after training".to_string() },
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(rally_bg),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(rally_text),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(rally_text_color),
                    ));
                })
                .id();
            commands.entity(actions).add_child(rally_btn);
        }
    }

    // Tower auto-attack toggle
    if kind == EntityKind::Tower {
        let is_enabled = auto_attack.map_or(true, |a| a.0);
        let toggle_bg = if is_enabled {
            Color::srgba(0.15, 0.35, 0.15, 0.9)
        } else {
            Color::srgba(0.25, 0.2, 0.2, 0.9)
        };
        let toggle_text = if is_enabled { "Auto-Attack: ON" } else { "Auto-Attack: OFF" };
        let toggle_color = if is_enabled {
            Color::srgb(0.5, 0.9, 0.5)
        } else {
            Color::srgb(0.7, 0.5, 0.5)
        };
        let toggle_btn = commands
            .spawn((
                Button,
                ToggleAutoAttackButton,
                Node {
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(toggle_bg),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(toggle_text),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(toggle_color),
                ));
            })
            .id();
        commands.entity(actions).add_child(toggle_btn);
    }

    // Bottom row: training queue display
    if let Some(queue) = training_queue {
        if !queue.queue.is_empty() || queue.timer.is_some() {
            spawn_training_queue_ui(commands, container, queue, icons, registry);
        }
    }
}

fn spawn_training_queue_ui(
    commands: &mut Commands,
    parent: Entity,
    queue: &TrainingQueue,
    icons: &IconAssets,
    _registry: &BlueprintRegistry,
) {
    let queue_row = commands
        .spawn((
            TrainingQueueDisplay,
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.8)),
        ))
        .id();
    commands.entity(parent).add_child(queue_row);

    for (i, unit_kind) in queue.queue.iter().enumerate() {
        let is_first = i == 0;
        let icon_size = if is_first { 28.0 } else { 22.0 };

        let item = commands
            .spawn((
                Button,
                CancelTrainButton(i),
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
            ))
            .with_children(|item| {
                item.spawn((
                    ImageNode::new(icons.entity_icon(*unit_kind)),
                    Node {
                        width: Val::Px(icon_size),
                        height: Val::Px(icon_size),
                        ..default()
                    },
                ));

                // Progress bar for first item
                if is_first {
                    item.spawn(Node {
                        width: Val::Px(28.0),
                        height: Val::Px(4.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    })
                    .insert(BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)))
                    .with_children(|bg| {
                        let fraction = queue.timer.as_ref().map_or(0.0, |t| t.fraction());
                        bg.spawn((
                            TrainingProgressBar,
                            Node {
                                width: Val::Percent(fraction * 100.0),
                                height: Val::Percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.2, 0.6, 0.9)),
                        ));
                    });
                }

                // "x" cancel hint on hover
                item.spawn((
                    Text::new("x"),
                    TextFont { font_size: 9.0, ..default() },
                    TextColor(Color::srgba(0.9, 0.4, 0.3, 0.6)),
                ));
            })
            .id();
        commands.entity(queue_row).add_child(item);
    }
}

// ── Under-construction action bar ──

fn spawn_construction_action_bar(
    commands: &mut Commands,
    parent: Entity,
    kind: EntityKind,
    construction: Option<&ConstructionProgress>,
    _registry: &BlueprintRegistry,
) {
    let container = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(8.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        })
        .insert(BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.9)))
        .insert(Interaction::None)
        .id();
    commands.entity(parent).add_child(container);

    // Building name
    let name = commands
        .spawn((
            Text::new(format!("Building {}", kind.display_name())),
            TextFont { font_size: 16.0, ..default() },
            TextColor(Color::srgb(0.8, 0.7, 0.3)),
        ))
        .id();
    commands.entity(container).add_child(name);

    // Progress bar
    if let Some(cp) = construction {
        let fraction = cp.timer.fraction();
        let pct_text = format!("{}%", (fraction * 100.0) as u32);

        let bar_bg = commands
            .spawn((
                Node {
                    width: Val::Px(200.0),
                    height: Val::Px(8.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
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
                    BackgroundColor(Color::srgb(0.8, 0.65, 0.2)),
                ));
            })
            .id();
        commands.entity(container).add_child(bar_bg);

        let pct = commands
            .spawn((
                Text::new(pct_text),
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgb(0.7, 0.7, 0.6)),
            ))
            .id();
        commands.entity(container).add_child(pct);

        // Worker count text
        let worker_text = commands
            .spawn((
                ConstructionWorkerCountText,
                Text::new("Waiting for workers..."),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.6, 0.7, 0.9)),
            ))
            .id();
        commands.entity(container).add_child(worker_text);
    }

    // Cancel/demolish button (refunds 100% during construction)
    let cancel_btn = commands
        .spawn((
            Button,
            DemolishButton,
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.4, 0.15, 0.1, 0.9)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Cancel"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgb(0.9, 0.6, 0.5)),
            ));
        })
        .id();
    commands.entity(container).add_child(cancel_btn);
}

fn format_cost(cost: &crate::blueprints::ResourceCost) -> String {
    let mut parts = Vec::new();
    if cost.wood > 0 { parts.push(format!("W:{}", cost.wood)); }
    if cost.copper > 0 { parts.push(format!("C:{}", cost.copper)); }
    if cost.iron > 0 { parts.push(format!("I:{}", cost.iron)); }
    if cost.gold > 0 { parts.push(format!("G:{}", cost.gold)); }
    if cost.oil > 0 { parts.push(format!("O:{}", cost.oil)); }
    parts.join(" ")
}

// ── Card hand spawning ──

fn spawn_card_hand(
    commands: &mut Commands,
    parent: Entity,
    completed: &CompletedBuildings,
    icons: &IconAssets,
    registry: &BlueprintRegistry,
) {
    let building_kinds = registry.building_kinds();
    let total = building_kinds.len();

    for (i, kind) in building_kinds.iter().enumerate() {
        let bp = registry.get(*kind);
        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let enabled = match prereq {
            None => true,
            Some(prereq_kind) => completed.has(prereq_kind),
        };

        let (rot_deg, y_off) = fan_params(i, total);
        let label = kind.display_name();

        let bg_color = if enabled {
            Color::srgba(0.18, 0.20, 0.28, 0.92)
        } else {
            Color::srgba(0.12, 0.12, 0.12, 0.5)
        };

        let text_color = if enabled {
            Color::WHITE
        } else {
            Color::srgba(0.5, 0.5, 0.5, 0.7)
        };

        let cost_color = if enabled {
            Color::srgb(0.6, 0.6, 0.5)
        } else {
            Color::srgba(0.4, 0.4, 0.4, 0.5)
        };

        let margin_left = if i == 0 { 0.0 } else { -8.0 };

        let card = commands
            .spawn((
                BuildCard {
                    building_kind: *kind,
                    index: i,
                    total,
                    enabled,
                },
                BuildButton(*kind),
                Button,
                CardAnimState::new(rot_deg, y_off),
                CardDealIn {
                    delay_timer: Timer::from_seconds(i as f32 * 0.08, TimerMode::Once),
                    anim_timer: Timer::from_seconds(0.25, TimerMode::Once),
                    started: false,
                },
                Node {
                    width: Val::Px(90.0),
                    height: Val::Px(130.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(4.0),
                    margin: UiRect {
                        left: Val::Px(margin_left),
                        bottom: Val::Px(-200.0),
                        ..default()
                    },
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(bg_color),
                Transform::from_scale(Vec3::splat(0.5))
                    .with_rotation(Quat::from_rotation_z(rot_deg.to_radians())),
                ZIndex(i as i32),
            ))
            .with_children(|card_node| {
                // Icon
                card_node.spawn((
                    ImageNode::new(icons.entity_icon(*kind)),
                    Node {
                        width: Val::Px(48.0),
                        height: Val::Px(48.0),
                        ..default()
                    },
                ));
                // Name
                card_node.spawn((
                    CardNameText,
                    Text::new(label),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(text_color),
                ));
                // Cost row
                card_node
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(3.0),
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        for (rt, amount) in bp.cost.cost_entries() {
                            spawn_cost_entry(row, icons, rt, amount, cost_color);
                        }
                    });
                // Glow overlay
                card_node.spawn((
                    CardGlow,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.4, 0.5, 0.9, 0.0)),
                ));
                // Tooltip wrapper
                card_node
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: Val::Percent(100.0),
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::Center,
                            padding: UiRect::bottom(Val::Px(4.0)),
                            ..default()
                        },
                        Visibility::Hidden,
                        CardTooltip,
                    ))
                    .with_children(|wrapper| {
                        wrapper.spawn((
                            Text::new(""),
                            TextFont { font_size: 9.0, ..default() },
                            TextColor(Color::srgba(0.9, 0.88, 0.82, 0.95)),
                            TextLayout::new_with_justify(Justify::Center),
                            Node {
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.1, 0.1, 0.14, 0.95)),
                            BoxShadow::new(
                                Color::srgba(0.0, 0.0, 0.0, 0.4),
                                Val::Px(0.0),
                                Val::Px(1.0),
                                Val::Px(0.0),
                                Val::Px(6.0),
                            ),
                        ));
                    });
            })
            .id();

        commands.entity(parent).add_child(card);
    }
}

fn spawn_cost_entry(
    parent: &mut ChildSpawnerCommands,
    icons: &IconAssets,
    rt: ResourceType,
    amount: u32,
    color: Color,
) {
    parent
        .spawn((
            CardCostEntry {
                resource_type: rt,
                amount,
            },
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(1.0),
                ..default()
            },
        ))
        .with_children(|entry| {
            entry.spawn((
                ImageNode::new(icons.resource_icon(rt)),
                Node {
                    width: Val::Px(12.0),
                    height: Val::Px(12.0),
                    ..default()
                },
            ));
            entry.spawn((
                Text::new(format!("{}", amount)),
                TextFont { font_size: 9.0, ..default() },
                TextColor(color),
            ));
        });
}

fn spawn_train_button(commands: &mut Commands, parent: Entity, kind: EntityKind, icons: &IconAssets, registry: &BlueprintRegistry) {
    let label = kind.display_name();
    let bp = registry.get(kind);
    let cost_str = format_cost_from_blueprint(bp);

    // Build tooltip text with stats
    let tooltip = if let Some(ref combat) = bp.combat {
        format!(
            "{}\nHP: {} | DMG: {} | Range: {:.0}\nCost: {} | Train: {:.0}s",
            label, combat.hp as u32, combat.damage as u32, combat.attack_range,
            cost_str, bp.train_time_secs,
        )
    } else {
        format!("{}\nCost: {}", label, cost_str)
    };

    let child = commands
        .spawn((
            TrainButton(kind),
            Button,
            ActionTooltipTrigger { text: tooltip },
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                row_gap: Val::Px(2.0),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.25, 0.25, 0.3, 0.9)),
        ))
        .with_children(|btn| {
            btn.spawn((
                ImageNode::new(icons.entity_icon(kind)),
                Node {
                    width: Val::Px(32.0),
                    height: Val::Px(32.0),
                    ..default()
                },
            ));
            btn.spawn((
                Text::new(format!("Train {}", label)),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::WHITE),
            ));
            btn.spawn((
                Text::new(cost_str),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.6, 0.6, 0.5)),
            ));
        })
        .id();

    commands.entity(parent).add_child(child);
}

fn format_cost_from_blueprint(bp: &crate::blueprints::Blueprint) -> String {
    let mut parts = Vec::new();
    if bp.cost.wood > 0 { parts.push(format!("W:{}", bp.cost.wood)); }
    if bp.cost.copper > 0 { parts.push(format!("C:{}", bp.cost.copper)); }
    if bp.cost.iron > 0 { parts.push(format!("I:{}", bp.cost.iron)); }
    if bp.cost.gold > 0 { parts.push(format!("G:{}", bp.cost.gold)); }
    if bp.cost.oil > 0 { parts.push(format!("O:{}", bp.cost.oil)); }
    parts.join(" ")
}

fn handle_build_buttons(
    mut commands: Commands,
    interactions: Query<(Entity, &Interaction, &BuildButton, Option<&BuildCard>), Changed<Interaction>>,
    mut placement: ResMut<BuildingPlacementState>,
    completed: Res<CompletedBuildings>,
    player_res: Res<PlayerResources>,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for (entity, interaction, build_btn, card) in &interactions {
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            let kind = build_btn.0;
            let bp = registry.get(kind);

            let prereq_met = if let Some(ref bd) = bp.building {
                match bd.prerequisite {
                    None => true,
                    Some(prereq_kind) => completed.has(prereq_kind),
                }
            } else {
                true
            };
            if !prereq_met {
                continue;
            }

            if !bp.cost.can_afford(&player_res) {
                continue;
            }

            if card.is_some() {
                commands.entity(entity).insert(CardPlayOut {
                    timer: Timer::from_seconds(0.3, TimerMode::Once),
                });
            }

            placement.mode = PlacementMode::Placing(kind);
            placement.awaiting_release = true;
        }
    }
}

fn handle_train_buttons(
    interactions: Query<(&Interaction, &TrainButton), Changed<Interaction>>,
    mut player_res: ResMut<PlayerResources>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut queues: Query<&mut TrainingQueue>,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for (interaction, train_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;

        let kind = train_btn.0;
        let bp = registry.get(kind);
        if !bp.cost.can_afford(&player_res) {
            continue;
        }

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queues.get_mut(building_entity) {
                bp.cost.deduct(&mut player_res);
                queue.queue.push(kind);
                break;
            }
        }
    }
}

// ── Upgrade button handler ──

fn handle_upgrade_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<UpgradeButton>)>,
    mut player_res: ResMut<PlayerResources>,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingLevel, &BuildingState),
        (With<Building>, With<Selected>, Without<UpgradeProgress>),
    >,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;

        if let Ok((entity, kind, level, state)) = selected_buildings.single() {
            if *state != BuildingState::Complete {
                continue;
            }
            buildings::start_upgrade(
                &mut commands,
                entity,
                level.0,
                *kind,
                &registry,
                &mut player_res,
            );
        }
    }
}

// ── Demolish button handler ──

fn handle_demolish_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<DemolishButton>)>,
    action_bar: Query<Entity, With<ActionBarInner>>,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingState, &Transform),
        (With<Building>, With<Selected>),
    >,
    registry: Res<BlueprintRegistry>,
    existing_confirm: Query<Entity, With<DemolishConfirmPanel>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;

        // Don't show another confirm if one already exists
        if !existing_confirm.is_empty() {
            continue;
        }

        let Ok(bar_entity) = action_bar.single() else {
            continue;
        };

        if let Ok((_entity, kind, state, _transform)) = selected_buildings.single() {
            let bp = registry.get(*kind);
            let refund_pct = if *state == BuildingState::Complete { 50 } else { 100 };
            let refund_str = format!(
                "Refunds ~{}% (W:{} C:{})",
                refund_pct,
                bp.cost.wood * refund_pct as u32 / 100,
                bp.cost.copper * refund_pct as u32 / 100,
            );

            // Spawn confirm panel
            let panel = commands
                .spawn((
                    DemolishConfirmPanel,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Percent(100.0),
                        left: Val::Px(0.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.15, 0.05, 0.05, 0.95)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Demolish?"),
                        TextFont { font_size: 14.0, ..default() },
                        TextColor(Color::srgb(0.9, 0.5, 0.4)),
                    ));
                    panel.spawn((
                        Text::new(refund_str),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgb(0.7, 0.7, 0.6)),
                    ));
                    // Buttons row
                    panel.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Button,
                            ConfirmDemolishButton,
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.5, 0.15, 0.1, 0.9)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("Yes"),
                                TextFont { font_size: 12.0, ..default() },
                                TextColor(Color::WHITE),
                            ));
                        });

                        row.spawn((
                            Button,
                            CancelDemolishButton,
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.25, 0.25, 0.3, 0.9)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("No"),
                                TextFont { font_size: 12.0, ..default() },
                                TextColor(Color::WHITE),
                            ));
                        });
                    });
                })
                .id();
            commands.entity(bar_entity).add_child(panel);
        }
    }
}

fn handle_demolish_confirm(
    mut commands: Commands,
    confirm_interactions: Query<&Interaction, (Changed<Interaction>, With<ConfirmDemolishButton>)>,
    cancel_interactions: Query<&Interaction, (Changed<Interaction>, With<CancelDemolishButton>)>,
    selected_buildings: Query<
        (Entity, &Transform, &BuildingState),
        (With<Building>, With<Selected>),
    >,
    mut player_res: ResMut<PlayerResources>,
    registry: Res<BlueprintRegistry>,
    building_kinds: Query<&EntityKind, With<Building>>,
    confirm_panels: Query<Entity, With<DemolishConfirmPanel>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    // Handle confirm
    for interaction in &confirm_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;

        if let Ok((entity, transform, state)) = selected_buildings.single() {
            if *state == BuildingState::UnderConstruction {
                // Cancel construction — refund 100%
                if let Ok(kind) = building_kinds.get(entity) {
                    let bp = registry.get(*kind);
                    player_res.wood += bp.cost.wood;
                    player_res.copper += bp.cost.copper;
                    player_res.iron += bp.cost.iron;
                    player_res.gold += bp.cost.gold;
                    player_res.oil += bp.cost.oil;
                }
                commands.entity(entity).despawn();
            } else {
                buildings::start_demolish(&mut commands, entity, transform);
            }
        }

        // Remove confirm panel
        for panel in &confirm_panels {
            commands.entity(panel).despawn();
        }
    }

    // Handle cancel
    for interaction in &cancel_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        for panel in &confirm_panels {
            commands.entity(panel).despawn();
        }
    }
}

// ── Rally point button handler ──

fn handle_rally_point_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<RallyPointButton>)>,
    mut rally_mode: ResMut<RallyPointMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    // Toggle rally mode on button press
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            rally_mode.0 = !rally_mode.0;
        }
    }

    // Set rally point on ground click while in rally mode
    if rally_mode.0 && mouse.just_pressed(MouseButton::Left) {
        // Get ground position from cursor
        let Ok(window) = windows.single() else { return };
        let Some(cursor) = window.cursor_position() else { return };
        let Ok((camera, cam_gt)) = camera_q.single() else { return };
        let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else { return };
        let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else { return };
        let world_pos = ray.get_point(dist);

        for entity in &selected_buildings {
            commands.entity(entity).insert(RallyPoint(world_pos));
        }
        rally_mode.0 = false;
    }

    // Cancel on right click
    if rally_mode.0 && mouse.just_pressed(MouseButton::Right) {
        rally_mode.0 = false;
    }
}

// ── Live training queue progress display ──

fn update_training_queue_display(
    selected_buildings: Query<&TrainingQueue, (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<TrainingProgressBar>>,
) {
    let Ok(queue) = selected_buildings.single() else { return };
    for mut node in &mut progress_bars {
        let fraction = queue.timer.as_ref().map_or(0.0, |t| t.fraction());
        node.width = Val::Percent(fraction * 100.0);
    }
}

// ── Live construction progress display ──

fn update_construction_progress_display(
    selected_buildings: Query<(Entity, &ConstructionProgress), (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<ConstructionProgressBar>>,
    mut worker_texts: Query<(&mut Text, &mut TextColor), With<ConstructionWorkerCountText>>,
    workers: Query<&WorkerTask, With<Unit>>,
) {
    let Ok((building_entity, progress)) = selected_buildings.single() else { return };
    for mut node in &mut progress_bars {
        node.width = Val::Percent(progress.timer.fraction() * 100.0);
    }

    let builder_count = workers
        .iter()
        .filter(|task| matches!(task, WorkerTask::Building(e) if *e == building_entity))
        .count();

    for (mut text, mut color) in &mut worker_texts {
        if builder_count == 0 {
            **text = "Waiting for workers...".to_string();
            *color = TextColor(Color::srgb(0.9, 0.5, 0.3));
        } else {
            let pct = (progress.timer.fraction() * 100.0) as u32;
            **text = format!("{}% ({} worker{})", pct, builder_count, if builder_count == 1 { "" } else { "s" });
            *color = TextColor(Color::srgb(0.6, 0.8, 0.5));
        }
    }
}

fn button_hover_visual(
    mut query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, Without<BuildCard>),
    >,
) {
    for (interaction, mut bg) in &mut query {
        *bg = match interaction {
            Interaction::Pressed => BackgroundColor(Color::srgba(0.15, 0.15, 0.2, 0.9)),
            Interaction::Hovered => BackgroundColor(Color::srgba(0.35, 0.35, 0.4, 0.9)),
            Interaction::None => BackgroundColor(Color::srgba(0.25, 0.25, 0.3, 0.9)),
        };
    }
}

// ── Toggle auto-attack handler ──

fn handle_toggle_auto_attack(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ToggleAutoAttackButton>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut auto_attacks: Query<&mut TowerAutoAttackEnabled>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        for entity in &selected_buildings {
            if let Ok(mut aa) = auto_attacks.get_mut(entity) {
                aa.0 = !aa.0;
            }
        }
    }
}

// ── Cancel training queue item handler ──

fn handle_cancel_train(
    interactions: Query<(&Interaction, &CancelTrainButton), Changed<Interaction>>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut queues: Query<&mut TrainingQueue>,
    registry: Res<BlueprintRegistry>,
    mut player_res: ResMut<PlayerResources>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for (interaction, cancel_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queues.get_mut(building_entity) {
                let idx = cancel_btn.0;
                if idx < queue.queue.len() {
                    let removed_kind = queue.queue.remove(idx);
                    // Refund cost
                    let bp = registry.get(removed_kind);
                    player_res.wood += bp.cost.wood;
                    player_res.copper += bp.cost.copper;
                    player_res.iron += bp.cost.iron;
                    player_res.gold += bp.cost.gold;
                    player_res.oil += bp.cost.oil;
                    // If we removed the currently training item (index 0), reset timer
                    if idx == 0 {
                        queue.timer = None;
                    }
                }
                break;
            }
        }
    }
}

// ── Live upgrade progress display ──

fn update_upgrade_progress_display(
    selected_buildings: Query<&UpgradeProgress, (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<UpgradeProgressBar>>,
) {
    let Ok(progress) = selected_buildings.single() else { return };
    for mut node in &mut progress_bars {
        node.width = Val::Percent(progress.timer.fraction() * 100.0);
    }
}

// ── Action button tooltips ──

fn show_action_tooltips(
    mut commands: Commands,
    triggers: Query<(Entity, &Interaction, &ActionTooltipTrigger, Option<&Children>), Changed<Interaction>>,
    existing_tooltips: Query<Entity, With<ActionTooltip>>,
) {
    for (entity, interaction, trigger, children) in &triggers {
        match interaction {
            Interaction::Hovered => {
                // Check if tooltip already exists
                let has_tooltip = children.map_or(false, |c| {
                    c.iter().any(|child| existing_tooltips.get(child).is_ok())
                });
                if has_tooltip {
                    continue;
                }
                let tooltip = commands
                    .spawn((
                        ActionTooltip,
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: Val::Percent(100.0),
                            left: Val::Px(0.0),
                            padding: UiRect::all(Val::Px(6.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            max_width: Val::Px(180.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.95)),
                        GlobalZIndex(100),
                    ))
                    .with_children(|tt| {
                        tt.spawn((
                            Text::new(&trigger.text),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(Color::srgb(0.8, 0.8, 0.75)),
                        ));
                    })
                    .id();
                commands.entity(entity).add_child(tooltip);
            }
            _ => {
                // Remove tooltip
                if let Some(children) = children {
                    for child in children.iter() {
                        if existing_tooltips.get(child).is_ok() {
                            commands.entity(child).despawn();
                        }
                    }
                }
            }
        }
    }
}

// ── Card animation systems ──

fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}

fn card_deal_in_system(
    time: Res<Time>,
    mut cards: Query<(&mut CardDealIn, &mut CardAnimState, &mut Node, &mut Transform)>,
) {
    for (mut deal, mut anim, mut node, mut tf) in &mut cards {
        deal.delay_timer.tick(time.delta());
        if !deal.delay_timer.is_finished() {
            continue;
        }
        deal.started = true;
        deal.anim_timer.tick(time.delta());

        let t = ease_out_cubic(deal.anim_timer.fraction());

        let start_y = -200.0_f32;
        let target_y = anim.target_offset_y;
        anim.offset_y = start_y + (target_y - start_y) * t;
        anim.scale = 0.5 + 0.5 * t;
        anim.opacity = t;

        node.margin.bottom = Val::Px(-anim.offset_y);
        tf.scale = Vec3::splat(anim.scale);
        tf.rotation = Quat::from_rotation_z(anim.rotation_deg.to_radians());
    }
}

fn card_hover_system(
    completed: Res<CompletedBuildings>,
    registry: Res<BlueprintRegistry>,
    mut cards: Query<
        (&Interaction, &BuildCard, &mut CardAnimState),
        (Changed<Interaction>, Without<CardPlayOut>),
    >,
) {
    for (interaction, card, mut anim) in &mut cards {
        let bp = registry.get(card.building_kind);
        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let prereq_met = match prereq {
            None => true,
            Some(prereq_kind) => completed.has(prereq_kind),
        };
        if !prereq_met {
            continue;
        }
        let (rest_rot, rest_y) = fan_params(card.index, card.total);
        match interaction {
            Interaction::Hovered => {
                anim.target_offset_y = rest_y - 30.0;
                anim.target_scale = 1.12;
                anim.target_rotation_deg = 0.0;
            }
            Interaction::Pressed => {
                anim.target_scale = 1.05;
            }
            Interaction::None => {
                anim.target_offset_y = rest_y;
                anim.target_scale = 1.0;
                anim.target_rotation_deg = rest_rot;
            }
        }
    }
}

fn card_play_out_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cards: Query<(Entity, &mut CardPlayOut, &mut CardAnimState)>,
) {
    for (entity, mut play, mut anim) in &mut cards {
        play.timer.tick(time.delta());
        let t = play.timer.fraction();
        anim.scale = 1.0 + 0.4 * t;
        anim.opacity = 1.0 - t;
        if play.timer.is_finished() {
            commands.entity(entity).remove::<CardPlayOut>();
        }
    }
}

fn card_placement_mode_system(
    placement: Res<BuildingPlacementState>,
    mut cards: Query<(Entity, &BuildCard, &mut CardAnimState), Without<CardPlayOut>>,
) {
    if !placement.is_changed() {
        return;
    }
    match placement.mode {
        PlacementMode::Placing(_) => {
            for (_entity, _card, mut anim) in &mut cards {
                anim.target_offset_y = 100.0;
                anim.target_opacity = 0.0;
            }
        }
        PlacementMode::None => {
            for (_entity, card, mut anim) in &mut cards {
                let (rot, y_off) = fan_params(card.index, card.total);
                anim.target_offset_y = y_off;
                anim.target_scale = 1.0;
                anim.target_rotation_deg = rot;
                anim.target_opacity = 1.0;
            }
        }
    }
}

fn card_spring_back_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cards: Query<(Entity, &BuildCard, &mut CardSpringBack, &mut CardAnimState)>,
) {
    for (entity, card, mut spring, mut anim) in &mut cards {
        spring.timer.tick(time.delta());
        let t = spring.timer.fraction();
        let (_, rest_y) = fan_params(card.index, card.total);

        if t < 1.0 {
            let overshoot = if t < 0.5 {
                -8.0 * (t * 2.0)
            } else {
                -8.0 * (1.0 - (t - 0.5) * 2.0)
            };
            anim.offset_y = rest_y + overshoot;
        }

        if spring.timer.is_finished() {
            anim.offset_y = rest_y;
            commands.entity(entity).remove::<CardSpringBack>();
        }
    }
}

fn card_anim_lerp_system(
    time: Res<Time>,
    completed: Res<CompletedBuildings>,
    registry: Res<BlueprintRegistry>,
    mut cards: Query<
        (&mut CardAnimState, &mut Node, &mut Transform, &mut BackgroundColor, &BuildCard),
        Without<CardDealIn>,
    >,
) {
    let dt = time.delta_secs();
    let speed = 12.0;
    let alpha = 1.0 - (-speed * dt).exp();

    for (mut anim, mut node, mut tf, mut bg, card) in &mut cards {
        anim.offset_y += (anim.target_offset_y - anim.offset_y) * alpha;
        anim.scale += (anim.target_scale - anim.scale) * alpha;
        anim.rotation_deg += (anim.target_rotation_deg - anim.rotation_deg) * alpha;
        anim.opacity += (anim.target_opacity - anim.opacity) * alpha;

        node.margin.bottom = Val::Px(-anim.offset_y);
        tf.scale = Vec3::splat(anim.scale);
        tf.rotation = Quat::from_rotation_z(anim.rotation_deg.to_radians());

        let bp = registry.get(card.building_kind);
        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let prereq_met = match prereq {
            None => true,
            Some(prereq_kind) => completed.has(prereq_kind),
        };
        let base = if card.enabled {
            Color::srgba(0.18, 0.20, 0.28, 0.92 * anim.opacity)
        } else if prereq_met {
            Color::srgba(0.15, 0.17, 0.24, 0.78 * anim.opacity)
        } else {
            Color::srgba(0.12, 0.12, 0.12, 0.5 * anim.opacity)
        };
        *bg = BackgroundColor(base);
    }
}

fn update_card_states(
    completed: Res<CompletedBuildings>,
    player_res: Res<PlayerResources>,
    registry: Res<BlueprintRegistry>,
    mut cards: Query<(&mut BuildCard, &Children)>,
    mut name_texts: Query<&mut TextColor, (With<CardNameText>, Without<CardCostEntry>)>,
    cost_entries: Query<(&CardCostEntry, &Children), Without<CardNameText>>,
    mut cost_text_colors: Query<&mut TextColor, (Without<CardNameText>, Without<CardCostEntry>)>,
    tooltip_wrappers: Query<&Children, With<CardTooltip>>,
    mut texts: Query<&mut Text>,
) {
    for (mut card, card_children) in &mut cards {
        let kind = card.building_kind;
        let bp = registry.get(kind);

        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let prereq_met = match prereq {
            None => true,
            Some(prereq_kind) => completed.has(prereq_kind),
        };

        let can_afford = bp.cost.can_afford(&player_res);
        card.enabled = prereq_met && can_afford;

        let tooltip_text = if !prereq_met {
            "Requires Base"
        } else if !can_afford {
            "Not enough resources"
        } else {
            ""
        };

        for child in card_children.iter() {
            if let Ok(mut text_color) = name_texts.get_mut(child) {
                if !prereq_met {
                    text_color.0 = Color::srgba(0.5, 0.5, 0.5, 0.7);
                } else if !can_afford {
                    text_color.0 = Color::srgba(0.8, 0.8, 0.8, 0.9);
                } else {
                    text_color.0 = Color::WHITE;
                }
            }

            if let Ok((cost_entry, entry_children)) = cost_entries.get(child) {
                let has_enough = player_res.get(cost_entry.resource_type) >= cost_entry.amount;
                let entry_color = if !prereq_met {
                    Color::srgba(0.4, 0.4, 0.4, 0.5)
                } else if !has_enough {
                    Color::srgb(0.9, 0.25, 0.2)
                } else {
                    Color::srgb(0.6, 0.6, 0.5)
                };
                for entry_child in entry_children.iter() {
                    if let Ok(mut tc) = cost_text_colors.get_mut(entry_child) {
                        tc.0 = entry_color;
                    }
                }
            }

            if let Ok(wrapper_children) = tooltip_wrappers.get(child) {
                for wc in wrapper_children.iter() {
                    if let Ok(mut text) = texts.get_mut(wc) {
                        if text.0 != tooltip_text {
                            text.0 = tooltip_text.to_string();
                        }
                    }
                }
            }
        }
    }
}

fn card_tooltip_system(
    cards: Query<(&Interaction, &BuildCard, &Children), Without<CardPlayOut>>,
    mut tooltips: Query<&mut Visibility, With<CardTooltip>>,
) {
    for (interaction, card, children) in &cards {
        let show = !card.enabled && *interaction == Interaction::Hovered;
        for child in children.iter() {
            if let Ok(mut vis) = tooltips.get_mut(child) {
                *vis = if show {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

fn card_glow_system(
    time: Res<Time>,
    completed: Res<CompletedBuildings>,
    registry: Res<BlueprintRegistry>,
    cards: Query<(&Interaction, &BuildCard, &Children), Without<CardPlayOut>>,
    mut glows: Query<&mut BackgroundColor, With<CardGlow>>,
) {
    let speed = 12.0;
    let alpha = 1.0 - (-speed * time.delta_secs()).exp();

    for (interaction, card, children) in &cards {
        let bp = registry.get(card.building_kind);
        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let prereq_met = match prereq {
            None => true,
            Some(prereq_kind) => completed.has(prereq_kind),
        };
        if !prereq_met {
            continue;
        }
        let target_opacity = match interaction {
            Interaction::Hovered | Interaction::Pressed => 0.15,
            Interaction::None => 0.0,
        };

        for child in children.iter() {
            if let Ok(mut glow_bg) = glows.get_mut(child) {
                let current = glow_bg.0.to_srgba();
                let new_a = current.alpha + (target_opacity - current.alpha) * alpha;
                *glow_bg = BackgroundColor(Color::srgba(0.4, 0.5, 0.9, new_a));
            }
        }
    }
}
