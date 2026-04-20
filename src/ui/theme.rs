//! Theme palette, typography, and spacing scales for both Iron (dark)
//! and Parchment (light) modes. Central source of UI styling constants.

use bevy::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Theme Mode
// ─────────────────────────────────────────────────────────────────────────────

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ThemeMode {
    #[default]
    Dark, // "Iron"
    Light, // "Parchment"
}

// ─────────────────────────────────────────────────────────────────────────────
// Color Palette
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ColorPalette {
    // Backgrounds (The Material Hierarchy)
    pub bg_panel: Color,    // Surface (The Desk)
    pub bg_surface: Color,  // Surface Container (Bolted Plates)
    pub bg_elevated: Color, // Surface Container High (Raised Modules)
    pub bg_recessed: Color, // Surface Container Lowest (Etched Readouts)
    pub bg_transparent: Color,
    pub bg_menu: Color,

    // Text (The Dialogue)
    pub text_primary: Color,   // Authoritative (Noto Serif style)
    pub text_secondary: Color, // Active Intel (Primary Teal)
    pub text_disabled: Color,  // Weathered Metal

    // Accent / Status
    pub accent: Color,      // Primary Teal (#accdcc)
    pub prestige: Color,    // Burnished Gold (#e9c176)
    pub destructive: Color, // Error/Danger (#ffb4ab)
    pub success: Color,     // Desaturated Green
    pub warning: Color,     // Burnished Gold (Semantic)

    // Borders / Separators (The "Etched" look)
    pub separator: Color,
    pub border_subtle: Color,

    // Buttons (Physical Command)
    pub btn_primary: Color,
    pub btn_hover: Color,
    pub btn_pressed: Color,

    // HP bar
    pub hp_bar_bg: Color,

    // Stat colors (Industrial Precision)
    pub stat_dmg: Color,
    pub stat_rng: Color,
    pub stat_spd: Color,

    // Panel accents
    pub panel_accent_friendly: Color,
    pub panel_accent_enemy: Color,
    pub panel_accent_construction: Color,
    pub icon_frame_bg: Color,

    // Input fields
    pub input_bg: Color,
    pub input_border: Color,
    pub input_border_focused: Color,

    // Grid overlay
    pub grid_line: Color,

    // Focus / highlight (selection indicators, nav focus, active states)
    pub highlight: Color,
    pub highlight_subtle: Color,

    // Overlays / scrims
    pub overlay: Color,

    // Tooltip
    pub tooltip_bg: Color,
    pub tooltip_border: Color,

    // Stance colors
    pub stance_passive: Color,
    pub stance_defensive: Color,
    pub stance_aggressive: Color,
}

