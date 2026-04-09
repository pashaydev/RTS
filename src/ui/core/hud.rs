use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::types::*;
use crate::ui::theme::{self, Theme};

/// Root UI container that holds all widgets
#[derive(Component)]
pub struct UiRoot;


#[derive(Component)]
pub struct MainHudRoot;

/// Container for the widget grid area, positioned below the header bar.
/// Widgets are parented to this entity so percent-based grid positioning
/// works within the remaining screen space.
#[derive(Component)]
pub struct WidgetGridArea;

/// Floating label showing biome placement feedback
#[derive(Component)]
pub struct PlacementHintLabel;

pub fn spawn_hud_roots(
    mut commands: Commands,
    theme: Res<Theme>,
    existing_roots: Query<Entity, With<UiRoot>>,
) {
    if !existing_roots.is_empty() {
        return;
    }

    // Root full-screen container for in-game HUD layering
    let root = commands
        .spawn((
            GameWorld,
            UiRoot,
            DespawnOnExit(AppState::InGame),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();

    let back_overlay_root = commands
        .spawn((
            GameWorld,
            WorldOverlayBackRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(-10),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(root).add_child(back_overlay_root);

    let hud_root = commands
        .spawn((
            GameWorld,
            MainHudRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(root).add_child(hud_root);

    // Widget grid area — sits below the header bar, fills remaining space.
    let grid_area = commands
        .spawn((
            GameWorld,
            WidgetGridArea,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(super::framework::HEADER_HEIGHT_PX),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(hud_root).add_child(grid_area);

    let front_overlay_root = commands
        .spawn((
            GameWorld,
            WorldOverlayFrontRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(80),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(root).add_child(front_overlay_root);

    // Spawn placement hint label (hidden by default)
    let placement_hint = commands
        .spawn((
            PlacementHintLabel,
            WorldOverlayFrontItem,
            Text::new(""),
            TextFont {
                font_size: theme.typography.body,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                max_width: Val::Px(420.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                // border_radius: BorderRadius::all(Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme::TOOLTIP_BG.with_alpha(0.9)),
            BorderColor::all(theme::TOOLTIP_BORDER.with_alpha(0.5)),
            GlobalZIndex(90),
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    commands
        .entity(front_overlay_root)
        .add_child(placement_hint);
}

pub fn compute_ui_mode(
    mut ui_mode: ResMut<UiMode>,
    placement: Res<BuildingPlacementState>,
    selected_units: Query<Entity, (With<Unit>, With<Selected>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
) {
    let new_mode = match placement.mode {
        PlacementMode::Placing(kind) => UiMode::PlacingBuilding(kind),
        PlacementMode::PlotBase => UiMode::PlacingBuilding(crate::blueprints::EntityKind::Base),
        PlacementMode::None
        | PlacementMode::PlotWall { .. }
        | PlacementMode::PlotGate
        | PlacementMode::PlotFloor => {
            if let Ok(building_entity) = selected_buildings.single() {
                UiMode::SelectedBuilding(building_entity)
            } else {
                let units: Vec<Entity> = selected_units.iter().collect();
                if units.is_empty() {
                    UiMode::Idle
                } else {
                    UiMode::SelectedUnits(units)
                }
            }
        }
    };

    if *ui_mode != new_mode {
        *ui_mode = new_mode;
    }
}

fn placement_default_hint(mode: PlacementMode) -> Option<String> {
    match mode {
        PlacementMode::None => None,
        PlacementMode::Placing(kind) => Some(format!(
            "Placing {}: Left-click ground to place (Right-click/Escape to cancel, H/J to rotate)",
            kind.display_name()
        )),
        PlacementMode::PlotBase => Some(
            "Founding Base: Left-click ground to place (Right-click/Escape to cancel)".to_string(),
        ),
        PlacementMode::PlotWall { start } => {
            if start == Vec3::ZERO {
                Some("Wall: Click ground to start (Right-click/Escape to cancel)".to_string())
            } else {
                Some(
                    "Wall: Move cursor, then left-click to confirm (Right-click/Escape to cancel)"
                        .to_string(),
                )
            }
        }
        PlacementMode::PlotGate => Some(
            "Gatehouse: Hover owned wall and left-click (Right-click/Escape to cancel)".to_string(),
        ),
        PlacementMode::PlotFloor => Some(
            "Floor: Hold left-click and drag to paint (Right-click/Escape to cancel)".to_string(),
        ),
    }
}

fn is_error_hint(hint: &str) -> bool {
    hint.contains("Not enough")
        || hint.contains("must")
        || hint.contains("blocked")
        || hint.contains("No workers")
        || hint.contains("Cannot place")
}

pub fn update_placement_hint(
    placement: Res<BuildingPlacementState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    theme: Res<Theme>,
    mut hint_q: Query<
        (&mut Text, &mut TextColor, &mut Node, &mut Visibility),
        With<PlacementHintLabel>,
    >,
) {
    let Ok((mut text, mut color, mut node, mut vis)) = hint_q.single_mut() else {
        return;
    };

    let hint = placement
        .hint_text
        .clone()
        .or_else(|| placement_default_hint(placement.mode));

    if let Some(hint) = hint {
        **text = hint.clone();
        *color = TextColor(if is_error_hint(&hint) {
            theme.colors.destructive
        } else {
            theme.colors.text_secondary
        });

        if let Ok(window) = windows.single() {
            if let Some(cursor) = window.cursor_position() {
                let scale = ui_scale.0.max(0.001);
                let ui_w = window.width() / scale;
                let ui_h = window.height() / scale;
                let left = (cursor.x / scale + 14.0).clamp(6.0, (ui_w - 430.0).max(6.0));
                let top = (cursor.y / scale + 18.0).clamp(6.0, (ui_h - 70.0).max(6.0));
                node.left = Val::Px(left);
                node.top = Val::Px(top);
            }
        }

        *vis = Visibility::Inherited;
    } else {
        *vis = Visibility::Hidden;
    }
}

pub fn update_ui_scale(
    graphics: Res<GraphicsSettings>,
    snapshot: Option<Res<crate::ui::menu::OptionsSnapshot>>,
    mut ui_scale: ResMut<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else { return };
    let logical_height = window.physical_height() as f32 / window.scale_factor();
    let base_height = 720.0_f32;
    let auto = logical_height / base_height;
    // Use snapshot value while editing options, current value otherwise
    let scale_value = snapshot.map(|s| s.graphics.ui_scale).unwrap_or(graphics.ui_scale);
    let new_scale = auto * scale_value;
    if (ui_scale.0 - new_scale).abs() > 0.001 {
        ui_scale.0 = new_scale;
    }
}

pub fn adopt_back_overlay_items(
    root_q: Query<Entity, With<WorldOverlayBackRoot>>,
    items: Query<Entity, (With<WorldOverlayBackItem>, Without<ChildOf>)>,
    mut commands: Commands,
) {
    let Ok(root) = root_q.single() else {
        return;
    };
    for entity in &items {
        commands.entity(root).add_child(entity);
    }
}

pub fn adopt_front_overlay_items(
    root_q: Query<Entity, With<WorldOverlayFrontRoot>>,
    items: Query<Entity, (With<WorldOverlayFrontItem>, Without<ChildOf>)>,
    mut commands: Commands,
) {
    let Ok(root) = root_q.single() else {
        return;
    };
    for entity in &items {
        commands.entity(root).add_child(entity);
    }
}
