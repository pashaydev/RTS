use bevy::prelude::*;

use super::core::components as ui_components;
use super::core::fonts::UiFonts;
use super::core::framework::{spawn_widget_frame, WidgetId, WidgetRegistry};
use super::core::hud::MainHudRoot;
use super::core::shared::{hp_color, spawn_hp_bar};
use super::group_hotkeys_widget::{group_color, ControlGroups};
use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme::Theme;

pub struct SelectionWidgetPlugin;

#[derive(Component)]
struct FormationControls;

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
            rebuild_selection_panel.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            update_label_visibility_footer.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            (
                update_hp_bars,
                handle_unit_card_click,
                handle_formation_preset_click,
                handle_toggle_unit_labels_click,
                clear_stale_inspected,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn spawn_selection_widget(
    mut commands: Commands,
    registry: Res<WidgetRegistry>,
    theme: Res<Theme>,
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
        &theme,
    );
    commands.entity(selection_content).insert((
        Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            row_gap: Val::Px(8.0),
            ..default()
        },
        SelectionInfoPanel,
    ));

    let body = commands
        .spawn((
            SelectionInfoBody,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .id();
    commands.entity(selection_content).add_child(body);

    spawn_selection_footer(&mut commands, selection_content, true, &theme);
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
    theme_res: Res<Theme>,
    mut hp_fills: Query<(&HpBarFill, &mut Node, &mut BackgroundColor)>,
    healths: Query<&Health>,
) {
    for (hp_bar, mut node, mut bg) in &mut hp_fills {
        if let Ok(health) = healths.get(hp_bar.0) {
            let pct = (health.current / health.max).clamp(0.0, 1.0) * 100.0;
            node.width = Val::Percent(pct);
            *bg = BackgroundColor(hp_color(&theme_res, health.current, health.max));
        }
    }
}

fn handle_formation_preset_click(
    interactions: Query<
        (&Interaction, &FormationPresetButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut formation: ResMut<ActiveFormation>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, preset) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        formation.formation = preset.0;
    }
}

fn handle_toggle_unit_labels_click(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ToggleUnitLabelsButton>)>,
    mut label_visibility: ResMut<EntityLabelVisibility>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        label_visibility.show_unit_labels = !label_visibility.show_unit_labels;
    }
}