impl ColorPalette {
    pub fn dark() -> Self {
        Self {
            // Backgrounds: Deep Charcoal "Iron"
            bg_panel: Color::srgb(0.07, 0.08, 0.09), // #121416
            bg_surface: Color::srgb(0.10, 0.11, 0.12), // #1a1d1f
            bg_elevated: Color::srgb(0.14, 0.15, 0.16), // #23272a
            bg_recessed: Color::srgb(0.04, 0.04, 0.05), // #0a0b0c
            bg_transparent: Color::srgba(0.0, 0.0, 0.0, 0.0),
            bg_menu: Color::srgb(0.05, 0.05, 0.06),

            // Text: High contrast vs Teal/Gold
            text_primary: Color::srgb(0.88, 0.91, 0.91), // Authoritative Off-white
            text_secondary: Color::srgb(0.67, 0.80, 0.80), // #accdcc (Teal)
            text_disabled: Color::srgb(0.35, 0.38, 0.40), // Weathered

            // Accent / Status
            accent: Color::srgb(0.67, 0.80, 0.80), // Primary Teal
            prestige: Color::srgb(0.91, 0.76, 0.46), // Burnished Gold
            destructive: Color::srgb(1.0, 0.71, 0.67), // Soft Red Error
            success: Color::srgb(0.55, 0.75, 0.55), // Desaturated Green
            warning: Color::srgb(0.91, 0.76, 0.46), // Gold

            // Borders: Etched into metal
            separator: Color::srgba(0.67, 0.80, 0.80, 0.1),
            border_subtle: Color::srgba(0.91, 0.76, 0.46, 0.15),

            // Buttons: Heavy toggles
            btn_primary: Color::srgba(0.12, 0.13, 0.15, 0.0),
            btn_hover: Color::srgba(0.67, 0.80, 0.80, 0.08), // Teal glow
            btn_pressed: Color::srgba(0.05, 0.05, 0.06, 0.94),

            // HP bar: Segmented feel
            hp_bar_bg: Color::srgba(0.05, 0.05, 0.06, 0.9),

            // Stats: Clinical Work Sans palette
            stat_dmg: Color::srgb(1.0, 0.71, 0.67),
            stat_rng: Color::srgb(0.67, 0.80, 0.80),
            stat_spd: Color::srgb(0.91, 0.76, 0.46),

            // Panel accents: Sharp edge highlights
            panel_accent_friendly: Color::srgba(0.67, 0.80, 0.80, 0.4),
            panel_accent_enemy: Color::srgba(1.0, 0.71, 0.67, 0.4),
            panel_accent_construction: Color::srgba(0.91, 0.76, 0.46, 0.4),
            icon_frame_bg: Color::srgba(0.10, 0.11, 0.12, 0.95),

            // Input: Recessed
            input_bg: Color::srgb(0.04, 0.04, 0.05),
            input_border: Color::srgb(0.14, 0.15, 0.16),
            input_border_focused: Color::srgb(0.91, 0.76, 0.46), // Gold focus

            // Grid: Tactical overlay
            grid_line: Color::srgba(0.67, 0.80, 0.80, 0.08),

            // Focus / highlight
            highlight: Color::srgba(0.67, 0.80, 0.80, 0.4),
            highlight_subtle: Color::srgba(0.67, 0.80, 0.80, 0.15),

            // Overlays
            overlay: Color::srgba(0.0, 0.0, 0.0, 0.6),

            // Tooltip
            tooltip_bg: Color::srgba(0.05, 0.05, 0.07, 0.96),
            tooltip_border: Color::srgba(0.25, 0.25, 0.30, 0.6),

            // Stances
            stance_passive: Color::srgb(0.5, 0.5, 0.8),
            stance_defensive: Color::srgb(0.3, 0.7, 0.3),
            stance_aggressive: Color::srgb(0.9, 0.3, 0.2),
        }
    }

    pub fn light() -> Self {
        Self {
            // Backgrounds: Weathered "Parchment"
            bg_panel: Color::srgb(0.94, 0.92, 0.88),
            bg_surface: Color::srgb(0.98, 0.97, 0.95),
            bg_elevated: Color::srgb(1.0, 1.0, 1.0),
            bg_recessed: Color::srgb(0.88, 0.85, 0.80),
            bg_transparent: Color::srgba(0.0, 0.0, 0.0, 0.0),
            bg_menu: Color::srgb(0.90, 0.88, 0.85),

            // Text: Ink on paper
            text_primary: Color::srgb(0.12, 0.14, 0.16),
            text_secondary: Color::srgb(0.25, 0.45, 0.45), // Darker Teal
            text_disabled: Color::srgb(0.65, 0.65, 0.68),

            // Accent / Status
            accent: Color::srgb(0.25, 0.45, 0.45),
            prestige: Color::srgb(0.75, 0.55, 0.25), // Tarnished Gold
            destructive: Color::srgb(0.80, 0.25, 0.25),
            success: Color::srgb(0.30, 0.55, 0.30),
            warning: Color::srgb(0.75, 0.55, 0.25),

            // Borders
            separator: Color::srgba(0.12, 0.14, 0.16, 0.1),
            border_subtle: Color::srgba(0.75, 0.55, 0.25, 0.2),

            // Buttons
            btn_primary: Color::srgba(0.88, 0.85, 0.80, 0.0),
            btn_hover: Color::srgba(0.25, 0.45, 0.45, 0.05),
            btn_pressed: Color::srgba(0.80, 0.78, 0.75, 0.94),

            // HP bar
            hp_bar_bg: Color::srgba(0.85, 0.82, 0.78, 0.9),

            // Stats
            stat_dmg: Color::srgb(0.80, 0.25, 0.25),
            stat_rng: Color::srgb(0.25, 0.45, 0.45),
            stat_spd: Color::srgb(0.75, 0.55, 0.25),

            // Panel accents
            panel_accent_friendly: Color::srgba(0.25, 0.45, 0.45, 0.3),
            panel_accent_enemy: Color::srgba(0.80, 0.25, 0.25, 0.3),
            panel_accent_construction: Color::srgba(0.75, 0.55, 0.25, 0.3),
            icon_frame_bg: Color::srgba(0.98, 0.97, 0.95, 0.95),

            // Input
            input_bg: Color::srgb(1.0, 1.0, 1.0),
            input_border: Color::srgb(0.85, 0.82, 0.78),
            input_border_focused: Color::srgb(0.25, 0.45, 0.45),

            // Grid
            grid_line: Color::srgba(0.25, 0.45, 0.45, 0.1),

            // Focus / highlight
            highlight: Color::srgba(0.25, 0.45, 0.45, 0.3),
            highlight_subtle: Color::srgba(0.25, 0.45, 0.45, 0.12),

            // Overlays
            overlay: Color::srgba(0.0, 0.0, 0.0, 0.5),

            // Tooltip
            tooltip_bg: Color::srgba(0.92, 0.90, 0.86, 0.96),
            tooltip_border: Color::srgba(0.65, 0.62, 0.58, 0.6),

            // Stances
            stance_passive: Color::srgb(0.4, 0.4, 0.65),
            stance_defensive: Color::srgb(0.25, 0.55, 0.25),
            stance_aggressive: Color::srgb(0.75, 0.25, 0.18),
        }
    }

