use bevy::prelude::*;

// ── Panel backgrounds ──
pub const BG_PANEL: Color = Color::srgba(0.07, 0.07, 0.07, 0.94);
pub const BG_SURFACE: Color = Color::srgba(0.12, 0.12, 0.12, 0.94);
pub const BG_ELEVATED: Color = Color::srgba(0.14, 0.14, 0.14, 0.94);
pub const BG_TRANSPARENT: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
pub const BG_MENU: Color = Color::srgb(0.04, 0.04, 0.05);

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

// ── Buttons ──
pub const BTN_PRIMARY: Color = Color::srgba(0.17, 0.17, 0.17, 0.94);
pub const BTN_HOVER: Color = Color::srgba(0.22, 0.22, 0.22, 0.94);
pub const BTN_PRESSED: Color = Color::srgba(0.10, 0.10, 0.10, 0.94);

// ── HP bar backgrounds ──
pub const HP_BAR_BG: Color = Color::srgba(0.08, 0.08, 0.08, 0.9);

// HP thresholds reuse status colors
pub const HP_HIGH: Color = SUCCESS;
pub const HP_MID: Color = WARNING;
pub const HP_LOW: Color = DESTRUCTIVE;

// ── Stat colors ──
pub const STAT_DMG: Color = Color::srgb(0.85, 0.35, 0.30);
pub const STAT_RNG: Color = Color::srgb(0.40, 0.70, 0.95);
pub const STAT_SPD: Color = Color::srgb(0.40, 0.80, 0.45);

// ── Panel accents ──
pub const PANEL_ACCENT_FRIENDLY: Color = Color::srgba(0.29, 0.62, 1.0, 0.6);
pub const PANEL_ACCENT_ENEMY: Color = Color::srgba(0.80, 0.27, 0.27, 0.6);
pub const PANEL_ACCENT_CONSTRUCTION: Color = Color::srgba(1.0, 0.65, 0.15, 0.6);
pub const ICON_FRAME_BG: Color = Color::srgba(0.16, 0.16, 0.18, 0.95);

// ── Input fields ──
pub const INPUT_BG: Color = BG_SURFACE;
pub const INPUT_BORDER: Color = BORDER_SUBTLE;
pub const INPUT_BORDER_FOCUSED: Color = ACCENT;

// ── Font sizes (base design at 720p, scaled globally by UiScale) ──
pub const FONT_DISPLAY: f32 = 48.0; // Menu title
pub const FONT_HEADING: f32 = 28.0; // Section headers
pub const FONT_BUTTON: f32 = 18.0; // Menu buttons, large labels
pub const FONT_LARGE: f32 = 15.0; // Popups, unit names
pub const FONT_MEDIUM: f32 = 13.0; // Selector labels, resource counts
pub const FONT_BODY: f32 = 12.0; // Standard text, tooltips
pub const FONT_SMALL: f32 = 10.0; // Widget titles, costs
pub const FONT_CAPTION: f32 = 10.0; // Toolbar, queue labels
pub const FONT_TINY: f32 = 8.0; // Close buttons, badges
pub const FONT_MICRO: f32 = 8.0; // Event log timestamps

// ── Grid overlay ──
pub const GRID_LINE: Color = Color::srgba(0.29, 0.62, 1.0, 0.15);
