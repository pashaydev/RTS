//! Core application types: state, game config, factions, graphics settings.

use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ── Random Name Pool ──

pub const COMMANDER_NAMES: &[&str] = &[
    "Commander",
    "General",
    "Warlord",
    "Captain",
    "Marshal",
    "Overlord",
    "Strategist",
    "Vanguard",
    "Centurion",
    "Paladin",
    "Sentinel",
    "Arbiter",
    "Conqueror",
    "Vindicator",
    "Sovereign",
    "Crusader",
    "Phantom",
    "Templar",
    "Warmaster",
    "Executor",
    "Pathfinder",
    "Nomad",
    "Ironclad",
    "Stormcaller",
];

pub fn random_commander_name() -> String {
    let mut rng = rand::rng();
    COMMANDER_NAMES[rng.random_range(0..COMMANDER_NAMES.len())].to_string()
}

// ── Map Seed ──

#[derive(Resource, Debug, Clone, Copy)]
pub struct MapSeed(pub u64);

// ── App State ──

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    MainMenu,
    InGame,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum OverlayLifecycleSet {
    Manage,
    Adopt,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameFlowSet {
    Input,
    NetworkReceive,
    Simulation,
    NetworkBroadcast,
    Ui,
    Presentation,
    Diagnostics,
}

/// Sub-stages nested under [`GameFlowSet::Simulation`]. Ordering inside a
/// single tick runs top-to-bottom: Ai decides → Command validates → Movement
/// integrates → Combat resolves → Economy ticks → Spatial refreshes indices.
/// Systems may freely remain in `GameFlowSet::Simulation` without picking a
/// sub-set; opt in when intra-tick ordering matters.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet {
    Ai,
    Command,
    Movement,
    Combat,
    Economy,
    Spatial,
}

/// Deterministic tick counter for the fixed-step simulation.
///
/// Incremented once per [`bevy::prelude::FixedUpdate`] tick. Sim systems that
/// need a stable time source (replay/rollback, checksums, tick-indexed
/// buffers) should read this instead of [`bevy::time::Time::elapsed`].
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SimClock {
    /// Number of fixed ticks executed since the game entered `InGame`.
    pub tick: u64,
}

// ── Faction ──

#[derive(
    Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Serialize, Deserialize,
)]
pub enum Faction {
    Player1,
    Player2,
    Player3,
    Player4,
    Neutral,
}

impl Faction {
    pub const PLAYERS: [Faction; 4] = [
        Faction::Player1,
        Faction::Player2,
        Faction::Player3,
        Faction::Player4,
    ];

    /// Convert to numeric index for network serialization.
    /// Player1=0, Player2=1, Player3=2, Player4=3, Neutral=4.
    pub fn to_net_index(&self) -> u8 {
        match self {
            Faction::Player1 => 0,
            Faction::Player2 => 1,
            Faction::Player3 => 2,
            Faction::Player4 => 3,
            Faction::Neutral => 4,
        }
    }

    /// Convert from numeric index back to Faction. Returns None if out of range.
    pub fn from_net_index(idx: u8) -> Option<Faction> {
        match idx {
            0 => Some(Faction::Player1),
            1 => Some(Faction::Player2),
            2 => Some(Faction::Player3),
            3 => Some(Faction::Player4),
            4 => Some(Faction::Neutral),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Faction::Player1 => "Player 1",
            Faction::Player2 => "Player 2",
            Faction::Player3 => "Player 3",
            Faction::Player4 => "Player 4",
            Faction::Neutral => "Neutral",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Faction::Player1 => Color::srgb(0.2, 0.6, 1.0), // Blue
            Faction::Player2 => Color::srgb(1.0, 0.3, 0.2), // Red
            Faction::Player3 => Color::srgb(0.7, 0.3, 0.9), // Purple
            Faction::Player4 => Color::srgb(0.2, 0.8, 0.3), // Green
            Faction::Neutral => Color::srgb(0.8, 0.2, 0.2),
        }
    }

    pub fn team_color(&self) -> TeamColor {
        match self {
            Faction::Player1 => TeamColor::Blue,
            Faction::Player2 => TeamColor::Red,
            Faction::Player3 => TeamColor::Purple,
            Faction::Player4 => TeamColor::Green,
            Faction::Neutral => TeamColor::Black,
        }
    }
}

/// Team configuration — factions with the same team number are allied.
#[derive(Resource)]
pub struct TeamConfig {
    pub teams: HashMap<Faction, u8>,
}

impl Default for TeamConfig {
    fn default() -> Self {
        // Default: FFA — each faction on its own team
        let mut teams = HashMap::new();
        teams.insert(Faction::Player1, 0);
        teams.insert(Faction::Player2, 1);
        teams.insert(Faction::Player3, 2);
        teams.insert(Faction::Player4, 3);
        Self { teams }
    }
}

