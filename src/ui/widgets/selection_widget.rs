use bevy::prelude::*;

use super::core::constants::*;
use super::core::components as ui_components;
use super::core::fonts::UiFonts;
use super::core::framework::{spawn_widget_frame, WidgetId, WidgetRegistry};
use super::core::hud::WidgetGridArea;
use super::core::shared::hp_color;
use super::group_hotkeys_widget::ControlGroups;
use super::selection_cards::{
    spawn_friendly_detail_card, spawn_building_detail_card, spawn_enemy_detail_card,
    spawn_multi_inventory_summary, spawn_single_inventory_section, spawn_unit_mini_card,
};
use crate::blueprints::EntityKind;
use crate::types::*;
use crate::simulation::items::{
    InventoryChanged, ItemAssets, ItemPickupCollected,
    ItemPickupFailed, ItemRuntimeState, ItemTransferFailed, ItemRegistry, RequestDropItem,
    RequestTransferItem, UnitInventory,
};
use crate::ui::theme::Theme;

pub struct SelectionWidgetPlugin;

#[derive(Component)]
struct FormationControls;

#[derive(Component)]
pub(super) struct DropInventoryItemButton {
    pub(super) unit: Entity,
    pub(super) slot: usize,
}

#[derive(Component)]
pub(super) struct InventorySlotButton {
    pub(super) unit: Entity,
    pub(super) slot: usize,
}

#[derive(Component)]
pub(super) struct InventoryFocusUnitButton {
    pub(super) unit: Entity,
}

#[derive(Component)]
pub(super) struct TransferInventoryItemButton {
    pub(super) from_unit: Entity,
    pub(super) from_slot: usize,
    pub(super) to_unit: Entity,
}

#[derive(Clone, Debug)]
pub(super) struct InventoryWarningState {
    text: String,
    expires_at: f32,
}

#[derive(Clone, Debug)]
pub(super) struct TransferTargetOption {
    pub(super) unit: Entity,
    pub(super) label: String,
}

#[derive(Resource, Default)]
pub(super) struct SelectionInventoryUiState {
    pub(super) warning: Option<InventoryWarningState>,
    pub(super) focused_unit: Option<Entity>,
    pub(super) focused_slot: Option<usize>,
}

