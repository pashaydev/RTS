//! UI types: markers, buttons, animations, overlays, input.

use bevy::prelude::*;
use std::collections::HashMap;

use super::app::Faction;
use super::economy::ResourceType;
use super::units::{AbilityId, FormationType};
use crate::blueprints::EntityKind;

// ── Selection & Input markers ──

#[derive(Component)]
pub struct Selected;

#[derive(Component)]
pub struct Hovered;

/// Bounding sphere radius for mouse picking (ray-sphere intersection).
#[derive(Component)]
pub struct PickRadius(pub f32);

#[derive(Resource, Default)]
pub struct UiClickedThisFrame(pub u8);

/// Set to true when a mouse press starts on UI; cleared on mouse release.
#[derive(Resource, Default)]
pub struct UiPressActive(pub bool);

/// True when the cursor is hovering any UI node (blocks camera input).
#[derive(Resource, Default)]
pub struct CursorOverUi(pub bool);

/// Tracks double-tap timing for group recall + camera center
#[derive(Resource, Default)]
pub struct ControlGroupState {
    pub active_group: Option<usize>,
    pub last_recall_group: Option<usize>,
    pub last_recall_time: f64,
}

/// Tracks Tab cycling through unit types in current selection
#[derive(Resource, Default)]
pub struct SubgroupCycleState {
    pub subgroup_kinds: Vec<EntityKind>,
    pub current_index: usize,
    pub active: bool,
    /// Snapshot of entities at time subgroup mode was activated
    pub original_selection: Vec<Entity>,
}

/// Tracks double-click detection for same-type selection
#[derive(Resource, Default)]
pub struct DoubleClickDetector {
    pub last_click_entity: Option<Entity>,
    pub last_click_time: f64,
}

/// Active command mode for hotkey-based unit commands (A-click, P-click).
#[derive(Resource, Default, PartialEq, Eq, Debug, Clone, Copy)]
pub enum CommandMode {
    #[default]
    Normal,
    AttackMove,
    Patrol,
    AbilityTarget(AbilityId),
}

#[derive(Component)]
pub struct HoverRing;

#[derive(Resource)]
pub struct HoverRingAssets {
    pub mesh: Handle<Mesh>,
}

// ── UI Panel markers ──

#[derive(Component)]
pub struct ResourceText(pub ResourceType);

#[derive(Component)]
pub struct ActionBarInner;

#[derive(Resource, Default)]
pub struct InspectedEnemy {
    pub entity: Option<Entity>,
}

#[derive(Resource)]
pub struct EntityLabelVisibility {
    pub show_unit_labels: bool,
}

impl Default for EntityLabelVisibility {
    fn default() -> Self {
        Self {
            show_unit_labels: false,
        }
    }
}

#[derive(Component)]
pub struct SelectionInfoPanel;

#[derive(Component)]
pub struct SelectionInfoBody;

#[derive(Component)]
pub struct SelectionFooter;

#[derive(Component)]
pub struct ToggleUnitLabelsButton;

#[derive(Component)]
pub struct ToggleUnitLabelsButtonText;

#[derive(Component)]
pub struct UnitLabelsStatusText;

/// Single source of truth for which UI mode is active.
#[derive(Resource, Clone, PartialEq, Debug)]
pub enum UiMode {
    /// Default: no selection, show building grid
    Idle,
    /// One or more own units selected
    SelectedUnits(Vec<Entity>),
    /// One own building selected
    SelectedBuilding(Entity),
    /// Placing a building from a card/grid
    PlacingBuilding(EntityKind),
}

impl Default for UiMode {
    fn default() -> Self {
        UiMode::Idle
    }
}

#[derive(Component)]
pub struct UnitCardGrid;

#[derive(Component)]
pub struct UnitCardRef(pub Entity);

#[derive(Component)]
pub struct ArmyOverviewEntry {
    pub kind: EntityKind,
}

#[derive(Component)]
pub struct ArmyOverviewHighlighted;

#[derive(Component)]
pub struct HpBarFill(pub Entity);

