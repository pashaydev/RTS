use bevy::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Theme Mode
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

// ─────────────────────────────────────────────────────────────────────────────
// Color Palette
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ColorPalette {
    // Backgrounds
    pub bg_panel: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_transparent: Color,
    pub bg_menu: Color,

    // Text
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,

    // Accent / Status
    pub accent: Color,
    pub destructive: Color,
    pub success: Color,
    pub warning: Color,

    // Borders / separators
    pub separator: Color,
    pub border_subtle: Color,

    // Buttons
    pub btn_primary: Color,
    pub btn_hover: Color,
    pub btn_pressed: Color,

    // HP bar
    pub hp_bar_bg: Color,

    // Stat colors
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
}

impl ColorPalette {
    pub fn dark() -> Self {
        Self {
            // Backgrounds
            bg_panel: Color::srgba(0.07, 0.07, 0.07, 0.94),
            bg_surface: Color::srgba(0.12, 0.12, 0.12, 0.94),
            bg_elevated: Color::srgba(0.14, 0.14, 0.14, 0.94),
            bg_transparent: Color::srgba(0.0, 0.0, 0.0, 0.0),
            bg_menu: Color::srgb(0.04, 0.04, 0.05),

            // Text
            text_primary: Color::srgb(0.88, 0.88, 0.88),
            text_secondary: Color::srgb(0.53, 0.53, 0.53),
            text_disabled: Color::srgb(0.33, 0.33, 0.33),

            // Accent / Status
            accent: Color::srgb(0.29, 0.62, 1.0),
            destructive: Color::srgb(0.80, 0.27, 0.27),
            success: Color::srgb(0.30, 0.69, 0.31),
            warning: Color::srgb(1.0, 0.65, 0.15),

            // Borders
            separator: Color::srgb(0.20, 0.20, 0.20),
            border_subtle: Color::srgb(0.33, 0.33, 0.33),

            // Buttons
            btn_primary: Color::srgba(0.15, 0.15, 0.15, 0.0),
            btn_hover: Color::srgba(0.22, 0.22, 0.22, 0.94),
            btn_pressed: Color::srgba(0.10, 0.10, 0.10, 0.94),

            // HP bar
            hp_bar_bg: Color::srgba(0.08, 0.08, 0.08, 0.9),

            // Stats
            stat_dmg: Color::srgb(0.85, 0.35, 0.30),
            stat_rng: Color::srgb(0.40, 0.70, 0.95),
            stat_spd: Color::srgb(0.40, 0.80, 0.45),

            // Panel accents
            panel_accent_friendly: Color::srgba(0.29, 0.62, 1.0, 0.6),
            panel_accent_enemy: Color::srgba(0.80, 0.27, 0.27, 0.6),
            panel_accent_construction: Color::srgba(1.0, 0.65, 0.15, 0.6),
            icon_frame_bg: Color::srgba(0.16, 0.16, 0.18, 0.95),

            // Input
            input_bg: Color::srgba(0.12, 0.12, 0.12, 0.94),
            input_border: Color::srgb(0.33, 0.33, 0.33),
            input_border_focused: Color::srgb(0.29, 0.62, 1.0),

            // Grid
            grid_line: Color::srgba(0.29, 0.62, 1.0, 0.15),
        }
    }

    pub fn light() -> Self {
        Self {
            // Backgrounds
            bg_panel: Color::srgba(0.96, 0.96, 0.97, 0.94),
            bg_surface: Color::srgba(1.0, 1.0, 1.0, 0.94),
            bg_elevated: Color::srgba(0.98, 0.98, 0.99, 0.94),
            bg_transparent: Color::srgba(0.0, 0.0, 0.0, 0.0),
            bg_menu: Color::srgb(0.93, 0.93, 0.95),

            // Text
            text_primary: Color::srgb(0.13, 0.13, 0.15),
            text_secondary: Color::srgb(0.45, 0.45, 0.50),
            text_disabled: Color::srgb(0.70, 0.70, 0.72),

            // Accent / Status
            accent: Color::srgb(0.22, 0.55, 0.95),
            destructive: Color::srgb(0.80, 0.27, 0.27),
            success: Color::srgb(0.30, 0.69, 0.31),
            warning: Color::srgb(1.0, 0.65, 0.15),

            // Borders
            separator: Color::srgb(0.82, 0.82, 0.85),
            border_subtle: Color::srgb(0.75, 0.75, 0.78),

            // Buttons
            btn_primary: Color::srgba(0.90, 0.90, 0.92, 0.0),
            btn_hover: Color::srgba(0.85, 0.85, 0.88, 0.94),
            btn_pressed: Color::srgba(0.80, 0.80, 0.83, 0.94),

            // HP bar
            hp_bar_bg: Color::srgba(0.90, 0.90, 0.90, 0.9),

            // Stats (same in both themes — they're semantic colors)
            stat_dmg: Color::srgb(0.85, 0.35, 0.30),
            stat_rng: Color::srgb(0.40, 0.70, 0.95),
            stat_spd: Color::srgb(0.40, 0.80, 0.45),

            // Panel accents
            panel_accent_friendly: Color::srgba(0.22, 0.55, 0.95, 0.6),
            panel_accent_enemy: Color::srgba(0.80, 0.27, 0.27, 0.6),
            panel_accent_construction: Color::srgba(1.0, 0.65, 0.15, 0.6),
            icon_frame_bg: Color::srgba(0.92, 0.92, 0.94, 0.95),

            // Input
            input_bg: Color::srgba(1.0, 1.0, 1.0, 0.94),
            input_border: Color::srgb(0.75, 0.75, 0.78),
            input_border_focused: Color::srgb(0.22, 0.55, 0.95),

            // Grid
            grid_line: Color::srgba(0.22, 0.55, 0.95, 0.15),
        }
    }

