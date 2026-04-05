use bevy::prelude::*;
use std::collections::HashMap;

use super::core::components as ui_components;
use super::core::fonts::UiFonts;
use super::core::framework::{spawn_widget_frame, WidgetId, WidgetRegistry};
use super::core::hud::MainHudRoot;
use super::core::shared::{hp_color, spawn_hp_bar};
use super::group_hotkeys_widget::{group_color, ControlGroups};
use crate::blueprints::EntityKind;
use crate::components::*;
use crate::items::{
    inferred_inventory_capacity, InventoryChanged, ItemAssets, ItemKind, ItemPickupCollected,
    ItemPickupFailed, ItemRuntimeState, RequestDropItem, UnitInventory,
};
use crate::theme::Theme;

pub struct SelectionWidgetPlugin;

#[derive(Component)]
struct FormationControls;

#[derive(Component)]
struct DropInventoryItemButton {
    unit: Entity,
    slot: usize,
}

#[derive(Component)]
struct InventorySlotButton {
    unit: Entity,
    slot: usize,
}

#[derive(Clone, Debug)]
struct InventoryWarningState {
    text: String,
    expires_at: f32,
}

#[derive(Resource, Default)]
struct SelectionInventoryUiState {
    warning: Option<InventoryWarningState>,
    focused_unit: Option<Entity>,
    focused_slot: Option<usize>,
}

impl Plugin for SelectionWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionInventoryUiState>()
        .add_systems(
            Update,
            spawn_selection_widget
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(
            Update,
            (
                maintain_selection_inventory_ui_state,
                handle_item_inventory_feedback,
                handle_inventory_slot_focus,
                handle_drop_inventory_item_click,
            )
                .run_if(in_state(AppState::InGame)),
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
    // Don't overwrite the content Node — it has overflow: scroll_y() from spawn_widget_frame.
    // Instead, add a wrapper child for layout.
    let inner = commands
        .spawn((
            SelectionInfoPanel,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(8.0),
                ..default()
            },
        ))
        .id();
    commands.entity(selection_content).add_child(inner);

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
    commands.entity(inner).add_child(body);

    spawn_selection_footer(&mut commands, inner, true, &theme);
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

fn handle_item_inventory_feedback(
    time: Res<Time>,
    mut inventory_ui: ResMut<SelectionInventoryUiState>,
    mut pickup_failed: MessageReader<ItemPickupFailed>,
    mut pickup_collected: MessageReader<ItemPickupCollected>,
    mut inventory_changed: MessageReader<InventoryChanged>,
) {
    let mut clear_warning = false;

    for failure in pickup_failed.read() {
        inventory_ui.warning = Some(InventoryWarningState {
            text: format!("{}: {}", failure.item.display_name(), failure.reason.label()),
            expires_at: time.elapsed_secs() + 4.0,
        });
    }

    for _ in pickup_collected.read() {
        clear_warning = true;
    }
    for _ in inventory_changed.read() {
        clear_warning = true;
    }

    if clear_warning {
        inventory_ui.warning = None;
    }
}

fn maintain_selection_inventory_ui_state(
    time: Res<Time>,
    ui_mode: Res<UiMode>,
    mut inventory_ui: ResMut<SelectionInventoryUiState>,
) {
    if inventory_ui
        .warning
        .as_ref()
        .is_some_and(|warning| time.elapsed_secs() >= warning.expires_at)
    {
        inventory_ui.warning = None;
    }

    match &*ui_mode {
        UiMode::SelectedUnits(units) if units.len() == 1 => {
            let unit = units[0];
            if inventory_ui.focused_unit != Some(unit) {
                inventory_ui.focused_unit = Some(unit);
                inventory_ui.focused_slot = None;
            }
        }
        _ => {
            inventory_ui.focused_unit = None;
            inventory_ui.focused_slot = None;
        }
    }

    if ui_mode.is_changed() {
        inventory_ui.warning = None;
    }
}

fn handle_inventory_slot_focus(
    interactions: Query<(&Interaction, &InventorySlotButton), (Changed<Interaction>, With<Button>)>,
    mut inventory_ui: ResMut<SelectionInventoryUiState>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, button) in &interactions {
        match *interaction {
            Interaction::Hovered => {
                inventory_ui.focused_unit = Some(button.unit);
                inventory_ui.focused_slot = Some(button.slot);
            }
            Interaction::Pressed => {
                inventory_ui.focused_unit = Some(button.unit);
                inventory_ui.focused_slot = Some(button.slot);
                ui_clicked.0 = 2;
                ui_press.0 = true;
            }
            Interaction::None => {}
        }
    }
}

