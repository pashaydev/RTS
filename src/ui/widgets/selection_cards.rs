use bevy::prelude::*;
use std::collections::HashMap;

use super::core::constants::*;
use super::core::components as ui_components;
use super::core::shared::spawn_hp_bar;
use super::group_hotkeys_widget::{group_color, ControlGroups};
use super::selection_widget::{
    DropInventoryItemButton, InventorySlotButton, SelectionInventoryUiState,
    TransferInventoryItemButton, TransferTargetOption,
};
use crate::blueprints::EntityKind;
use crate::types::*;
use crate::simulation::items::{
    inferred_inventory_capacity, item_effect_requirement_message, ItemAssets, ItemKind,
    ItemRegistry, ItemRuntimeState, UnitInventory,
};
use crate::ui::theme::Theme;

pub(super) fn spawn_friendly_detail_card(
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
    transfer_targets: &[TransferTargetOption],
    item_registry: &ItemRegistry,
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
                // border_radius: RADIUS_XS,
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
                // border_radius: RADIUS_LG,
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
            UnitStance::Passive => ("Passive", crate::ui::theme::STANCE_PASSIVE),
            UnitStance::Defensive => ("Defensive", crate::ui::theme::STANCE_DEFENSIVE),
            UnitStance::Aggressive => ("Aggressive", crate::ui::theme::STANCE_AGGRESSIVE),
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
        transfer_targets,
        item_registry,
        item_assets,
        theme,
    );
}

pub(super) fn displayed_inventory_capacity(kind: EntityKind, inventory: Option<&UnitInventory>) -> usize {
    inventory
        .map(|inventory| inventory.capacity as usize)
        .unwrap_or_else(|| inferred_inventory_capacity(kind) as usize)
}

pub(super) fn displayed_inventory_items(inventory: Option<&UnitInventory>) -> &[ItemKind] {
    inventory.map(|inventory| inventory.items.as_slice()).unwrap_or(&[])
}

pub(super) fn spawn_single_inventory_section(
    commands: &mut Commands,
    parent: Entity,
    unit: Entity,
    kind: EntityKind,
    inventory: Option<&UnitInventory>,
    runtime_state: Option<&ItemRuntimeState>,
    inventory_ui: &SelectionInventoryUiState,
    transfer_targets: &[TransferTargetOption],
    item_registry: &ItemRegistry,
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
            Text::new("Click a slot to inspect it. Drop works on the selected slot only."),
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
                    border: BORDER_1,
                    // border_radius: RADIUS_LG,
                    padding: PAD_MD,
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
            padding: PAD_LG,
            border: BORDER_1,
            // border_radius: RADIUS_LG,
            ..default()
        })
        .insert(BorderColor::all(theme.colors.border_subtle))
        .insert(BackgroundColor(theme.colors.bg_surface))
        .id();
    commands.entity(section).add_child(details);

    match focused_slot.and_then(|slot| items.get(slot).copied().map(|item| (slot, item))) {
        Some((slot, item)) => {
            let runtime_entry = runtime_state.and_then(|state| state.items.get(slot));
            let status = runtime_entry
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
            let effect_note = runtime_entry
                .filter(|entry| !entry.enabled)
                .and_then(|_| item_effect_requirement_message(item_registry, kind, item));

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
                if let Some(note) = effect_note {
                    details.spawn((
                        Text::new(format!("Why inactive: {}", note)),
                        TextFont {
                            font_size: theme.typography.small,
                            ..default()
                        },
                        TextColor(theme.colors.warning),
                    ));
                }
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

            if !transfer_targets.is_empty() {
                let transfer_section = commands
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        margin: UiRect::top(Val::Px(4.0)),
                        ..default()
                    })
                    .id();
                commands.entity(details).add_child(transfer_section);

                commands.entity(transfer_section).with_children(|section| {
                    section.spawn((
                        Text::new("Transfer To"),
                        TextFont {
                            font_size: theme.typography.small,
                            ..default()
                        },
                        TextColor(theme.colors.text_primary),
                    ));
                });

                let transfer_row = commands
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .id();
                commands.entity(transfer_section).add_child(transfer_row);

                for target in transfer_targets {
                    let button = commands
                        .spawn((
                            Button,
                            StandardButton,
                            TransferInventoryItemButton {
                                from_unit: unit,
                                from_slot: slot,
                                to_unit: target.unit,
                            },
                            ui_components::compact_button_node(10.0, 5.0),
                            ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
                            ActionTooltipTrigger {
                                text: format!("Transfer {} to {}", item.display_name(), target.label),
                            },
                        ))
                        .with_children(|button| {
                            button.spawn((
                                Text::new(target.label.clone()),
                                TextFont {
                                    font_size: theme.typography.tiny,
                                    ..default()
                                },
                                TextColor(theme.colors.text_primary),
                            ));
                        })
                        .id();
                    commands.entity(transfer_row).add_child(button);
                }
            }
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

pub(super) fn spawn_multi_inventory_summary(
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
            padding: PAD_LG,
            border: BORDER_1,
            // border_radius: RADIUS_LG,
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
                    border: BORDER_1,
                    // border_radius: BorderRadius::all(Val::Px(999.0)),
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

pub(super) fn spawn_building_detail_card(
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
                // border_radius: RADIUS_XS,
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
                // border_radius: RADIUS_LG,
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

pub(super) fn spawn_enemy_detail_card(
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
                // border_radius: RADIUS_LG,
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
                // border_radius: RADIUS_LG,
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

pub(super) fn spawn_unit_mini_card(
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
                padding: PAD_SM,
                row_gap: Val::Px(2.0),
                min_width: Val::Px(56.0),
                flex_grow: 1.0,
                border: BORDER_1,
                // border_radius: BorderRadius::all(Val::Px(5.0)),
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
                        // border_radius: RADIUS_XS,
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
                    TextColor(crate::ui::theme::BG_RECESSED),
                    Node {
                        padding: UiRect::axes(Val::Px(2.0), Val::Px(0.0)),
                        // border_radius: RADIUS_SM,
                        ..default()
                    },
                    BackgroundColor(group_color(gi)),
                ))
                .id();
            commands.entity(badge_row).add_child(badge);
        }
    }
}