    /// HP threshold color (same in both themes).
    pub fn hp_high(&self) -> Color {
        self.success
    }
    pub fn hp_mid(&self) -> Color {
        self.warning
    }
    pub fn hp_low(&self) -> Color {
        self.destructive
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Typography
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Typography {
    pub display: f32,  // 48 — menu title
    pub heading: f32,  // 28 — section headers
    pub button: f32,   // 18 — menu buttons, large labels
    pub large: f32,    // 15 — popups, unit names
    pub medium: f32,   // 13 — selector labels, resource counts
    pub body: f32,     // 12 — standard text, tooltips
    pub small: f32,    // 10 — widget titles, costs
    pub caption: f32,  // 10 — toolbar, queue labels
    pub tiny: f32,     //  8 — close buttons, badges
    pub micro: f32,    //  8 — event log timestamps
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
        Self {
            mode: ThemeMode::Dark,
            colors: ColorPalette::dark(),
            typography: Typography::default(),
        }
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
// Deprecated const aliases — kept for incremental migration.
// Prefer `Res<Theme>` access in new code.
// ─────────────────────────────────────────────────────────────────────────────

// Panel backgrounds
pub const BG_PANEL: Color = Color::srgba(0.07, 0.07, 0.07, 0.94);
pub const BG_SURFACE: Color = Color::srgba(0.12, 0.12, 0.12, 0.94);
pub const BG_ELEVATED: Color = Color::srgba(0.14, 0.14, 0.14, 0.94);
pub const BG_TRANSPARENT: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
pub const BG_MENU: Color = Color::srgb(0.04, 0.04, 0.05);

// Text colors
pub const TEXT_PRIMARY: Color = Color::srgb(0.88, 0.88, 0.88);
pub const TEXT_SECONDARY: Color = Color::srgb(0.53, 0.53, 0.53);
pub const TEXT_DISABLED: Color = Color::srgb(0.33, 0.33, 0.33);

// Accent / status
pub const ACCENT: Color = Color::srgb(0.29, 0.62, 1.0);
pub const DESTRUCTIVE: Color = Color::srgb(0.80, 0.27, 0.27);
pub const SUCCESS: Color = Color::srgb(0.30, 0.69, 0.31);
pub const WARNING: Color = Color::srgb(1.0, 0.65, 0.15);

// Borders / separators
pub const SEPARATOR: Color = Color::srgb(0.20, 0.20, 0.20);
pub const BORDER_SUBTLE: Color = Color::srgb(0.33, 0.33, 0.33);

// Buttons
pub const BTN_PRIMARY: Color = Color::srgba(0.15, 0.15, 0.15, 0.0);
pub const BTN_HOVER: Color = Color::srgba(0.22, 0.22, 0.22, 0.94);
pub const BTN_PRESSED: Color = Color::srgba(0.10, 0.10, 0.10, 0.94);

// HP bar backgrounds
pub const HP_BAR_BG: Color = Color::srgba(0.08, 0.08, 0.08, 0.9);
pub const HP_HIGH: Color = SUCCESS;
pub const HP_MID: Color = WARNING;
pub const HP_LOW: Color = DESTRUCTIVE;

// Stat colors
pub const STAT_DMG: Color = Color::srgb(0.85, 0.35, 0.30);
pub const STAT_RNG: Color = Color::srgb(0.40, 0.70, 0.95);
pub const STAT_SPD: Color = Color::srgb(0.40, 0.80, 0.45);

// Panel accents
pub const PANEL_ACCENT_FRIENDLY: Color = Color::srgba(0.29, 0.62, 1.0, 0.6);
pub const PANEL_ACCENT_ENEMY: Color = Color::srgba(0.80, 0.27, 0.27, 0.6);
pub const PANEL_ACCENT_CONSTRUCTION: Color = Color::srgba(1.0, 0.65, 0.15, 0.6);
pub const ICON_FRAME_BG: Color = Color::srgba(0.16, 0.16, 0.18, 0.95);

// Input fields
pub const INPUT_BG: Color = BG_SURFACE;
pub const INPUT_BORDER: Color = BORDER_SUBTLE;
pub const INPUT_BORDER_FOCUSED: Color = ACCENT;

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

// Grid overlay
pub const GRID_LINE: Color = Color::srgba(0.29, 0.62, 1.0, 0.15);