fn update_label_visibility_footer(
    label_visibility: Res<EntityLabelVisibility>,
    theme: Res<Theme>,
    footer_q: Query<Entity, With<SelectionFooter>>,
    mut button_q: Query<
        (&mut BackgroundColor, &mut ButtonAnimState),
        With<ToggleUnitLabelsButton>,
    >,
    mut button_text_q: Query<&mut Text, With<ToggleUnitLabelsButtonText>>,
    mut button_text_color_q: Query<&mut TextColor, With<ToggleUnitLabelsButtonText>>,
    mut status_text_q: Query<&mut Text, (With<UnitLabelsStatusText>, Without<ToggleUnitLabelsButtonText>)>,
) {
    if !label_visibility.is_changed() {
        return;
    }
    if footer_q.is_empty() {
        return;
    }

    let (bg_color, text, text_color, status) = label_visibility_presentation(&theme, label_visibility.show_unit_labels);
    for (mut bg, mut anim) in &mut button_q {
        *bg = BackgroundColor(bg_color);
        let bg_array = bg_color.to_srgba().to_f32_array();
        anim.bg_current = bg_array;
        anim.bg_target = bg_array;
    }
    for mut text_component in &mut button_text_q {
        **text_component = text.to_string();
    }
    for mut color_component in &mut button_text_color_q {
        *color_component = TextColor(text_color);
    }
    for mut text_component in &mut status_text_q {
        **text_component = status.to_string();
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
    theme: Res<Theme>,
    inspected: Res<InspectedEnemy>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    icons: Res<IconAssets>,
    panel_q: Query<Entity, With<SelectionInfoBody>>,
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
    inspected_queries: (
        Query<&Faction>,
        Query<
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
        Query<(&EntityKind, &BuildingState, &Health), With<Building>>,
    ),
    control_groups: Res<ControlGroups>,
    formation: Res<ActiveFormation>,
) {
    let (faction_q, inspected_unit_q, inspected_building_q) = inspected_queries;
    let Ok(panel_entity) = panel_q.single() else {
        return;
    };
    let panel_is_empty = children_q
        .get(panel_entity)
        .map(|children| children.is_empty())
        .unwrap_or(true);

    if !panel_is_empty
        && !ui_mode.is_changed()
        && !inspected.is_changed()
        && !formation.is_changed()
    {
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
                    &theme,
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
                    &theme,
                );
            }
        }
        UiMode::SelectedUnits(entities) if entities.len() > 1 => {
            let grid_container = commands
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                })
                .id();
            commands.entity(panel_entity).add_child(grid_container);

            let formation_controls = commands
                .spawn((
                    FormationControls,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        margin: UiRect::bottom(Val::Px(8.0)),
                        ..default()
                    },
                ))
                .id();
            commands.entity(grid_container).add_child(formation_controls);

            let formation_label = commands
                .spawn((
                    Text::new("Move Pattern"),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ))
                .id();
            commands.entity(formation_controls).add_child(formation_label);

            let formation_row = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .id();
            commands.entity(formation_controls).add_child(formation_row);

            for preset in FormationType::ALL {
                let is_active = formation.formation == preset;
                let button = commands
                    .spawn((
                        Button,
                        FormationPresetButton(preset),
                        Node {
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BorderColor::all(if is_active {
                            theme.colors.accent
                        } else {
                            theme.colors.border_subtle
                        }),
                        BackgroundColor(if is_active {
                            theme.colors.accent.with_alpha(0.18)
                        } else {
                            theme.colors.bg_surface
                        }),
                        ActionTooltipTrigger {
                            text: format!(
                                "{} Formation\n{}",
                                preset.display_name(),
                                preset.tooltip_text()
                            ),
                        },
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(preset.display_name()),
                            TextFont {
                                font_size: theme.typography.body,
                                ..default()
                            },
                            TextColor(if is_active {
                                theme.colors.accent
                            } else {
                                theme.colors.text_primary
                            }),
                        ));
                    })
                    .id();
                commands.entity(formation_row).add_child(button);
            }

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
                            font_size: theme.typography.small,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
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
                        &theme,
                    );
                }
            }

            for (kind, entities) in &building_groups {
                let header = commands
                    .spawn((
                        Text::new(format!("{} ({})", kind.display_name(), entities.len())),
                        TextFont {
                            font_size: theme.typography.small,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
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
                        &theme,
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
                        BackgroundColor(theme.colors.separator),
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
                &theme,
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
                        BackgroundColor(theme.colors.separator),
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
                &theme,
            );
            let label = commands
                .spawn((
                    Text::new(relationship),
                    TextFont {
                        font_size: theme.typography.body,
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
                        BackgroundColor(theme.colors.separator),
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
                &theme,
            );
            let label = commands
                .spawn((
                    Text::new(relationship),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(relationship_color),
                ))
                .id();
            commands.entity(panel_entity).add_child(label);
        }
    }
}

fn spawn_selection_footer(
    commands: &mut Commands,
    parent: Entity,
    show_unit_labels: bool,
    theme: &Theme,
) {
    let (button_bg, button_text, button_text_color, status_text) =
        label_visibility_presentation(theme, show_unit_labels);

    let footer = commands
        .spawn((
            SelectionFooter,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                margin: UiRect::top(Val::Px(4.0)),
                padding: UiRect::top(Val::Px(8.0)),
                border: UiRect::top(Val::Px(1.0)),
                ..default()
            },
        ))
        .insert(BorderColor::all(theme.colors.separator))
        .id();
    commands.entity(parent).add_child(footer);

    let top_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(6.0),
            ..default()
        })
        .id();
    commands.entity(footer).add_child(top_row);

    let title_block = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .id();
    commands.entity(top_row).add_child(title_block);

    let footer_label = commands
        .spawn((
            Text::new("Unit Labels"),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ))
        .id();
    commands.entity(title_block).add_child(footer_label);

    let status = commands
        .spawn((
            UnitLabelsStatusText,
            Text::new(status_text),
            TextFont {
                font_size: theme.typography.tiny,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
        ))
        .id();
    commands.entity(title_block).add_child(status);

    let button = commands
        .spawn((
            Button,
            StandardButton,
            ToggleUnitLabelsButton,
            ui_components::compact_button_node(10.0, 5.0),
            ui_components::filled_button_chrome(theme, ui_components::UiTone::Neutral),
            ActionTooltipTrigger {
                text: "Toggle ambient unit labels\nHotkey: L".to_string(),
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                ToggleUnitLabelsButtonText,
                Text::new(button_text),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(button_text_color),
            ));
        })
        .id();
    commands.entity(top_row).add_child(button);

    commands.entity(button).insert(BackgroundColor(button_bg));
    commands.entity(button).insert(ButtonAnimState::new(button_bg.to_srgba().to_f32_array()));
}