fn handle_drop_inventory_item_click(
    interactions: Query<(&Interaction, &DropInventoryItemButton), (Changed<Interaction>, With<Button>)>,
    mut drop_requests: MessageWriter<RequestDropItem>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        drop_requests.write(RequestDropItem {
            unit: button.unit,
            slot: button.slot,
        });
    }
}

fn update_label_visibility_footer(
    label_visibility: Res<EntityLabelVisibility>,
    theme: Res<Theme>,
    footer_q: Query<Entity, With<SelectionFooter>>,
    mut button_q: Query<(&mut BackgroundColor, &mut ButtonAnimState), With<ToggleUnitLabelsButton>>,
    mut button_text_q: Query<&mut Text, With<ToggleUnitLabelsButtonText>>,
    mut button_text_color_q: Query<&mut TextColor, With<ToggleUnitLabelsButtonText>>,
    mut status_text_q: Query<
        &mut Text,
        (
            With<UnitLabelsStatusText>,
            Without<ToggleUnitLabelsButtonText>,
        ),
    >,
) {
    if !label_visibility.is_changed() {
        return;
    }
    if footer_q.is_empty() {
        return;
    }

    let (bg_color, text, text_color, status) =
        label_visibility_presentation(&theme, label_visibility.show_unit_labels);
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

fn rebuild_selection_panel(
    mut commands: Commands,
    resources: (
        Res<UiMode>,
        Res<Theme>,
        Res<SelectionInventoryUiState>,
        Res<InspectedEnemy>,
        Res<ActivePlayer>,
        Res<TeamConfig>,
        Res<IconAssets>,
        Res<ItemAssets>,
        Res<ControlGroups>,
        Res<ActiveFormation>,
    ),
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
            Option<&UnitInventory>,
            Option<&ItemRuntimeState>,
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
    selected_inventory_updates: Query<
        (),
        (
            With<Unit>,
            With<Selected>,
            Or<(Changed<UnitInventory>, Changed<ItemRuntimeState>)>,
        ),
    >,
) {
    let (
        ui_mode,
        theme,
        inventory_ui,
        inspected,
        active_player,
        teams,
        icons,
        item_assets,
        control_groups,
        formation,
    ) = resources;
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
        && !inventory_ui.is_changed()
        && !inspected.is_changed()
        && !formation.is_changed()
        && selected_inventory_updates.is_empty()
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

    if let Some(warning) = inventory_ui.warning.as_ref() {
        spawn_inventory_warning(&mut commands, panel_entity, &warning.text, &theme);
    }

    match &*ui_mode {
        UiMode::SelectedUnits(entities) if entities.len() == 1 => {
            if let Some((
                entity,
                kind,
                display_name,
                health,
                dmg,
                rng,
                spd,
                stance,
                inventory,
                runtime_state,
            )) =
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
                    inventory,
                    runtime_state,
                    &inventory_ui,
                    &icons,
                    &item_assets,
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
            commands
                .entity(grid_container)
                .add_child(formation_controls);

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
            commands
                .entity(formation_controls)
                .add_child(formation_label);

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
                Vec<(
                    Entity,
                    Option<&UnitDisplayName>,
                    &Health,
                    Option<&UnitInventory>,
                    Option<&ItemRuntimeState>,
                )>,
            )> = Vec::new();
            for (entity, kind, display_name, health, _, _, _, _, inventory, runtime) in &selected_units {
                if let Some(group) = unit_groups.iter_mut().find(|(k, _)| *k == *kind) {
                    group.1.push((entity, display_name, health, inventory, runtime));
                } else {
                    unit_groups.push((*kind, vec![(entity, display_name, health, inventory, runtime)]));
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

            spawn_multi_inventory_summary(
                &mut commands,
                grid_container,
                &selected_units,
                &item_assets,
                &theme,
            );

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

                for (entity, display_name, health, inventory, runtime) in entities {
                    spawn_unit_mini_card(
                        &mut commands,
                        grid,
                        *entity,
                        display_name.map(|name| name.0.as_str()),
                        *kind,
                        health,
                        *inventory,
                        *runtime,
                        &icons,
                        &item_assets,
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
                        None,
                        None,
                        &icons,
                        &item_assets,
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
                None,
                None,
                &inventory_ui,
                &icons,
                &item_assets,
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
    commands
        .entity(button)
        .insert(ButtonAnimState::new(button_bg.to_srgba().to_f32_array()));
}

fn spawn_inventory_warning(
    commands: &mut Commands,
    parent: Entity,
    warning: &str,
    theme: &Theme,
) {
    let banner = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(7.0)),
                margin: UiRect::bottom(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(theme.colors.warning.with_alpha(0.12)),
            BorderColor::all(theme.colors.warning.with_alpha(0.75)),
        ))
        .id();
    commands.entity(parent).add_child(banner);
    commands.entity(banner).with_children(|banner| {
        banner.spawn((
            Text::new(warning),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.warning),
        ));
    });
}

fn label_visibility_presentation(
    theme: &Theme,
    show_unit_labels: bool,
) -> (Color, &'static str, Color, &'static str) {
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

fn spawn_friendly_detail_card(
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
    inventory: Option<&UnitInventory>,
    runtime_state: Option<&ItemRuntimeState>,
    inventory_ui: &SelectionInventoryUiState,
    icons: &IconAssets,
    item_assets: &ItemAssets,
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

    let stat_colors = [
        theme.colors.stat_dmg,
        theme.colors.stat_rng,
        theme.colors.stat_spd,
    ];
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

    spawn_single_inventory_section(
        commands,
        info,
        entity,
        kind,
        inventory,
        runtime_state,
        inventory_ui,
        item_assets,
        theme,
    );
}

fn displayed_inventory_capacity(kind: EntityKind, inventory: Option<&UnitInventory>) -> usize {
    inventory
        .map(|inventory| inventory.capacity as usize)
        .unwrap_or_else(|| inferred_inventory_capacity(kind) as usize)
}

fn displayed_inventory_items(inventory: Option<&UnitInventory>) -> &[ItemKind] {
    inventory.map(|inventory| inventory.items.as_slice()).unwrap_or(&[])
}

fn spawn_single_inventory_section(
    commands: &mut Commands,
    parent: Entity,
    unit: Entity,
    kind: EntityKind,
    inventory: Option<&UnitInventory>,
    runtime_state: Option<&ItemRuntimeState>,
    inventory_ui: &SelectionInventoryUiState,
    item_assets: &ItemAssets,
    theme: &Theme,
) {
    let capacity = displayed_inventory_capacity(kind, inventory);
    if capacity == 0 {
        return;
    }
    let items = displayed_inventory_items(inventory);
    let focused_slot = inventory_ui
        .focused_slot
        .filter(|slot| inventory_ui.focused_unit == Some(unit) && *slot < capacity)
        .or_else(|| {
            if items.is_empty() {
                None
            } else {
                Some(0)
            }
        });

    let section = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            margin: UiRect::top(Val::Px(6.0)),
            padding: UiRect::top(Val::Px(6.0)),
            border: UiRect::top(Val::Px(1.0)),
            ..default()
        })
        .insert(BorderColor::all(theme.colors.separator))
        .id();
    commands.entity(parent).add_child(section);

    let header = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .id();
    commands.entity(section).add_child(header);

    commands.entity(header).with_children(|header| {
        header.spawn((
            Text::new("Inventory"),
            TextFont {
                font_size: theme.typography.body,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ));
        header.spawn((
            Text::new(format!("{}/{} slots", items.len().min(capacity), capacity)),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
        ));
    });

    commands.entity(section).with_children(|section| {
        section.spawn((
            Text::new("Hover a slot to inspect it. Drop works on the focused slot only."),
            TextFont {
                font_size: theme.typography.tiny,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
        ));
    });

    let slots = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.0),
            row_gap: Val::Px(6.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .id();
    commands.entity(section).add_child(slots);

    for slot in 0..capacity {
        let filled = items.get(slot).copied();
        let is_focused = focused_slot == Some(slot);
        let slot_node = commands
            .spawn((
                Button,
                StandardButton,
                InventorySlotButton { unit, slot },
                Node {
                    width: Val::Px(64.0),
                    height: Val::Px(64.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::FlexStart,
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    padding: UiRect::all(Val::Px(6.0)),
                    ..default()
                },
                BorderColor::all(if is_focused {
                    theme.colors.accent
                } else if filled.is_some() {
                    theme.colors.accent.with_alpha(0.45)
                } else {
                    theme.colors.border_subtle
                }),
                BackgroundColor(if is_focused {
                    theme.colors.accent.with_alpha(0.12)
                } else {
                    theme.colors.icon_frame_bg
                }),
            ))
            .id();
        commands.entity(slots).add_child(slot_node);

        commands.entity(slot_node).with_children(|slot_parent| {
            slot_parent.spawn((
                Text::new(format!("S{}", slot + 1)),
                TextFont {
                    font_size: theme.typography.tiny,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ));
            if let Some(item) = filled {
                slot_parent.spawn((
                    ImageNode::new(item_assets.icon(item)),
                    Node {
                        width: Val::Px(30.0),
                        height: Val::Px(30.0),
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                ));
                slot_parent.spawn((
                    Text::new(item.category().label()),
                    TextFont {
                        font_size: theme.typography.tiny,
                        ..default()
                    },
                    TextColor(theme.colors.accent),
                ));
            } else {
                slot_parent.spawn((
                    Text::new("Empty"),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.text_disabled),
                    Node {
                        margin: UiRect::top(Val::Px(10.0)),
                        ..default()
                    },
                ));
            }
        });
    }

    let details = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        })
        .insert(BorderColor::all(theme.colors.border_subtle))
        .insert(BackgroundColor(theme.colors.bg_surface))
        .id();
    commands.entity(section).add_child(details);

    match focused_slot.and_then(|slot| items.get(slot).copied().map(|item| (slot, item))) {
        Some((slot, item)) => {
            let status = runtime_state
                .and_then(|state| state.items.iter().find(|entry| entry.item == item))
                .map(|entry| {
                    if entry.cooldown_remaining > 0.0 {
                        format!("Cooldown {:.1}s", entry.cooldown_remaining)
                    } else if entry.enabled {
                        if entry.active_toggled {
                            "Active".to_string()
                        } else {
                            "Ready".to_string()
                        }
                    } else {
                        entry
                            .disabled_reason
                            .map(|reason| format!("Disabled: {}", reason.label()))
                            .unwrap_or_else(|| "Disabled".to_string())
                    }
                })
                .unwrap_or_else(|| "Ready".to_string());

            let header = commands
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .id();
            commands.entity(details).add_child(header);

            commands.entity(header).with_children(|header| {
                header.spawn((
                    Text::new(item.display_name()),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_primary),
                ));
                header.spawn((
                    Text::new(format!("Slot {}", slot + 1)),
                    TextFont {
                        font_size: theme.typography.tiny,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
            });

            commands.entity(details).with_children(|details| {
                details.spawn((
                    Text::new(format!("Category: {}", item.category().label())),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.accent),
                ));
                details.spawn((
                    Text::new(format!("Status: {}", status)),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.text_primary),
                ));
                details.spawn((
                    Text::new(item.effect_summary()),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
            });

            let drop_button = commands
                .spawn((
                    Button,
                    StandardButton,
                    DropInventoryItemButton { unit, slot },
                    ui_components::compact_button_node(10.0, 5.0),
                    ui_components::ghost_button_chrome(theme, ui_components::UiTone::Destructive),
                    ActionTooltipTrigger {
                        text: format!(
                            "Drop {}\nPlace this item back on the ground",
                            item.display_name()
                        ),
                    },
                ))
                .with_children(|button| {
                    button.spawn((
                        Text::new("Drop Selected Item"),
                        TextFont {
                            font_size: theme.typography.tiny,
                            ..default()
                        },
                        TextColor(theme.colors.warning),
                    ));
                })
                .id();
            commands.entity(details).add_child(drop_button);
        }
        None if items.is_empty() => {
            commands.entity(details).with_children(|details| {
                details.spawn((
                    Text::new("No items equipped."),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
            });
        }
        None => {
            commands.entity(details).with_children(|details| {
                details.spawn((
                    Text::new("Select a filled slot to inspect its details."),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
            });
        }
    }
}

fn spawn_multi_inventory_summary(
    commands: &mut Commands,
    parent: Entity,
    selected_units: &Query<
        (
            Entity,
            &EntityKind,
            Option<&UnitDisplayName>,
            &Health,
            &AttackDamage,
            &AttackRange,
            &UnitSpeed,
            Option<&UnitStance>,
            Option<&UnitInventory>,
            Option<&ItemRuntimeState>,
        ),
        (With<Unit>, With<Selected>),
    >,
    item_assets: &ItemAssets,
    theme: &Theme,
) {
    let mut total_capacity = 0usize;
    let mut total_filled = 0usize;
    let mut counts: HashMap<ItemKind, usize> = HashMap::new();

    for (_, kind, _, _, _, _, _, _, inventory, _) in selected_units.iter() {
        let capacity = displayed_inventory_capacity(*kind, inventory);
        total_capacity += capacity;

        let items = displayed_inventory_items(inventory);
        total_filled += items.len().min(capacity);
        for &item in items.iter().take(capacity) {
            *counts.entry(item).or_insert(0) += 1;
        }
    }

    if total_capacity == 0 {
        return;
    }

    let section = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            margin: UiRect::bottom(Val::Px(8.0)),
            padding: UiRect::all(Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        })
        .insert(BorderColor::all(theme.colors.border_subtle))
        .insert(BackgroundColor(theme.colors.bg_surface))
        .id();
    commands.entity(parent).add_child(section);

    commands.entity(section).with_children(|section| {
        section.spawn((
            Text::new(format!("Squad Inventory {}/{}", total_filled, total_capacity)),
            TextFont {
                font_size: theme.typography.body,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ));
    });

    if counts.is_empty() {
        commands.entity(section).with_children(|section| {
            section.spawn((
                Text::new("No equipped items across the current selection."),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ));
        });
        return;
    }

    let mut item_counts: Vec<(ItemKind, usize)> = counts.into_iter().collect();
    item_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.display_name().cmp(b.0.display_name())));

    let chip_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(6.0),
            row_gap: Val::Px(6.0),
            ..default()
        })
        .id();
    commands.entity(section).add_child(chip_row);

    for (item, count) in item_counts {
        let chip = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    ..default()
                },
                BorderColor::all(theme.colors.border_subtle),
                BackgroundColor(theme.colors.icon_frame_bg),
            ))
            .id();
        commands.entity(chip_row).add_child(chip);

        commands.entity(chip).with_children(|chip| {
            chip.spawn((
                ImageNode::new(item_assets.icon(item)),
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    ..default()
                },
            ));
            chip.spawn((
                Text::new(format!("{} x{}", item.display_name(), count)),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_primary),
            ));
        });
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
    inventory: Option<&UnitInventory>,
    _runtime_state: Option<&ItemRuntimeState>,
    icons: &IconAssets,
    _item_assets: &ItemAssets,
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

    let capacity = displayed_inventory_capacity(kind, inventory);
    if capacity > 0 {
        let filled = displayed_inventory_items(inventory).len().min(capacity);
        let slots = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(2.0),
                margin: UiRect::top(Val::Px(1.0)),
                ..default()
            })
            .id();
        commands.entity(card).add_child(slots);

        for idx in 0..capacity {
            let tone = if idx < filled {
                theme.colors.accent
            } else {
                theme.colors.border_subtle
            };
            let slot = commands
                .spawn((
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(tone),
                ))
                .id();
            commands.entity(slots).add_child(slot);
        }
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
