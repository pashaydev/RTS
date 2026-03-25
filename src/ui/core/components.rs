use bevy::prelude::*;

use crate::components::{ButtonAnimState, ButtonStyle};
use crate::theme;

pub type UiButtonChrome = (
    ButtonAnimState,
    ButtonStyle,
    BackgroundColor,
    BorderColor,
    BoxShadow,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTone {
    Neutral,
    Accent,
    Destructive,
}

fn tone_color(tone: UiTone) -> Color {
    match tone {
        UiTone::Neutral => theme::BTN_PRIMARY,
        UiTone::Accent => theme::ACCENT,
        UiTone::Destructive => theme::DESTRUCTIVE,
    }
}

fn tone_border(tone: UiTone) -> Color {
    match tone {
        UiTone::Neutral => Color::srgba(0.35, 0.48, 0.65, 0.16),
        UiTone::Accent => Color::srgba(0.58, 0.80, 1.0, 0.22),
        UiTone::Destructive => Color::srgba(1.0, 0.58, 0.58, 0.22),
    }
}

fn tone_shadow(tone: UiTone, alpha: f32) -> Color {
    match tone {
        UiTone::Neutral => Color::srgba(0.0, 0.0, 0.0, alpha),
        UiTone::Accent => Color::srgba(0.29, 0.62, 1.0, alpha),
        UiTone::Destructive => Color::srgba(0.55, 0.18, 0.18, alpha),
    }
}

pub fn filled_button_chrome(tone: UiTone) -> UiButtonChrome {
    let bg = tone_color(tone);
    (
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Filled,
        BackgroundColor(bg),
        BorderColor::all(tone_border(tone)),
        BoxShadow::new(
            tone_shadow(tone, if tone == UiTone::Neutral { 0.18 } else { 0.22 }),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(0.0),
        ),
    )
}

pub fn ghost_button_chrome(tone: UiTone) -> UiButtonChrome {
    (
        ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
        match tone {
            UiTone::Destructive => ButtonStyle::Destructive,
            UiTone::Neutral | UiTone::Accent => ButtonStyle::Ghost,
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(match tone {
            UiTone::Neutral => Color::srgba(0.30, 0.52, 0.82, 0.10),
            UiTone::Accent => Color::srgba(0.35, 0.68, 1.0, 0.14),
            UiTone::Destructive => Color::srgba(0.88, 0.40, 0.40, 0.14),
        }),
        BoxShadow::new(
            tone_shadow(UiTone::Neutral, 0.16),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(0.0),
        ),
    )
}

pub fn button_node(width: f32, height: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(height),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        margin: UiRect::vertical(Val::Px(4.0)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(8.0)),
        ..default()
    }
}

pub fn compact_button_node(pad_x: f32, pad_y: f32) -> Node {
    Node {
        padding: UiRect::axes(Val::Px(pad_x), Val::Px(pad_y)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
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
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    }
}

pub fn input_node(width: f32, height: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(height),
        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        align_items: AlignItems::Center,
        overflow: Overflow::clip(),
        ..default()
    }
}

pub fn input_chrome() -> (BackgroundColor, BorderColor, BoxShadow) {
    (
        BackgroundColor(theme::INPUT_BG),
        BorderColor::all(theme::INPUT_BORDER),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.16),
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
        border_radius: BorderRadius::all(Val::Px(10.0)),
        row_gap: Val::Px(8.0),
        ..default()
    }
}

pub fn card_chrome(border: Color) -> (BackgroundColor, BorderColor, BoxShadow) {
    (
        BackgroundColor(theme::BG_SURFACE),
        BorderColor::all(border),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.18),
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
        border_radius: BorderRadius::all(Val::Px(radius)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}