#[derive(Component)]
pub struct EnemyInspectPanel;

// ── Selection state ──

#[derive(Resource, Default)]
pub struct DragState {
    pub start: Option<Vec2>,
    pub current: Option<Vec2>,
    pub dragging: bool,
}

#[derive(Component)]
pub struct SelectionBox;

// ── Building interaction buttons ──

#[derive(Component)]
pub struct UpgradeButton;

#[derive(Component)]
pub struct DemolishButton;

#[derive(Component)]
pub struct RallyPointButton;

#[derive(Component)]
pub struct ScuttleUnitButton;

#[derive(Component)]
pub struct DropCargoButton;

#[derive(Component)]
pub struct ConfirmDemolishButton;

#[derive(Component)]
pub struct CancelDemolishButton;

#[derive(Component)]
pub struct DemolishConfirmPanel;

#[derive(Component)]
pub struct AssignWorkerButton;

#[derive(Component)]
pub struct UnassignWorkerButton;

#[derive(Component)]
pub struct UnassignOneWorkerButton;

#[derive(Component)]
pub struct PauseBuildingButton;

#[derive(Component)]
pub struct SelectRecipeButton(pub usize);

#[derive(Component)]
pub struct TrainingQueueDisplay;

#[derive(Component)]
pub struct TrainingProgressBar;

#[derive(Component)]
pub struct ConstructionProgressBar;

#[derive(Component)]
pub struct ConstructionWorkerCountText;

#[derive(Component)]
pub struct UpgradeProgressBar;

#[derive(Component)]
pub struct ToggleAutoAttackButton;

#[derive(Component)]
pub struct CancelTrainButton(pub usize);

#[derive(Component)]
pub struct CancelTrainQueueItemButton {
    pub building: Entity,
    pub index: usize,
}

#[derive(Component)]
pub struct CancelUnitTaskButton {
    pub unit: Entity,
    pub task_id: Option<u64>,
    pub is_current: bool,
}

#[derive(Component)]
pub struct CommandModeButton(pub CommandMode);

#[derive(Component)]
pub struct HoldPositionButton;

#[derive(Component)]
pub struct StopButton;

#[derive(Component)]
pub struct CycleStanceButton;

#[derive(Component)]
pub struct AbilityButton(pub AbilityId);

#[derive(Component)]
pub struct CycleFormationButton;

#[derive(Component)]
pub struct FormationPresetButton(pub FormationType);

#[derive(Component)]
pub struct ActionTooltip {
    pub owner: Entity,
}

#[derive(Component)]
pub struct ActionTooltipTrigger {
    pub text: String,
}

#[derive(Component)]
pub struct BuildingHpBarFill;

// ── Button styling ──

/// Marker for standard (non-ghost) buttons that receive hover/press visuals.
#[derive(Component)]
pub struct StandardButton;

/// Smooth lerp-based button animation state.
#[derive(Component)]
pub struct ButtonAnimState {
    pub bg_current: [f32; 4],
    pub bg_target: [f32; 4],
    pub scale_current: f32,
    pub scale_target: f32,
    pub lift_current: f32,
    pub lift_target: f32,
    pub _shadow_current: f32,
    pub _shadow_target: f32,
}

impl ButtonAnimState {
    pub fn new(rest_bg: [f32; 4]) -> Self {
        Self {
            bg_current: rest_bg,
            bg_target: rest_bg,
            scale_current: 1.0,
            scale_target: 1.0,
            lift_current: 0.0,
            lift_target: 0.0,
            _shadow_current: 0.0,
            _shadow_target: 0.0,
        }
    }
}

/// Which visual style a ButtonAnimState button uses.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// Filled background (train buttons)
    Filled,
    /// Ghost/outline style (upgrade, rally, demolish)
    Ghost,
    /// Destructive ghost (demolish)
    Destructive,
    /// Accent style — dark bg with gold left accent (menu buttons)
    Accent,
}

/// Marks a button as disabled. The optional string is a hint shown below the button.
#[derive(Component)]
pub struct ButtonDisabled(pub Option<String>);

