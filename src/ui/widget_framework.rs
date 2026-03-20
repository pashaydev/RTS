use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

use crate::theme;
use crate::ui::fonts::{self, UiFonts};

// ── Widget Identifiers ──

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum WidgetId {
    Resources,
    Selection,
    Actions,
    Minimap,
    ProductionQueue,
    ArmyOverview,
    TechTree,
    GroupHotkeys,
    EventLog,
    Debug,
}

impl WidgetId {
    pub const ALL: &'static [WidgetId] = &[
        WidgetId::Resources,
        WidgetId::ArmyOverview,
        WidgetId::Selection,
        WidgetId::Actions,
        WidgetId::Minimap,
        WidgetId::ProductionQueue,
        WidgetId::TechTree,
        WidgetId::GroupHotkeys,
        WidgetId::EventLog,
        WidgetId::Debug,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            WidgetId::Resources => "Resources",
            WidgetId::Selection => "Selection",
            WidgetId::Actions => "Actions",
            WidgetId::Minimap => "Map",
            WidgetId::ProductionQueue => "Queue",
            WidgetId::ArmyOverview => "Army",
            WidgetId::TechTree => "Tech",
            WidgetId::GroupHotkeys => "Groups",
            WidgetId::EventLog => "Log",
            WidgetId::Debug => "Debug",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            WidgetId::Resources => "R",
            WidgetId::Selection => "S",
            WidgetId::Actions => "A",
            WidgetId::Minimap => "M",
            WidgetId::ProductionQueue => "Q",
            WidgetId::ArmyOverview => "U",
            WidgetId::TechTree => "T",
            WidgetId::GroupHotkeys => "G",
            WidgetId::EventLog => "L",
            WidgetId::Debug => "D",
        }
    }

    pub fn hotkey(self) -> KeyCode {
        match self {
            WidgetId::Resources => KeyCode::F1,
            WidgetId::ArmyOverview => KeyCode::F2,
            WidgetId::Selection => KeyCode::F3,
            WidgetId::Actions => KeyCode::F4,
            WidgetId::Minimap => KeyCode::F5,
            WidgetId::ProductionQueue => KeyCode::F6,
            WidgetId::TechTree => KeyCode::F7,
            WidgetId::GroupHotkeys => KeyCode::F8,
            WidgetId::EventLog => KeyCode::F9,
            WidgetId::Debug => KeyCode::F10,
        }
    }
}

// ── Grid Slot ──

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GridSlot {
    pub col: u8,
    pub row: u8,
    pub col_span: u8,
    pub row_span: u8,
}

impl GridSlot {
    pub const fn new(col: u8, row: u8, col_span: u8, row_span: u8) -> Self {
        Self {
            col,
            row,
            col_span,
            row_span,
        }
    }
}

// ── Components ──

#[derive(Component)]
pub struct Widget {
    pub id: WidgetId,
}

#[derive(Component)]
pub struct WidgetContent;

#[derive(Component)]
pub struct WidgetCloseButton(pub WidgetId);

