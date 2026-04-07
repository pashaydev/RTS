use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;

use crate::ages::FactionAges;
use crate::ai::types::AiState;
use crate::ai::AiWorldSnapshot;
use crate::combat::CombatBudgetState;
use crate::components::*;
use crate::fog::{FogTextureUploadState, FogTextures, FogTweakSettings};
use crate::ground::{TerrainShapeSyncState, TerrainShapeUpdateQueue, TerrainSurfaceDirtyQueue};
use crate::multiplayer::{HostNetState, NetRole};
use crate::net_bridge::EntityNetMap;
use crate::pathfinding::{NavGrid, NavGridDirty, PathRequestQueue};
use crate::spatial::{SpatialHashGrid, WallSpatialGrid};
use crate::theme::{self, Theme};
use crate::ui::core::framework::{
    GridInteractionActive, WidgetDragState, WidgetEditState, WidgetRegistry, WidgetResizeState,
};
use crate::ui::core::interactions::UiClickEvent;
use crate::ui::fonts::UiFonts;
use crate::ui::widgets::event_log_widget::{EventLogRenderState, GameEventLog};
use crate::ui::widgets::group_hotkeys_widget::ControlGroups;
use crate::ui::widgets::hints_widget::HintState;
use crate::victory::VictoryState;

/// Tracks keyboard focus index for pause menu navigation.
#[derive(Resource, Default)]
struct PauseNavFocus {
    index: usize,
}

pub struct PauseMenuPlugin;

impl Plugin for PauseMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InGameOverlay>()
            .init_resource::<FactionStats>()
            .init_resource::<StatsTimer>()
            .init_resource::<PauseNavFocus>()
            .add_systems(
                Update,
                handle_pause_buttons.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    handle_escape_key,
                    update_spectator_stats_display,
                    pause_keyboard_nav,
                    pause_nav_focus_visuals,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (update_faction_stats, check_player_elimination)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_game_world);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PausePanel {
    Main,
    Options,
    HostEndConfirm,
}

// ── Timer for periodic stats update ──

#[derive(Resource)]
struct StatsTimer {
    timer: Timer,
    game_start: Timer,
}

impl Default for StatsTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            game_start: Timer::from_seconds(5.0, TimerMode::Once),
        }
    }
}

// ── Escape Key Handler ──

fn handle_escape_key(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<InGameOverlay>,
    command_mode: Res<CommandMode>,
    placement: Res<BuildingPlacementState>,
    pause_roots: Query<Entity, With<PauseOverlayRoot>>,
    fonts: Res<UiFonts>,
    theme: Res<Theme>,
    net_role: Res<NetRole>,
    mut pause_nav: ResMut<PauseNavFocus>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }

    pause_nav.index = 0;

    match *overlay {
        InGameOverlay::None => {
            // Let other systems handle escape first
            if *command_mode != CommandMode::Normal {
                return;
            }
            if placement.mode != PlacementMode::None {
                return;
            }
            *overlay = InGameOverlay::PauseMenu;
            spawn_pause_overlay(&mut commands, &fonts, &theme, PausePanel::Main, *net_role);
        }
        InGameOverlay::PauseMenu => {
            *overlay = InGameOverlay::None;
            for e in &pause_roots {
                commands.entity(e).try_despawn();
            }
        }
        InGameOverlay::PauseOptions => {
            *overlay = InGameOverlay::PauseMenu;
            for e in &pause_roots {
                commands.entity(e).try_despawn();
            }
            spawn_pause_overlay(&mut commands, &fonts, &theme, PausePanel::Main, *net_role);
        }
        InGameOverlay::PauseConfirmEndMatch => {
            *overlay = InGameOverlay::PauseMenu;
            for e in &pause_roots {
                commands.entity(e).try_despawn();
            }
            spawn_pause_overlay(&mut commands, &fonts, &theme, PausePanel::Main, *net_role);
        }
        InGameOverlay::DeathScreen => {
            // Can't dismiss death screen with Escape
        }
        InGameOverlay::Spectating => {
            *overlay = InGameOverlay::PauseMenu;
            spawn_pause_overlay(&mut commands, &fonts, &theme, PausePanel::Main, *net_role);
        }
    }
}

