//! Shared UI spacing, radius, and layout constants.
//!
//! Use these instead of inline `Val::Px(...)` values to keep the UI consistent.

use bevy::prelude::*;

// ── Spacing ──

pub const GAP_XS: Val = Val::Px(2.0);
pub const GAP_SM: Val = Val::Px(3.0);
pub const GAP_MD: Val = Val::Px(4.0);
pub const GAP_LG: Val = Val::Px(6.0);
pub const GAP_XL: Val = Val::Px(8.0);
pub const GAP_2XL: Val = Val::Px(10.0);

// ── Padding presets ──

pub const PAD_SM: UiRect = UiRect::all(Val::Px(4.0));
pub const PAD_MD: UiRect = UiRect::all(Val::Px(6.0));
pub const PAD_LG: UiRect = UiRect::all(Val::Px(8.0));

/// Common button/card padding: 8px horizontal, 4px vertical.
pub const PAD_BUTTON: UiRect = UiRect {
    left: Val::Px(8.0),
    right: Val::Px(8.0),
    top: Val::Px(4.0),
    bottom: Val::Px(4.0),
};

/// Compact padding: 6px horizontal, 3px vertical.
pub const PAD_COMPACT: UiRect = UiRect {
    left: Val::Px(6.0),
    right: Val::Px(6.0),
    top: Val::Px(3.0),
    bottom: Val::Px(3.0),
};

// ── Border radius ──

pub const RADIUS_XS: BorderRadius = BorderRadius::all(Val::Px(0.0));
pub const RADIUS_SM: BorderRadius = BorderRadius::all(Val::Px(0.0));
pub const RADIUS_MD: BorderRadius = BorderRadius::all(Val::Px(0.0));
pub const RADIUS_LG: BorderRadius = BorderRadius::all(Val::Px(0.0));
pub const RADIUS_XL: BorderRadius = BorderRadius::all(Val::Px(0.0));
pub const RADIUS_2XL: BorderRadius = BorderRadius::all(Val::Px(0.0));

// ── Border widths ──

pub const BORDER_1: UiRect = UiRect::all(Val::Px(1.0));
pub const BORDER_2: UiRect = UiRect::all(Val::Px(2.0));

// ── Layout helpers ──

/// Full-width column with a given row gap.
pub fn flex_col(gap: Val) -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Stretch,
        row_gap: gap,
        ..default()
    }
}

/// Full-width row with a given column gap, items centered vertically.
pub fn flex_row(gap: Val) -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: gap,
        ..default()
    }
}

/// Wrapping row with column and row gaps.
pub fn flex_wrap(col_gap: Val, row_gap: Val) -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::Center,
        column_gap: col_gap,
        row_gap,
        ..default()
    }
}

/// Progress bar background track.
pub fn progress_track(width: Val, height: f32) -> Node {
    Node {
        width,
        height: Val::Px(height),
        // border_radius: RADIUS_SM,
        ..default()
    }
}

/// Progress bar fill (percentage-based width).
pub fn progress_fill(pct: f32) -> Node {
    Node {
        width: Val::Percent(pct),
        height: Val::Percent(100.0),
        // border_radius: RADIUS_SM,
        ..default()
    }
}
