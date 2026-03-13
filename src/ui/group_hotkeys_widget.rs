use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme;

// ── Control Groups Resource ──

#[derive(Resource)]
pub struct ControlGroups {
    pub groups: [Vec<Entity>; 9],
}

impl Default for ControlGroups {
    fn default() -> Self {
        Self {
            groups: Default::default(),
        }
    }
}

// ── Widget UI ──

#[derive(Component)]
pub struct GroupHotkeyContent;

#[derive(Component)]
pub struct GroupSlotButton(pub usize);

pub fn update_group_hotkeys_widget(
    mut commands: Commands,
    icons: Res<IconAssets>,
    control_groups: Res<ControlGroups>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    existing: Query<Entity, With<GroupHotkeyContent>>,
    unit_kinds: Query<&EntityKind, With<Unit>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::GroupHotkeys) {
        return;
    }

    let Some(content) = super::widget_framework::find_widget_content(WidgetId::GroupHotkeys, &widget_q, &content_q) else { return; };

    // Clear existing
    for entity in &existing {
        commands.entity(entity).try_despawn();
    }

    let container = commands
        .spawn((
            GroupHotkeyContent,
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(3.0),
                row_gap: Val::Px(3.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(container);

    for (i, group) in control_groups.groups.iter().enumerate() {
        // Filter to alive entities
        let alive: Vec<Entity> = group.iter().copied().filter(|e| unit_kinds.get(*e).is_ok()).collect();

        let is_empty = alive.is_empty();
        let bg_color = if is_empty {
            Color::srgba(0.15, 0.15, 0.15, 0.3)
        } else {
            Color::srgba(0.2, 0.2, 0.25, 0.6)
        };

        let slot = commands
            .spawn((
                GroupSlotButton(i),
                Button,
                Node {
                    width: Val::Px(44.0),
                    height: Val::Px(36.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    row_gap: Val::Px(1.0),
                    ..default()
                },
                BackgroundColor(bg_color),
            ))
            .id();
        commands.entity(container).add_child(slot);

        // Group number
        let num_color = if is_empty { theme::TEXT_DISABLED } else { theme::TEXT_PRIMARY };
        let num = commands
            .spawn((
                Text::new(format!("{}", i + 1)),
                TextFont { font_size: theme::FONT_SMALL, ..default() },
                TextColor(num_color),
            ))
            .id();
        commands.entity(slot).add_child(num);

        if !is_empty {
            // Show first unit type icon + count
            let first_kind = unit_kinds.get(alive[0]).ok();
            let count_row = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(1.0),
                    ..default()
                })
                .id();
            commands.entity(slot).add_child(count_row);

            if let Some(kind) = first_kind {
                let icon = commands
                    .spawn((
                        ImageNode::new(icons.entity_icon(*kind)),
                        Node {
                            width: Val::Px(12.0),
                            height: Val::Px(12.0),
                            ..default()
                        },
                    ))
                    .id();
                commands.entity(count_row).add_child(icon);
            }

            let count_text = commands
                .spawn((
                    Text::new(format!("{}", alive.len())),
                    TextFont { font_size: theme::FONT_TINY, ..default() },
                    TextColor(theme::TEXT_SECONDARY),
                ))
                .id();
            commands.entity(count_row).add_child(count_text);
        }
    }
}

/// Handle Ctrl+1..9 to assign, 1..9 to recall, Shift+1..9 to add to group
pub fn handle_control_group_keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut control_groups: ResMut<ControlGroups>,
    selected: Query<Entity, (With<Unit>, With<Selected>)>,
    mut ui_press: ResMut<UiPressActive>,
) {
    let digit_keys = [
        KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
        KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
        KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
    ];

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    for (i, key) in digit_keys.iter().enumerate() {
        if !keys.just_pressed(*key) {
            continue;
        }

        if ctrl {
            // Assign selected units to group
            let units: Vec<Entity> = selected.iter().collect();
            if !units.is_empty() {
                control_groups.groups[i] = units;
            }
        } else if shift {
            // Add selected to group
            let units: Vec<Entity> = selected.iter().collect();
            for entity in units {
                if !control_groups.groups[i].contains(&entity) {
                    control_groups.groups[i].push(entity);
                }
            }
        } else {
            // Recall group — select those units
            let group = &control_groups.groups[i];
            if group.is_empty() {
                continue;
            }
            // Deselect all
            for entity in selected.iter() {
                commands.entity(entity).remove::<Selected>();
            }
            // Select group members (only alive ones)
            for entity in group {
                // Just try to insert; if entity doesn't exist it's a no-op
                commands.entity(*entity).try_insert(Selected);
            }
            ui_press.0 = true;
        }
    }
}

pub fn handle_group_slot_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &GroupSlotButton), Changed<Interaction>>,
    control_groups: Res<ControlGroups>,
    selected: Query<Entity, (With<Unit>, With<Selected>)>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, slot) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let group = &control_groups.groups[slot.0];
        if group.is_empty() {
            continue;
        }
        ui_press.0 = true;
        for entity in selected.iter() {
            commands.entity(entity).remove::<Selected>();
        }
        for entity in group {
            commands.entity(*entity).try_insert(Selected);
        }
    }
}
