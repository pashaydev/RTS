use bevy::prelude::*;

use crate::types::{ButtonAnimState, ButtonStyle};
use crate::ui::theme::{self, Theme};

pub type UiButtonChrome = (ButtonAnimState, ButtonStyle, BackgroundColor, BorderColor);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTone {
    Neutral,
    Accent,
    Destructive,
}

fn tone_color(theme: &Theme, tone: UiTone) -> Color {
    match tone {
        UiTone::Neutral => theme.colors.btn_primary,
        UiTone::Accent => theme.colors.accent,
        UiTone::Destructive => theme.colors.destructive,
    }
}

pub fn filled_button_chrome(theme: &Theme, tone: UiTone) -> UiButtonChrome {
    let bg = tone_color(theme, tone);
    (
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Filled,
        BackgroundColor(bg),
        BorderColor::all(Color::NONE),
    )
}

pub fn accent_button_chrome(theme: &Theme) -> UiButtonChrome {
    let bg = theme.colors.bg_menu.with_alpha(0.75);
    (
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Accent,
        BackgroundColor(bg),
        BorderColor::all(Color::srgba(0.91, 0.76, 0.46, 0.08)),
    )
}

pub fn ghost_button_chrome(_theme: &Theme, tone: UiTone) -> UiButtonChrome {
    (
        ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
        match tone {
            UiTone::Destructive => ButtonStyle::Destructive,
            UiTone::Neutral | UiTone::Accent => ButtonStyle::Ghost,
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    )
}

pub fn button_node(width: f32, height: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(height),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        margin: UiRect::vertical(Val::Px(4.0)),
        // border_radius: BorderRadius::all(Val::Px(8.0)),
        ..default()
    }
}

pub fn compact_button_node(pad_x: f32, pad_y: f32) -> Node {
    Node {
        padding: UiRect::axes(Val::Px(pad_x), Val::Px(pad_y)),
        // border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    }
}

pub fn compact_button_node_with_margin(pad_x: f32, pad_y: f32, margin_x: f32) -> Node {
    let mut node = compact_button_node(pad_x, pad_y);
    node.margin = UiRect::horizontal(Val::Px(margin_x));
    node
}

pub fn icon_button_node(size: f32) -> Node {
    Node {
        width: Val::Px(size),
        height: Val::Px(size),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        // border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    }
}

pub fn input_node(width: f32, height: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(height),
        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
        border: UiRect::all(Val::Px(1.0)),
        // border_radius: BorderRadius::all(Val::Px(6.0)),
        align_items: AlignItems::Center,
        overflow: Overflow::clip(),
        ..default()
    }
}

pub fn input_chrome(theme: &Theme) -> (BackgroundColor, BorderColor, BoxShadow) {
    (
        BackgroundColor(theme.colors.input_bg),
        BorderColor::all(theme.colors.input_border),
        BoxShadow::new(
            theme::OVERLAY.with_alpha(0.16),
            Val::Px(0.0),
            Val::Px(2.0),
            Val::Px(0.0),
            Val::Px(0.0),
        ),
    )
}

pub fn card_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(Val::Px(10.0)),
        margin: UiRect::vertical(Val::Px(4.0)),
        border: UiRect::all(Val::Px(1.0)),
        // border_radius: BorderRadius::all(Val::Px(10.0)),
        row_gap: Val::Px(8.0),
        ..default()
    }
}

pub fn card_chrome(theme: &Theme, border: Color) -> (BackgroundColor, BorderColor, BoxShadow) {
    (
        BackgroundColor(theme.colors.bg_surface),
        BorderColor::all(border),
        BoxShadow::new(
            theme::OVERLAY.with_alpha(0.18),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(0.0),
        ),
    )
}

pub fn badge_node(size: f32, radius: f32) -> Node {
    Node {
        width: Val::Px(size),
        height: Val::Px(size),
        // border_radius: BorderRadius::all(Val::Px(radius)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}
