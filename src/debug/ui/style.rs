use bevy::prelude::*;

pub(crate) fn debug_control_surface() -> Color {
    Color::srgba(0.06, 0.06, 0.06, 0.96)
}

pub(crate) fn debug_control_border() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.14)
}

pub(crate) fn debug_hover_surface() -> Color {
    Color::srgba(0.12, 0.12, 0.12, 0.98)
}

pub(crate) fn debug_pressed_surface() -> Color {
    Color::srgba(0.18, 0.18, 0.18, 0.98)
}

pub(crate) fn debug_active_surface() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.14)
}

pub(crate) fn debug_slider_fill() -> Color {
    Color::srgba(0.92, 0.92, 0.92, 0.96)
}

pub(crate) fn debug_text_primary() -> Color {
    Color::srgb(0.94, 0.94, 0.94)
}

pub(crate) fn debug_text_secondary() -> Color {
    Color::srgb(0.64, 0.64, 0.64)
}

pub(crate) fn debug_inverse_text() -> Color {
    Color::srgb(0.05, 0.05, 0.05)
}

pub(crate) fn debug_separator() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.10)
}

pub(crate) fn debug_emphasis_border() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.30)
}

pub(crate) fn debug_card_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(4.0),
        width: Val::Percent(100.0),
        padding: UiRect::all(Val::Px(6.0)),
        border: UiRect::all(Val::Px(1.0)),
        // border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

pub(crate) fn debug_row_node() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(10.0),
        width: Val::Percent(100.0),
        padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
        border: UiRect::all(Val::Px(1.0)),
        // border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

pub(crate) fn debug_pill_node() -> Node {
    Node {
        padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
        // border_radius: BorderRadius::all(Val::Px(999.0)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

pub(crate) fn format_tweak_float(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{:.0}", v)
    } else if v.abs() >= 10.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.3}", v)
    }
}