    pub fn hp_high(&self) -> Color {
        self.accent
    } // Teal for safety
    pub fn hp_mid(&self) -> Color {
        self.prestige
    } // Gold for warning
    pub fn hp_low(&self) -> Color {
        self.destructive
    } // Red for danger
}

// ─────────────────────────────────────────────────────────────────────────────
// Typography (Noto Serif vs Work Sans)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Typography {
    pub display: f32, // 48 — Noto Serif (Mission Titles)
    pub heading: f32, // 28 — Noto Serif (Section Headers)
    pub button: f32,  // 18 — Work Sans (Command Actions)
    pub large: f32,   // 15 — Work Sans (Unit Names)
    pub medium: f32,  // 13 — Work Sans (Resource Labels)
    pub body: f32,    // 12 — Work Sans (Standard Data)
    pub small: f32,   // 10 — Work Sans (Technical Specs)
    pub caption: f32, // 10 — Work Sans (Serial Numbers)
    pub tiny: f32,    //  8 — Badges
    pub micro: f32,   //  8 — Event Log
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            display: 48.0,
            heading: 28.0,
            button: 18.0,
            large: 15.0,
            medium: 13.0,
            body: 12.0,
            small: 10.0,
            caption: 10.0,
            tiny: 8.0,
            micro: 8.0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Theme Resource
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Resource, Clone, Debug)]
pub struct Theme {
    pub mode: ThemeMode,
    pub colors: ColorPalette,
    pub typography: Typography,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_mode(ThemeMode::Dark)
    }
}

impl Theme {
    pub fn from_mode(mode: ThemeMode) -> Self {
        Self {
            mode,
            colors: match mode {
                ThemeMode::Dark => ColorPalette::dark(),
                ThemeMode::Light => ColorPalette::light(),
            },
            typography: Typography::default(),
        }
    }

    pub fn set_mode(&mut self, mode: ThemeMode) {
        self.mode = mode;
        self.colors = match mode {
            ThemeMode::Dark => ColorPalette::dark(),
            ThemeMode::Light => ColorPalette::light(),
        };
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Deprecated const aliases (Updated for Tactical Command Interface)
// ─────────────────────────────────────────────────────────────────────────────

pub const BG_PANEL: Color = Color::srgb(0.07, 0.08, 0.09);
pub const BG_SURFACE: Color = Color::srgb(0.10, 0.11, 0.12);
pub const BG_ELEVATED: Color = Color::srgb(0.14, 0.15, 0.16);
pub const BG_RECESSED: Color = Color::srgb(0.04, 0.04, 0.05);
pub const BG_TRANSPARENT: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);

pub const TEXT_PRIMARY: Color = Color::srgb(0.88, 0.91, 0.91);
pub const TEXT_SECONDARY: Color = Color::srgb(0.67, 0.80, 0.80);
pub const TEXT_DISABLED: Color = Color::srgb(0.35, 0.38, 0.40);

pub const ACCENT: Color = Color::srgb(0.67, 0.80, 0.80);
pub const PRESTIGE: Color = Color::srgb(0.91, 0.76, 0.46);
pub const DESTRUCTIVE: Color = Color::srgb(1.0, 0.71, 0.67);
pub const SUCCESS: Color = Color::srgb(0.55, 0.75, 0.55);
pub const WARNING: Color = PRESTIGE;

pub const SEPARATOR: Color = Color::srgba(0.67, 0.80, 0.80, 0.1);
pub const BORDER_SUBTLE: Color = Color::srgba(0.91, 0.76, 0.46, 0.15);

pub const BTN_PRIMARY: Color = Color::srgba(0.12, 0.13, 0.15, 0.0);
pub const BTN_HOVER: Color = Color::srgba(0.67, 0.80, 0.80, 0.08);
pub const BTN_PRESSED: Color = Color::srgba(0.05, 0.05, 0.06, 0.94);

pub const HP_BAR_BG: Color = Color::srgba(0.05, 0.05, 0.06, 0.9);
pub const HP_HIGH: Color = ACCENT;
pub const HP_MID: Color = PRESTIGE;
pub const HP_LOW: Color = DESTRUCTIVE;

pub const STAT_DMG: Color = DESTRUCTIVE;
pub const STAT_RNG: Color = ACCENT;
pub const STAT_SPD: Color = PRESTIGE;

pub const PANEL_ACCENT_FRIENDLY: Color = Color::srgba(0.67, 0.80, 0.80, 0.4);
pub const PANEL_ACCENT_ENEMY: Color = Color::srgba(1.0, 0.71, 0.67, 0.4);
pub const PANEL_ACCENT_CONSTRUCTION: Color = Color::srgba(0.91, 0.76, 0.46, 0.4);

pub const GRID_LINE: Color = Color::srgba(0.67, 0.80, 0.80, 0.08);

pub const HIGHLIGHT: Color = Color::srgba(0.67, 0.80, 0.80, 0.4);
pub const HIGHLIGHT_SUBTLE: Color = Color::srgba(0.67, 0.80, 0.80, 0.15);

pub const OVERLAY: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);

