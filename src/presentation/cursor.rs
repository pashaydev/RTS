//! Contextual custom cursor backed by the StoneCursorWenrexa asset pack.
//!
//! The cursor sprite swaps based on what the player is hovering and which
//! command mode is active, giving classic RTS "smart cursor" feedback.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use crate::types::*;

/// All semantic roles mapped to the StoneCursorWenrexa sprites.
///
/// Most roles still use their original asset number, but a few are remapped to
/// better-fit sprites from the pack.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum CursorKind {
    #[default]
    Default,
    HoverFriendly,
    HoverEnemy,
    AttackMoveCursor,
    AttackTarget,
    MoveReady,
    Forbidden,
    Diplomacy,
    DefaultDark,
    RallyHover,
    BoxSelect,
    Repair,
    Garrison,
    AbilityTarget,
    AttackMoveTarget,
    PatrolTarget,
    Gather,
    Wait,
    PlaceInvalid,
    Disabled,
}

impl CursorKind {
    const COUNT: usize = 20;

    fn index(self) -> usize {
        self.sprite_number() - 1
    }

    fn sprite_number(self) -> usize {
        match self {
            CursorKind::Default => 1,
            CursorKind::HoverFriendly => 2,
            CursorKind::HoverEnemy => 3,
            CursorKind::AttackMoveCursor => 15,
            CursorKind::AttackTarget => 17,
            CursorKind::MoveReady => 13,
            CursorKind::Forbidden => 7,
            CursorKind::Diplomacy => 8,
            CursorKind::DefaultDark => 9,
            CursorKind::RallyHover => 10,
            CursorKind::BoxSelect => 11,
            CursorKind::Repair => 12,
            CursorKind::Garrison => 13,
            CursorKind::AbilityTarget => 14,
            CursorKind::AttackMoveTarget => 15,
            CursorKind::PatrolTarget => 16,
            CursorKind::Gather => 17,
            CursorKind::Wait => 18,
            CursorKind::PlaceInvalid => 19,
            CursorKind::Disabled => 20,
        }
    }

    /// Hotspot in texture pixels. Pointer arrows use the tip (~upper-left);
    /// centered compass/gear/icon cursors use the image center.
    fn hotspot(self) -> (u16, u16) {
        match self.sprite_number() {
            1 | 2 | 3 | 6 | 7 | 9 | 10 | 13 | 19 => (4, 4),
            8 | 11 | 12 | 14 | 15 | 16 | 17 | 18 | 20 => (16, 16),
            _ => (4, 4),
        }
    }
}

/// Owns handles to all 20 cursor sprite images.
#[derive(Resource)]
pub struct CursorAssets {
    images: [Handle<Image>; CursorKind::COUNT],
}

impl CursorAssets {
    fn handle(&self, kind: CursorKind) -> Handle<Image> {
        self.images[kind.index()].clone()
    }
}

/// Which cursor is currently installed on the primary window.
#[derive(Resource, Default)]
pub struct CurrentCursor(pub CursorKind);

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentCursor>()
            .add_systems(Startup, load_cursor_assets)
            .add_systems(
                Update,
                update_cursor_system
                    .in_set(GameFlowSet::Presentation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnEnter(AppState::MainMenu), reset_cursor_on_menu);
    }
}

fn load_cursor_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    primary: Query<Entity, With<PrimaryWindow>>,
) {
    let images = std::array::from_fn(|i| {
        let name = format!("textures/StoneCursorWenrexa/PNG/{:02}.png", i + 1);
        asset_server.load(name)
    });
    let assets = CursorAssets { images };

    // Install the default sprite immediately so the OS pointer swaps on spawn.
    if let Ok(window_entity) = primary.single() {
        let kind = CursorKind::Default;
        install_cursor(&mut commands, window_entity, &assets, kind);
    }

    commands.insert_resource(assets);
}

fn reset_cursor_on_menu(
    mut commands: Commands,
    assets: Option<Res<CursorAssets>>,
    mut current: ResMut<CurrentCursor>,
    primary: Query<Entity, With<PrimaryWindow>>,
) {
    let Some(assets) = assets else { return };
    let Ok(window_entity) = primary.single() else {
        return;
    };
    current.0 = CursorKind::Default;
    install_cursor(&mut commands, window_entity, &assets, CursorKind::Default);
}

