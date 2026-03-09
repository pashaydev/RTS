use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::buildings;
use crate::components::*;
use crate::theme::{self, BG_TRANSPARENT};

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
                ),
            )
            .add_systems(Update, update_action_bar)
            .add_systems(
                Update,
                (
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
                    card_tilt_update_system,
                    card_drag_pickup_system,
                    card_drag_follow_system,
                ),
            )
            .add_systems(
                Update,
                (
                    card_drag_gap_system,
                    card_drag_release_system,
                    card_play_out_system,
                    card_placement_mode_system,
                    card_spring_back_system,
                    card_anim_lerp_system,
                    update_card_states,
                    card_tooltip_system,
                ),
            )
            .add_systems(
                Update,
                (
                    card_glow_system,
                    card_border_glow_system,
                    card_shine_sweep_system,
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
            width: Val::Px(150.0),
            padding: UiRect::all(Val::Px(10.0)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: Val::Px(6.0),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        })
        .insert(BackgroundColor(theme::BG_TRANSPARENT))
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
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(theme::TEXT_PRIMARY),
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
        BackgroundColor(theme::BG_PANEL),
        Visibility::Hidden,
    ));

    // ── Bottom panel — context-sensitive action bar ──
    // Offset right padding to avoid overlapping the minimap (180+margin ~200px)
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            width: Val::Percent(100.0),
            padding: UiRect {
                right: Val::Px(210.0),
                ..default()
            },
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
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    mut text_q: Query<(&mut Text, &ResourceText)>,
) {
    let player_res = all_resources.get(&active_player.0);
    for (mut text, rt_marker) in &mut text_q {
        let rt = rt_marker.0;
        let val = player_res.get(rt);
        **text = format!("{}", val);
    }
}

fn hp_color(current: f32, max: f32) -> Color {
    let pct = (current / max).clamp(0.0, 1.0);
    if pct > 0.6 {
        theme::HP_HIGH
    } else if pct > 0.3 {
        theme::HP_MID
    } else {
        theme::HP_LOW
    }
}

fn spawn_hp_bar(commands: &mut Commands, parent: Entity, tracked_entity: Entity, health: &Health, width: f32) {
    let pct = (health.current / health.max).clamp(0.0, 1.0) * 100.0;

    let bg = commands
        .spawn((
            Node {
                width: Val::Px(width),
                height: Val::Px(4.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(theme::HP_BAR_BG),
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
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 120.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: 10.0, ..default() },
            TextColor(theme::TEXT_SECONDARY),
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
                TextColor(theme::TEXT_SECONDARY),
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
        BuildingState::UnderConstruction => theme::WARNING,
        BuildingState::Complete => theme::TEXT_PRIMARY,
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
            BorderColor::all(theme::BORDER_ENEMY),
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
            TextColor(theme::WARNING),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 120.0);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont { font_size: 10.0, ..default() },
            TextColor(theme::TEXT_SECONDARY),
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
                TextColor(theme::TEXT_SECONDARY),
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
            BackgroundColor(theme::BG_SURFACE),
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
            TextColor(theme::TEXT_SECONDARY),
        ))
        .id();
    commands.entity(card).add_child(label);
}

