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
pub const BORDER_ENEMY: Color = Color::srgb(0.8, 0.3, 0.1);

// ── Buttons ──
pub const BTN_PRIMARY: Color = Color::srgba(0.17, 0.17, 0.17, 0.94);
pub const BTN_HOVER: Color = Color::srgba(0.22, 0.22, 0.22, 0.94);
pub const BTN_PRESSED: Color = Color::srgba(0.10, 0.10, 0.10, 0.94);

// ── Cards ──
pub const CARD_BG: Color = Color::srgba(0.12, 0.13, 0.16, 0.94);
pub const CARD_DISABLED: Color = Color::srgba(0.12, 0.12, 0.12, 0.5);
pub const CARD_BG_TOP: Color = Color::srgba(0.18, 0.19, 0.23, 0.96);
pub const CARD_BG_BOTTOM: Color = Color::srgba(0.08, 0.08, 0.11, 0.96);
pub const CARD_BORDER: Color = Color::srgba(0.25, 0.25, 0.30, 0.8);
pub const CARD_BORDER_HOVER: Color = Color::srgba(0.45, 0.50, 0.60, 0.9);
pub const CARD_SHINE: Color = Color::srgba(1.0, 1.0, 1.0, 0.08);

// ── Tier accent colors ──
pub const TIER_COMMON: Color = Color::srgb(0.55, 0.58, 0.63);
pub const TIER_UNCOMMON: Color = Color::srgb(0.30, 0.75, 0.35);
pub const TIER_RARE: Color = Color::srgb(0.30, 0.55, 1.0);
pub const TIER_EPIC: Color = Color::srgb(0.65, 0.30, 0.90);

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

// ── Button animation colors ──
pub const BTN_HOVER_BRIGHT: Color = Color::srgba(0.25, 0.25, 0.25, 0.94);
pub const BTN_GHOST_HOVER_BG: Color = Color::srgba(0.29, 0.62, 1.0, 0.08);
pub const BTN_GHOST_PRESS_BG: Color = Color::srgba(0.29, 0.62, 1.0, 0.14);

// ── Input fields ──
pub const INPUT_BG: Color = BG_SURFACE;
pub const INPUT_BORDER: Color = BORDER_SUBTLE;
pub const INPUT_BORDER_FOCUSED: Color = ACCENT;

// ── Font sizes (base design at 720p, scaled globally by UiScale) ──
pub const FONT_DISPLAY: f32 = 48.0;  // Menu title
pub const FONT_HEADING: f32 = 28.0;  // Section headers
pub const FONT_BUTTON: f32 = 18.0;   // Menu buttons, large labels
pub const FONT_LARGE: f32 = 15.0;    // Popups, unit names
pub const FONT_MEDIUM: f32 = 13.0;   // Selector labels, resource counts
pub const FONT_BODY: f32 = 12.0;     // Standard text, tooltips
pub const FONT_SMALL: f32 = 10.0;    // Widget titles, costs
pub const FONT_CAPTION: f32 = 9.0;   // Toolbar, queue labels
pub const FONT_TINY: f32 = 8.0;      // Close buttons, badges
pub const FONT_MICRO: f32 = 7.0;     // Event log timestamps

// ── Grid overlay ──
pub const GRID_LINE: Color = Color::srgba(0.29, 0.62, 1.0, 0.15);
#[allow(dead_code)]
pub const DEMOLISH_HOVER: Color = Color::srgb(0.95, 0.35, 0.35);