fn label_visibility_presentation(theme: &Theme, show_unit_labels: bool) -> (Color, &'static str, Color, &'static str) {
    if show_unit_labels {
        (
            theme.colors.accent,
            "On (L)",
            Color::WHITE,
            "Ambient labels are visible for units on screen.",
        )
    } else {
        (
            theme.colors.btn_primary,
            "Off (L)",
            theme.colors.text_primary,
            "Only hovered and selected units show labels.",
        )
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
    theme: &Theme,
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
            BorderColor::all(theme.colors.panel_accent_friendly),
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
            BackgroundColor(theme.colors.icon_frame_bg),
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
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ))
        .id();
    commands.entity(info).add_child(name);

    if display_name.is_some() {
        let unit_type = commands
            .spawn((
                Text::new(kind.display_name()),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();
        commands.entity(info).add_child(unit_type);
    }

    spawn_hp_bar(commands, info, entity, health, 160.0, theme);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
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

    let stat_colors = [theme.colors.stat_dmg, theme.colors.stat_rng, theme.colors.stat_spd];
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
                    font_size: theme.typography.body,
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
                    font_size: theme.typography.small,
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
    theme: &Theme,
) {
    let accent_color = match state {
        BuildingState::UnderConstruction => theme.colors.panel_accent_construction,
        BuildingState::Complete => theme.colors.panel_accent_friendly,
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
            BackgroundColor(theme.colors.icon_frame_bg),
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
        BuildingState::UnderConstruction => theme.colors.warning,
        BuildingState::Complete => theme.colors.text_primary,
    };
    let name = commands
        .spawn((
            Text::new(format!("{}{}", kind.display_name(), state_str)),
            TextFont {
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(name_color),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 160.0, theme);
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
    theme: &Theme,
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
            BorderColor::all(theme.colors.panel_accent_enemy),
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
            BackgroundColor(theme.colors.icon_frame_bg),
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
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(theme.colors.warning),
        ))
        .id();
    commands.entity(info).add_child(name);

    spawn_hp_bar(commands, info, entity, health, 160.0, theme);

    let hp_text = commands
        .spawn((
            Text::new(format!("{:.0}/{:.0}", health.current, health.max)),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
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
        ("DMG", format!("{:.0}", damage.0), theme.colors.stat_dmg),
        ("RNG", format!("{:.1}", range.0), theme.colors.stat_rng),
        ("AGR", format!("{:.0}", aggro.0), theme.colors.warning),
        ("SPD", format!("{:.1}", speed.0), theme.colors.stat_spd),
    ];
    for (label, value, color) in &stat_data {
        let stat = commands
            .spawn((
                Text::new(format!("{}: {}", label, value)),
                TextFont {
                    font_size: theme.typography.body,
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
    theme: &Theme,
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
            BackgroundColor(theme.colors.bg_surface),
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

    spawn_hp_bar(commands, card, entity, health, 54.0, theme);

    let name = commands
        .spawn((
            Text::new(display_name.unwrap_or(kind.display_name())),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            TextColor(theme.colors.text_primary),
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
                TextColor(theme.colors.text_secondary),
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