#[derive(Component)]
pub struct WidgetTitleBar(#[allow(dead_code)] pub WidgetId);

#[derive(Component)]
pub struct WidgetDragHandle(pub Entity);

#[derive(Component)]
pub struct WidgetResizeHandle(pub Entity);

#[derive(Resource, Default)]
pub struct WidgetResizeState {
    pub active_widget: Option<Entity>,
    pub start_cursor: Vec2,
    pub start_size: Vec2,
}

#[derive(Resource, Default)]
pub struct WidgetDragState {
    pub active_widget: Option<Entity>,
    pub start_cursor: Vec2,
    pub start_pos: Vec2,
}

#[derive(Component)]
pub struct GridOverlay;

#[derive(Resource, Default)]
pub struct GridInteractionActive(pub bool);

// ── Registry Resource ──

#[derive(Resource)]
pub struct WidgetRegistry {
    pub slots: HashMap<WidgetId, GridSlot>,
    pub visibility: HashMap<WidgetId, bool>,
    pub top_z: i32,
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        let mut slots = HashMap::new();
        slots.insert(WidgetId::Resources, GridSlot::new(0, 0, 1, 2));
        slots.insert(WidgetId::ArmyOverview, GridSlot::new(0, 2, 1, 2));
        slots.insert(WidgetId::GroupHotkeys, GridSlot::new(0, 4, 1, 3));
        slots.insert(WidgetId::Selection, GridSlot::new(0, 7, 2, 5));
        slots.insert(WidgetId::Actions, GridSlot::new(2, 7, 2, 5));
        slots.insert(WidgetId::ProductionQueue, GridSlot::new(7, 7, 2, 5));
        slots.insert(WidgetId::Minimap, GridSlot::new(9, 7, 3, 5));
        slots.insert(WidgetId::EventLog, GridSlot::new(10, 0, 2, 2));
        slots.insert(WidgetId::TechTree, GridSlot::new(3, 4, 6, 4));
        slots.insert(WidgetId::Debug, GridSlot::new(10, 0, 4, 6));

        let mut visibility = HashMap::new();
        visibility.insert(WidgetId::Resources, true);
        visibility.insert(WidgetId::Selection, true);
        visibility.insert(WidgetId::Actions, true);
        visibility.insert(WidgetId::Minimap, true);
        visibility.insert(WidgetId::ArmyOverview, true);
        visibility.insert(WidgetId::ProductionQueue, true);
        visibility.insert(WidgetId::TechTree, false);
        visibility.insert(WidgetId::GroupHotkeys, true);
        visibility.insert(WidgetId::EventLog, true);
        visibility.insert(WidgetId::Debug, false);

        Self {
            slots,
            visibility,
            top_z: 0,
        }
    }
}

impl WidgetRegistry {
    pub fn is_visible(&self, id: WidgetId) -> bool {
        self.visibility.get(&id).copied().unwrap_or(false)
    }

    pub fn toggle(&mut self, id: WidgetId) {
        let vis = self.visibility.entry(id).or_insert(false);
        *vis = !*vis;
    }

    pub fn set_visible(&mut self, id: WidgetId, visible: bool) {
        self.visibility.insert(id, visible);
    }
}

// ── Grid-to-Style Conversion ──

const GRID_COLS: f32 = 12.0;
const GRID_ROWS: f32 = 12.0;

pub fn grid_to_style(slot: &GridSlot) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(slot.col as f32 * (100.0 / GRID_COLS)),
        top: Val::Percent(slot.row as f32 * (100.0 / GRID_ROWS)),
        width: Val::Percent(slot.col_span as f32 * (100.0 / GRID_COLS)),
        height: Val::Percent(slot.row_span as f32 * (100.0 / GRID_ROWS)),
        flex_direction: FlexDirection::Column,
        overflow: Overflow::clip(),
        ..default()
    }
}

// ── Widget Frame Spawning ──