// ── Spawn Pause Overlay ──

fn spawn_pause_overlay(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    panel_kind: PausePanel,
    role: NetRole,
) {
    let root = commands
        .spawn((
            PauseOverlayRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ZIndex(100),
            UiFadeIn {
                timer: Timer::from_seconds(0.2, TimerMode::Once),
            },
        ))
        .id();

    // Center panel
    let panel = commands
        .spawn((
            Node {
                width: Val::Px(400.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(32.0)),
                border: UiRect::all(Val::Px(1.0)),
                // border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(
                theme.colors.bg_panel.to_srgba().red,
                theme.colors.bg_panel.to_srgba().green,
                theme.colors.bg_panel.to_srgba().blue,
                0.96,
            )),
            BorderColor::all(theme.colors.separator),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.6),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(0.0),
                Val::Px(24.0),
            ),
            UiScaleIn {
                from: 0.92,
                timer: Timer::from_seconds(0.25, TimerMode::Once),
                elastic: false,
            },
        ))
        .id();

    commands.entity(root).add_child(panel);

    match panel_kind {
        PausePanel::Main => spawn_pause_content(commands, panel, fonts, theme, role),
        PausePanel::Options => spawn_options_content(commands, panel, fonts, theme),
        PausePanel::HostEndConfirm => spawn_host_end_confirm_content(commands, panel, fonts, theme),
    }
}

fn spawn_pause_content(
    commands: &mut Commands,
    panel: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    role: NetRole,
) {
    // Buttons
    let menu_label = if role == NetRole::Host {
        "End Match"
    } else {
        "Main Menu"
    };

    // Save/Load only in single-player (offline) mode
    let is_offline = role == NetRole::Offline;

    let mut buttons: Vec<(&str, PauseAction, bool)> =
        vec![("Continue", PauseAction::Continue, true)];
    if is_offline {
        buttons.push(("Save Game", PauseAction::SaveGame, false));
        buttons.push(("Load Game", PauseAction::LoadGame, false));
    }
    buttons.push(("Restart", PauseAction::Restart, false));
    buttons.push((menu_label, PauseAction::MainMenu, false));
    buttons.push(("Quit", PauseAction::Quit, false));

    for (i, (label, action, accent)) in buttons.into_iter().enumerate() {
        let btn = spawn_overlay_button(commands, label, action, accent, fonts, theme, Some(i));
        commands.entity(panel).add_child(btn);
    }

    // Controls hint
    spawn_pause_controls_hint(commands, panel, fonts, theme);
}

