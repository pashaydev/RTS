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
                update_selected_panel,
                update_action_bar,
                handle_build_buttons,
                handle_train_buttons,
                button_hover_visual,
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

    // ── Left panel — selected units / buildings ──
    commands
        .spawn((
            SelectedUnitsPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(50.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                min_width: Val::Px(120.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            BorderRadius::all(Val::Px(4.0)),
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Selected"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
            ));
            panel.spawn((
                SelectedUnitsSummaryText,
                Text::new(""),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });

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
                    column_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                BorderRadius::all(Val::Px(6.0)),
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

fn update_selected_panel(
    selected_units: Query<&UnitType, (With<Unit>, With<Selected>)>,
    selected_buildings: Query<(&BuildingType, &BuildingState), (With<Building>, With<Selected>)>,
    mut panel_q: Query<&mut Visibility, With<SelectedUnitsPanel>>,
    mut summary_q: Query<&mut Text, With<SelectedUnitsSummaryText>>,
) {
    let mut workers = 0u32;
    let mut soldiers = 0u32;
    let mut archers = 0u32;
    let mut tanks = 0u32;

    for ut in &selected_units {
        match ut {
            UnitType::Worker => workers += 1,
            UnitType::Soldier => soldiers += 1,
            UnitType::Archer => archers += 1,
            UnitType::Tank => tanks += 1,
        }
    }

    let unit_total = workers + soldiers + archers + tanks;
    let building_count = selected_buildings.iter().count();
    let total = unit_total + building_count as u32;

    if let Ok(mut vis) = panel_q.get_single_mut() {
        *vis = if total > 0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    if let Ok(mut text) = summary_q.get_single_mut() {
        let mut lines = Vec::new();
        if workers > 0 {
            lines.push(format!("{} Worker(s)", workers));
        }
        if soldiers > 0 {
            lines.push(format!("{} Soldier(s)", soldiers));
        }
        if archers > 0 {
            lines.push(format!("{} Archer(s)", archers));
        }
        if tanks > 0 {
            lines.push(format!("{} Tank(s)", tanks));
        }
        for (bt, state) in &selected_buildings {
            let state_str = match state {
                BuildingState::UnderConstruction => " (building...)",
                BuildingState::Complete => "",
            };
            lines.push(format!("{}{}", building_type_label(*bt), state_str));
        }
        **text = lines.join("\n");
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
) {
    // Only rebuild if something changed
    let has_new = !added_selected.is_empty();
    let has_removed = removed_selected.read().count() > 0;
    let has_building_change = !changed_buildings.is_empty();
    let completed_changed = completed.is_changed();

    if !has_new && !has_removed && !has_building_change && !completed_changed {
        return;
    }

    let Ok(bar_entity) = action_bar.get_single() else {
        return;
    };

    // Despawn all children
    if let Ok(children) = children_q.get(bar_entity) {
        for child in children.iter() {
            commands.entity(*child).despawn_recursive();
        }
    }

    // Check if a building is selected
    if let Ok((bt, state)) = selected_buildings.get_single() {
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
        // Nothing selected - show building placement buttons
        let building_types = [
            BuildingType::Base,
            BuildingType::Barracks,
            BuildingType::Workshop,
            BuildingType::Tower,
            BuildingType::Storage,
        ];
        for bt in building_types {
            let enabled = match building_prerequisite(bt) {
                None => true,
                Some(BuildingType::Base) => completed.has_base,
                Some(_) => false,
            };
            spawn_build_button(&mut commands, bar_entity, bt, enabled, &icons);
        }
    }
}

fn spawn_build_button(
    commands: &mut Commands,
    parent: Entity,
    bt: BuildingType,
    enabled: bool,
    icons: &IconAssets,
) {
    let label = building_type_label(bt);
    let (w, c, i, g, o, _) = building_cost(bt);
    let cost_str = format_cost(w, c, i, g, o);

    let color = if enabled {
        Color::srgba(0.25, 0.25, 0.3, 0.9)
    } else {
        Color::srgba(0.15, 0.15, 0.15, 0.6)
    };

    let child = commands
        .spawn((
            BuildButton(bt),
            Button,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(color),
            BorderRadius::all(Val::Px(4.0)),
        ))
        .with_children(|btn| {
            btn.spawn((
                ImageNode::new(icons.building_icon(bt)),
                Node {
                    width: Val::Px(32.0),
                    height: Val::Px(32.0),
                    ..default()
                },
            ));
            btn.spawn((
                Text::new(label),
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
                ..default()
            },
            BackgroundColor(Color::srgba(0.25, 0.25, 0.3, 0.9)),
            BorderRadius::all(Val::Px(4.0)),
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
    interactions: Query<(&Interaction, &BuildButton), Changed<Interaction>>,
    mut placement: ResMut<BuildingPlacementState>,
    completed: Res<CompletedBuildings>,
    player_res: Res<PlayerResources>,
) {
    for (interaction, build_btn) in &interactions {
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

            placement.mode = PlacementMode::Placing(bt);
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
        (Changed<Interaction>, With<Button>),
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
