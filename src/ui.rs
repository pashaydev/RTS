use bevy::prelude::*;

use crate::components::*;
use crate::units::spawn_unit_of_type;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (
                    update_resource_texts,
                    update_selected_panel,
                    handle_spawn_buttons,
                    button_hover_visual,
                ),
            );
    }
}

fn resource_color(rt: ResourceType) -> Color {
    match rt {
        ResourceType::Wood => Color::srgb(0.15, 0.6, 0.1),
        ResourceType::Copper => Color::srgb(0.8, 0.5, 0.2),
        ResourceType::Iron => Color::srgb(0.6, 0.6, 0.65),
        ResourceType::Gold => Color::srgb(0.95, 0.85, 0.2),
        ResourceType::Oil => Color::srgb(0.2, 0.2, 0.25),
    }
}

fn unit_type_color(ut: UnitType) -> Color {
    match ut {
        UnitType::Worker => Color::srgb(0.9, 0.8, 0.2),
        UnitType::Soldier => Color::srgb(0.8, 0.15, 0.15),
        UnitType::Archer => Color::srgb(0.15, 0.7, 0.2),
        UnitType::Tank => Color::srgb(0.35, 0.35, 0.4),
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

fn spawn_hud(mut commands: Commands) {
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
                    // Colored square
                    entry.spawn((
                        Node {
                            width: Val::Px(16.0),
                            height: Val::Px(16.0),
                            ..default()
                        },
                        BackgroundColor(resource_color(rt)),
                    ));
                    // Text
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

    // ── Left panel — selected units ──
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

    // ── Bottom panel — spawn buttons ──
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|wrapper| {
            wrapper
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                    BorderRadius::all(Val::Px(6.0)),
                ))
                .with_children(|panel| {
                    let unit_types = [
                        UnitType::Worker,
                        UnitType::Soldier,
                        UnitType::Archer,
                        UnitType::Tank,
                    ];
                    for ut in unit_types {
                        panel
                            .spawn((
                                SpawnButton(ut),
                                Button,
                                Node {
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(6.0),
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.25, 0.25, 0.3, 0.9)),
                                BorderRadius::all(Val::Px(4.0)),
                            ))
                            .with_children(|btn| {
                                // Color icon
                                btn.spawn((
                                    Node {
                                        width: Val::Px(14.0),
                                        height: Val::Px(14.0),
                                        ..default()
                                    },
                                    BackgroundColor(unit_type_color(ut)),
                                    BorderRadius::all(Val::Px(2.0)),
                                ));
                                // Label
                                btn.spawn((
                                    Text::new(unit_type_label(ut)),
                                    TextFont {
                                        font_size: 16.0,
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                ));
                            });
                    }
                });
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
    selected: Query<&UnitType, With<Selected>>,
    mut panel_q: Query<&mut Visibility, With<SelectedUnitsPanel>>,
    mut summary_q: Query<&mut Text, With<SelectedUnitsSummaryText>>,
) {
    let mut workers = 0u32;
    let mut soldiers = 0u32;
    let mut archers = 0u32;
    let mut tanks = 0u32;

    for ut in &selected {
        match ut {
            UnitType::Worker => workers += 1,
            UnitType::Soldier => soldiers += 1,
            UnitType::Archer => archers += 1,
            UnitType::Tank => tanks += 1,
        }
    }

    let total = workers + soldiers + archers + tanks;

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
        **text = lines.join("\n");
    }
}

fn handle_spawn_buttons(
    mut commands: Commands,
    interactions: Query<(&Interaction, &SpawnButton), Changed<Interaction>>,
    unit_mats: Res<UnitMaterials>,
    unit_meshes: Res<UnitMeshes>,
) {
    for (interaction, spawn_btn) in &interactions {
        if *interaction == Interaction::Pressed {
            spawn_unit_of_type(
                &mut commands,
                &unit_mats,
                &unit_meshes,
                spawn_btn.0,
                Vec3::new(0.0, 0.0, 3.0),
            );
        }
    }
}

fn button_hover_visual(
    mut query: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<SpawnButton>)>,
) {
    for (interaction, mut bg) in &mut query {
        *bg = match interaction {
            Interaction::Pressed => BackgroundColor(Color::srgba(0.15, 0.15, 0.2, 0.9)),
            Interaction::Hovered => BackgroundColor(Color::srgba(0.35, 0.35, 0.4, 0.9)),
            Interaction::None => BackgroundColor(Color::srgba(0.25, 0.25, 0.3, 0.9)),
        };
    }
}