impl TeamConfig {
    /// Two factions are allied if they share the same team number.
    pub fn is_allied(&self, a: &Faction, b: &Faction) -> bool {
        if a == b {
            return true;
        }
        if *a == Faction::Neutral || *b == Faction::Neutral {
            return false;
        }
        match (self.teams.get(a), self.teams.get(b)) {
            (Some(ta), Some(tb)) => ta == tb,
            _ => false,
        }
    }

    /// Two factions are hostile if they are NOT allied.
    pub fn is_hostile(&self, a: &Faction, b: &Faction) -> bool {
        !self.is_allied(a, b)
    }
}

/// Team color for texture-based faction coloring (ToonyTinyPeople assets).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TeamColor {
    Blue,
    Red,
    Purple,
    Green,
    Black,
}

/// Marker: team color texture has been applied to this entity's scene meshes.
#[derive(Component)]
pub struct TeamColorApplied;

/// Runtime mapping: faction → visual TeamColor.
#[derive(Resource)]
pub struct FactionColors {
    pub colors: HashMap<Faction, TeamColor>,
}

impl Default for FactionColors {
    fn default() -> Self {
        let mut colors = HashMap::new();
        colors.insert(Faction::Player1, TeamColor::Blue);
        colors.insert(Faction::Player2, TeamColor::Red);
        colors.insert(Faction::Player3, TeamColor::Purple);
        colors.insert(Faction::Player4, TeamColor::Green);
        colors.insert(Faction::Neutral, TeamColor::Black);
        Self { colors }
    }
}

impl FactionColors {
    /// Look up the visual color for a faction. Falls back to the hardcoded default.
    pub fn get(&self, faction: &Faction) -> TeamColor {
        self.colors
            .get(faction)
            .copied()
            .unwrap_or(faction.team_color())
    }

    /// Convert a color_index (from network messages) to a TeamColor.
    pub fn from_index(index: u8) -> TeamColor {
        match index {
            0 => TeamColor::Blue,
            1 => TeamColor::Red,
            2 => TeamColor::Purple,
            3 => TeamColor::Green,
            _ => TeamColor::Black,
        }
    }
}

/// Which factions are AI-controlled (not human players).
/// `BTreeSet` gives deterministic iteration order — lockstep AI systems
/// iterate this to drive per-faction ticks, so order must be identical on
/// every peer.
#[derive(Resource)]
pub struct AiControlledFactions {
    pub factions: std::collections::BTreeSet<Faction>,
}

impl Default for AiControlledFactions {
    fn default() -> Self {
        let mut factions = std::collections::BTreeSet::new();
        factions.insert(Faction::Player2);
        factions.insert(Faction::Player3);
        factions.insert(Faction::Player4);
        Self { factions }
    }
}

/// AI difficulty level — affects tick speed, resource bonuses, and thresholds.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiDifficulty {
    Easy,
    #[default]
    Medium,
    Hard,
}

impl AiDifficulty {
    pub fn tick_multiplier(&self) -> f32 {
        match self {
            Self::Easy => 1.5,
            Self::Medium => 1.0,
            Self::Hard => 0.75,
        }
    }

    pub fn resource_bonus(&self) -> f32 {
        match self {
            Self::Easy => 0.0,
            Self::Medium => 0.0,
            Self::Hard => 0.15,
        }
    }

    pub fn max_concurrent_builds(&self) -> usize {
        match self {
            Self::Easy => 2,
            Self::Medium => 3,
            Self::Hard => 4,
        }
    }

    pub fn worker_offset(&self) -> i32 {
        match self {
            Self::Easy => -2,
            Self::Medium => 0,
            Self::Hard => 2,
        }
    }
}

/// Which faction the human player is currently controlling.
#[derive(Resource)]
pub struct ActivePlayer(pub Faction);

impl Default for ActivePlayer {
    fn default() -> Self {
        Self(Faction::Player1)
    }
}

// ── Game Setup Config ──

/// What occupies a faction slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotOccupant {
    /// A human player (local or remote).
    Human,
    /// An AI player with the given difficulty.
    Ai(AiDifficulty),
    /// Slot is not in use.
    Closed,
    /// Multiplayer-only: waiting for a client to connect.
    Open,
}