pub const TOOLTIP_BG: Color = Color::srgba(0.05, 0.05, 0.07, 0.96);
pub const TOOLTIP_BORDER: Color = Color::srgba(0.25, 0.25, 0.30, 0.6);

pub const STANCE_PASSIVE: Color = Color::srgb(0.5, 0.5, 0.8);
pub const STANCE_DEFENSIVE: Color = Color::srgb(0.3, 0.7, 0.3);
pub const STANCE_AGGRESSIVE: Color = Color::srgb(0.9, 0.3, 0.2);

// Team tier colors
pub const TEAM_COLOR_1: Color = Color::srgb(0.9, 0.75, 0.2); // Gold
pub const TEAM_COLOR_2: Color = Color::srgb(0.2, 0.75, 0.85); // Cyan
pub const TEAM_COLOR_3: Color = Color::srgb(0.85, 0.3, 0.65); // Pink
pub const TEAM_COLOR_4: Color = Color::srgb(0.95, 0.5, 0.15); // Orange
pub const TEAM_COLORS: [Color; 4] = [TEAM_COLOR_1, TEAM_COLOR_2, TEAM_COLOR_3, TEAM_COLOR_4];

// Event log level colors
pub const LOG_INFO: Color = Color::srgb(0.4, 0.75, 1.0);
pub const LOG_RESOURCE: Color = Color::srgb(0.4, 0.75, 1.0);
pub const LOG_UPGRADE: Color = Color::srgb(0.9, 0.7, 0.2);
pub const LOG_DEMOLISH: Color = Color::srgb(0.8, 0.3, 0.3);
pub const LOG_NETWORK: Color = Color::srgb(0.2, 0.8, 0.6);
pub const LOG_SYNC: Color = Color::srgb(0.6, 0.5, 0.9);

// Group hotkey slot colors
pub const GROUP_COLORS: [Color; 9] = [
    Color::srgb(0.29, 0.62, 1.00), // #1 Blue
    Color::srgb(0.30, 0.80, 0.40), // #2 Green
    Color::srgb(1.00, 0.65, 0.15), // #3 Orange
    Color::srgb(0.80, 0.35, 0.85), // #4 Purple
    Color::srgb(0.90, 0.35, 0.35), // #5 Red
    Color::srgb(0.20, 0.80, 0.80), // #6 Cyan
    Color::srgb(0.95, 0.85, 0.30), // #7 Yellow
    Color::srgb(0.60, 0.45, 0.30), // #8 Brown
    Color::srgb(0.70, 0.70, 0.75), // #9 Silver
];

// Font sizes
pub const FONT_DISPLAY: f32 = 48.0;
pub const FONT_HEADING: f32 = 28.0;
pub const FONT_BUTTON: f32 = 18.0;
pub const FONT_LARGE: f32 = 15.0;
pub const FONT_MEDIUM: f32 = 13.0;
pub const FONT_BODY: f32 = 12.0;
pub const FONT_SMALL: f32 = 10.0;
pub const FONT_CAPTION: f32 = 10.0;
pub const FONT_TINY: f32 = 8.0;
pub const FONT_MICRO: f32 = 8.0;
