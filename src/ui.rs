use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::buildings::{building_cost, building_prerequisite, building_type_label, training_cost};
use crate::components::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud).add_systems(
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
                button_hover_visual,
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

fn unit_type_label(ut: UnitType) -> &'static str {
    match ut {
        UnitType::Worker => "Worker",
        UnitType::Soldier => "Soldier",
        UnitType::Archer => "Archer",
        UnitType::Tank => "Tank",
    }
}

fn spawn_hud(mut commands: Commands, icons: Res<IconAssets>) {
    // ── Top bar — resource display ──
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Px(40.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(20.0),
            ..default()
        })
        .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)))
        .with_children(|bar| {
            let resource_types = [
                ResourceType::Wood,
                ResourceType::Copper,
                ResourceType::Iron,
                ResourceType::Gold,
                ResourceType::Oil,
            ];
            for rt in resource_types {
                bar.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|entry| {
                    entry.spawn((
                        ImageNode::new(icons.resource_icon(rt)),
                        Node {
                            width: Val::Px(24.0),
                            height: Val::Px(24.0),
                            ..default()
                        },
                    ));
                    entry.spawn((
                        ResourceText(rt),
                        Text::new(format!("{:?}: 0", rt)),
                        TextFont {
                            font_size: 18.0,
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

fn mob_type_label(mt: MobType) -> &'static str {
    match mt {
        MobType::Goblin => "Goblin",
        MobType::Skeleton => "Skeleton",
        MobType::Orc => "Orc",
        MobType::Demon => "Demon",
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
    unit_type: UnitType,
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

    // Icon
    let icon = commands
        .spawn((
            ImageNode::new(icons.unit_icon(unit_type)),
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

    // Info column
    let info = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .id();
    commands.entity(card).add_child(info);

    // Name
    let name = commands
        .spawn((
            Text::new(unit_type_label(unit_type)),
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(info).add_child(name);

    // HP bar
    spawn_hp_bar(commands, info, entity, health, 120.0);

    // HP text
    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: 10.0, ..default() },
            TextColor(Color::srgb(0.6, 0.6, 0.6)),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    // Stats row
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
    building_type: BuildingType,
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

    // Icon
    let icon = commands
        .spawn((
            ImageNode::new(icons.building_icon(building_type)),
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

    // Info column
    let info = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .id();
    commands.entity(card).add_child(info);

    // Name + state
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
            Text::new(format!("{}{}", building_type_label(building_type), state_str)),
            TextFont { font_size: 15.0, ..default() },
            TextColor(name_color),
        ))
        .id();
    commands.entity(info).add_child(name);

    // HP bar
    spawn_hp_bar(commands, info, entity, health, 120.0);
}

fn spawn_enemy_detail_card(
    commands: &mut Commands,
    parent: Entity,
    entity: Entity,
    mob_type: MobType,
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

    // Icon
    let icon = commands
        .spawn((
            ImageNode::new(icons.mob_icon(mob_type)),
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

    // Info column
    let info = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .id();
    commands.entity(card).add_child(info);

    // Name
    let name_str = if is_boss {
        format!("{} Boss", mob_type_label(mob_type))
    } else {
        mob_type_label(mob_type).to_string()
    };
    let name = commands
        .spawn((
            Text::new(name_str),
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::srgb(1.0, 0.6, 0.3)),
        ))
        .id();
    commands.entity(info).add_child(name);

    // HP bar
    spawn_hp_bar(commands, info, entity, health, 120.0);

    // HP text
    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: 10.0, ..default() },
            TextColor(Color::srgb(0.6, 0.6, 0.6)),
        ))
        .id();
    commands.entity(info).add_child(hp_text);

    // Stats
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
    unit_type: UnitType,
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

    // Small icon
    let icon = commands
        .spawn((
            ImageNode::new(icons.unit_icon(unit_type)),
            Node {
                width: Val::Px(26.0),
                height: Val::Px(26.0),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(icon);

    // Mini HP bar
    spawn_hp_bar(commands, card, entity, health, 48.0);

    // Type label
    let label = commands
        .spawn((
            Text::new(unit_type_label(unit_type)),
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
        (Entity, &UnitType, &Health, &AttackDamage, &AttackRange, &UnitSpeed),
        (With<Unit>, With<Selected>),
    >,
    selected_buildings: Query<
        (Entity, &BuildingType, &BuildingState, &Health),
        (With<Building>, With<Selected>),
    >,
    mob_query: Query<
        (&MobType, &Health, &AttackDamage, &AttackRange, &UnitSpeed, &AggroRange, Has<Boss>),
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

    // Despawn all children
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

    // Friendly section
    if unit_count == 1 && building_count == 0 {
        let (entity, ut, health, dmg, rng, spd) = selected_units.iter().next().unwrap();
        spawn_friendly_detail_card(&mut commands, panel_entity, entity, *ut, health, dmg, rng, spd, &icons);
    } else if unit_count == 0 && building_count == 1 {
        let (entity, bt, state, health) = selected_buildings.iter().next().unwrap();
        spawn_building_detail_card(&mut commands, panel_entity, entity, *bt, *state, health, &icons);
    } else if unit_count + building_count > 1 {
        // Multi-select grid
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

        for (entity, ut, health, _, _, _) in &selected_units {
            spawn_unit_mini_card(&mut commands, grid, entity, *ut, health, &icons);
        }
        for (entity, bt, _state, health) in &selected_buildings {
            // Reuse mini card style for buildings in multi-select
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
                    ImageNode::new(icons.building_icon(*bt)),
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
                    Text::new(building_type_label(*bt)),
                    TextFont { font_size: 9.0, ..default() },
                    TextColor(Color::srgb(0.75, 0.75, 0.75)),
                ))
                .id();
            commands.entity(card).add_child(label);
        }
    }

    // Enemy inspect section
    if let Some(enemy_entity) = inspected.entity {
        if let Ok((mt, health, dmg, rng, spd, aggro, is_boss)) = mob_query.get(enemy_entity) {
            // Divider
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
                *mt, is_boss, health, dmg, rng, spd, aggro, &icons,
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
) {
    for (interaction, card_ref) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        // Deselect all, select only this unit
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
    selected_units: Query<&UnitType, (With<Unit>, With<Selected>)>,
    selected_buildings: Query<
        (&BuildingType, &BuildingState),
        (With<Building>, With<Selected>),
    >,
    completed: Res<CompletedBuildings>,
    action_bar: Query<Entity, With<ActionBarInner>>,
    children_q: Query<&Children>,
    added_selected: Query<Entity, Added<Selected>>,
    mut removed_selected: RemovedComponents<Selected>,
    changed_buildings: Query<Entity, Changed<BuildingState>>,
    icons: Res<IconAssets>,
    placement: Res<BuildingPlacementState>,
    existing_cards: Query<Entity, With<BuildCard>>,
) {
    // Only rebuild if something changed
    let has_new = !added_selected.is_empty();
    let has_removed = removed_selected.read().count() > 0;
    let has_building_change = !changed_buildings.is_empty();
    let completed_changed = completed.is_changed();

    if !has_new && !has_removed && !has_building_change && !completed_changed {
        return;
    }

    // Don't rebuild if we're in placement mode (cards are animating out)
    if placement.mode != PlacementMode::None {
        return;
    }

    let Ok(bar_entity) = action_bar.single() else {
        return;
    };

    // Figure out what we need to show
    let has_selected_building = selected_buildings.single().is_ok();
    let has_selected_units = selected_units.iter().count() > 0;
    let need_cards = !has_selected_building && !has_selected_units;

    // If we need cards and they already exist, skip rebuild
    if need_cards && !existing_cards.is_empty() {
        return;
    }

    // Despawn all children
    if let Ok(children) = children_q.get(bar_entity) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    // Check if a building is selected
    if let Ok((bt, state)) = selected_buildings.single() {
        if *state == BuildingState::Complete {
            match bt {
                BuildingType::Barracks => {
                    spawn_train_button(&mut commands, bar_entity, UnitType::Worker, &icons);
                    spawn_train_button(&mut commands, bar_entity, UnitType::Soldier, &icons);
                    spawn_train_button(&mut commands, bar_entity, UnitType::Archer, &icons);
                }
                BuildingType::Workshop => {
                    spawn_train_button(&mut commands, bar_entity, UnitType::Tank, &icons);
                }
                BuildingType::Base => {
                    spawn_train_button(&mut commands, bar_entity, UnitType::Worker, &icons);
                }
                _ => {
                    let child = commands
                        .spawn((
                            Text::new(building_type_label(*bt).to_string()),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ))
                        .id();
                    commands.entity(bar_entity).add_child(child);
                }
            }
        } else {
            let child = commands
                .spawn((
                    Text::new("Under Construction..."),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.8, 0.7, 0.3)),
                ))
                .id();
            commands.entity(bar_entity).add_child(child);
        }
    } else if selected_units.iter().count() > 0 {
        let child = commands
            .spawn((
                Text::new("Units selected"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ))
            .id();
        commands.entity(bar_entity).add_child(child);
    } else {
        // Nothing selected — spawn card hand
        spawn_card_hand(&mut commands, bar_entity, &completed, &icons);
    }
}

// ── Card hand spawning ──

fn spawn_card_hand(
    commands: &mut Commands,
    parent: Entity,
    completed: &CompletedBuildings,
    icons: &IconAssets,
) {
    let building_types = [
        BuildingType::Base,
        BuildingType::Barracks,
        BuildingType::Workshop,
        BuildingType::Tower,
        BuildingType::Storage,
    ];
    let total = building_types.len();

    for (i, bt) in building_types.iter().enumerate() {
        let enabled = match building_prerequisite(*bt) {
            None => true,
            Some(BuildingType::Base) => completed.has_base,
            Some(_) => false,
        };

        let (rot_deg, y_off) = fan_params(i, total);
        let label = building_type_label(*bt);
        let (w, c, iron, g, o, _) = building_cost(*bt);

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
                    building_type: *bt,
                    index: i,
                    total,
                    enabled,
                },
                BuildButton(*bt),
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
                        bottom: Val::Px(-200.0), // start off-screen
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
                    ImageNode::new(icons.building_icon(*bt)),
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
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(text_color),
                ));
                // Cost row with resource icons
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
                        let costs = [
                            (w, ResourceType::Wood),
                            (c, ResourceType::Copper),
                            (iron, ResourceType::Iron),
                            (g, ResourceType::Gold),
                            (o, ResourceType::Oil),
                        ];
                        for (amount, rt) in costs {
                            if amount > 0 {
                                spawn_cost_entry(row, icons, rt, amount, cost_color);
                            }
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
                // Tooltip wrapper (centers the tooltip above the card)
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
                            TextFont {
                                font_size: 9.0,
                                ..default()
                            },
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
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(color),
            ));
        });
}

fn spawn_train_button(commands: &mut Commands, parent: Entity, ut: UnitType, icons: &IconAssets) {
    let label = unit_type_label(ut);
    let (w, c, i, g, o, _) = training_cost(ut);
    let cost_str = format_cost(w, c, i, g, o);

    let child = commands
        .spawn((
            TrainButton(ut),
            Button,
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
                ImageNode::new(icons.unit_icon(ut)),
                Node {
                    width: Val::Px(32.0),
                    height: Val::Px(32.0),
                    ..default()
                },
            ));
            btn.spawn((
                Text::new(format!("Train {}", label)),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            btn.spawn((
                Text::new(cost_str),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.5)),
            ));
        })
        .id();

    commands.entity(parent).add_child(child);
}

fn format_cost(w: u32, c: u32, i: u32, g: u32, o: u32) -> String {
    let mut parts = Vec::new();
    if w > 0 {
        parts.push(format!("W:{}", w));
    }
    if c > 0 {
        parts.push(format!("C:{}", c));
    }
    if i > 0 {
        parts.push(format!("I:{}", i));
    }
    if g > 0 {
        parts.push(format!("G:{}", g));
    }
    if o > 0 {
        parts.push(format!("O:{}", o));
    }
    parts.join(" ")
}

fn handle_build_buttons(
    mut commands: Commands,
    interactions: Query<(Entity, &Interaction, &BuildButton, Option<&BuildCard>), Changed<Interaction>>,
    mut placement: ResMut<BuildingPlacementState>,
    completed: Res<CompletedBuildings>,
    player_res: Res<PlayerResources>,
) {
    for (entity, interaction, build_btn, card) in &interactions {
        if *interaction == Interaction::Pressed {
            let bt = build_btn.0;
            let prereq_met = match building_prerequisite(bt) {
                None => true,
                Some(BuildingType::Base) => completed.has_base,
                Some(_) => false,
            };
            if !prereq_met {
                continue;
            }

            let (w, c, i, g, o, _) = building_cost(bt);
            if !player_res.can_afford(w, c, i, g, o) {
                continue;
            }

            // Insert CardPlayOut on the card for the fly-away animation
            if card.is_some() {
                commands.entity(entity).insert(CardPlayOut {
                    timer: Timer::from_seconds(0.3, TimerMode::Once),
                });
            }

            placement.mode = PlacementMode::Placing(bt);
            placement.awaiting_release = true;
        }
    }
}

fn handle_train_buttons(
    interactions: Query<(&Interaction, &TrainButton), Changed<Interaction>>,
    mut player_res: ResMut<PlayerResources>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut queues: Query<&mut TrainingQueue>,
) {
    for (interaction, train_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let ut = train_btn.0;
        let (w, c, i, g, o, _) = training_cost(ut);
        if !player_res.can_afford(w, c, i, g, o) {
            continue;
        }

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queues.get_mut(building_entity) {
                player_res.subtract(w, c, i, g, o);
                queue.queue.push(ut);
                break;
            }
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

// ── Card animation systems ──

/// Ease-out cubic: 1 - (1-t)^3
fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}

/// 1 — Deal-in: staggered entrance from below
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

        // Animate from off-screen (-200) to target offset
        let start_y = -200.0_f32;
        let target_y = anim.target_offset_y;
        anim.offset_y = start_y + (target_y - start_y) * t;

        // Scale: 0.5 -> 1.0
        anim.scale = 0.5 + 0.5 * t;

        // Opacity: 0 -> 1
        anim.opacity = t;

        // Apply immediately during deal-in (bypass lerp for snappy entrance)
        node.margin.bottom = Val::Px(-anim.offset_y);
        tf.scale = Vec3::splat(anim.scale);
        tf.rotation = Quat::from_rotation_z(anim.rotation_deg.to_radians());
    }
}

/// 2 — Hover: sets targets on Interaction change
fn card_hover_system(
    completed: Res<CompletedBuildings>,
    mut cards: Query<
        (&Interaction, &BuildCard, &mut CardAnimState),
        (Changed<Interaction>, Without<CardPlayOut>),
    >,
) {
    for (interaction, card, mut anim) in &mut cards {
        // Locked cards (prerequisite missing) don't animate on hover
        let prereq_met = match building_prerequisite(card.building_type) {
            None => true,
            Some(BuildingType::Base) => completed.has_base,
            Some(_) => false,
        };
        if !prereq_met {
            continue;
        }
        let (rest_rot, rest_y) = fan_params(card.index, card.total);
        match interaction {
            Interaction::Hovered => {
                anim.target_offset_y = rest_y - 30.0; // lift up
                anim.target_scale = 1.12;
                anim.target_rotation_deg = 0.0; // straighten
            }
            Interaction::Pressed => {
                anim.target_scale = 1.05; // brief press
            }
            Interaction::None => {
                anim.target_offset_y = rest_y;
                anim.target_scale = 1.0;
                anim.target_rotation_deg = rest_rot;
            }
        }
    }
}

/// 3 — Play out: clicked card scales up and fades
fn card_play_out_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cards: Query<(Entity, &mut CardPlayOut, &mut CardAnimState)>,
) {
    for (entity, mut play, mut anim) in &mut cards {
        play.timer.tick(time.delta());
        let t = play.timer.fraction();
        anim.scale = 1.0 + 0.4 * t; // 1.0 -> 1.4
        anim.opacity = 1.0 - t;
        if play.timer.is_finished() {
            commands.entity(entity).remove::<CardPlayOut>();
        }
    }
}

/// 4 — Placement mode: hide siblings when placing, show on cancel
fn card_placement_mode_system(
    placement: Res<BuildingPlacementState>,
    mut cards: Query<(Entity, &BuildCard, &mut CardAnimState), Without<CardPlayOut>>,
) {
    if !placement.is_changed() {
        return;
    }
    match placement.mode {
        PlacementMode::Placing(_) => {
            // Hide all non-play-out cards
            for (_entity, _card, mut anim) in &mut cards {
                anim.target_offset_y = 100.0;
                anim.target_opacity = 0.0;
            }
        }
        PlacementMode::None => {
            // Restore all cards to fan position
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

/// 5 — Spring back: overshoot then settle after cancel
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
            // Overshoot phase: go past rest by 8px, then settle
            let overshoot = if t < 0.5 {
                // Going past
                -8.0 * (t * 2.0)
            } else {
                // Settling back
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

/// 6 — Continuous lerp: drives actual Node/Transform from CardAnimState
fn card_anim_lerp_system(
    time: Res<Time>,
    completed: Res<CompletedBuildings>,
    mut cards: Query<
        (&mut CardAnimState, &mut Node, &mut Transform, &mut BackgroundColor, &BuildCard),
        Without<CardDealIn>,
    >,
) {
    let dt = time.delta_secs();
    let speed = 12.0;
    let alpha = 1.0 - (-speed * dt).exp();

    for (mut anim, mut node, mut tf, mut bg, card) in &mut cards {
        // Lerp current toward targets
        anim.offset_y += (anim.target_offset_y - anim.offset_y) * alpha;
        anim.scale += (anim.target_scale - anim.scale) * alpha;
        anim.rotation_deg += (anim.target_rotation_deg - anim.rotation_deg) * alpha;
        anim.opacity += (anim.target_opacity - anim.opacity) * alpha;

        // Apply to UI
        node.margin.bottom = Val::Px(-anim.offset_y);
        tf.scale = Vec3::splat(anim.scale);
        tf.rotation = Quat::from_rotation_z(anim.rotation_deg.to_radians());

        // Apply opacity to background color with 3 visual states
        let prereq_met = match building_prerequisite(card.building_type) {
            None => true,
            Some(BuildingType::Base) => completed.has_base,
            Some(_) => false,
        };
        let base = if card.enabled {
            // Fully enabled
            Color::srgba(0.18, 0.20, 0.28, 0.92 * anim.opacity)
        } else if prereq_met {
            // Can't afford — slightly dimmed but not fully grayed
            Color::srgba(0.15, 0.17, 0.24, 0.78 * anim.opacity)
        } else {
            // Locked — fully grayed out
            Color::srgba(0.12, 0.12, 0.12, 0.5 * anim.opacity)
        };
        *bg = BackgroundColor(base);
    }
}

/// 7 — Dynamic card state: updates enabled/disabled, cost coloring, tooltip text
fn update_card_states(
    completed: Res<CompletedBuildings>,
    player_res: Res<PlayerResources>,
    mut cards: Query<(&mut BuildCard, &Children)>,
    mut name_texts: Query<&mut TextColor, (With<CardNameText>, Without<CardCostEntry>)>,
    cost_entries: Query<(&CardCostEntry, &Children), Without<CardNameText>>,
    mut cost_text_colors: Query<&mut TextColor, (Without<CardNameText>, Without<CardCostEntry>)>,
    tooltip_wrappers: Query<&Children, With<CardTooltip>>,
    mut texts: Query<&mut Text>,
) {
    for (mut card, card_children) in &mut cards {
        let bt = card.building_type;

        // Check prerequisite
        let prereq_met = match building_prerequisite(bt) {
            None => true,
            Some(BuildingType::Base) => completed.has_base,
            Some(_) => false,
        };

        // Check affordability
        let (w, c, iron, g, o, _) = building_cost(bt);
        let can_afford = player_res.can_afford(w, c, iron, g, o);

        // Determine card state
        card.enabled = prereq_met && can_afford;

        // Determine tooltip text
        let tooltip_text = if !prereq_met {
            "Requires Base"
        } else if !can_afford {
            "Not enough resources"
        } else {
            ""
        };

        // Update children
        for child in card_children.iter() {
            // Update name text color
            if let Ok(mut text_color) = name_texts.get_mut(child) {
                if !prereq_met {
                    text_color.0 = Color::srgba(0.5, 0.5, 0.5, 0.7);
                } else if !can_afford {
                    text_color.0 = Color::srgba(0.8, 0.8, 0.8, 0.9);
                } else {
                    text_color.0 = Color::WHITE;
                }
            }

            // Update cost entry colors
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

            // Update tooltip text (the Text is a child of the CardTooltip wrapper)
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

/// 8 — Tooltip visibility on hover over disabled cards
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

/// 9 — Glow overlay opacity on hover
fn card_glow_system(
    time: Res<Time>,
    completed: Res<CompletedBuildings>,
    cards: Query<(&Interaction, &BuildCard, &Children), Without<CardPlayOut>>,
    mut glows: Query<&mut BackgroundColor, With<CardGlow>>,
) {
    let speed = 12.0;
    let alpha = 1.0 - (-speed * time.delta_secs()).exp();

    for (interaction, card, children) in &cards {
        // Locked cards (prerequisite missing) don't glow
        let prereq_met = match building_prerequisite(card.building_type) {
            None => true,
            Some(BuildingType::Base) => completed.has_base,
            Some(_) => false,
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