fn rebuild_selection_panel(
    mut commands: Commands,
    inspected: Res<InspectedEnemy>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
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
    faction_q: Query<&Faction>,
    inspected_unit_q: Query<(&EntityKind, &Health, &AttackDamage, &AttackRange, &UnitSpeed), With<Unit>>,
    inspected_building_q: Query<(&EntityKind, &BuildingState, &Health), With<Building>>,
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
            commands.entity(child).try_despawn();
        }
    }

    let unit_count = selected_units.iter().count();
    let building_count = selected_buildings.iter().count();
    let has_inspected_mob = inspected.entity.and_then(|e| mob_query.get(e).ok()).is_some();
    let has_inspected_player = inspected.entity.map_or(false, |e| {
        inspected_unit_q.get(e).is_ok() || inspected_building_q.get(e).is_ok()
    });
    let has_inspected = has_inspected_mob || has_inspected_player;

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
                    BackgroundColor(theme::BG_SURFACE),
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
                    TextColor(theme::TEXT_SECONDARY),
                ))
                .id();
            commands.entity(card).add_child(label);
        }
    }

    // Inspect section (mobs, enemy/allied player entities)
    if let Some(inspected_entity) = inspected.entity {
        // Determine relationship label
        let relationship = faction_q.get(inspected_entity).map(|f| {
            if teams.is_allied(&active_player.0, f) { "Allied" } else { "Enemy" }
        }).unwrap_or("Neutral");
        let relationship_color = if relationship == "Allied" {
            Color::srgb(0.3, 0.8, 0.3)
        } else {
            Color::srgb(1.0, 0.3, 0.3)
        };

        if let Ok((kind, health, dmg, rng, spd, aggro, is_boss)) = mob_query.get(inspected_entity) {
            if unit_count + building_count > 0 {
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
            // Inspected player unit (ally or enemy)
            if unit_count + building_count > 0 {
                let divider = commands.spawn((
                    Node { width: Val::Px(1.0), height: Val::Px(50.0), margin: UiRect::axes(Val::Px(6.0), Val::Px(0.0)), ..default() },
                    BackgroundColor(theme::SEPARATOR),
                )).id();
                commands.entity(panel_entity).add_child(divider);
            }
            // Reuse friendly detail card but add relationship label
            spawn_friendly_detail_card(&mut commands, panel_entity, inspected_entity, *kind, health, dmg, rng, spd, &icons);
            let label = commands.spawn((
                Text::new(relationship),
                TextFont { font_size: 11.0, ..default() },
                TextColor(relationship_color),
            )).id();
            commands.entity(panel_entity).add_child(label);
        } else if let Ok((kind, state, health)) = inspected_building_q.get(inspected_entity) {
            // Inspected player building (ally or enemy)
            if unit_count + building_count > 0 {
                let divider = commands.spawn((
                    Node { width: Val::Px(1.0), height: Val::Px(50.0), margin: UiRect::axes(Val::Px(6.0), Val::Px(0.0)), ..default() },
                    BackgroundColor(theme::SEPARATOR),
                )).id();
                commands.entity(panel_entity).add_child(divider);
            }
            spawn_building_detail_card(&mut commands, panel_entity, inspected_entity, *kind, *state, health, &icons);
            let label = commands.spawn((
                Text::new(relationship),
                TextFont { font_size: 11.0, ..default() },
                TextColor(relationship_color),
            )).id();
            commands.entity(panel_entity).add_child(label);
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
    unit_query: Query<Entity, With<Unit>>,
    building_query: Query<Entity, With<Building>>,
) {
    if let Some(e) = inspected.entity {
        // Entity is valid if it's a mob, unit, or building that still exists
        let exists = mob_query.get(e).is_ok()
            || unit_query.get(e).is_ok()
            || building_query.get(e).is_ok();
        if !exists {
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
    player_state: (Res<AllCompletedBuildings>, Res<ActivePlayer>, Res<AllPlayerResources>),
    registry: Res<BlueprintRegistry>,
    action_bar: Query<(Entity, Option<&Children>), With<ActionBarInner>>,
    added_selected: Query<Entity, Added<Selected>>,
    mut removed_selected: RemovedComponents<Selected>,
    changed_buildings: Query<Entity, Or<(Changed<BuildingState>, Changed<BuildingLevel>, Changed<UpgradeProgress>, Changed<TowerAutoAttackEnabled>)>>,
    mut last_queue_len: Local<usize>,
    ui_state: (Res<IconAssets>, Res<BuildingPlacementState>, Res<RallyPointMode>),
    existing_cards: Query<Entity, With<BuildCard>>,
    confirm_panels: Query<Entity, With<DemolishConfirmPanel>>,
) {
    let (all_completed, active_player, all_resources) = player_state;
    let (icons, placement, rally_mode) = ui_state;

    // Don't rebuild while a demolish confirm panel is open
    if !confirm_panels.is_empty() {
        return;
    }

    let has_new = !added_selected.is_empty();
    let has_removed = removed_selected.read().count() > 0;
    let has_building_change = !changed_buildings.is_empty();
    let completed_changed = all_completed.is_changed();
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
            commands.entity(child).try_despawn();
        }
    }

    if let Ok((_entity, kind, state, level, upgrade_progress, construction, training_queue, storage_inv, health, auto_attack)) = selected_buildings.single() {
        if *state == BuildingState::Complete {
            let player_res = all_resources.get(&active_player.0);
            spawn_building_action_bar(
                &mut commands, bar_entity, *kind, level.0, upgrade_progress,
                training_queue, storage_inv, health, auto_attack,
                &icons, &registry, player_res, &rally_mode,
            );
        } else {
            // Under construction — show progress + name + demolish (cancel)
            spawn_construction_action_bar(&mut commands, bar_entity, *kind, construction, &registry);
        }
    } else if selected_units.iter().count() > 0 {
        spawn_units_action_bar(&mut commands, bar_entity, &selected_units);
    } else {
        let completed = all_completed.completed_for(&active_player.0);
        spawn_card_hand(&mut commands, bar_entity, completed, &icons, &registry);
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
            TextFont { font_size: 15.0, ..default() },
            TextColor(theme::TEXT_PRIMARY),
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
                                TextColor(theme::WARNING),
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
                            WorkerTask::ManualMove => "Moving",
                            WorkerTask::MovingToResource(_) => "Moving to resource",
                            WorkerTask::Gathering(_) => "Gathering",
                            WorkerTask::ReturningToDeposit { .. } => "Returning to depot",
                            WorkerTask::Depositing { .. } => "Depositing",
                            WorkerTask::MovingToBuild(_) => "Moving to build",
                            WorkerTask::Building(_) => "Building",
                            WorkerTask::WaitingForStorage { .. } => "Storage full!",
                        };
                        let state_label = commands
                            .spawn((
                                Text::new(state_text),
                                TextFont { font_size: 12.0, ..default() },
                                TextColor(theme::TEXT_SECONDARY),
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
            align_items: AlignItems::Stretch,
            row_gap: Val::Px(0.0),
            padding: UiRect::all(Val::Px(10.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            min_width: Val::Px(220.0),
            ..default()
        })
        .insert(BackgroundColor(theme::BG_PANEL))
        .insert(Interaction::None)
        .id();
    commands.entity(parent).add_child(container);

    // ── Name row: name left, level pill right ──
    let name_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::bottom(Val::Px(6.0)),
            ..default()
        })
        .id();
    commands.entity(container).add_child(name_row);

    let name_child = commands
        .spawn((
            Text::new(kind.display_name()),
            TextFont { font_size: 15.0, ..default() },
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(name_row).add_child(name_child);

    // Level pill badge
    let level_pill = commands
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(theme::BG_ELEVATED),
        ))
        .with_children(|pill| {
            pill.spawn((
                Text::new(format!("Lv {}", level)),
                TextFont { font_size: 11.0, ..default() },
                TextColor(theme::TEXT_SECONDARY),
            ));
        })
        .id();
    commands.entity(name_row).add_child(level_pill);

    // ── HP row: bar + text on same line ──
    if let Some(hp) = health {
        let hp_fraction = hp.current / hp.max;
        let hp_color = if hp_fraction > 0.6 {
            theme::HP_HIGH
        } else if hp_fraction > 0.3 {
            theme::HP_MID
        } else {
            theme::HP_LOW
        };

        let hp_row = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::bottom(Val::Px(6.0)),
                ..default()
            })
            .id();
        commands.entity(container).add_child(hp_row);

        let hp_bar_bg = commands
            .spawn((
                Node {
                    width: Val::Px(140.0),
                    height: Val::Px(4.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(theme::HP_BAR_BG),
            ))
            .with_children(|bg| {
                bg.spawn((
                    BuildingHpBarFill,
                    Node {
                        width: Val::Percent(hp_fraction * 100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(hp_color),
                ));
            })
            .id();
        commands.entity(hp_row).add_child(hp_bar_bg);

        let hp_text = commands
            .spawn((
                Text::new(format!("{}/{}", hp.current as u32, hp.max as u32)),
                TextFont { font_size: 10.0, ..default() },
                TextColor(theme::TEXT_SECONDARY),
            ))
            .id();
        commands.entity(hp_row).add_child(hp_text);
    }

    // ── Separator ──
    let sep1 = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::SEPARATOR),
        ))
        .id();
    commands.entity(container).add_child(sep1);

    // ── Storage inventory display ──
    if let Some(inv) = storage_inventory {
        let total = inv.total();
        let capacity_color = if total >= inv.capacity {
            theme::DESTRUCTIVE
        } else if total as f32 >= inv.capacity as f32 * 0.8 {
            theme::WARNING
        } else {
            theme::TEXT_SECONDARY
        };

        let storage_row = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                flex_wrap: FlexWrap::Wrap,
                padding: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                ..default()
            })
            .id();
        commands.entity(container).add_child(storage_row);

        let cap_text = commands
            .spawn((
                Text::new(format!("Storage: {}/{}", total, inv.capacity)),
                TextFont { font_size: 11.0, ..default() },
                TextColor(capacity_color),
            ))
            .id();
        commands.entity(storage_row).add_child(cap_text);

        if total > 0 {
            for (rt, amount) in [
                (ResourceType::Wood, inv.wood),
                (ResourceType::Copper, inv.copper),
                (ResourceType::Iron, inv.iron),
                (ResourceType::Gold, inv.gold),
                (ResourceType::Oil, inv.oil),
            ] {
                if amount == 0 { continue; }
                let color = rt.carry_color();
                let entry = commands
                    .spawn((
                        Text::new(format!("{}: {}", rt.display_name(), amount)),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(color),
                    ))
                    .id();
                commands.entity(storage_row).add_child(entry);
            }
        }

        // Storage separator
        let sep_storage = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ))
            .id();
        commands.entity(container).add_child(sep_storage);
    }

    // ── Train buttons row ──
    if let Some(ref bd) = bp.building {
        if !bd.trains.is_empty() {
            let train_row = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexEnd,
                    column_gap: Val::Px(4.0),
                    padding: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                    ..default()
                })
                .id();
            commands.entity(container).add_child(train_row);

            for unit_kind in &bd.trains {
                spawn_train_button(commands, train_row, *unit_kind, icons, registry);
            }

            // Separator after train buttons
            let sep_train = commands
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(theme::SEPARATOR),
                ))
                .id();
            commands.entity(container).add_child(sep_train);
        }
    }

    // ── Upgrade + Rally ghost buttons row ──
    let actions_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(6.0),
            padding: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
            ..default()
        })
        .id();
    commands.entity(container).add_child(actions_row);

    // Upgrade button (ghost style)
    if let Some(ref bd) = bp.building {
        if level < 3 && !bd.level_upgrades.is_empty() {
            let upgrade_index = (level - 1) as usize;
            if upgrade_index < bd.level_upgrades.len() {
                let upgrade_data = &bd.level_upgrades[upgrade_index];
                let can_afford = upgrade_data.cost.can_afford(player_res);

                if is_upgrading {
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
                        .insert(BackgroundColor(theme::BG_SURFACE))
                        .with_children(|c| {
                            c.spawn((
                                Text::new(format!("Upgrading L{} — {:.0}s", target_lvl, remaining)),
                                TextFont { font_size: 11.0, ..default() },
                                TextColor(theme::WARNING),
                            ));
                            c.spawn(Node {
                                width: Val::Px(100.0),
                                height: Val::Px(4.0),
                                border_radius: BorderRadius::all(Val::Px(2.0)),
                                ..default()
                            })
                            .insert(BackgroundColor(theme::HP_BAR_BG))
                            .with_children(|bg| {
                                bg.spawn((
                                    UpgradeProgressBar,
                                    Node {
                                        width: Val::Percent(fraction * 100.0),
                                        height: Val::Percent(100.0),
                                        border_radius: BorderRadius::all(Val::Px(2.0)),
                                        ..default()
                                    },
                                    BackgroundColor(theme::WARNING),
                                ));
                            });
                        })
                        .id();
                    commands.entity(actions_row).add_child(upgrade_container);
                } else {
                    let cost_str = format_cost(&upgrade_data.cost);
                    let text_color = if can_afford {
                        theme::TEXT_PRIMARY
                    } else {
                        theme::DESTRUCTIVE
                    };

                    let btn = commands
                        .spawn((
                            Button,
                            UpgradeButton,
                            Node {
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            BorderColor::all(theme::BORDER_SUBTLE),
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
                                TextColor(theme::TEXT_SECONDARY),
                            ));
                        })
                        .id();
                    commands.entity(actions_row).add_child(btn);
                }
            }
        } else if level >= 3 {
            let max_label = commands
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(Color::srgba(0.33, 0.33, 0.33, 0.4)),
                ))
                .with_children(|pill| {
                    pill.spawn((
                        Text::new("MAX"),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(theme::TEXT_DISABLED),
                    ));
                })
                .id();
            commands.entity(actions_row).add_child(max_label);
        }
    }

    // Rally point button (ghost style)
    if let Some(ref bd) = bp.building {
        if !bd.trains.is_empty() {
            let is_rally_active = rally_mode.0;
            let rally_border = if is_rally_active {
                theme::ACCENT
            } else {
                theme::BORDER_SUBTLE
            };
            let rally_text = if is_rally_active { "Click Ground..." } else { "Set Rally" };
            let rally_text_color = if is_rally_active {
                theme::ACCENT
            } else {
                theme::TEXT_SECONDARY
            };
            let rally_bg = if is_rally_active {
                Color::srgba(0.29, 0.62, 1.0, 0.1)
            } else {
                Color::NONE
            };
            let rally_btn = commands
                .spawn((
                    Button,
                    RallyPointButton,
                    ActionTooltipTrigger { text: "Set rally point\nNew units will move here after training".to_string() },
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(rally_bg),
                    BorderColor::all(rally_border),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(rally_text),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(rally_text_color),
                    ));
                })
                .id();
            commands.entity(actions_row).add_child(rally_btn);
        }
    }

    // ── Tower auto-attack toggle (pill style) ──
    if kind == EntityKind::Tower {
        let is_enabled = auto_attack.map_or(true, |a| a.0);
        let toggle_bg = if is_enabled {
            Color::srgba(0.30, 0.69, 0.31, 0.15)
        } else {
            Color::srgba(0.80, 0.27, 0.27, 0.15)
        };
        let toggle_text = if is_enabled { "Auto-Attack: ON" } else { "Auto-Attack: OFF" };
        let toggle_color = if is_enabled {
            theme::SUCCESS
        } else {
            theme::DESTRUCTIVE
        };
        let toggle_btn = commands
            .spawn((
                Button,
                ToggleAutoAttackButton,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    margin: UiRect::top(Val::Px(2.0)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
                BackgroundColor(toggle_bg),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(toggle_text),
                    TextFont { font_size: 11.0, ..default() },
                    TextColor(toggle_color),
                ));
            })
            .id();
        commands.entity(container).add_child(toggle_btn);
    }

    // ── Training queue section ──
    if let Some(queue) = training_queue {
        if !queue.queue.is_empty() || queue.timer.is_some() {
            // Separator before queue
            let sep_queue = commands
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(theme::SEPARATOR),
                ))
                .id();
            commands.entity(container).add_child(sep_queue);

            spawn_training_queue_ui(commands, container, queue, icons, registry);
        }
    }

    // ── Separator before demolish ──
    let sep_demolish = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                margin: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::SEPARATOR),
        ))
        .id();
    commands.entity(container).add_child(sep_demolish);

    // ── Demolish: right-aligned text link ──
    let refund_pct = 50;
    let demolish_tooltip = format!(
        "Demolish building\nRefunds {}% of cost",
        refund_pct,
    );
    let demolish_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexEnd,
            ..default()
        })
        .id();
    commands.entity(container).add_child(demolish_row);

    let demolish_btn = commands
        .spawn((
            Button,
            DemolishButton,
            ActionTooltipTrigger { text: demolish_tooltip },
            Node {
                padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Demolish"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(theme::DESTRUCTIVE),
            ));
        })
        .id();
    commands.entity(demolish_row).add_child(demolish_btn);
}

