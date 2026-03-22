//! Victory / defeat condition checking and UI overlay.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::BuildingState;
use crate::components::*;
use crate::multiplayer::NetRole;
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};

pub struct VictoryPlugin;

impl Plugin for VictoryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VictoryState::default())
            .add_systems(
                Update,
                (
                    victory_check_system,
                    victory_ui_spawn_system,
                    victory_ui_button_system,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Resources ──

/// Grace period before a faction without bases is eliminated.
const GRACE_PERIOD_SECS: f32 = 60.0;
/// How often to run the victory check (seconds).
const CHECK_INTERVAL_SECS: f32 = 5.0;

#[derive(Clone, PartialEq, Debug)]
pub enum FactionStatus {
    Alive,
    /// Faction lost all bases; timer counts down before elimination.
    GracePeriod { remaining: f32 },
    Eliminated,
}

#[derive(Resource)]
pub struct VictoryState {
    pub faction_status: HashMap<Faction, FactionStatus>,
    pub check_timer: Timer,
    pub game_over: bool,
    pub winner: Option<Faction>,
    pub winner_team: Option<u8>,
    /// Prevents spawning the overlay more than once.
    pub overlay_spawned: bool,
    /// Queued network events to broadcast (host only). Consumed by multiplayer systems.
    pub pending_net_events: Vec<game_state::message::GameEvent>,
}

impl Default for VictoryState {
    fn default() -> Self {
        Self {
            faction_status: HashMap::new(),
            check_timer: Timer::from_seconds(CHECK_INTERVAL_SECS, TimerMode::Repeating),
            game_over: false,
            winner: None,
            winner_team: None,
            overlay_spawned: false,
            pending_net_events: Vec::new(),
        }
    }
}

// ── Marker components ──

#[derive(Component)]
struct VictoryOverlay;

#[derive(Component)]
struct VictoryMenuButton;

// ── Systems ──

fn victory_check_system(
    time: Res<Time>,
    net_role: Res<NetRole>,
    mut victory: ResMut<VictoryState>,
    teams: Res<TeamConfig>,
    _active_player: Res<ActivePlayer>,
    _ai_factions: Res<AiControlledFactions>,
    buildings: Query<(&EntityKind, &Faction, &BuildingState), With<Building>>,
    all_resources: Res<AllPlayerResources>,
    mut event_log: ResMut<GameEventLog>,
    game_config: Res<GameSetupConfig>,
) {
    if victory.game_over {
        return;
    }

    victory.check_timer.tick(time.delta());
    if !victory.check_timer.just_finished() {
        return;
    }

    // Determine which factions are active in this match
    let active_factions: Vec<Faction> = game_config
        .active_factions()
        .into_iter()
        .map(|i| Faction::PLAYERS[i])
        .collect();

    if active_factions.len() < 2 {
        return; // Need at least 2 factions for victory conditions
    }

    // Initialize faction statuses on first check
    if victory.faction_status.is_empty() {
        for &faction in &active_factions {
            victory.faction_status.insert(faction, FactionStatus::Alive);
        }
    }

    // Count completed bases per faction
    let mut base_counts: HashMap<Faction, u32> = HashMap::new();
    for (kind, faction, state) in &buildings {
        if *kind == EntityKind::Base && *state == BuildingState::Complete {
            *base_counts.entry(*faction).or_default() += 1;
        }
    }

    // Base cost for rebuild check
    let base_cost = crate::blueprints::ResourceCost::new()
        .with(ResourceType::Wood, 90)
        .with(ResourceType::Iron, 15);

    // Update faction statuses
    let delta = time.delta_secs();
    let mut newly_eliminated = Vec::new();

    for &faction in &active_factions {
        let bases = base_counts.get(&faction).copied().unwrap_or(0);
        let status = victory
            .faction_status
            .entry(faction)
            .or_insert(FactionStatus::Alive);

        match status {
            FactionStatus::Alive => {
                if bases == 0 {
                    *status = FactionStatus::GracePeriod {
                        remaining: GRACE_PERIOD_SECS,
                    };
                    event_log.push_with_level(
                        time.elapsed_secs(),
                        format!(
                            "{} lost all bases! {}s to rebuild.",
                            faction.display_name(),
                            GRACE_PERIOD_SECS as u32
                        ),
                        EventCategory::Alert,
                        LogLevel::Warning,
                        None,
                        Some(faction),
                    );
                }
            }
            FactionStatus::GracePeriod { remaining } => {
                if bases > 0 {
                    // Rebuilt a base — back to alive
                    *status = FactionStatus::Alive;
                    event_log.push(
                        time.elapsed_secs(),
                        format!("{} rebuilt their base!", faction.display_name()),
                        EventCategory::Alert,
                        None,
                        Some(faction),
                    );
                } else {
                    *remaining -= delta * CHECK_INTERVAL_SECS; // approximate: timer fires every CHECK_INTERVAL
                    // Check if they can afford to rebuild
                    let can_rebuild = all_resources
                        .resources
                        .get(&faction)
                        .map(|r| r.can_afford_cost(&base_cost))
                        .unwrap_or(false);

                    if *remaining <= 0.0 && !can_rebuild {
                        *status = FactionStatus::Eliminated;
                        newly_eliminated.push(faction);
                    }
                }
            }
            FactionStatus::Eliminated => {}
        }
    }

    // Log eliminations and queue network events
    for faction in &newly_eliminated {
        event_log.push_with_level(
            time.elapsed_secs(),
            format!("{} has been eliminated!", faction.display_name()),
            EventCategory::Alert,
            LogLevel::Error,
            None,
            Some(*faction),
        );
        if *net_role == NetRole::Host {
            victory.pending_net_events.push(
                game_state::message::GameEvent::FactionEliminated {
                    faction_index: faction.to_net_index(),
                },
            );
        }
    }

    // Check for winner: last faction (or team) standing
    let alive_factions: Vec<Faction> = active_factions
        .iter()
        .filter(|f| {
            victory
                .faction_status
                .get(f)
                .map(|s| *s != FactionStatus::Eliminated)
                .unwrap_or(true)
        })
        .copied()
        .collect();

    if alive_factions.len() <= 1 && !active_factions.is_empty() {
        victory.game_over = true;
        if let Some(&winner) = alive_factions.first() {
            victory.winner = Some(winner);
            victory.winner_team = teams.teams.get(&winner).copied();
            event_log.push_with_level(
                time.elapsed_secs(),
                format!("{} is victorious!", winner.display_name()),
                EventCategory::Alert,
                LogLevel::Warning,
                None,
                Some(winner),
            );
            if *net_role == NetRole::Host {
                let wt = victory.winner_team;
                victory.pending_net_events.push(
                    game_state::message::GameEvent::Victory {
                        winner_faction: winner.to_net_index(),
                        winner_team: wt,
                    },
                );
            }
        }
    } else {
        // Team-based check: if all surviving factions belong to the same team
        let alive_teams: Vec<u8> = alive_factions
            .iter()
            .filter_map(|f| teams.teams.get(f).copied())
            .collect::<std::collections::HashSet<u8>>()
            .into_iter()
            .collect();

        if alive_teams.len() == 1 && !alive_factions.is_empty() {
            victory.game_over = true;
            victory.winner = Some(alive_factions[0]);
            victory.winner_team = Some(alive_teams[0]);
            event_log.push_with_level(
                time.elapsed_secs(),
                format!("Team {} is victorious!", alive_teams[0] + 1),
                EventCategory::Alert,
                LogLevel::Warning,
                None,
                None,
            );
            if *net_role == NetRole::Host {
                let wt = victory.winner_team;
                victory.pending_net_events.push(
                    game_state::message::GameEvent::Victory {
                        winner_faction: alive_factions[0].to_net_index(),
                        winner_team: wt,
                    },
                );
            }
        }
    }
}

fn victory_ui_spawn_system(
    mut commands: Commands,
    mut victory: ResMut<VictoryState>,
    active_player: Res<ActivePlayer>,
) {
    if !victory.game_over || victory.overlay_spawned {
        return;
    }
    victory.overlay_spawned = true;

    // Determine if local player won
    let is_winner = if let Some(winner) = victory.winner {
        if victory.winner_team.is_some() {
            // Team game: check if active player's faction was not eliminated
            victory
                .faction_status
                .get(&active_player.0)
                .map(|s| *s != FactionStatus::Eliminated)
                .unwrap_or(false)
        } else {
            winner == active_player.0
        }
    } else {
        false
    };

    let (title, color) = if is_winner {
        ("VICTORY!", Color::srgb(1.0, 0.85, 0.2))
    } else {
        ("DEFEAT", Color::srgb(0.9, 0.2, 0.2))
    };

    commands
        .spawn((
            VictoryOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(30.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            GlobalZIndex(100),
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new(title),
                TextFont {
                    font_size: 72.0,
                    ..default()
                },
                TextColor(color),
            ));

            // Subtitle
            let subtitle = if let Some(winner) = victory.winner {
                format!("{} wins the match", winner.display_name())
            } else {
                "Draw".to_string()
            };
            parent.spawn((
                Text::new(subtitle),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgba(0.8, 0.8, 0.8, 1.0)),
            ));

            // Return to menu button
            parent
                .spawn((
                    VictoryMenuButton,
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(32.0), Val::Px(12.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.3, 0.3, 0.3, 0.9)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Return to Menu"),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn victory_ui_button_system(
    mut commands: Commands,
    mut victory: ResMut<VictoryState>,
    interactions: Query<&Interaction, (Changed<Interaction>, With<VictoryMenuButton>)>,
    overlay: Query<Entity, With<VictoryOverlay>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            // Clean up overlay
            for entity in &overlay {
                commands.entity(entity).despawn();
            }
            // Reset victory state
            *victory = VictoryState::default();
            // Return to main menu
            next_state.set(AppState::MainMenu);
        }
    }
}

