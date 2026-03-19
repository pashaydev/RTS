use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;

use crate::components::*;
use crate::fog::FogTweakSettings;
use crate::multiplayer::{HostNetState, NetRole};
use crate::net_bridge::EntityNetMap;
use crate::theme;
use crate::ui::fonts::UiFonts;

pub struct PauseMenuPlugin;

impl Plugin for PauseMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InGameOverlay>()
            .init_resource::<FactionStats>()
            .init_resource::<StatsTimer>()
            .add_systems(
                Update,
                (
                    handle_escape_key,
                    handle_pause_buttons,
                    update_spectator_stats_display,
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
    net_role: Res<NetRole>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }

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
            spawn_pause_overlay(&mut commands, &fonts, PausePanel::Main, *net_role);
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
            spawn_pause_overlay(&mut commands, &fonts, PausePanel::Main, *net_role);
        }
        InGameOverlay::PauseConfirmEndMatch => {
            *overlay = InGameOverlay::PauseMenu;
            for e in &pause_roots {
                commands.entity(e).try_despawn();
            }
            spawn_pause_overlay(&mut commands, &fonts, PausePanel::Main, *net_role);
        }
        InGameOverlay::DeathScreen => {
            // Can't dismiss death screen with Escape
        }
        InGameOverlay::Spectating => {
            *overlay = InGameOverlay::PauseMenu;
            spawn_pause_overlay(&mut commands, &fonts, PausePanel::Main, *net_role);
        }
    }
}

// ── Spawn Pause Overlay ──

fn spawn_pause_overlay(
    commands: &mut Commands,
    fonts: &UiFonts,
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
            BackgroundColor(Color::srgba(0.07, 0.07, 0.07, 0.96)),
            BorderColor::all(theme::SEPARATOR),
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
        PausePanel::Main => spawn_pause_content(commands, panel, fonts, role),
        PausePanel::Options => spawn_options_content(commands, panel, fonts),
        PausePanel::HostEndConfirm => spawn_host_end_confirm_content(commands, panel, fonts),
    }
}

fn spawn_pause_content(commands: &mut Commands, panel: Entity, fonts: &UiFonts, role: NetRole) {
    // // Title
    // let title = commands
    //     .spawn((
    //         Text::new("PAUSED"),
    //         TextFont {
    //             font: fonts.heading.clone(),
    //             font_size: theme::FONT_DISPLAY,
    //             ..default()
    //         },
    //         TextColor(theme::TEXT_PRIMARY),
    //         Node {
    //             margin: UiRect::bottom(Val::Px(24.0)),
    //             ..default()
    //         },
    //     ))
    //     .id();
    // commands.entity(panel).add_child(title);

    // Buttons
    let menu_label = if role == NetRole::Host {
        "End Match"
    } else {
        "Main Menu"
    };
    let buttons = vec![
        ("Continue", PauseAction::Continue, true),
        ("Restart", PauseAction::Restart, false),
        (menu_label, PauseAction::MainMenu, false),
        ("Quit", PauseAction::Quit, false),
    ];

    for (label, action, accent) in buttons {
        let btn = spawn_overlay_button(commands, label, action, accent, fonts);
        commands.entity(panel).add_child(btn);
    }
}