#[cfg(not(target_arch = "wasm32"))]
fn make_cursor_icon(assets: &CursorAssets, kind: CursorKind) -> CursorIcon {
    CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
        handle: assets.handle(kind),
        texture_atlas: None,
        flip_x: false,
        flip_y: false,
        rect: None,
        hotspot: kind.hotspot(),
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn install_cursor(
    commands: &mut Commands,
    window_entity: Entity,
    assets: &CursorAssets,
    kind: CursorKind,
) {
    commands
        .entity(window_entity)
        .insert(make_cursor_icon(assets, kind));
}

#[cfg(target_arch = "wasm32")]
fn install_cursor(
    _commands: &mut Commands,
    _window_entity: Entity,
    _assets: &CursorAssets,
    kind: CursorKind,
) {
    apply_wasm_cursor(kind);
}

#[derive(SystemParam)]
struct HoverClassify<'w, 's> {
    selected_units: Query<'w, 's, (), (With<Unit>, With<Selected>)>,
    hovered: Query<'w, 's, Entity, With<Hovered>>,
    unit_q: Query<'w, 's, &'static Faction, With<Unit>>,
    mob_q: Query<'w, 's, (), With<Mob>>,
    building_q:
        Query<'w, 's, (&'static Faction, Option<&'static BuildingState>), With<Building>>,
    resource_q: Query<'w, 's, (), With<ResourceNode>>,
    health_q: Query<'w, 's, &'static Health>,
}

#[allow(clippy::too_many_arguments)]
fn update_cursor_system(
    mut commands: Commands,
    assets: Res<CursorAssets>,
    mut current: ResMut<CurrentCursor>,
    primary: Query<Entity, With<PrimaryWindow>>,
    over_ui: Res<CursorOverUi>,
    command_mode: Res<CommandMode>,
    placement: Res<BuildingPlacementState>,
    drag: Res<DragState>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    hover: HoverClassify,
) {
    let Ok(window_entity) = primary.single() else {
        return;
    };

    let kind = decide_cursor(
        &over_ui,
        &command_mode,
        &placement,
        &drag,
        &active_player,
        &teams,
        &hover,
    );

    if kind == current.0 {
        return;
    }
    current.0 = kind;
    install_cursor(&mut commands, window_entity, &assets, kind);
}

fn decide_cursor(
    over_ui: &CursorOverUi,
    command_mode: &CommandMode,
    placement: &BuildingPlacementState,
    drag: &DragState,
    active_player: &ActivePlayer,
    teams: &TeamConfig,
    hover: &HoverClassify,
) -> CursorKind {
    // Over UI → plain pointer (never the stone‑tipped world cursor).
    if over_ui.0 {
        return CursorKind::Default;
    }

    // Drag box‑select takes precedence over all world hover logic.
    if drag.dragging {
        return CursorKind::BoxSelect;
    }

    // Building placement mode: show worker "garrison" cursor. Validity is
    // communicated by the ghost preview, so we keep a single state here.
    if placement.mode != PlacementMode::None {
        return CursorKind::Garrison;
    }

    let has_selection = !hover.selected_units.is_empty();
    let hovered_entity = hover.hovered.iter().next();

    // Command‑mode targeting overrides normal hover cursors.
    match *command_mode {
        CommandMode::AbilityTarget(_) => return CursorKind::AbilityTarget,
        CommandMode::AttackMove => {
            let hostile_under_cursor = hovered_entity
                .map(|e| is_hostile_hover(e, active_player, teams, hover))
                .unwrap_or(false);
            if hostile_under_cursor {
                return CursorKind::AttackMoveTarget;
            }
            return CursorKind::AttackMoveCursor;
        }
        CommandMode::Patrol => return CursorKind::PatrolTarget,
        CommandMode::Normal => {}
    }

    // Classify whatever the player is hovering.
    if let Some(entity) = hovered_entity {
        if hover.resource_q.contains(entity) {
            return CursorKind::Gather;
        }
        if hover.mob_q.contains(entity) {
            return if has_selection {
                CursorKind::AttackTarget
            } else {
                CursorKind::HoverEnemy
            };
        }
        if let Ok(faction) = hover.unit_q.get(entity) {
            let hostile = teams.is_hostile(&active_player.0, faction);
            return if hostile {
                if has_selection {
                    CursorKind::AttackTarget
                } else {
                    CursorKind::HoverEnemy
                }
            } else {
                CursorKind::HoverFriendly
            };
        }
        if let Ok((faction, state)) = hover.building_q.get(entity) {
            let hostile = teams.is_hostile(&active_player.0, faction);
            if hostile {
                return if has_selection {
                    CursorKind::AttackTarget
                } else {
                    CursorKind::HoverEnemy
                };
            }
            // Friendly building: repair if damaged (or still under construction).
            let damaged = matches!(state, Some(BuildingState::UnderConstruction))
                || hover
                    .health_q
                    .get(entity)
                    .map(|h| h.current < h.max)
                    .unwrap_or(false);
            return if damaged {
                CursorKind::Repair
            } else {
                CursorKind::HoverFriendly
            };
        }
    }

    // Empty ground.
    if has_selection {
        CursorKind::MoveReady
    } else {
        CursorKind::Default
    }
}

fn is_hostile_hover(
    entity: Entity,
    active_player: &ActivePlayer,
    teams: &TeamConfig,
    hover: &HoverClassify,
) -> bool {
    if hover.mob_q.contains(entity) {
        return true;
    }
    if let Ok(faction) = hover.unit_q.get(entity) {
        return teams.is_hostile(&active_player.0, faction);
    }
    if let Ok((faction, _)) = hover.building_q.get(entity) {
        return teams.is_hostile(&active_player.0, faction);
    }
    false
}

#[cfg(target_arch = "wasm32")]
fn apply_wasm_cursor(kind: CursorKind) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Ok(Some(canvas)) = document.query_selector("canvas") else {
        return;
    };
    let Ok(canvas) = canvas.dyn_into::<web_sys::HtmlElement>() else {
        return;
    };
    let _ = canvas.style().set_property("cursor", &css_cursor(kind));
}

#[cfg(target_arch = "wasm32")]
fn css_cursor(kind: CursorKind) -> String {
    let (x, y) = kind.hotspot();
    format!(
        "url(\"assets/textures/StoneCursorWenrexa/PNG/{:02}.png\") {} {}, auto",
        kind.index() + 1,
        x,
        y
    )
}
