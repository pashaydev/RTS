use bevy::prelude::*;

// ── Panel backgrounds ──
pub const BG_PANEL: Color = Color::srgba(0.07, 0.07, 0.07, 0.94);
pub const BG_SURFACE: Color = Color::srgba(0.12, 0.12, 0.12, 0.94);
pub const BG_ELEVATED: Color = Color::srgba(0.14, 0.14, 0.14, 0.94);

// ── Text colors ──
pub const TEXT_PRIMARY: Color = Color::srgb(0.88, 0.88, 0.88);
pub const TEXT_SECONDARY: Color = Color::srgb(0.53, 0.53, 0.53);
pub const TEXT_DISABLED: Color = Color::srgb(0.33, 0.33, 0.33);

// ── Accent / status ──
pub const ACCENT: Color = Color::srgb(0.29, 0.62, 1.0);
pub const DESTRUCTIVE: Color = Color::srgb(0.80, 0.27, 0.27);
pub const SUCCESS: Color = Color::srgb(0.30, 0.69, 0.31);
pub const WARNING: Color = Color::srgb(1.0, 0.65, 0.15);

// ── Borders / separators ──
pub const SEPARATOR: Color = Color::srgb(0.20, 0.20, 0.20);
pub const BORDER_SUBTLE: Color = Color::srgb(0.33, 0.33, 0.33);
pub const BORDER_ENEMY: Color = Color::srgb(0.8, 0.3, 0.1);

// ── Buttons ──
pub const BTN_PRIMARY: Color = Color::srgba(0.17, 0.17, 0.17, 0.94);
pub const BTN_HOVER: Color = Color::srgba(0.22, 0.22, 0.22, 0.94);
pub const BTN_PRESSED: Color = Color::srgba(0.10, 0.10, 0.10, 0.94);

// ── Cards ──
pub const CARD_BG: Color = Color::srgba(0.12, 0.13, 0.16, 0.94);
pub const CARD_DISABLED: Color = Color::srgba(0.12, 0.12, 0.12, 0.5);

// ── HP bar backgrounds ──
pub const HP_BAR_BG: Color = Color::srgba(0.08, 0.08, 0.08, 0.9);

// HP thresholds reuse status colors
pub const HP_HIGH: Color = SUCCESS;
pub const HP_MID: Color = WARNING;
pub const HP_LOW: Color = DESTRUCTIVE;