fn spawn_options_content(commands: &mut Commands, panel: Entity, fonts: &UiFonts) {
    let title = commands
        .spawn((
            Text::new("OPTIONS"),
            TextFont {
                font: fonts.heading.clone(),
                font_size: theme::FONT_HEADING,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
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
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::bottom(Val::Px(24.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(panel).add_child(placeholder);

    let btn = spawn_overlay_button(commands, "Back", PauseAction::BackFromOptions, true, fonts);
    commands.entity(panel).add_child(btn);
}

fn spawn_host_end_confirm_content(commands: &mut Commands, panel: Entity, fonts: &UiFonts) {
    let title = commands
        .spawn((
            Text::new("END MATCH FOR EVERYONE?"),
            TextFont {
                font: fonts.heading.clone(),
                font_size: theme::FONT_HEADING,
                ..default()
            },
            TextColor(theme::WARNING),
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
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::bottom(Val::Px(20.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(panel).add_child(body);

    let cancel_btn = spawn_overlay_button(commands, "Cancel", PauseAction::CancelHostEnd, false, fonts);
    let confirm_btn = spawn_overlay_button(commands, "End Match", PauseAction::ConfirmHostEnd, true, fonts);
    commands.entity(panel).add_children(&[cancel_btn, confirm_btn]);
}

fn spawn_overlay_button(
    commands: &mut Commands,
    label: &str,
    action: PauseAction,
    accent: bool,
    fonts: &UiFonts,
) -> Entity {
    let bg = if accent {
        theme::ACCENT
    } else {
        theme::BTN_PRIMARY
    };

    let btn = commands
        .spawn((
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
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(Color::NONE),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme::FONT_BUTTON,
                ..default()
            },
            TextColor(if accent {
                Color::WHITE
            } else {
                theme::TEXT_PRIMARY
            }),
        ))
        .id();

    btn
}

// ── Handle Pause Buttons ──

fn handle_pause_buttons(
    mut commands: Commands,
    interactions: Query<(&Interaction, &PauseMenuButton), Changed<Interaction>>,
    mut overlay: ResMut<InGameOverlay>,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    pause_roots: Query<Entity, With<PauseOverlayRoot>>,
    death_roots: Query<Entity, With<DeathScreenRoot>>,
    _spectator_roots: Query<Entity, With<SpectatorHudRoot>>,
    fonts: Res<UiFonts>,
    mut fog_settings: ResMut<FogTweakSettings>,
    mut fog_map: Option<ResMut<FogOfWarMap>>,
    faction_stats: Res<FactionStats>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    net_role: Res<NetRole>,
    host_state: Option<Res<HostNetState>>,
    time: Res<Time>,
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
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
                spawn_pause_overlay(&mut commands, &fonts, PausePanel::Options, *net_role);
            }
            PauseAction::Quit => {
                exit.write(AppExit::Success);
            }
            PauseAction::BackFromOptions => {
                *overlay = InGameOverlay::PauseMenu;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
                spawn_pause_overlay(&mut commands, &fonts, PausePanel::Main, *net_role);
            }
            PauseAction::ApplySettings => {
                // Handled same as BackFromOptions for now
                *overlay = InGameOverlay::PauseMenu;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
                spawn_pause_overlay(&mut commands, &fonts, PausePanel::Main, *net_role);
            }
            PauseAction::ConfirmHostEnd => {
                if *net_role == NetRole::Host {
                    if let Some(host) = host_state.as_ref() {
                        broadcast_host_shutdown(host, &time);
                    }
                }
                next_state.set(AppState::MainMenu);
            }
            PauseAction::CancelHostEnd => {
                *overlay = InGameOverlay::PauseMenu;
                for e in &pause_roots {
                    commands.entity(e).try_despawn();
                }
                spawn_pause_overlay(&mut commands, &fonts, PausePanel::Main, *net_role);
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
                        *e = true;
                    }
                    for d in fog.display.iter_mut() {
                        *d = 1.0;
                    }
                }
                // Spawn spectator HUD
                spawn_spectator_hud(&mut commands, &fonts, &faction_stats);
            }
        }
    }
}

fn broadcast_host_shutdown(host: &HostNetState, time: &Time) {
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
    if let Ok(bytes) = game_state::codec::encode(&msg) {
        let senders = host.client_senders.lock().unwrap();
        for (_id, sender) in senders.iter() {
            let _ = sender.send(bytes.clone());
        }
    }
}

// ── Death Screen ──

fn spawn_death_screen(
    commands: &mut Commands,
    fonts: &UiFonts,
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
            TextColor(theme::DESTRUCTIVE),
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
            BackgroundColor(Color::srgba(0.07, 0.07, 0.07, 0.9)),
            BorderColor::all(theme::SEPARATOR),
        ))
        .id();
    commands.entity(root).add_child(stats_panel);

    // Stats header
    let header = commands
        .spawn((
            Text::new("Battle Results"),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme::FONT_LARGE,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
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
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(theme::TEXT_PRIMARY),
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
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(theme::TEXT_SECONDARY),
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
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(theme::TEXT_SECONDARY),
                    Node {
                        width: Val::Px(80.0),
                        ..default()
                    },
                ))
                .id();

            // Status badge
            let (badge_text, badge_color) = if status.eliminated {
                ("ELIMINATED", theme::DESTRUCTIVE)
            } else {
                ("ALIVE", theme::SUCCESS)
            };
            let badge = commands
                .spawn((
                    Text::new(badge_text),
                    TextFont {
                        font: fonts.body_emphasis.clone(),
                        font_size: theme::FONT_SMALL,
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

    let menu_btn = spawn_overlay_button(commands, "Main Menu", PauseAction::MainMenu, false, fonts);
    let spec_btn = spawn_overlay_button(commands, "Spectate", PauseAction::Spectate, true, fonts);
    commands
        .entity(btn_row)
        .add_children(&[menu_btn, spec_btn]);
}

// ── Spectator HUD ──

fn spawn_spectator_hud(
    commands: &mut Commands,
    fonts: &UiFonts,
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
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::WARNING),
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
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                },
            ))
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
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
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
                font_size: theme::FONT_TINY,
                ..default()
            },
            TextColor(theme::TEXT_DISABLED),
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
        let eliminated = stats_timer.game_start.is_finished()
            && unit_count == 0
            && building_count == 0;

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
            spawn_death_screen(&mut commands, &fonts, &faction_stats);
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

// ── Cleanup Game World ──

fn cleanup_game_world(
    mut commands: Commands,
    game_entities: Query<Entity, With<GameWorld>>,
    pause_roots: Query<Entity, With<PauseOverlayRoot>>,
    death_roots: Query<Entity, With<DeathScreenRoot>>,
    spectator_roots: Query<Entity, With<SpectatorHudRoot>>,
    mut fog_settings: ResMut<FogTweakSettings>,
) {
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

    // Reset overlay
    commands.insert_resource(InGameOverlay::None);
    // Reset fog settings
    *fog_settings = FogTweakSettings::default();
    // Reset stats
    commands.insert_resource(FactionStats::default());
    commands.insert_resource(StatsTimer::default());
}