fn spawn_options_content(commands: &mut Commands, panel: Entity, fonts: &UiFonts, theme: &Theme) {
    let title = commands
        .spawn((
            Text::new("OPTIONS"),
            TextFont {
                font: fonts.heading.clone(),
                font_size: theme.typography.heading,
                ..default()
            },
            TextColor(theme.colors.text_primary),
            Node {
                margin: UiRect::bottom(Val::Px(24.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(panel).add_child(title);

    // Placeholder text
    let placeholder = commands
        .spawn((
            Text::new("(Settings can be changed from the main menu)"),
            TextFont {
                font: fonts.body.clone(),
                font_size: theme.typography.body,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
            Node {
                margin: UiRect::bottom(Val::Px(24.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(panel).add_child(placeholder);

    let btn = spawn_overlay_button(
        commands,
        "Back",
        PauseAction::BackFromOptions,
        true,
        fonts,
        theme,
        Some(0),
    );
    commands.entity(panel).add_child(btn);
}

fn spawn_host_end_confirm_content(
    commands: &mut Commands,
    panel: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let title = commands
        .spawn((
            Text::new("END MATCH FOR EVERYONE?"),
            TextFont {
                font: fonts.heading.clone(),
                font_size: theme.typography.heading,
                ..default()
            },
            TextColor(theme.colors.warning),
            Node {
                margin: UiRect::bottom(Val::Px(16.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(panel).add_child(title);

    let body = commands
        .spawn((
            Text::new("All connected clients will be forced back to main menu."),
            TextFont {
                font: fonts.body.clone(),
                font_size: theme.typography.body,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
            Node {
                margin: UiRect::bottom(Val::Px(20.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(panel).add_child(body);

    let cancel_btn = spawn_overlay_button(
        commands,
        "Cancel",
        PauseAction::CancelHostEnd,
        false,
        fonts,
        theme,
        Some(0),
    );
    let confirm_btn = spawn_overlay_button(
        commands,
        "End Match",
        PauseAction::ConfirmHostEnd,
        true,
        fonts,
        theme,
        Some(1),
    );
    commands
        .entity(panel)
        .add_children(&[cancel_btn, confirm_btn]);
}

fn spawn_overlay_button(
    commands: &mut Commands,
    label: &str,
    action: PauseAction,
    accent: bool,
    fonts: &UiFonts,
    theme: &Theme,
    nav_index: Option<usize>,
) -> Entity {
    let bg = if accent {
        theme.colors.accent
    } else {
        theme.colors.btn_primary
    };

    let mut ec = commands.spawn((
        PauseMenuButton(action),
        Button,
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Filled,
        Node {
            width: Val::Px(240.0),
            height: Val::Px(44.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(4.0)),
            border: UiRect::all(Val::Px(2.0)),
            // border_radius: BorderRadius::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(bg),
        BorderColor::all(Color::NONE),
    ));
    if let Some(idx) = nav_index {
        ec.insert(NavFocusable(idx));
    }
    ec.with_child((
        Text::new(label),
        TextFont {
            font: fonts.body_emphasis.clone(),
            font_size: theme.typography.button,
            ..default()
        },
        TextColor(if accent {
            Color::WHITE
        } else {
            theme.colors.text_primary
        }),
        Pickable::IGNORE,
    ));

    ec.id()
}

// ── Handle Pause Buttons ──

fn handle_pause_buttons(
    mut commands: Commands,
    mut click_events: MessageReader<UiClickEvent>,
    buttons: Query<&PauseMenuButton>,
    mut overlay: ResMut<InGameOverlay>,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    pause_roots: Query<Entity, With<PauseOverlayRoot>>,
    death_roots: Query<Entity, With<DeathScreenRoot>>,
    _spectator_roots: Query<Entity, With<SpectatorHudRoot>>,
    fonts: Res<UiFonts>,
    theme: Res<Theme>,
    mut fog_settings: ResMut<FogTweakSettings>,
    mut fog_map: Option<ResMut<FogOfWarMap>>,
    faction_stats: Res<FactionStats>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    net: (
        Res<NetRole>,
        Option<Res<HostNetState>>,
        Option<ResMut<bevy_matchbox::prelude::MatchboxSocket>>,
        Res<Time>,
    ),
) {
    let (net_role, host_state, mut matchbox_socket, time) = net;
    for event in click_events.read() {
        let Ok(btn) = buttons.get(event.entity) else {
            continue;
        };
        ui_clicked.0 = 2;

        match btn.0 {
            PauseAction::Continue => {
                *overlay = if *overlay == InGameOverlay::PauseMenu {
                    InGameOverlay::None
                } else {
                    // If we came from spectating, return to spectating
                    InGameOverlay::Spectating
                };
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
            }
            PauseAction::Restart => {
                commands.insert_resource(RestartRequested);
                next_state.set(AppState::MainMenu);
            }
            PauseAction::MainMenu => {
                if *net_role == NetRole::Host {
                    *overlay = InGameOverlay::PauseConfirmEndMatch;
                    for e in &pause_roots {
                        commands.entity(e).try_despawn();
                    }
                    for e in &death_roots {
                        commands.entity(e).try_despawn();
                    }
                    spawn_pause_overlay(
                        &mut commands,
                        &fonts,
                        &theme,
                        PausePanel::HostEndConfirm,
                        *net_role,
                    );
                } else {
                    next_state.set(AppState::MainMenu);
                }
            }
            PauseAction::Options => {
                *overlay = InGameOverlay::PauseOptions;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
                spawn_pause_overlay(
                    &mut commands,
                    &fonts,
                    &theme,
                    PausePanel::Options,
                    *net_role,
                );
            }
            PauseAction::Quit => {
                exit.write(AppExit::Success);
            }
            PauseAction::BackFromOptions => {
                *overlay = InGameOverlay::PauseMenu;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
                spawn_pause_overlay(&mut commands, &fonts, &theme, PausePanel::Main, *net_role);
            }
            PauseAction::ApplySettings => {
                // Handled same as BackFromOptions for now
                *overlay = InGameOverlay::PauseMenu;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
                spawn_pause_overlay(&mut commands, &fonts, &theme, PausePanel::Main, *net_role);
            }
            PauseAction::ConfirmHostEnd => {
                if *net_role == NetRole::Host {
                    if let (Some(host), Some(ref mut socket)) =
                        (host_state.as_ref(), matchbox_socket.as_mut())
                    {
                        broadcast_host_shutdown(host, socket, &time);
                    }
                }
                next_state.set(AppState::MainMenu);
            }
            PauseAction::CancelHostEnd => {
                *overlay = InGameOverlay::PauseMenu;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
                spawn_pause_overlay(&mut commands, &fonts, &theme, PausePanel::Main, *net_role);
            }
            PauseAction::Spectate => {
                *overlay = InGameOverlay::Spectating;
                for e in &death_roots {
                    commands.entity(e).try_despawn();
                }
                // Disable fog of war
                fog_settings.enable_los = false;
                if let Some(ref mut fog) = fog_map {
                    for v in fog.visible.iter_mut() {
                        *v = 1.0;
                    }
                    for e in fog.explored.iter_mut() {
                        *e = u8::MAX;
                    }
                    for d in fog.display.iter_mut() {
                        *d = 1.0;
                    }
                }
                // Spawn spectator HUD
                spawn_spectator_hud(&mut commands, &fonts, &theme, &faction_stats);
            }
            PauseAction::SaveGame => {
                // Insert a trigger resource that save_load::handle_save_trigger picks up
                commands.insert_resource(crate::save_load::SaveTrigger { label: None });
                // Close pause menu after save
                *overlay = InGameOverlay::None;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
            }
            PauseAction::LoadGame => {
                // Go to main menu's load game page
                commands.insert_resource(crate::menu::MenuPage::LoadGame);
                next_state.set(AppState::MainMenu);
            }
        }
    }
}

fn broadcast_host_shutdown(
    host: &HostNetState,
    socket: &mut bevy_matchbox::prelude::MatchboxSocket,
    time: &Time,
) {
    use game_state::message::{GameEvent, ServerMessage};

    let seq = {
        let mut s = host.seq.lock().unwrap();
        *s += 1;
        *s
    };
    let msg = ServerMessage::Event {
        seq,
        timestamp: time.elapsed_secs_f64(),
        events: vec![GameEvent::HostShutdown {
            reason: "Host ended the match".to_string(),
        }],
    };
    crate::multiplayer::matchbox_transport::broadcast_reliable(socket, &msg);
}

// ── Death Screen ──

fn spawn_death_screen(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    faction_stats: &FactionStats,
) {
    let root = commands
        .spawn((
            DeathScreenRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ZIndex(100),
            UiFadeIn {
                timer: Timer::from_seconds(0.5, TimerMode::Once),
            },
        ))
        .id();

    // DEFEAT text
    let defeat_text = commands
        .spawn((
            Text::new("DEFEAT"),
            TextFont {
                font: fonts.heading.clone(),
                font_size: 72.0,
                ..default()
            },
            TextColor(theme.colors.destructive),
            Node {
                margin: UiRect::bottom(Val::Px(32.0)),
                ..default()
            },
            UiScaleIn {
                from: 0.5,
                timer: Timer::from_seconds(0.6, TimerMode::Once),
                elastic: true,
            },
        ))
        .id();
    commands.entity(root).add_child(defeat_text);

    // Stats panel
    let stats_panel = commands
        .spawn((
            Node {
                width: Val::Px(450.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(20.0)),
                margin: UiRect::bottom(Val::Px(24.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.bg_panel),
            BorderColor::all(theme.colors.separator),
        ))
        .id();
    commands.entity(root).add_child(stats_panel);

    // Stats header
    let header = commands
        .spawn((
            Text::new("Battle Results"),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(theme.colors.text_primary),
            Node {
                margin: UiRect::bottom(Val::Px(12.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(stats_panel).add_child(header);

    // Per-faction stats rows
    for faction in Faction::PLAYERS.iter() {
        if let Some(status) = faction_stats.stats.get(faction) {
            let row = commands
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    margin: UiRect::vertical(Val::Px(4.0)),
                    padding: UiRect::all(Val::Px(6.0)),
                    ..default()
                },))
                .id();

            // Color swatch
            let swatch = commands
                .spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        margin: UiRect::right(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(faction.color()),
                ))
                .id();

            // Name
            let name = commands
                .spawn((
                    Text::new(faction.display_name()),
                    TextFont {
                        font: fonts.body.clone(),
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_primary),
                    Node {
                        width: Val::Px(100.0),
                        ..default()
                    },
                ))
                .id();

            // Unit count
            let units = commands
                .spawn((
                    Text::new(format!("{} units", status.unit_count)),
                    TextFont {
                        font: fonts.body.clone(),
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                    Node {
                        width: Val::Px(80.0),
                        ..default()
                    },
                ))
                .id();

            // Building count
            let buildings = commands
                .spawn((
                    Text::new(format!("{} bldg", status.building_count)),
                    TextFont {
                        font: fonts.body.clone(),
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                    Node {
                        width: Val::Px(80.0),
                        ..default()
                    },
                ))
                .id();

            // Status badge
            let (badge_text, badge_color) = if status.eliminated {
                ("ELIMINATED", theme.colors.destructive)
            } else {
                ("ALIVE", theme.colors.success)
            };
            let badge = commands
                .spawn((
                    Text::new(badge_text),
                    TextFont {
                        font: fonts.body_emphasis.clone(),
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(badge_color),
                ))
                .id();

            commands
                .entity(row)
                .add_children(&[swatch, name, units, buildings, badge]);
            commands.entity(stats_panel).add_child(row);
        }
    }

    // Buttons
    let btn_row = commands
        .spawn((Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(16.0),
            ..default()
        },))
        .id();
    commands.entity(root).add_child(btn_row);

    let menu_btn = spawn_overlay_button(
        commands,
        "Main Menu",
        PauseAction::MainMenu,
        false,
        fonts,
        theme,
        Some(0),
    );
    let spec_btn = spawn_overlay_button(
        commands,
        "Spectate",
        PauseAction::Spectate,
        true,
        fonts,
        theme,
        Some(1),
    );
    commands.entity(btn_row).add_children(&[menu_btn, spec_btn]);
}

// ── Spectator HUD ──

fn spawn_spectator_hud(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    faction_stats: &FactionStats,
) {
    let root = commands
        .spawn((
            SpectatorHudRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(36.0),
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(16.0)),
                column_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            ZIndex(90),
        ))
        .id();

    // "SPECTATING" label
    let label = commands
        .spawn((
            Text::new("SPECTATING"),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.warning),
            Node {
                margin: UiRect::right(Val::Px(16.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(root).add_child(label);

    // Per-faction mini stats
    for faction in Faction::PLAYERS.iter() {
        let count = faction_stats
            .stats
            .get(faction)
            .map(|s| s.unit_count + s.building_count)
            .unwrap_or(0);

        let stat = commands
            .spawn((Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            },))
            .id();

        let swatch = commands
            .spawn((
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(faction.color()),
            ))
            .id();

        let text = commands
            .spawn((
                SpectatorStatsText,
                Text::new(format!("{}", count)),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();

        commands.entity(stat).add_children(&[swatch, text]);
        commands.entity(root).add_child(stat);
    }

    // ESC hint
    let hint = commands
        .spawn((
            Text::new("ESC for menu"),
            TextFont {
                font: fonts.body.clone(),
                font_size: theme.typography.tiny,
                ..default()
            },
            TextColor(theme.colors.text_disabled),
            Node {
                margin: UiRect::left(Val::Px(16.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(root).add_child(hint);
}

// ── Update Faction Stats ──

fn update_faction_stats(
    time: Res<Time>,
    mut stats_timer: ResMut<StatsTimer>,
    units: Query<&Faction, With<Unit>>,
    buildings: Query<&Faction, With<Building>>,
    mut faction_stats: ResMut<FactionStats>,
) {
    stats_timer.timer.tick(time.delta());
    stats_timer.game_start.tick(time.delta());

    if !stats_timer.timer.just_finished() {
        return;
    }

    for faction in Faction::PLAYERS.iter() {
        let unit_count = units.iter().filter(|f| **f == *faction).count() as u32;
        let building_count = buildings.iter().filter(|f| **f == *faction).count() as u32;
        let eliminated =
            stats_timer.game_start.is_finished() && unit_count == 0 && building_count == 0;

        faction_stats.stats.insert(
            *faction,
            FactionStatus {
                unit_count,
                building_count,
                eliminated,
            },
        );
    }
}

// ── Check Player Elimination ──

fn check_player_elimination(
    mut commands: Commands,
    overlay: Res<InGameOverlay>,
    active_player: Res<ActivePlayer>,
    faction_stats: Res<FactionStats>,
    fonts: Res<UiFonts>,
    theme: Res<Theme>,
    net_role: Res<NetRole>,
    net_map: Option<Res<EntityNetMap>>,
) {
    if *overlay != InGameOverlay::None {
        return;
    }

    // A network client can enter InGame before the first replicated spawns land.
    // Avoid showing defeat while the client world is still empty.
    if *net_role == NetRole::Client
        && net_map
            .as_ref()
            .map(|map| map.to_ecs.is_empty())
            .unwrap_or(true)
    {
        return;
    }

    if let Some(status) = faction_stats.stats.get(&active_player.0) {
        if status.eliminated {
            commands.insert_resource(InGameOverlay::DeathScreen);
            spawn_death_screen(&mut commands, &fonts, &theme, &faction_stats);
        }
    }
}

// ── Update Spectator Stats Display ──

fn update_spectator_stats_display(
    overlay: Res<InGameOverlay>,
    faction_stats: Res<FactionStats>,
    _spectator_roots: Query<&Children, With<SpectatorHudRoot>>,
    mut text_writer: Query<&mut Text, With<SpectatorStatsText>>,
) {
    if *overlay != InGameOverlay::Spectating {
        return;
    }

    // Simple approach: update text content based on current stats
    let mut faction_idx = 0;
    for mut text in &mut text_writer {
        if faction_idx < Faction::PLAYERS.len() {
            let faction = &Faction::PLAYERS[faction_idx];
            let count = faction_stats
                .stats
                .get(faction)
                .map(|s| s.unit_count + s.building_count)
                .unwrap_or(0);
            **text = format!("{}", count);
            faction_idx += 1;
        }
    }
}

// ── Pause Menu Keyboard Navigation ──

fn pause_keyboard_nav(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut nav: ResMut<PauseNavFocus>,
    mut click_events: MessageWriter<UiClickEvent>,
    focusables: Query<(Entity, &NavFocusable), With<PauseMenuButton>>,
    mut commands: Commands,
    focused_q: Query<Entity, (With<NavFocused>, With<PauseMenuButton>)>,
    overlay: Res<InGameOverlay>,
) {
    // Only navigate when a pause panel is visible
    if !matches!(
        *overlay,
        InGameOverlay::PauseMenu
            | InGameOverlay::PauseOptions
            | InGameOverlay::PauseConfirmEndMatch
            | InGameOverlay::DeathScreen
    ) {
        return;
    }

    let mut items: Vec<(Entity, usize)> = focusables.iter().map(|(e, nf)| (e, nf.0)).collect();
    if items.is_empty() {
        return;
    }
    items.sort_by_key(|&(_, order)| order);
    let count = items.len();

    let up = keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW);
    let down = keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS);
    let confirm =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);

    if up {
        nav.index = if nav.index == 0 {
            count - 1
        } else {
            nav.index - 1
        };
    }
    if down {
        nav.index = (nav.index + 1) % count;
    }
    nav.index = nav.index.min(count - 1);

    if up || down {
        for e in &focused_q {
            commands.entity(e).remove::<NavFocused>();
        }
        let (entity, _) = items[nav.index];
        commands.entity(entity).insert(NavFocused);
    }

    if focused_q.is_empty() {
        let (entity, _) = items[nav.index];
        commands.entity(entity).insert(NavFocused);
    }

    if confirm {
        let (entity, _) = items[nav.index];
        click_events.write(UiClickEvent { entity });
    }
}

fn pause_nav_focus_visuals(
    focused: Query<Entity, (Added<NavFocused>, With<PauseMenuButton>)>,
    all_focusable: Query<Entity, (With<NavFocusable>, With<PauseMenuButton>)>,
    nav_focused_q: Query<&NavFocused>,
    mut commands: Commands,
    children_q: Query<&Children>,
    mut text_colors: Query<&mut TextColor>,
    theme: Res<Theme>,
) {
    if focused.is_empty() {
        return;
    }

    for entity in &focused {
        commands.entity(entity).insert((
            BorderColor::all(theme.colors.accent),
            BoxShadow::new(
                theme.colors.accent.with_alpha(0.4),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(10.0),
            ),
        ));
        if let Ok(children) = children_q.get(entity) {
            for child in children.iter() {
                if let Ok(mut tc) = text_colors.get_mut(child) {
                    tc.0 = Color::WHITE;
                }
            }
        }
    }

    for entity in &all_focusable {
        if nav_focused_q.get(entity).is_ok() {
            continue;
        }
        commands
            .entity(entity)
            .insert(BorderColor::all(Color::NONE));
        commands.entity(entity).remove::<BoxShadow>();
    }
}

fn spawn_pause_controls_hint(
    commands: &mut Commands,
    panel: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let hint = commands
        .spawn((
            ControlsHint,
            Node {
                width: Val::Percent(100.0),
                margin: UiRect::top(Val::Px(16.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(12.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            let items = [("W/S", "Navigate"), ("Enter", "Confirm"), ("Esc", "Close")];
            for (key, action) in items {
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Node {
                                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                // border_radius: BorderRadius::all(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.9)),
                            BorderColor::all(theme.colors.text_disabled),
                        ))
                        .with_children(|badge| {
                            badge.spawn((
                                Text::new(key),
                                TextFont {
                                    font: fonts.body_emphasis.clone(),
                                    font_size: theme.typography.tiny,
                                    ..default()
                                },
                                TextColor(theme.colors.text_secondary),
                            ));
                        });
                        row.spawn((
                            Text::new(action),
                            TextFont {
                                font: fonts.body.clone(),
                                font_size: theme.typography.tiny,
                                ..default()
                            },
                            TextColor(theme.colors.text_disabled),
                        ));
                    });
            }
        })
        .id();
    commands.entity(panel).add_child(hint);
}

// ── Cleanup Game World ──

fn cleanup_game_world(
    mut commands: Commands,
    game_entities: Query<Entity, With<GameWorld>>,
    pause_roots: Query<Entity, With<PauseOverlayRoot>>,
    death_roots: Query<Entity, With<DeathScreenRoot>>,
    spectator_roots: Query<Entity, With<SpectatorHudRoot>>,
) {
    // ── Despawn entities ──
    for e in &game_entities {
        commands.entity(e).try_despawn();
    }
    for e in &pause_roots {
        commands.entity(e).try_despawn();
    }
    for e in &death_roots {
        commands.entity(e).try_despawn();
    }
    for e in &spectator_roots {
        commands.entity(e).try_despawn();
    }

    // ── Reset all game-state resources ──
    // Most of these are initialized via init_resource (which only inserts if
    // absent), so they MUST be reset here to get fresh state on the next game.

    // Core game state
    commands.insert_resource(ActivePlayer::default());
    commands.insert_resource(AllPlayerResources::default());
    commands.insert_resource(AllCompletedBuildings::default());
    commands.insert_resource(FactionBaseState::default());
    commands.insert_resource(TeamConfig::default());
    commands.insert_resource(FactionColors::default());
    commands.insert_resource(AiControlledFactions::default());
    commands.insert_resource(VictoryState::default());
    commands.insert_resource(FactionAges::default());

    // AI
    commands.insert_resource(AiState::default());
    commands.insert_resource(AiWorldSnapshot::default());
    commands.insert_resource(AllyNotifications::default());
    commands.insert_resource(AiFactionSettings::default());

    // Combat & unit AI
    commands.insert_resource(DecisionTimer::default());
    commands.insert_resource(CombatHotspots::default());
    commands.insert_resource(CombatTuning::default());
    commands.insert_resource(CombatBudget::default());
    commands.insert_resource(CombatStatsDebug::default());
    commands.insert_resource(MeleeSlotCache::default());
    commands.insert_resource(CombatBudgetState::default());

    // Economy
    commands.insert_resource(CarriedResourceTotals::default());
    commands.insert_resource(PendingCarriedDrains::default());

    // Pathfinding & spatial
    commands.insert_resource(PathRequestQueue::default());
    commands.insert_resource(NavGridDirty::default());
    commands.insert_resource(SpatialHashGrid::default());
    commands.insert_resource(WallSpatialGrid::default());

    // Fog of war
    commands.insert_resource(FogTweakSettings::default());
    commands.remove_resource::<FogOfWarMap>();
    commands.remove_resource::<FogTextures>();
    commands.remove_resource::<FogTextureUploadState>();

    // Selection & input
    commands.insert_resource(DragState::default());
    commands.insert_resource(CommandMode::default());
    commands.insert_resource(SubgroupCycleState::default());
    commands.insert_resource(DoubleClickDetector::default());
    commands.insert_resource(UiClickedThisFrame::default());
    commands.insert_resource(UiPressActive::default());
    commands.insert_resource(NextTaskId::default());
    commands.insert_resource(InspectedEnemy::default());
    commands.insert_resource(ActiveFormation::default());
    commands.insert_resource(ControlGroupState::default());

    // UI state
    commands.insert_resource(InGameOverlay::default());
    commands.insert_resource(GameEventLog::default());
    commands.insert_resource(EventLogRenderState::default());
    commands.insert_resource(ControlGroups::default());
    commands.insert_resource(HintState::default());
    commands.insert_resource(WidgetRegistry::default());
    commands.insert_resource(WidgetResizeState::default());
    commands.insert_resource(WidgetDragState::default());
    commands.insert_resource(GridInteractionActive::default());
    commands.insert_resource(WidgetEditState::default());

    // Buildings & terrain
    commands.insert_resource(BuildingPlacementState::default());
    commands.insert_resource(WallGrid::default());
    commands.insert_resource(FloorGrid::default());
    commands.insert_resource(TerrainShapeUpdateQueue::default());
    commands.insert_resource(TerrainShapeSyncState::default());
    commands.insert_resource(TerrainSurfaceDirtyQueue::default());

    // Pause menu
    commands.insert_resource(FactionStats::default());
    commands.insert_resource(StatsTimer::default());
    commands.insert_resource(PauseNavFocus::default());

    // Remove non-Default resources (recreated in OnEnter(InGame))
    commands.remove_resource::<NavGrid>();
    commands.remove_resource::<MatchStartTime>();
}