// ── UI Animations ──

/// Marks an action bar child for fade-out removal.
#[derive(Component)]
pub struct ActionBarFadeOut {
    pub timer: Timer,
    pub initial_offset: f32,
}

/// Marks an action bar child for fade-in entrance.
#[derive(Component)]
pub struct ActionBarFadeIn {
    pub timer: Timer,
    pub delay: Timer,
    pub started: bool,
}

/// Fades a UI node in over its duration (opacity 0 → 1).
#[derive(Component)]
pub struct UiFadeIn {
    pub timer: Timer,
}

/// Fades a UI node out over its duration (opacity 1 → 0), then despawns.
#[derive(Component)]
pub struct UiFadeOut {
    pub timer: Timer,
}

/// Slides a UI node in from an offset over its duration.
#[derive(Component)]
pub struct UiSlideIn {
    pub offset: Vec2,
    pub timer: Timer,
}

/// Scales a UI node in from a start scale to 1.0 with optional elastic overshoot.
#[derive(Component)]
pub struct UiScaleIn {
    pub from: f32,
    pub timer: Timer,
    pub elastic: bool,
}

/// Expands a separator line from zero width to full width.
#[derive(Component)]
pub struct UiLineExpand {
    pub target_width: f32,
    pub timer: Timer,
}

/// Floating ambient particle on the menu background.
#[derive(Component)]
pub struct MenuParticle {
    pub velocity: Vec2,
    pub base_alpha: f32,
    pub phase: f32,
}

/// Shimmer effect on title text — cycles hue/brightness.
#[derive(Component)]
pub struct TitleShimmer {
    pub phase_offset: f32,
}

/// Pulsing glow border on focused/hovered elements.
#[derive(Component)]
pub struct UiGlowPulse {
    pub color: Color,
    pub intensity: f32,
}

/// Tracks what text entity belongs to a train button's cost text (for coloring).
#[derive(Component)]
pub struct TrainCostText {
    pub kind: EntityKind,
}

// ── Attention & Damage Popup components ──

/// Tracks previous frame's health to detect damage without modifying combat code.
#[derive(Component)]
pub struct PreviousHealth(pub f32);

/// Timer reset whenever a unit takes damage; drives the "under attack" icon.
#[derive(Component)]
pub struct UnderAttackTimer(pub Timer);

/// Floating damage/heal number anchored to a world position.
#[derive(Component)]
pub struct DamagePopup {
    pub timer: Timer,
    #[allow(dead_code)]
    pub amount: f32,
    #[allow(dead_code)]
    pub is_damage: bool,
    pub world_pos: Vec3,
    pub offset_x: f32,
}

/// State icon displayed above a unit.
#[derive(Component)]
pub struct AttentionIcon {
    pub owner: Entity,
    pub kind: AttentionKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttentionKind {
    UnderAttack,
    Gathering,
    Attacking,
    Building,
}

#[derive(Resource)]
pub struct AttentionIconAssets {
    pub under_attack: Handle<Image>,
    pub gathering: Handle<Image>,
    pub attacking: Handle<Image>,
    pub building: Handle<Image>,
}

#[derive(Resource)]
pub struct DayCycleIconAssets {
    pub dawn: Handle<Image>,
    pub day: Handle<Image>,
    pub dusk: Handle<Image>,
    pub night: Handle<Image>,
}

// ── Text Input ──

#[derive(Component)]
pub struct TextInputField {
    pub value: String,
    pub cursor_pos: usize,
    /// When `Some`, a selection exists between `selection_anchor` and `cursor_pos`.
    pub selection_anchor: Option<usize>,
    pub max_len: usize,
}

impl TextInputField {
    /// Returns `(start, end)` of the current selection, or `None` if nothing is selected.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor
            .map(|anchor| {
                let start = anchor.min(self.cursor_pos);
                let end = anchor.max(self.cursor_pos);
                if start == end {
                    return (start, end);
                }
                (start, end)
            })
            .filter(|(s, e)| s != e)
    }

