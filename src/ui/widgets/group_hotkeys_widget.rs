use bevy::prelude::*;

use super::core::constants::*;
use super::core::framework::WidgetId;
use super::core::hud::MainHudRoot;
use crate::blueprints::EntityKind;
use crate::types::*;
use crate::ui::theme::{self, Theme};

pub struct GroupHotkeysWidgetPlugin;

impl Plugin for GroupHotkeysWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ControlGroups>()
            .add_systems(
                Update,
                spawn_group_hotkeys_widget
                    .run_if(in_state(AppState::InGame))
                    .run_if(any_with_component::<MainHudRoot>),
            )
            .add_systems(
                Update,
                (
                    update_group_hotkeys_widget,
                    handle_group_slot_click,
                    group_slot_interaction_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                handle_control_group_keys
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            );
    }
}

widget_spawn_system!(spawn_group_hotkeys_widget, WidgetId::GroupHotkeys);

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

// Group-specific colors for badges — sourced from the theme palette.
const GROUP_COLORS: [Color; 9] = theme::GROUP_COLORS;

pub fn group_color(index: usize) -> Color {
    GROUP_COLORS[index.min(8)]
}

// ── Widget UI ──

#[derive(Component)]
pub struct GroupHotkeyContent;

#[derive(Component)]
pub struct GroupSlotButton(pub usize);

fn grid_columns_for(count: usize) -> u16 {
    match count {
        0..=2 => count.max(1) as u16,
        3 => 3,
        4 => 2,
        5..=6 => 3,
        7..=8 => 4,
        _ => 3, // 9 slots -> 3x3
    }
}

pub fn update_group_hotkeys_widget(
    mut commands: Commands,
    icons: Res<IconAssets>,
    theme: Res<Theme>,
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
                row_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(root);

    // Responsive grid of group slots (CSS Grid)
    let cols = grid_columns_for(control_groups.groups.len());
    let container = commands
        .spawn(Node {
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::flex(cols, 1.0),
            column_gap: Val::Px(4.0),
            row_gap: Val::Px(4.0),
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
        let selected_in_group = selected_set.iter().filter(|e| alive.contains(e)).count();
        let has_selected_members = selected_in_group > 0;

        // Determine visual state (base/default, hover/press handled by interaction system)
        let (bg_color, border_color) = if is_active && !is_empty {
            // Currently recalled group
            (Color::srgba(0.15, 0.25, 0.45, 0.8), group_color(i))
        } else if has_selected_members {
            // Contains some of the currently selected units
            (
                Color::srgba(0.18, 0.22, 0.28, 0.7),
                group_color(i).with_alpha(0.5),
            )
        } else if is_empty && has_selection {
            // Empty slot while units are selected — assignable
            (
                theme::BG_SURFACE.with_alpha(0.3),
                theme::TEXT_DISABLED.with_alpha(0.3),
            )
        } else if is_empty {
            (theme::BG_ELEVATED.with_alpha(0.3), Color::NONE)
        } else {
            (theme::BG_ELEVATED.with_alpha(0.6), Color::NONE)
        };

        let slot = commands
            .spawn((
                GroupSlotButton(i),
                Button,
                Node {
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: PAD_SM,
                    border: BORDER_1,
                    // border_radius: RADIUS_LG,
                    min_width: Val::Px(25.0),
                    min_height: Val::Px(25.0),
                    flex_direction: FlexDirection::Column,
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
            theme.colors.text_disabled
        } else {
            theme.colors.text_primary
        };
        let num = commands
            .spawn((
                Text::new(format!("{}", i + 1)),
                TextFont {
                    font_size: theme.typography.small,
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
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme::TEXT_DISABLED.with_alpha(0.5)),
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
            let show_count = if kind_counts.len() > 3 {
                2
            } else {
                kind_counts.len()
            };
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
                            font_size: theme.typography.tiny,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
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
                            font_size: theme.typography.tiny,
                            ..default()
                        },
                        TextColor(theme.colors.text_disabled),
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
                            font_size: theme.typography.tiny,
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
                    font_size: theme.typography.tiny,
                    ..default()
                },
                TextColor(theme::TEXT_DISABLED.with_alpha(0.7)),
                Node {
                    margin: UiRect::top(Val::Px(1.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(root).add_child(hint);
    }
}

fn group_slot_interaction_system(
    mut interactions: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
            &GroupSlotButton,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    control_groups: Res<ControlGroups>,
    group_state: Res<ControlGroupState>,
    unit_kinds: Query<&EntityKind, With<Unit>>,
) {
    let hovered_bg = theme::BG_ELEVATED;
    let pressed_bg = theme::SUCCESS;
    let border_hovered = theme::TEXT_PRIMARY.with_alpha(0.25);
    let border_pressed = theme::SUCCESS.with_alpha(0.7);

    for (interaction, mut bg, mut border, slot) in &mut interactions {
        let i = slot.0;

        let alive: Vec<Entity> = control_groups.groups[i]
            .iter()
            .copied()
            .filter(|e| unit_kinds.get(*e).is_ok())
            .collect();

        let is_empty = alive.is_empty();
        let is_active = group_state.active_group == Some(i);

        let (base_bg, base_border) = if is_active && !is_empty {
            (Color::srgba(0.15, 0.25, 0.45, 0.8), group_color(i))
        } else if is_empty {
            (theme::BG_ELEVATED.with_alpha(0.3), Color::NONE)
        } else {
            (theme::BG_ELEVATED.with_alpha(0.6), Color::NONE)
        };

        match *interaction {
            Interaction::Pressed => {
                *bg = pressed_bg.into();
                *border = BorderColor::all(border_pressed);
            }
            Interaction::Hovered => {
                *bg = hovered_bg.into();
                *border = BorderColor::all(border_hovered);
            }
            Interaction::None => {
                *bg = base_bg.into();
                *border = BorderColor::all(base_border);
            }
        }
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
                let unit_set: std::collections::HashSet<Entity> = units.iter().copied().collect();
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