impl Plugin for SelectionWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionInventoryUiState>()
        .add_systems(
            Update,
            spawn_selection_widget
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<WidgetGridArea>),
        )
        .add_systems(
            Update,
            (
                maintain_selection_inventory_ui_state,
                handle_item_inventory_feedback,
                handle_inventory_focus_unit_click,
                handle_inventory_slot_focus,
                handle_drop_inventory_item_click,
                handle_transfer_inventory_item_click,
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
    grid_q: Query<Entity, Added<WidgetGridArea>>,
) {
    let Ok(grid_area) = grid_q.single() else {
        return;
    };
    let selection_content = spawn_widget_frame(
        &mut commands,
        grid_area,
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
    mut transfer_failed: MessageReader<ItemTransferFailed>,
    mut inventory_changed: MessageReader<InventoryChanged>,
) {
    let mut clear_warning = false;
    let mut next_warning = None;

    for failure in pickup_failed.read() {
        next_warning = Some(format!(
            "{}: {}",
            failure.item.display_name(),
            failure.reason.label()
        ));
    }

    for failure in transfer_failed.read() {
        next_warning = Some(format!(
            "{}: {}",
            failure.item.display_name(),
            failure.reason.label()
        ));
    }

    for collected in pickup_collected.read() {
        if let Some(message) = collected.info_message {
            next_warning = Some(format!(
                "{}: {}",
                collected.item.display_name(),
                message
            ));
        } else {
            clear_warning = true;
        }
    }
    for _ in inventory_changed.read() {
        clear_warning = true;
    }

    if let Some(text) = next_warning {
        inventory_ui.warning = Some(InventoryWarningState {
            text,
            expires_at: time.elapsed_secs() + 4.0,
        });
    } else if clear_warning {
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
        UiMode::SelectedUnits(units) if !units.is_empty() => {
            if inventory_ui
                .focused_unit
                .is_none_or(|focused| !units.contains(&focused))
            {
                inventory_ui.focused_unit = Some(units[0]);
                inventory_ui.focused_slot = None;
            }
        }
        _ => {
            if inventory_ui.focused_unit.is_some() {
                inventory_ui.focused_unit = None;
                inventory_ui.focused_slot = None;
            }
        }
    }

    if ui_mode.is_changed() {
        inventory_ui.warning = None;
    }
}

fn handle_inventory_focus_unit_click(
    interactions: Query<(&Interaction, &InventoryFocusUnitButton), (Changed<Interaction>, With<Button>)>,
    mut inventory_ui: ResMut<SelectionInventoryUiState>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        inventory_ui.focused_unit = Some(button.unit);
        inventory_ui.focused_slot = None;
        ui_clicked.0 = 2;
        ui_press.0 = true;
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
            Interaction::Hovered => {}
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

fn handle_transfer_inventory_item_click(
    interactions: Query<(&Interaction, &TransferInventoryItemButton), (Changed<Interaction>, With<Button>)>,
    mut transfer_requests: MessageWriter<RequestTransferItem>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        transfer_requests.write(RequestTransferItem {
            from_unit: button.from_unit,
            from_slot: button.from_slot,
            to_unit: button.to_unit,
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
        Res<ItemRegistry>,
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
        item_registry,
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
                    &transfer_targets_for_unit(entity, None, &selected_units),
                    &item_registry,
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

            let focused_unit = inventory_ui
                .focused_unit
                .filter(|unit| entities.contains(unit))
                .unwrap_or(entities[0]);

            let source_picker = commands
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    margin: UiRect::bottom(Val::Px(8.0)),
                    padding: PAD_LG,
                    border: BORDER_1,
                    ..default()
                })
                .insert(BorderColor::all(theme.colors.border_subtle))
                .insert(BackgroundColor(theme.colors.bg_surface))
                .id();
            commands.entity(grid_container).add_child(source_picker);

            commands.entity(source_picker).with_children(|picker| {
                picker.spawn((
                    Text::new("Inventory Source"),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_primary),
                ));
                picker.spawn((
                    Text::new("Pick a selected unit, then a slot to drop or transfer its item."),
                    TextFont {
                        font_size: theme.typography.tiny,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
            });

            let source_row = commands
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                })
                .id();
            commands.entity(source_picker).add_child(source_row);

            for (entity, kind, display_name, _, _, _, _, _, inventory, _) in &selected_units {
                let capacity = inventory.map(|inv| inv.capacity).unwrap_or(0);
                if capacity == 0 {
                    continue;
                }
                let is_focused = focused_unit == entity;
                let label = display_name
                    .map(|name| name.0.clone())
                    .unwrap_or_else(|| kind.display_name().to_string());
                let filled = inventory.map(|inv| inv.items.len().min(inv.capacity as usize)).unwrap_or(0);
                let button = commands
                    .spawn((
                        Button,
                        StandardButton,
                        InventoryFocusUnitButton { unit: entity },
                        ui_components::compact_button_node(10.0, 5.0),
                        if is_focused {
                            ui_components::filled_button_chrome(&theme, ui_components::UiTone::Accent)
                        } else {
                            ui_components::ghost_button_chrome(&theme, ui_components::UiTone::Neutral)
                        },
                        ActionTooltipTrigger {
                            text: format!("Inspect {} inventory", label),
                        },
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new(format!("{} {}/{}", label, filled, capacity)),
                            TextFont {
                                font_size: theme.typography.tiny,
                                ..default()
                            },
                            TextColor(if is_focused {
                                crate::ui::theme::TEXT_PRIMARY
                            } else {
                                theme.colors.text_primary
                            }),
                        ));
                    })
                    .id();
                commands.entity(source_row).add_child(button);
            }

            if let Some((
                entity,
                kind,
                _display_name,
                _health,
                _dmg,
                _rng,
                _spd,
                _stance,
                inventory,
                runtime_state,
            )) = selected_units.iter().find(|(entity, _, _, _, _, _, _, _, _, _)| *entity == focused_unit)
            {
                spawn_single_inventory_section(
                    &mut commands,
                    grid_container,
                    entity,
                    *kind,
                    inventory,
                    runtime_state,
                    &inventory_ui,
                    &transfer_targets_for_unit(entity, inventory_ui.focused_slot, &selected_units),
                    &item_registry,
                    &item_assets,
                    &theme,
                );
            }

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
                            padding: PAD_BUTTON,
                            border: BORDER_1,
                            // border_radius: RADIUS_MD,
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
            crate::ui::theme::SUCCESS
        } else {
            crate::ui::theme::DESTRUCTIVE
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
                &[],
                &item_registry,
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

fn transfer_targets_for_unit(
    source_unit: Entity,
    source_slot: Option<usize>,
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
) -> Vec<TransferTargetOption> {
    let Some((_, _, _, _, _, _, _, _, inventory, _)) = selected_units
        .iter()
        .find(|(entity, _, _, _, _, _, _, _, _, _)| *entity == source_unit)
    else {
        return Vec::new();
    };
    let Some(inventory) = inventory else {
        return Vec::new();
    };
    let effective_slot = source_slot.or_else(|| (!inventory.items.is_empty()).then_some(0));
    let Some(source_item) = effective_slot.and_then(|slot| inventory.items.get(slot).copied())
    else {
        return Vec::new();
    };

    let mut targets = Vec::new();
    for (entity, kind, display_name, _, _, _, _, _, inventory, _) in selected_units.iter() {
        if entity == source_unit {
            continue;
        }
        let Some(inventory) = inventory else {
            continue;
        };
        if inventory.capacity == 0 || inventory.items.len() >= inventory.capacity as usize {
            continue;
        }
        if inventory
            .items
            .iter()
            .any(|existing| existing.category() == source_item.category())
        {
            continue;
        }
        let label = display_name
            .map(|name| name.0.clone())
            .unwrap_or_else(|| kind.display_name().to_string());
        targets.push(TransferTargetOption { unit: entity, label });
    }
    targets.sort_by(|a, b| a.label.cmp(&b.label));
    targets
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
                border: BORDER_1,
                // border_radius: RADIUS_XL,
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
            crate::ui::theme::TEXT_PRIMARY,
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

// Detail card rendering functions have been extracted to selection_cards.rs
