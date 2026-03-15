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

impl ControlGroups {
    /// Returns list of group indices (0-based) that contain this entity
    pub fn groups_for_entity(&self, entity: Entity) -> Vec<usize> {
        self.groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.contains(&entity))
            .map(|(i, _)| i)
            .collect()
    }
}

// Group-specific colors for badges (visually distinct, saturated but not harsh)
const GROUP_COLORS: [Color; 9] = [
    Color::srgb(0.29, 0.62, 1.00), // 1 - blue
    Color::srgb(0.30, 0.80, 0.40), // 2 - green
    Color::srgb(1.00, 0.65, 0.15), // 3 - orange
    Color::srgb(0.80, 0.35, 0.85), // 4 - purple
    Color::srgb(0.90, 0.35, 0.35), // 5 - red
    Color::srgb(0.20, 0.80, 0.80), // 6 - cyan
    Color::srgb(0.95, 0.85, 0.30), // 7 - yellow
    Color::srgb(0.60, 0.45, 0.30), // 8 - brown
    Color::srgb(0.70, 0.70, 0.75), // 9 - silver
];

pub fn group_color(index: usize) -> Color {
    GROUP_COLORS[index.min(8)]
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
    group_state: Res<ControlGroupState>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    existing: Query<Entity, With<GroupHotkeyContent>>,
    unit_kinds: Query<&EntityKind, With<Unit>>,
    registry: Res<super::widget_framework::WidgetRegistry>,
    selected: Query<Entity, (With<Unit>, With<Selected>)>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::GroupHotkeys) {
        return;
    }

    let Some(content) =
        super::widget_framework::find_widget_content(WidgetId::GroupHotkeys, &widget_q, &content_q)
    else {
        return;
    };

    // Clear existing
    for entity in &existing {
        commands.entity(entity).try_despawn();
    }

    let has_selection = !selected.is_empty();
    let selected_set: Vec<Entity> = selected.iter().collect();

    let root = commands
        .spawn((
            GroupHotkeyContent,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(root);

    // Grid of group slots
    let container = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(3.0),
            row_gap: Val::Px(3.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(container);

    for (i, group) in control_groups.groups.iter().enumerate() {
        // Filter to alive entities
        let alive: Vec<Entity> = group
            .iter()
            .copied()
            .filter(|e| unit_kinds.get(*e).is_ok())
            .collect();

        let is_empty = alive.is_empty();
        let is_active = group_state.active_group == Some(i);
        // How many of the currently selected units are in this group
        let selected_in_group = selected_set
            .iter()
            .filter(|e| alive.contains(e))
            .count();
        let has_selected_members = selected_in_group > 0;

        // Determine visual state
        let (bg_color, border_color, border_width) = if is_active && !is_empty {
            // Currently recalled group
            (
                Color::srgba(0.15, 0.25, 0.45, 0.8),
                group_color(i),
                1.5,
            )
        } else if has_selected_members {
            // Contains some of the currently selected units
            (
                Color::srgba(0.18, 0.22, 0.28, 0.7),
                group_color(i).with_alpha(0.5),
                1.0,
            )
        } else if is_empty && has_selection {
            // Empty slot while units are selected — assignable
            (
                Color::srgba(0.12, 0.12, 0.12, 0.3),
                Color::srgba(0.4, 0.4, 0.4, 0.3),
                1.0,
            )
        } else if is_empty {
            (
                Color::srgba(0.15, 0.15, 0.15, 0.3),
                Color::NONE,
                0.0,
            )
        } else {
            (
                Color::srgba(0.2, 0.2, 0.25, 0.6),
                Color::NONE,
                0.0,
            )
        };

        let slot = commands
            .spawn((
                GroupSlotButton(i),
                Button,
                Node {
                    width: Val::Percent(31.0),
                    min_width: Val::Px(32.0),
                    min_height: Val::Px(32.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(border_width)),
                    row_gap: Val::Px(1.0),
                    ..default()
                },
                BackgroundColor(bg_color),
                BorderColor::all(border_color),
            ))
            .id();
        commands.entity(container).add_child(slot);

        // Group number label (top)
        let num_color = if is_active && !is_empty {
            group_color(i)
        } else if has_selected_members {
            group_color(i)
        } else if is_empty {
            theme::TEXT_DISABLED
        } else {
            theme::TEXT_PRIMARY
        };
        let num = commands
            .spawn((
                Text::new(format!("{}", i + 1)),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(num_color),
            ))
            .id();
        commands.entity(slot).add_child(num);

        if is_empty && has_selection {
            // Show "+" invite for assignable empty slots
            let plus = commands
                .spawn((
                    Text::new("+"),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(Color::srgba(0.5, 0.5, 0.5, 0.5)),
                ))
                .id();
            commands.entity(slot).add_child(plus);
        } else if !is_empty {
            // Group entities by EntityKind and count
            let mut kind_counts: Vec<(EntityKind, u32)> = Vec::new();
            for &e in &alive {
                if let Ok(kind) = unit_kinds.get(e) {
                    if let Some(entry) = kind_counts.iter_mut().find(|(k, _)| *k == *kind) {
                        entry.1 += 1;
                    } else {
                        kind_counts.push((*kind, 1));
                    }
                }
            }
            kind_counts.sort_by(|a, b| b.1.cmp(&a.1));

            let count_row = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(1.0),
                    ..default()
                })
                .id();
            commands.entity(slot).add_child(count_row);

            // Show up to 3 types, or top 2 + "+N" if more than 3
            let show_count = if kind_counts.len() > 3 { 2 } else { kind_counts.len() };
            for (kind, count) in kind_counts.iter().take(show_count) {
                let icon = commands
                    .spawn((
                        ImageNode::new(icons.entity_icon(*kind)),
                        Node {
                            width: Val::Px(10.0),
                            height: Val::Px(10.0),
                            ..default()
                        },
                    ))
                    .id();
                commands.entity(count_row).add_child(icon);

                let ct = commands
                    .spawn((
                        Text::new(format!("{}", count)),
                        TextFont {
                            font_size: theme::FONT_TINY,
                            ..default()
                        },
                        TextColor(theme::TEXT_SECONDARY),
                    ))
                    .id();
                commands.entity(count_row).add_child(ct);
            }

            if kind_counts.len() > 3 {
                let extra = kind_counts.len() - 2;
                let more = commands
                    .spawn((
                        Text::new(format!("+{}", extra)),
                        TextFont {
                            font_size: theme::FONT_TINY,
                            ..default()
                        },
                        TextColor(theme::TEXT_DISABLED),
                    ))
                    .id();
                commands.entity(count_row).add_child(more);
            }

            // If some selected units are in this group, show a small member indicator
            if has_selected_members && selected_in_group < alive.len() {
                let indicator = commands
                    .spawn((
                        Text::new(format!("{}/{}", selected_in_group, alive.len())),
                        TextFont {
                            font_size: theme::FONT_TINY,
                            ..default()
                        },
                        TextColor(group_color(i).with_alpha(0.7)),
                    ))
                    .id();
                commands.entity(slot).add_child(indicator);
            }
        }
    }

    // Hint row at bottom (only when units are selected)
    if has_selection {
        let hint = commands
            .spawn((
                Text::new("Ctrl+# set  Shift+# add  R-click assign"),
                TextFont {
                    font_size: theme::FONT_TINY,
                    ..default()
                },
                TextColor(Color::srgba(0.45, 0.45, 0.50, 0.7)),
                Node {
                    margin: UiRect::top(Val::Px(1.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(root).add_child(hint);
    }
}

/// Handle Ctrl+1..9 to assign, 1..9 to recall, Shift+1..9 to add, Alt+1..9 to steal
pub fn handle_control_group_keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut control_groups: ResMut<ControlGroups>,
    selected: Query<Entity, (With<Unit>, With<Selected>)>,
    time: Res<Time<Real>>,
    mut group_state: ResMut<ControlGroupState>,
    unit_transforms: Query<&GlobalTransform, With<Unit>>,
    mut camera_q: Query<&mut RtsCamera>,
) {
    let digit_keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);

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
        } else if alt {
            // Steal: assign selected to this group and remove from all others
            let units: Vec<Entity> = selected.iter().collect();
            if !units.is_empty() {
                let unit_set: std::collections::HashSet<Entity> =
                    units.iter().copied().collect();
                for (j, group) in control_groups.groups.iter_mut().enumerate() {
                    if j != i {
                        group.retain(|e| !unit_set.contains(e));
                    }
                }
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

            let now = time.elapsed_secs_f64();

            // Check for double-tap: same group recalled within 0.4s → center camera
            if group_state.last_recall_group == Some(i)
                && (now - group_state.last_recall_time) < 0.4
            {
                let mut sum = Vec3::ZERO;
                let mut count = 0u32;
                for entity in group {
                    if let Ok(gt) = unit_transforms.get(*entity) {
                        sum += gt.translation();
                        count += 1;
                    }
                }
                if count > 0 {
                    let center = sum / count as f32;
                    if let Ok(mut cam) = camera_q.single_mut() {
                        cam.target_pivot = center;
                    }
                }
                group_state.last_recall_group = None;
            } else {
                group_state.last_recall_group = Some(i);
                group_state.last_recall_time = now;
            }

            group_state.active_group = Some(i);

            for entity in selected.iter() {
                commands.entity(entity).remove::<Selected>();
            }
            for entity in group {
                commands.entity(*entity).try_insert(Selected);
            }
        }
    }
}

/// Left-click recalls group, right-click assigns selected units to group
pub fn handle_group_slot_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &GroupSlotButton), Changed<Interaction>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut control_groups: ResMut<ControlGroups>,
    selected: Query<Entity, (With<Unit>, With<Selected>)>,
    mut ui_press: ResMut<UiPressActive>,
    mut group_state: ResMut<ControlGroupState>,
) {
    for (interaction, slot) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_press.0 = true;

        if mouse.just_pressed(MouseButton::Right) {
            // Right-click: assign selected units to this group
            let units: Vec<Entity> = selected.iter().collect();
            if !units.is_empty() {
                control_groups.groups[slot.0] = units;
                group_state.active_group = Some(slot.0);
            }
        } else {
            // Left-click: recall group
            let group = &control_groups.groups[slot.0];
            if group.is_empty() {
                // If empty and units are selected, assign them
                let units: Vec<Entity> = selected.iter().collect();
                if !units.is_empty() {
                    control_groups.groups[slot.0] = units;
                    group_state.active_group = Some(slot.0);
                }
                continue;
            }
            group_state.active_group = Some(slot.0);
            for entity in selected.iter() {
                commands.entity(entity).remove::<Selected>();
            }
            for entity in group {
                commands.entity(*entity).try_insert(Selected);
            }
        }
    }
}