fn spawn_training_queue_ui(
    commands: &mut Commands,
    parent: Entity,
    queue: &TrainingQueue,
    icons: &IconAssets,
    _registry: &BlueprintRegistry,
) {
    // "Queue (N)" header
    let header = commands
        .spawn((
            Text::new(format!("Queue ({})", queue.queue.len())),
            TextFont { font_size: 10.0, ..default() },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::bottom(Val::Px(2.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(header);

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
            BackgroundColor(theme::BG_SURFACE),
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
                    .insert(BackgroundColor(theme::HP_BAR_BG))
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
                            BackgroundColor(theme::ACCENT),
                        ));
                    });
                }

                // "x" cancel hint
                item.spawn((
                    Text::new("x"),
                    TextFont { font_size: 9.0, ..default() },
                    TextColor(Color::srgba(0.80, 0.27, 0.27, 0.3)),
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
        .insert(BackgroundColor(theme::BG_PANEL))
        .insert(Interaction::None)
        .id();
    commands.entity(parent).add_child(container);

    // Building name
    let name = commands
        .spawn((
            Text::new(format!("Building {}", kind.display_name())),
            TextFont { font_size: 16.0, ..default() },
            TextColor(theme::WARNING),
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
                BackgroundColor(theme::HP_BAR_BG),
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
                    BackgroundColor(theme::WARNING),
                ));
            })
            .id();
        commands.entity(container).add_child(bar_bg);

        let pct = commands
            .spawn((
                Text::new(pct_text),
                TextFont { font_size: 12.0, ..default() },
                TextColor(theme::TEXT_SECONDARY),
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
            BackgroundColor(Color::NONE),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("Cancel"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(theme::DESTRUCTIVE),
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
    completed: &[EntityKind],
    icons: &IconAssets,
    registry: &BlueprintRegistry,
) {
    let building_kinds = registry.building_kinds();
    // Filter to only buildings whose prerequisites are met
    let available: Vec<EntityKind> = building_kinds.iter().copied().filter(|kind| {
        let bp = registry.get(*kind);
        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        match prereq {
            None => true,
            Some(prereq_kind) => completed.contains(&prereq_kind),
        }
    }).collect();
    let total = available.len();

    for (i, kind) in available.iter().enumerate() {
        let bp = registry.get(*kind);
        let enabled = true;

        let (rot_deg, y_off) = fan_params(i, total);
        let label = kind.display_name();

        let total_cost = bp.cost.wood + bp.cost.copper + bp.cost.iron + bp.cost.gold + bp.cost.oil;
        let tier = CardTier::from_total_cost(total_cost);
        let tier_color = tier_accent_color(tier);
        let tier_srgba = tier_color.to_srgba();

        let text_color = if enabled {
            theme::TEXT_PRIMARY
        } else {
            Color::srgba(0.33, 0.33, 0.33, 0.7)
        };

        let cost_color = if enabled {
            theme::TEXT_SECONDARY
        } else {
            Color::srgba(0.33, 0.33, 0.33, 0.5)
        };

        // More overlap when there are many cards to keep the hand compact
        let overlap = if total > 8 { -30.0 } else if total > 5 { -25.0 } else { -20.0 };
        let margin_left = if i == 0 { 0.0 } else { overlap };

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
                CardAnimState::new(rot_deg, y_off, i),
                CardDragState::default(),
                tier,
                CardDealIn {
                    delay_timer: Timer::from_seconds(i as f32 * 0.10, TimerMode::Once),
                    anim_timer: Timer::from_seconds(0.45, TimerMode::Once),
                    started: false,
                },
                Node {
                    width: Val::Px(100.0),
                    height: Val::Px(140.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::FlexStart,
                    padding: UiRect::top(Val::Px(3.0)),
                    row_gap: Val::Px(4.0),
                    margin: UiRect {
                        left: Val::Px(margin_left),
                        bottom: Val::Px(-250.0),
                        ..default()
                    },
                    border: UiRect::all(Val::Px(1.5)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderColor::all(Color::srgba(tier_srgba.red, tier_srgba.green, tier_srgba.blue, 0.35)),
                BackgroundColor(theme::CARD_BG_BOTTOM),
                Transform::from_scale(Vec3::splat(0.2))
                    .with_rotation(Quat::from_rotation_z(rot_deg.to_radians())),
                ZIndex(i as i32),
                BoxShadow::new(
                    Color::srgba(0.0, 0.0, 0.0, 0.6),
                    Val::Px(0.0),
                    Val::Px(4.0),
                    Val::Px(0.0),
                    Val::Px(12.0),
                ),
            ))
            .with_children(|card_node| {
                // Tier accent strip at top
                card_node.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Px(3.0),
                        ..default()
                    },
                    BackgroundColor(tier_color),
                ));
                // Gradient overlay (lighter top half)
                card_node.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(50.0),
                        border_radius: BorderRadius {
                            top_left: Val::Px(10.0),
                            top_right: Val::Px(10.0),
                            bottom_left: Val::Px(0.0),
                            bottom_right: Val::Px(0.0),
                        },
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.18, 0.19, 0.23, 0.5)),
                ));
                // Icon container (52x52 with 48x48 image)
                card_node.spawn((
                    CardIconContainer,
                    Node {
                        width: Val::Px(52.0),
                        height: Val::Px(52.0),
                        margin: UiRect::top(Val::Px(10.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                )).with_children(|icon_container| {
                    icon_container.spawn((
                        ImageNode::new(icons.entity_icon(*kind)),
                        Node {
                            width: Val::Px(48.0),
                            height: Val::Px(48.0),
                            ..default()
                        },
                    ));
                });
                // Name (uppercase, 12px)
                card_node.spawn((
                    CardNameText,
                    Text::new(label.to_uppercase()),
                    TextFont { font_size: 12.0, ..default() },
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
                // Glow overlay (tier-colored)
                card_node.spawn((
                    CardGlow,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(tier_srgba.red, tier_srgba.green, tier_srgba.blue, 0.0)),
                ));
                // Border glow overlay
                card_node.spawn((
                    CardBorderGlow,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(tier_srgba.red, tier_srgba.green, tier_srgba.blue, 0.0)),
                    BackgroundColor(Color::NONE),
                ));
                // Shine sweep effect
                card_node.spawn((
                    CardShineEffect,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(-30.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(30.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(theme::CARD_SHINE),
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
                    width: Val::Px(14.0),
                    height: Val::Px(14.0),
                    ..default()
                },
            ));
            entry.spawn((
                Text::new(format!("{}", amount)),
                TextFont { font_size: 10.0, ..default() },
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
            StandardButton,
            ActionTooltipTrigger { text: tooltip },
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                row_gap: Val::Px(2.0),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::BTN_PRIMARY),
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
                TextFont { font_size: 13.0, ..default() },
                TextColor(theme::TEXT_PRIMARY),
            ));
            btn.spawn((
                Text::new(cost_str),
                TextFont { font_size: 11.0, ..default() },
                TextColor(theme::TEXT_SECONDARY),
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
    interactions: Query<(Entity, &Interaction, &BuildButton, Option<&BuildCard>, Option<&CardDragState>), Changed<Interaction>>,
    mut placement: ResMut<BuildingPlacementState>,
    all_completed: Res<AllCompletedBuildings>,
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for (_entity, interaction, build_btn, card, _drag) in &interactions {
        // Skip cards — they use the drag system for placement
        if card.is_some() {
            continue;
        }
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            let kind = build_btn.0;
            let bp = registry.get(kind);

            let prereq_met = if let Some(ref bd) = bp.building {
                match bd.prerequisite {
                    None => true,
                    Some(prereq_kind) => all_completed.has(&active_player.0, prereq_kind),
                }
            } else {
                true
            };
            if !prereq_met {
                continue;
            }

            let player_res = all_resources.get(&active_player.0);
            if !bp.cost.can_afford(player_res) {
                continue;
            }

            placement.mode = PlacementMode::Placing(kind);
            placement.awaiting_release = true;
        }
    }
}

fn handle_train_buttons(
    interactions: Query<(&Interaction, &TrainButton), Changed<Interaction>>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
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
        let player_res = all_resources.get(&active_player.0);
        if !bp.cost.can_afford(player_res) {
            continue;
        }

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queues.get_mut(building_entity) {
                let player_res_mut = all_resources.get_mut(&active_player.0);
                bp.cost.deduct(player_res_mut);
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
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingLevel, &BuildingState, &Faction),
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

        if let Ok((entity, kind, level, state, faction)) = selected_buildings.single() {
            if *state != BuildingState::Complete {
                continue;
            }
            let player_res = all_resources.get_mut(&active_player.0);
            buildings::start_upgrade(
                &mut commands,
                entity,
                level.0,
                *kind,
                &registry,
                player_res,
                *faction,
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
                    BackgroundColor(theme::BG_PANEL),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Demolish?"),
                        TextFont { font_size: 14.0, ..default() },
                        TextColor(theme::DESTRUCTIVE),
                    ));
                    panel.spawn((
                        Text::new(refund_str),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(theme::TEXT_SECONDARY),
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
                            StandardButton,
                            ConfirmDemolishButton,
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(theme::DESTRUCTIVE),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("Yes"),
                                TextFont { font_size: 12.0, ..default() },
                                TextColor(theme::TEXT_PRIMARY),
                            ));
                        });

                        row.spawn((
                            Button,
                            StandardButton,
                            CancelDemolishButton,
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(theme::BTN_PRIMARY),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("No"),
                                TextFont { font_size: 12.0, ..default() },
                                TextColor(theme::TEXT_PRIMARY),
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
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
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
                    let player_res = all_resources.get_mut(&active_player.0);
                    player_res.wood += bp.cost.wood;
                    player_res.copper += bp.cost.copper;
                    player_res.iron += bp.cost.iron;
                    player_res.gold += bp.cost.gold;
                    player_res.oil += bp.cost.oil;
                }
                commands.entity(entity).try_despawn();
            } else {
                buildings::start_demolish(&mut commands, entity, transform);
            }
        }

        // Remove confirm panel
        for panel in &confirm_panels {
            commands.entity(panel).try_despawn();
        }
    }

    // Handle cancel
    for interaction in &cancel_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        for panel in &confirm_panels {
            commands.entity(panel).try_despawn();
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
        (Changed<Interaction>, With<StandardButton>),
    >,
) {
    for (interaction, mut bg) in &mut query {
        *bg = match interaction {
            Interaction::Pressed => BackgroundColor(theme::BTN_PRESSED),
            Interaction::Hovered => BackgroundColor(theme::BTN_HOVER),
            Interaction::None => BackgroundColor(theme::BTN_PRIMARY),
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
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
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
                    let player_res = all_resources.get_mut(&active_player.0);
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
                        BackgroundColor(theme::BG_PANEL),
                        GlobalZIndex(100),
                    ))
                    .with_children(|tt| {
                        tt.spawn((
                            Text::new(&trigger.text),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(theme::TEXT_PRIMARY),
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
                            commands.entity(child).try_despawn();
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

fn ease_out_elastic(t: f32) -> f32 {
    if t <= 0.0 { return 0.0; }
    if t >= 1.0 { return 1.0; }
    let p = 0.35;
    let s = p / 4.0;
    (2.0_f32.powf(-10.0 * t) * ((t - s) * std::f32::consts::TAU / p).sin()) + 1.0
}

fn tier_accent_color(tier: CardTier) -> Color {
    match tier {
        CardTier::Common => theme::TIER_COMMON,
        CardTier::Uncommon => theme::TIER_UNCOMMON,
        CardTier::Rare => theme::TIER_RARE,
        CardTier::Epic => theme::TIER_EPIC,
    }
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

        let t_raw = deal.anim_timer.fraction();
        let t_elastic = ease_out_elastic(t_raw);
        let t_opacity = ease_out_cubic(t_raw.min(0.5) * 2.0); // Fade in faster

        let start_y = -250.0_f32;
        let target_y = anim.target_offset_y;
        anim.offset_y = start_y + (target_y - start_y) * t_elastic;
        anim.scale = 0.2 + (0.58 - 0.2) * t_elastic;
        anim.opacity = t_opacity;

        node.margin.bottom = Val::Px(-anim.offset_y);
        tf.scale = Vec3::splat(anim.scale);
        tf.rotation = Quat::from_rotation_z(anim.rotation_deg.to_radians());
    }
}

fn card_hover_system(
    read_cards: Query<(&Interaction, &BuildCard, &CardDragState), Without<CardPlayOut>>,
    mut write_cards: Query<(&BuildCard, &mut CardAnimState, &CardDragState), Without<CardPlayOut>>,
) {
    // Pass 1: find hovered card index
    let mut hovered_index: Option<usize> = None;
    let mut pressed_index: Option<usize> = None;
    for (interaction, card, drag) in &read_cards {
        if drag.dragging { continue; }
        match interaction {
            Interaction::Pressed => {
                pressed_index = Some(card.index);
                hovered_index = Some(card.index);
            }
            Interaction::Hovered => {
                if hovered_index.is_none() {
                    hovered_index = Some(card.index);
                }
            }
            Interaction::None => {}
        }
    }

    // Pass 2: update all cards based on hover context
    for (card, mut anim, drag) in &mut write_cards {
        if drag.dragging { continue; }
        let (rest_rot, rest_y) = fan_params(card.index, card.total);

        if let Some(hi) = hovered_index {
            let dist = (card.index as i32 - hi as i32).unsigned_abs() as usize;
            if dist == 0 {
                // Hovered/pressed card — lifts up, scales bigger
                anim.target_offset_y = rest_y - 35.0;
                anim.target_scale = if pressed_index.is_some() { 0.95 } else { 1.05 };
                anim.target_rotation_deg = 0.0;
            } else if dist == 1 {
                // Immediate neighbors — push apart harder
                let push_dir = if (card.index as i32) < (hi as i32) { -1.0 } else { 1.0 };
                anim.target_rotation_deg = rest_rot + push_dir * 16.0;
                anim.target_scale = 0.63;
                anim.target_offset_y = rest_y;
            } else if dist == 2 {
                // Second neighbors — gentle push
                let push_dir = if (card.index as i32) < (hi as i32) { -1.0 } else { 1.0 };
                anim.target_rotation_deg = rest_rot + push_dir * 6.0;
                anim.target_scale = 0.54;
                anim.target_offset_y = rest_y;
            } else {
                // Far cards — shrink slightly for contrast
                anim.target_offset_y = rest_y;
                anim.target_scale = 0.54;
                anim.target_rotation_deg = rest_rot;
            }
        } else {
            // No hover — all cards at rest
            anim.target_offset_y = rest_y;
            anim.target_scale = 0.58;
            anim.target_rotation_deg = rest_rot;
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
        // Spiral zoom: grow + spin + fly up + fade
        anim.scale = 0.58 + 0.6 * t;
        anim.offset_y = anim.offset_y - 120.0 * time.delta_secs();
        anim.rotation_deg += 180.0 * time.delta_secs();
        anim.opacity = (1.0 - t * t).max(0.0); // Quadratic fade
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
                anim.target_scale = 0.58;
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
    all_completed: Res<AllCompletedBuildings>,
    active_player: Res<ActivePlayer>,
    registry: Res<BlueprintRegistry>,
    mut cards: Query<
        (&mut CardAnimState, &mut Node, &mut Transform, &mut BackgroundColor, &BuildCard, &Interaction, &mut ZIndex, &CardDragState),
        Without<CardDealIn>,
    >,
) {
    let dt = time.delta_secs();
    let speed = 16.0;
    let alpha = 1.0 - (-speed * dt).exp();

    let completed = all_completed.completed_for(&active_player.0);
    for (mut anim, mut node, mut tf, mut bg, card, interaction, mut z_index, drag) in &mut cards {
        if drag.dragging { continue; }

        anim.offset_y += (anim.target_offset_y - anim.offset_y) * alpha;
        anim.scale += (anim.target_scale - anim.scale) * alpha;
        anim.rotation_deg += (anim.target_rotation_deg - anim.rotation_deg) * alpha;
        anim.opacity += (anim.target_opacity - anim.opacity) * alpha;
        anim.tilt_x_deg += (anim.target_tilt_x_deg - anim.tilt_x_deg) * alpha;

        // Apply idle breathing as visual-only offset (only when at rest)
        let mut display_y = anim.offset_y;
        let mut display_scale = anim.scale;
        if *interaction == Interaction::None {
            let t = time.elapsed_secs();
            let phase = anim.idle_phase;
            display_y += (t * 0.8 * std::f32::consts::TAU + phase).sin() * 2.0;
            display_scale += (t * 1.2 * std::f32::consts::TAU + phase * 0.7).sin() * 0.008;
        }

        node.margin.bottom = Val::Px(-display_y);
        tf.scale = Vec3::splat(display_scale);
        // Combine Z rotation (fan) with X rotation (tilt)
        tf.rotation = Quat::from_rotation_z(anim.rotation_deg.to_radians())
            * Quat::from_rotation_x(anim.tilt_x_deg.to_radians());

        // Hovered card renders on top
        *z_index = match interaction {
            Interaction::Hovered | Interaction::Pressed => ZIndex(100),
            Interaction::None => ZIndex(card.index as i32),
        };

        let bp = registry.get(card.building_kind);
        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let prereq_met = match prereq {
            None => true,
            Some(prereq_kind) => completed.contains(&prereq_kind),
        };
        let base = if card.enabled {
            Color::srgba(0.10, 0.11, 0.14, 0.96 * anim.opacity)
        } else if prereq_met {
            Color::srgba(0.10, 0.11, 0.14, 0.78 * anim.opacity)
        } else {
            Color::srgba(0.09, 0.09, 0.09, 0.5 * anim.opacity)
        };
        *bg = BackgroundColor(base);
    }
}

fn update_card_states(
    all_completed: Res<AllCompletedBuildings>,
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    registry: Res<BlueprintRegistry>,
    mut cards: Query<(&mut BuildCard, &Children)>,
    mut name_texts: Query<&mut TextColor, (With<CardNameText>, Without<CardCostEntry>)>,
    cost_entries: Query<(&CardCostEntry, &Children), Without<CardNameText>>,
    mut cost_text_colors: Query<&mut TextColor, (Without<CardNameText>, Without<CardCostEntry>)>,
    tooltip_wrappers: Query<&Children, With<CardTooltip>>,
    mut texts: Query<&mut Text>,
) {
    let completed = all_completed.completed_for(&active_player.0);
    let player_res = all_resources.get(&active_player.0);
    for (mut card, card_children) in &mut cards {
        let kind = card.building_kind;
        let bp = registry.get(kind);

        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let prereq_met = match prereq {
            None => true,
            Some(prereq_kind) => completed.contains(&prereq_kind),
        };

        let can_afford = bp.cost.can_afford(player_res);
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
                    text_color.0 = Color::srgba(0.33, 0.33, 0.33, 0.7);
                } else if !can_afford {
                    text_color.0 = Color::srgba(0.88, 0.88, 0.88, 0.9);
                } else {
                    text_color.0 = theme::TEXT_PRIMARY;
                }
            }

            if let Ok((cost_entry, entry_children)) = cost_entries.get(child) {
                let has_enough = player_res.get(cost_entry.resource_type) >= cost_entry.amount;
                let entry_color = if !prereq_met {
                    Color::srgba(0.4, 0.4, 0.4, 0.5)
                } else if !has_enough {
                    theme::DESTRUCTIVE
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
    all_completed: Res<AllCompletedBuildings>,
    active_player: Res<ActivePlayer>,
    registry: Res<BlueprintRegistry>,
    cards: Query<(&Interaction, &BuildCard, &Children, &CardTier), Without<CardPlayOut>>,
    mut glows: Query<&mut BackgroundColor, With<CardGlow>>,
) {
    let speed = 12.0;
    let alpha = 1.0 - (-speed * time.delta_secs()).exp();

    let completed = all_completed.completed_for(&active_player.0);
    for (interaction, card, children, tier) in &cards {
        let bp = registry.get(card.building_kind);
        let prereq = bp.building.as_ref().and_then(|b| b.prerequisite);
        let prereq_met = match prereq {
            None => true,
            Some(prereq_kind) => completed.contains(&prereq_kind),
        };
        if !prereq_met {
            continue;
        }
        let target_opacity = match interaction {
            Interaction::Hovered | Interaction::Pressed => 0.18,
            Interaction::None => 0.0,
        };

        let tc = tier_accent_color(*tier).to_srgba();
        for child in children.iter() {
            if let Ok(mut glow_bg) = glows.get_mut(child) {
                let current = glow_bg.0.to_srgba();
                let new_a = current.alpha + (target_opacity - current.alpha) * alpha;
                *glow_bg = BackgroundColor(Color::srgba(tc.red, tc.green, tc.blue, new_a));
            }
        }
    }
}

// Breathing is applied directly in card_anim_lerp_system as a visual offset,
// not by modifying targets (which causes fighting with hover system).

// ── Pseudo-3D Tilt System ──
fn card_tilt_update_system(
    windows: Query<&Window>,
    mut cards: Query<(&Interaction, &mut CardAnimState, &CardDragState), (With<BuildCard>, Without<CardPlayOut>)>,
) {
    let Ok(window) = windows.single() else { return; };
    let cursor_pos = window.cursor_position().unwrap_or(Vec2::new(-1000.0, -1000.0));
    let window_center_x = window.width() / 2.0;

    for (interaction, mut anim, drag) in &mut cards {
        if drag.dragging {
            anim.target_tilt_x_deg = 0.0;
            continue;
        }
        match interaction {
            Interaction::Hovered | Interaction::Pressed => {
                // Use cursor position relative to window center as proxy
                let rel_x = ((cursor_pos.x - window_center_x) / (window.width() * 0.3)).clamp(-1.0, 1.0);
                anim.target_tilt_x_deg = rel_x * 4.0; // Subtle tilt to avoid hitbox shifting
            }
            Interaction::None => {
                anim.target_tilt_x_deg = 0.0;
            }
        }
    }
}

// ── Border Glow System ──
fn card_border_glow_system(
    time: Res<Time>,
    mut cards: Query<(&Interaction, &Children, &CardTier, &mut CardAnimState), (With<BuildCard>, Without<CardPlayOut>)>,
    mut border_glows: Query<&mut BorderColor, With<CardBorderGlow>>,
) {
    let dt = time.delta_secs();
    let speed = 10.0;
    let alpha = 1.0 - (-speed * dt).exp();
    let t = time.elapsed_secs();

    for (interaction, children, tier, mut anim) in &mut cards {
        anim.glow_pulse += dt;
        let tc = tier_accent_color(*tier).to_srgba();

        let target_a = match interaction {
            Interaction::Hovered | Interaction::Pressed => {
                let pulse = (t * 4.0).sin() * 0.15;
                (0.7 + pulse).clamp(0.0, 1.0)
            }
            Interaction::None => 0.0,
        };

        for child in children.iter() {
            if let Ok(mut border) = border_glows.get_mut(child) {
                let current_a = border.top.to_srgba().alpha;
                let new_a = current_a + (target_a - current_a) * alpha;
                let c = Color::srgba(tc.red, tc.green, tc.blue, new_a);
                *border = BorderColor::all(c);
            }
        }
    }
}

// ── Shine Sweep System ──
fn card_shine_sweep_system(
    time: Res<Time>,
    cards: Query<(&Interaction, &Children), (With<BuildCard>, Without<CardPlayOut>)>,
    mut shines: Query<&mut Node, With<CardShineEffect>>,
) {
    let t = time.elapsed_secs();

    for (interaction, children) in &cards {
        let is_hovered = matches!(interaction, Interaction::Hovered | Interaction::Pressed);

        for child in children.iter() {
            if let Ok(mut node) = shines.get_mut(child) {
                if is_hovered {
                    // Sweep cycle: 0.6s sweep + 1.9s pause = 2.5s total
                    let cycle = t % 2.5;
                    if cycle < 0.6 {
                        let sweep_t = cycle / 0.6;
                        // Move from -30% to 130%
                        let left_pct = -30.0 + sweep_t * 160.0;
                        node.left = Val::Percent(left_pct);
                    } else {
                        node.left = Val::Percent(-30.0); // Hidden
                    }
                } else {
                    node.left = Val::Percent(-30.0); // Hidden when not hovered
                }
            }
        }
    }
}

// ── Drag & Drop Systems ──

fn card_drag_pickup_system(
    mut cards: Query<(&Interaction, &BuildCard, &mut CardDragState)>,
    windows: Query<&Window>,
) {
    let Ok(window) = windows.single() else { return; };
    let cursor_pos = window.cursor_position().unwrap_or(Vec2::ZERO);

    for (interaction, card, mut drag) in &mut cards {
        if !card.enabled { continue; }
        if *interaction == Interaction::Pressed && !drag.dragging {
            drag.dragging = true;
            drag.screen_pos = cursor_pos;
            drag.pickup_origin = cursor_pos;
            drag.velocity = Vec2::ZERO;
            drag.drag_distance = 0.0;
        }
    }
}

fn card_drag_follow_system(
    time: Res<Time>,
    windows: Query<&Window>,
    mut cards: Query<(&mut CardDragState, &mut CardAnimState, &mut Node, &mut ZIndex), With<BuildCard>>,
) {
    let Ok(window) = windows.single() else { return; };
    let cursor_pos = window.cursor_position().unwrap_or(Vec2::ZERO);
    let dt = time.delta_secs();

    for (mut drag, mut anim, mut node, mut z_index) in &mut cards {
        if !drag.dragging { continue; }

        // Track total distance from pickup origin
        drag.drag_distance = (cursor_pos - drag.pickup_origin).length();
        drag.screen_pos = cursor_pos;

        // Only switch to absolute positioning once past drag threshold
        if drag.drag_distance < 5.0 {
            continue;
        }

        // Position card absolutely
        node.position_type = PositionType::Absolute;
        node.left = Val::Px(cursor_pos.x - 50.0);
        node.top = Val::Px(cursor_pos.y - 70.0);
        node.margin = UiRect::ZERO;

        // Rotation based on frame-to-frame movement
        let vel_x = cursor_pos.x - drag.pickup_origin.x;
        let vel_rot = (vel_x * 0.03).clamp(-12.0, 12.0);
        anim.rotation_deg = vel_rot;
        anim.scale = 1.05;
        anim.opacity = 0.92;
        anim.tilt_x_deg = 0.0;
        *z_index = ZIndex(200);
    }
}

fn card_drag_gap_system(
    cards: Query<(&BuildCard, &CardDragState)>,
    mut other_cards: Query<(&BuildCard, &mut CardAnimState, &CardDragState), Without<CardPlayOut>>,
) {
    // Find dragged card index
    let mut dragged_index: Option<usize> = None;
    for (card, drag) in &cards {
        if drag.dragging && drag.drag_distance > 5.0 {
            dragged_index = Some(card.index);
            break;
        }
    }

    if let Some(di) = dragged_index {
        // Recompute fan positions as if dragged card removed
        for (card, mut anim, drag) in &mut other_cards {
            if drag.dragging { continue; }
            let adjusted_index = if card.index > di { card.index - 1 } else { card.index };
            let adjusted_total = card.total - 1;
            if adjusted_total > 0 {
                let (rot, y_off) = fan_params(adjusted_index, adjusted_total);
                anim.target_rotation_deg = rot;
                anim.target_offset_y = y_off;
            }
        }
    }
}

fn card_drag_release_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    mut cards: Query<(Entity, &BuildCard, &mut CardDragState, &mut CardAnimState, &mut Node)>,
    mut placement: ResMut<BuildingPlacementState>,
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    if !mouse.just_released(MouseButton::Left) { return; }
    let Ok(window) = windows.single() else { return; };
    let window_height = window.height();

    for (entity, card, mut drag, mut anim, mut node) in &mut cards {
        if !drag.dragging { continue; }
        drag.dragging = false;

        // Reset positioning
        node.position_type = PositionType::Relative;
        node.left = Val::Auto;
        node.top = Val::Auto;
        let overlap = if card.total > 8 { -30.0 } else if card.total > 5 { -25.0 } else { -20.0 };
        let margin_left = if card.index == 0 { 0.0 } else { overlap };
        node.margin = UiRect {
            left: Val::Px(margin_left),
            bottom: Val::Px(-anim.offset_y),
            ..default()
        };

        let kind = card.building_kind;
        let bp = registry.get(kind);
        let player_res = all_resources.get(&active_player.0);
        let can_afford = bp.cost.can_afford(player_res);

        if drag.drag_distance < 5.0 {
            // Click — enter placement mode
            if can_afford {
                ui_clicked.0 = 2;
                commands.entity(entity).insert(CardPlayOut {
                    timer: Timer::from_seconds(0.35, TimerMode::Once),
                });
                placement.mode = PlacementMode::Placing(kind);
                placement.awaiting_release = true;
            }
        } else if drag.screen_pos.y < window_height * 0.7 && can_afford {
            // Released in upper 70% — place building
            ui_clicked.0 = 2;
            commands.entity(entity).insert(CardPlayOut {
                timer: Timer::from_seconds(0.35, TimerMode::Once),
            });
            placement.mode = PlacementMode::Placing(kind);
            placement.awaiting_release = false;
        } else {
            // Spring back to fan
            let (rot, y_off) = fan_params(card.index, card.total);
            anim.target_offset_y = y_off;
            anim.target_scale = 0.58;
            anim.target_rotation_deg = rot;
            anim.target_opacity = 1.0;
            commands.entity(entity).insert(CardSpringBack {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
            });
        }

        drag.velocity = Vec2::ZERO;
        drag.drag_distance = 0.0;
    }
}