#[derive(Resource, Clone, Debug)]
pub struct GameSetupConfig {
    pub player_name: String,
    /// Display names for AI slots (randomized on creation).
    pub ai_names: [String; 4],
    /// What occupies each faction slot (indexed 0=Player1, 1=Player2, 2=Player3, 3=Player4).
    pub slots: [SlotOccupant; 4],
    /// Which slot is the local human player (for single-player or multiplayer host).
    pub local_player_slot: usize,
    pub team_mode: TeamMode,
    pub player_teams: [u8; 4],
    pub map_size: MapSize,
    pub resource_density: ResourceDensity,
    pub day_cycle_secs: f32,
    pub night_spawn_intensity: NightSpawnIntensity,
    pub starting_resources_mult: f32,
    pub map_seed: u64, // 0 = random
}

impl Default for GameSetupConfig {
    fn default() -> Self {
        Self {
            player_name: random_commander_name(),
            ai_names: std::array::from_fn(|_| format!("AI {}", random_commander_name())),
            slots: [
                SlotOccupant::Human,
                SlotOccupant::Ai(AiDifficulty::Medium),
                SlotOccupant::Ai(AiDifficulty::Medium),
                SlotOccupant::Ai(AiDifficulty::Medium),
            ],
            local_player_slot: 0,
            team_mode: TeamMode::default(),
            player_teams: [0, 1, 2, 3],
            map_size: MapSize::default(),
            resource_density: ResourceDensity::default(),
            day_cycle_secs: DayNightPreset::Standard.day_cycle_secs(),
            night_spawn_intensity: NightSpawnIntensity::default(),
            starting_resources_mult: 1.0,
            map_seed: 0,
        }
    }
}