    /// Delete the current selection, returning the deleted text. Resets anchor.
    pub fn delete_selection(&mut self) -> Option<String> {
        if let Some((start, end)) = self.selection_range() {
            let removed: String = self.value[start..end].to_string();
            self.value.replace_range(start..end, "");
            self.cursor_pos = start;
            self.selection_anchor = None;
            Some(removed)
        } else {
            self.selection_anchor = None;
            None
        }
    }
}

#[derive(Component)]
pub struct TextInputFocused;

/// Marker for the cursor "|" TextSpan inside a text input's rich text.
#[derive(Component)]
pub struct TextInputCursor;

/// Marker for the selection-before-cursor TextSpan (highlighted).
#[derive(Component)]
pub struct TextInputSelBefore;

/// Marker for the selection-after-cursor TextSpan (highlighted).
#[derive(Component)]
pub struct TextInputSelAfter;

/// Marker for the post-selection/cursor TextSpan (normal color).
#[derive(Component)]
pub struct TextInputPostText;

#[derive(Component)]
pub struct RandomNameButton;

// ── In-Game Overlay ──

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum InGameOverlay {
    #[default]
    None,
    PauseMenu,
    PauseOptions,
    PauseConfirmEndMatch,
    DeathScreen,
    Spectating,
    HowToPlay,
}

/// Run-condition: returns true only when no overlay is active (player can issue commands).
pub fn player_can_command(overlay: Res<InGameOverlay>) -> bool {
    *overlay == InGameOverlay::None
}

#[derive(Resource, Default)]
pub struct FactionStats {
    pub stats: HashMap<Faction, FactionStatus>,
}

#[derive(Default, Clone)]
pub struct FactionStatus {
    pub unit_count: u32,
    pub building_count: u32,
    pub eliminated: bool,
}

/// Inserted as a resource when Restart is requested; menu reads & removes it.
#[derive(Resource)]
pub struct RestartRequested;

/// Records the `Time::elapsed_secs_f64()` when a match begins, for duration calculation.
#[derive(Resource)]
pub struct MatchStartTime(pub f64);

// ── Overlay UI markers ──

#[derive(Component)]
pub struct PauseOverlayRoot;

#[derive(Component)]
pub struct GuideOverlayRoot;

#[derive(Component)]
pub struct GuideContentArea;

#[derive(Component)]
pub struct GuidePageDots;

#[derive(Resource, Debug, Clone, Copy)]
pub struct GuideState {
    pub page: usize,
    pub is_multiplayer: bool,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum GuideButton {
    Back,
    Next,
    Close,
}

#[derive(Component)]
pub struct DeathScreenRoot;

#[derive(Component)]
pub struct SpectatorHudRoot;

#[derive(Component)]
pub struct WorldOverlayBackRoot;

#[derive(Component)]
pub struct WorldOverlayFrontRoot;

#[derive(Component)]
pub struct WorldOverlayBackItem;

#[derive(Component)]
pub struct WorldOverlayFrontItem;

/// Marks all in-game entities for cleanup on exit.
#[derive(Component)]
pub struct GameWorld;

#[derive(Component)]
pub struct PauseMenuButton(pub PauseAction);

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PauseAction {
    Continue,
    Restart,
    MainMenu,
    Options,
    ConfirmHostEnd,
    CancelHostEnd,
    Quit,
    BackFromOptions,
    ApplySettings,
    Spectate,
    SaveGame,
    LoadGame,
}

#[derive(Component)]
pub struct SpectatorStatsText;

// ── Menu Keyboard Navigation ──

/// Marks a button as focusable via keyboard nav.
#[derive(Component)]
pub struct NavFocusable(pub usize);

/// Marker added to the currently keyboard-focused button.
#[derive(Component)]
pub struct NavFocused;

/// Tracks keyboard focus index for menu navigation.
#[derive(Resource, Default)]
pub struct MenuNavFocus {
    pub index: usize,
}

/// Controls hint row shown in menus.
#[derive(Component)]
pub struct ControlsHint;