/// Spawns a widget frame with title bar, pin/close buttons, and content area.
/// Returns the content area entity for populating with widget-specific content.
pub fn spawn_widget_frame(
    commands: &mut Commands,
    parent: Entity,
    id: WidgetId,
    slot: &GridSlot,
    visible: bool,
    fonts: &UiFonts,
) -> Entity {
    let mut node = grid_to_style(slot);
    node.padding = UiRect::all(Val::Px(2.0));

    let widget_entity = commands
        .spawn((
            Widget { id },
            Interaction::None,
            ZIndex(0),
            node,
            BackgroundColor(theme::BG_PANEL),
            BorderColor::all(theme::SEPARATOR),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.5),
                Val::Px(0.0),
                Val::Px(2.0),
                Val::Px(0.0),
                Val::Px(8.0),
            ),
            if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ))
        .id();
    commands.entity(parent).add_child(widget_entity);

    // Title bar (Draggable Handle)
    let title_bar = commands
        .spawn((
            WidgetTitleBar(id),
            WidgetDragHandle(widget_entity),
            Interaction::None,
            Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                min_height: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.6)),
        ))
        .with_children(|bar| {
            // Title text
            bar.spawn((
                Text::new(id.display_name().to_uppercase()),
                fonts::heading(fonts, theme::FONT_SMALL),
                TextColor(theme::TEXT_SECONDARY),
            ));

            // Close button
            bar.spawn((
                Button,
                WidgetCloseButton(id),
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|close| {
                close.spawn((
                    Text::new("X"),
                    fonts::body_emphasis(fonts, theme::FONT_TINY),
                    TextColor(theme::TEXT_DISABLED),
                ));
            });
        })
        .id();
    commands.entity(widget_entity).add_child(title_bar);

    // Separator
    let sep = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                ..default()
            },
            BackgroundColor(theme::SEPARATOR),
        ))
        .id();
    commands.entity(widget_entity).add_child(sep);

    // Content area (scrollable)
    let content = commands
        .spawn((
            WidgetContent,
            Interaction::None,
            ScrollPosition::default(),
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(widget_entity).add_child(content);

    // Resize Handle
    let resize_handle = commands
        .spawn((
            WidgetResizeHandle(widget_entity),
            Interaction::None,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(12.0),
                height: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
        ))
        .id();
    commands.entity(widget_entity).add_child(resize_handle);

    content
}

// ── Widget Content Lookup ──

/// Find the content area entity for a widget by its ID.
pub fn find_widget_content(
    id: WidgetId,
    widget_q: &Query<(&Widget, &Children)>,
    content_q: &Query<Entity, With<WidgetContent>>,
) -> Option<Entity> {
    for (widget, widget_children) in widget_q {
        if widget.id == id {
            for wchild in widget_children.iter() {
                if content_q.get(wchild).is_ok() {
                    return Some(wchild);
                }
            }
        }
    }
    None
}

// ── Sync Widget Visibility ──

pub fn sync_widget_visibility(
    registry: Res<WidgetRegistry>,
    mut widgets: Query<(&Widget, &mut Visibility)>,
) {
    if !registry.is_changed() {
        return;
    }
    for (widget, mut vis) in &mut widgets {
        let should_show = registry.is_visible(widget.id);
        let new_vis = if should_show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != new_vis {
            *vis = new_vis;
        }
    }
}

// ── Handle Close Button ──

pub fn handle_widget_buttons(
    mut registry: ResMut<WidgetRegistry>,
    close_q: Query<(&Interaction, &WidgetCloseButton), Changed<Interaction>>,
) {
    for (interaction, close_btn) in &close_q {
        if *interaction == Interaction::Pressed {
            registry.set_visible(close_btn.0, false);
        }
    }
}

// ── Scroll, Drag, & Resize Systems ──

pub fn handle_widget_scroll(
    mut mouse_wheel: MessageReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut scroll_q: Query<
        (&mut ScrollPosition, &ComputedNode, &UiGlobalTransform),
        With<WidgetContent>,
    >,
) {
    let mut dy: f32 = 0.0;
    for ev in mouse_wheel.read() {
        dy += match ev.unit {
            MouseScrollUnit::Line => -ev.y * 24.0,
            MouseScrollUnit::Pixel => -ev.y,
        };
    }
    if dy.abs() < 0.001 {
        return;
    }

    let Some(cursor_phys) = windows
        .single()
        .ok()
        .and_then(|w| w.physical_cursor_position())
    else {
        return;
    };

    for (mut scroll_pos, computed, ui_tf) in &mut scroll_q {
        if computed.contains_point(*ui_tf, cursor_phys) {
            let max_scroll = (computed.content_size().y - computed.size().y).max(0.0)
                * computed.inverse_scale_factor();
            scroll_pos.y = (scroll_pos.y + dy).clamp(0.0, max_scroll);
        }
    }
}

// ── Grid Overlay ──

pub fn spawn_grid_overlay(mut commands: Commands, existing: Query<Entity, With<GridOverlay>>) {
    if !existing.is_empty() {
        return;
    }

    commands
        .spawn((
            GridOverlay,
            Visibility::Hidden,
            ZIndex(9999),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            // 13 vertical lines (at each column boundary 0..=12)
            for i in 0..=12 {
                let pct = i as f32 * (100.0 / GRID_COLS);
                parent.spawn((
                    Pickable::IGNORE,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(pct),
                        top: Val::Px(0.0),
                        width: Val::Px(1.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(theme::GRID_LINE),
                ));
            }
            // 13 horizontal lines (at each row boundary 0..=12)
            for i in 0..=12 {
                let pct = i as f32 * (100.0 / GRID_ROWS);
                parent.spawn((
                    Pickable::IGNORE,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Percent(pct),
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        ..default()
                    },
                    BackgroundColor(theme::GRID_LINE),
                ));
            }
        });
}

pub fn toggle_grid_overlay(
    interaction: Res<GridInteractionActive>,
    mut overlay: Query<&mut Visibility, With<GridOverlay>>,
) {
    if !interaction.is_changed() {
        return;
    }
    for mut vis in &mut overlay {
        *vis = if interaction.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

// ── Snap Helper ──

fn pixel_to_grid_slot(
    px_x: f32,
    px_y: f32,
    px_w: f32,
    px_h: f32,
    win_w: f32,
    win_h: f32,
) -> GridSlot {
    let cell_w = win_w / GRID_COLS;
    let cell_h = win_h / GRID_ROWS;

    let col = (px_x / cell_w).round().clamp(0.0, 11.0) as u8;
    let row = (px_y / cell_h).round().clamp(0.0, 11.0) as u8;
    let col_span = (px_w / cell_w).round().clamp(1.0, (12 - col) as f32) as u8;
    let row_span = (px_h / cell_h).round().clamp(1.0, (12 - row) as f32) as u8;

    GridSlot::new(col, row, col_span, row_span)
}

// ── Drag & Resize Systems ──

pub fn handle_widget_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut drag_state: ResMut<WidgetDragState>,
    mut registry: ResMut<WidgetRegistry>,
    mut grid_active: ResMut<GridInteractionActive>,
    interactions: Query<(&Interaction, &WidgetDragHandle)>,
    mut widget_nodes: Query<(&mut Node, &mut ZIndex, &Widget, &ComputedNode), With<Widget>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_raw) = window.cursor_position() else {
        return;
    };
    let scale = ui_scale.0.max(0.001);
    let cursor = cursor_raw / scale;

    // Snap on release (before clearing active_widget)
    if mouse.just_released(MouseButton::Left) {
        if let Some(widget_entity) = drag_state.active_widget {
            if let Ok((mut node, _, widget, computed)) = widget_nodes.get_mut(widget_entity) {
                let win_w = window.width() / scale;
                let win_h = window.height() / scale;
                let inv_scale = computed.inverse_scale_factor();

                let px_x = match node.left {
                    Val::Px(x) => x,
                    Val::Percent(p) => p / 100.0 * win_w,
                    _ => 0.0,
                };
                let px_y = match node.top {
                    Val::Px(y) => y,
                    Val::Percent(p) => p / 100.0 * win_h,
                    _ => 0.0,
                };
                let px_w = match node.width {
                    Val::Px(w) => w,
                    Val::Percent(p) => p / 100.0 * win_w,
                    _ => computed.size().x * inv_scale,
                };
                let px_h = match node.height {
                    Val::Px(h) => h,
                    Val::Percent(p) => p / 100.0 * win_h,
                    _ => computed.size().y * inv_scale,
                };

                let slot = pixel_to_grid_slot(px_x, px_y, px_w, px_h, win_w, win_h);
                let snapped = grid_to_style(&slot);
                node.left = snapped.left;
                node.top = snapped.top;
                node.width = snapped.width;
                node.height = snapped.height;
                registry.slots.insert(widget.id, slot);
            }
            grid_active.0 = false;
        }
        drag_state.active_widget = None;
    }

    if mouse.just_pressed(MouseButton::Left) {
        for (interaction, handle) in &interactions {
            if *interaction == Interaction::Pressed || *interaction == Interaction::Hovered {
                let widget_entity = handle.0;
                if let Ok((node, mut z_index, _, _)) = widget_nodes.get_mut(widget_entity) {
                    registry.top_z += 1;
                    *z_index = ZIndex(registry.top_z);

                    drag_state.active_widget = Some(widget_entity);
                    drag_state.start_cursor = cursor;
                    grid_active.0 = true;

                    let win_w = window.width();
                    let win_h = window.height();
                    let win_w = win_w / scale;
                    let win_h = win_h / scale;

                    let start_x = match node.left {
                        Val::Px(x) => x,
                        Val::Percent(p) => p / 100.0 * win_w,
                        _ => 0.0,
                    };
                    let start_y = match node.top {
                        Val::Px(y) => y,
                        Val::Percent(p) => p / 100.0 * win_h,
                        _ => 0.0,
                    };
                    drag_state.start_pos = Vec2::new(start_x, start_y);
                }
                break;
            }
        }
    }

    if mouse.pressed(MouseButton::Left) {
        if let Some(widget_entity) = drag_state.active_widget {
            let delta = cursor - drag_state.start_cursor;
            if let Ok((mut node, _, _, _)) = widget_nodes.get_mut(widget_entity) {
                node.left = Val::Px(drag_state.start_pos.x + delta.x);
                node.top = Val::Px(drag_state.start_pos.y + delta.y);
            }
        }
    }
}

pub fn handle_widget_resize(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut resize_state: ResMut<WidgetResizeState>,
    mut registry: ResMut<WidgetRegistry>,
    mut grid_active: ResMut<GridInteractionActive>,
    interactions: Query<(&Interaction, &WidgetResizeHandle)>,
    mut widget_nodes: Query<(&mut Node, &ComputedNode, &mut ZIndex, &Widget), With<Widget>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_raw) = window.cursor_position() else {
        return;
    };
    let scale = ui_scale.0.max(0.001);
    let cursor = cursor_raw / scale;

    // Snap on release (before clearing active_widget)
    if mouse.just_released(MouseButton::Left) {
        if let Some(widget_entity) = resize_state.active_widget {
            if let Ok((mut node, computed, _, widget)) = widget_nodes.get_mut(widget_entity) {
                let win_w = window.width() / scale;
                let win_h = window.height() / scale;
                let inv_scale = computed.inverse_scale_factor();

                let px_x = match node.left {
                    Val::Px(x) => x,
                    Val::Percent(p) => p / 100.0 * win_w,
                    _ => 0.0,
                };
                let px_y = match node.top {
                    Val::Px(y) => y,
                    Val::Percent(p) => p / 100.0 * win_h,
                    _ => 0.0,
                };
                let px_w = match node.width {
                    Val::Px(w) => w,
                    Val::Percent(p) => p / 100.0 * win_w,
                    _ => computed.size().x * inv_scale,
                };
                let px_h = match node.height {
                    Val::Px(h) => h,
                    Val::Percent(p) => p / 100.0 * win_h,
                    _ => computed.size().y * inv_scale,
                };

                let slot = pixel_to_grid_slot(px_x, px_y, px_w, px_h, win_w, win_h);
                let snapped = grid_to_style(&slot);
                node.left = snapped.left;
                node.top = snapped.top;
                node.width = snapped.width;
                node.height = snapped.height;
                registry.slots.insert(widget.id, slot);
            }
            grid_active.0 = false;
        }
        resize_state.active_widget = None;
    }

    if mouse.just_pressed(MouseButton::Left) {
        for (interaction, handle) in &interactions {
            if *interaction == Interaction::Pressed || *interaction == Interaction::Hovered {
                if let Ok((_, computed, mut z_index, _)) = widget_nodes.get_mut(handle.0) {
                    registry.top_z += 1;
                    *z_index = ZIndex(registry.top_z);

                    let inv_scale = computed.inverse_scale_factor();
                    resize_state.active_widget = Some(handle.0);
                    resize_state.start_cursor = cursor;
                    resize_state.start_size = computed.size() * inv_scale;
                    grid_active.0 = true;
                }
                break;
            }
        }
    }

    if mouse.pressed(MouseButton::Left) {
        if let Some(widget_entity) = resize_state.active_widget {
            let delta = cursor - resize_state.start_cursor;
            if let Ok((mut node, _, _, _)) = widget_nodes.get_mut(widget_entity) {
                let new_w = (resize_state.start_size.x + delta.x).max(120.0);
                let new_h = (resize_state.start_size.y + delta.y).max(70.0);
                node.width = Val::Px(new_w);
                node.height = Val::Px(new_h);
            }
        }
    } else {
        resize_state.active_widget = None;
    }
}

pub fn update_resize_handle_visuals(
    mut handles: Query<(&Interaction, &mut BackgroundColor), With<WidgetResizeHandle>>,
) {
    for (interaction, mut bg) in &mut handles {
        match interaction {
            Interaction::Pressed => *bg = BackgroundColor(theme::ACCENT),
            Interaction::Hovered => *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
            Interaction::None => *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
        }
    }
}