#[allow(dead_code)]
impl GameSetupConfig {
    /// Faction indices occupied by human players.
    pub fn human_faction_indices(&self) -> Vec<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s, SlotOccupant::Human))
            .map(|(i, _)| i)
            .collect()
    }

    /// Number of human players.
    pub fn human_count(&self) -> u8 {
        self.slots
            .iter()
            .filter(|s| matches!(s, SlotOccupant::Human))
            .count() as u8
    }

    /// Number of AI players.
    pub fn ai_count(&self) -> u8 {
        self.slots
            .iter()
            .filter(|s| matches!(s, SlotOccupant::Ai(_)))
            .count() as u8
    }

    /// Active faction indices (Human or AI, in order).
    pub fn active_factions(&self) -> Vec<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s, SlotOccupant::Human | SlotOccupant::Ai(_)))
            .map(|(i, _)| i)
            .collect()
    }

    /// AI faction indices with their difficulties.
    pub fn ai_factions_with_difficulty(&self) -> Vec<(usize, AiDifficulty)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                SlotOccupant::Ai(d) => Some((i, *d)),
                _ => None,
            })
            .collect()
    }

    /// Which day/night preset, if any, matches the current `day_cycle_secs`.
    /// Changing `night_spawn_intensity` on its own does not flip the preset —
    /// only the slider does (matching the task's stated behavior).
    pub fn day_night_preset(&self) -> DayNightPreset {
        for preset in [
            DayNightPreset::Short,
            DayNightPreset::Standard,
            DayNightPreset::Long,
        ] {
            if (preset.day_cycle_secs() - self.day_cycle_secs).abs() < 0.5 {
                return preset;
            }
        }
        DayNightPreset::Custom
    }

    pub fn spawn_positions(&self, seed: u64) -> Vec<(Faction, (f32, f32))> {
        let active = self.active_factions();
        let total = active.len();
        if total == 0 {
            return Vec::new();
        }
        let half_map = self.map_size.world_size() / 2.0;
        let radius = 0.6 * half_map;
        let rotation_offset = (seed % 360) as f32 * std::f32::consts::PI / 180.0;

        active
            .iter()
            .enumerate()
            .map(|(i, &faction_idx)| {
                let angle = 2.0 * std::f32::consts::PI * i as f32 / total as f32 + rotation_offset;
                let x = angle.cos() * radius;
                let z = angle.sin() * radius;
                (Faction::PLAYERS[faction_idx], (x, z))
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TeamMode {
    #[default]
    FFA,
    Teams,
    Custom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapSize {
    Small,
    #[default]
    Medium,
    Large,
    ExtraLarge,
}

impl MapSize {
    pub fn world_size(&self) -> f32 {
        match self {
            MapSize::Small => 500.0,
            MapSize::Medium => 700.0,
            MapSize::Large => 900.0,
            MapSize::ExtraLarge => 1200.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResourceDensity {
    Sparse,
    #[default]
    Normal,
    Dense,
}

impl ResourceDensity {
    pub fn multiplier(&self) -> f32 {
        match self {
            ResourceDensity::Sparse => 0.5,
            ResourceDensity::Normal => 1.0,
            ResourceDensity::Dense => 1.5,
        }
    }
}

// ── Day / Night Cadence ──

pub const DAY_CYCLE_SECS_MIN: f32 = 60.0;
pub const DAY_CYCLE_SECS_MAX: f32 = 600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayNightPreset {
    Short,
    Standard,
    Long,
    Custom,
}

impl DayNightPreset {
    /// Day-cycle duration (in seconds) that this preset represents.
    /// `Custom` returns the `Standard` value — callers should not rely on it.
    pub fn day_cycle_secs(&self) -> f32 {
        match self {
            Self::Short => 120.0,
            Self::Standard => 240.0,
            Self::Long => 480.0,
            Self::Custom => 240.0,
        }
    }

    pub fn night_intensity(&self) -> NightSpawnIntensity {
        match self {
            Self::Short => NightSpawnIntensity::Relentless,
            Self::Standard => NightSpawnIntensity::Standard,
            Self::Long => NightSpawnIntensity::Calm,
            Self::Custom => NightSpawnIntensity::Standard,
        }
    }

    pub fn index(&self) -> Option<usize> {
        match self {
            Self::Short => Some(0),
            Self::Standard => Some(1),
            Self::Long => Some(2),
            Self::Custom => None,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Short,
            1 => Self::Standard,
            2 => Self::Long,
            _ => Self::Custom,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NightSpawnIntensity {
    Calm,
    #[default]
    Standard,
    Relentless,
}

impl NightSpawnIntensity {
    /// Multiplier applied to the base mob count per wave.
    pub fn count_mult(&self) -> f32 {
        match self {
            Self::Calm => 0.6,
            Self::Standard => 1.0,
            Self::Relentless => 1.5,
        }
    }

    /// Extra mobs added per night (escalation rate).
    pub fn growth_per_night(&self) -> f32 {
        match self {
            Self::Calm => 1.0,
            Self::Standard => 2.0,
            Self::Relentless => 3.5,
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Self::Calm => 0,
            Self::Standard => 1,
            Self::Relentless => 2,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Calm,
            2 => Self::Relentless,
            _ => Self::Standard,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Calm => "CALM",
            Self::Standard => "STANDARD",
            Self::Relentless => "RELENTLESS",
        }
    }
}

// ── Graphics Settings ──

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ShadowQuality {
    Off,
    Low,
    #[default]
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AntiAliasingMode {
    Off,
    #[default]
    Smaa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EffectQuality {
    Off,
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphicsSettings {
    pub resolution: (u32, u32),
    pub fullscreen: bool,
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    pub shadow_quality: ShadowQuality,
    pub entity_lights: bool,
    #[serde(default)]
    pub anti_aliasing: AntiAliasingMode,
    #[serde(default)]
    pub bloom: EffectQuality,
    #[serde(default = "default_brightness")]
    pub brightness: f32,
    #[serde(default = "default_auto_exposure")]
    pub auto_exposure: bool,
    #[serde(default = "default_depth_of_field")]
    pub depth_of_field: EffectQuality,
    #[serde(default = "default_motion_blur")]
    pub motion_blur: EffectQuality,
    #[serde(default = "default_chromatic_aberration")]
    pub chromatic_aberration: EffectQuality,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub theme_mode: crate::ui::theme::ThemeMode,
}

fn default_vsync() -> bool {
    true
}

fn default_brightness() -> f32 {
    1.0
}

fn default_auto_exposure() -> bool {
    false
}

fn default_depth_of_field() -> EffectQuality {
    EffectQuality::Off
}

fn default_motion_blur() -> EffectQuality {
    EffectQuality::Off
}

fn default_chromatic_aberration() -> EffectQuality {
    EffectQuality::Off
}

fn default_ui_scale() -> f32 {
    1.0
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            resolution: (1280, 720),
            fullscreen: false,
            vsync: true,
            shadow_quality: ShadowQuality::High,
            entity_lights: true,
            anti_aliasing: AntiAliasingMode::Smaa,
            bloom: EffectQuality::Medium,
            brightness: 1.0,
            auto_exposure: false,
            depth_of_field: EffectQuality::Off,
            motion_blur: EffectQuality::Off,
            chromatic_aberration: EffectQuality::Off,
            ui_scale: 1.0,
            theme_mode: crate::ui::theme::ThemeMode::Dark,
        }
    }
}

/// Dynamic resolution list populated from monitor video modes at runtime.
#[derive(Resource, Clone, Debug)]
pub struct AvailableResolutions(pub Vec<(u32, u32)>);

impl Default for AvailableResolutions {
    fn default() -> Self {
        Self(vec![
            (1280, 720),
            (1366, 768),
            (1600, 900),
            (1920, 1080),
            (2560, 1440),
            (3440, 1440),
            (3840, 2160),
        ])
    }
}
