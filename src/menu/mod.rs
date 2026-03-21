mod multiplayer;
mod pages;
mod systems;

use bevy::prelude::*;

use crate::components::*;
use crate::ui::menu_helpers;

// ── Resources & Components ──

#[derive(Resource, Default, PartialEq, Eq)]
pub(crate) enum MenuPage {
    #[default]
    Title,
    NewGame,
    Options,
    Multiplayer,
    HostLobby,
    JoinLobby,
}

#[derive(Component)]
pub(crate) struct MenuRoot;

#[derive(Component)]
pub(crate) struct MenuCamera;

#[derive(Component)]
pub(crate) struct MenuButton(pub(crate) MenuAction);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    NewGame,
    Options,
    Quit,
    Back,
    StartGame,
    ApplySettings,
    Multiplayer,
    HostGame,
    JoinGame,
    ConnectToHost,
    RefreshLanHosts,
    StartMultiplayer,
    BackToMultiplayer,
    CopySessionCode,
    CancelHost,
    Disconnect,
    CancelCountdown,
    KickPlayer,
}

#[derive(Component)]
pub(crate) struct SlotCardContainer(pub(crate) usize);

#[derive(Component)]
pub(crate) struct LobbyStatusText;

#[derive(Component)]
pub(crate) struct LobbyPlayerSlot(pub(crate) usize);

#[derive(Component)]
pub(crate) struct SessionCodeText;

#[derive(Component)]
pub struct SessionCodeInput;

#[derive(Component)]
pub(crate) struct DiscoverLanHostsButton;

#[derive(Component)]
pub(crate) struct DiscoveredHostsList;

#[derive(Component)]
pub(crate) struct DiscoveredHostsListPopulated;

#[derive(Component)]
pub(crate) struct DiscoveredHostButton(pub(crate) usize);

#[derive(Component)]
pub(crate) struct CopyCodeButton;

#[derive(Component)]
pub(crate) struct CopyCodeLabel;

#[derive(Component)]
pub(crate) struct HostIpList;

#[derive(Component)]
pub(crate) struct HostIpListPopulated;

#[derive(Component)]
pub(crate) struct WebClientUrlText;

#[derive(Component)]
pub(crate) struct PasteCodeButton;

#[derive(Component)]
pub(crate) struct ClearCodeButton;

#[derive(Component)]
pub(crate) struct ConnectionStateBanner;

#[derive(Component)]
pub(crate) struct ConnectionElapsedText;

#[derive(Component)]
pub(crate) struct ConnectionDotAnim;

/// Timer tracking how long a connection attempt has been running.
#[derive(Resource)]
pub(crate) struct ConnectionTimer {
    pub started: f64,
    pub dot_phase: u8,
    pub dot_timer: f32,
}

/// Timer to reset COPY button label back to "COPY" after showing "COPIED!".
#[derive(Resource)]
pub(crate) struct CopyResetTimer(pub Timer);

/// Marker for the host lobby start button text (for countdown).
#[derive(Component)]
pub(crate) struct StartButtonText;

/// Countdown state before game starts (3-2-1-GO).
#[derive(Resource)]
pub(crate) struct CountdownState {
    pub timer: Timer,
    pub current_digit: u8,
    pub broadcast_sent: bool,
}

/// Marker for the countdown overlay text.
#[derive(Component)]
pub(crate) struct CountdownOverlay;

/// Kick player button (slot index).
#[derive(Component)]
pub(crate) struct KickPlayerButton(pub usize);

/// Preferred faction selection for joining clients.
#[derive(Resource, Default)]
pub(crate) struct PreferredFaction(pub Option<u8>);

/// Marker for the lobby ping text.
#[derive(Component)]
pub(crate) struct LobbyPingText;

/// Timer for lobby ping polling.
#[derive(Resource)]
pub(crate) struct LobbyPingTimer(pub Timer);

// ── Constants ──

pub(crate) const RANDOM_NAMES: &[&str] = &[
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
pub(crate) const DAY_CYCLE_OPTIONS: &[(f32, &str)] =
    &[(300.0, "5min"), (600.0, "10min"), (1200.0, "20min")];
pub(crate) const STARTING_RES_OPTIONS: &[(f32, &str)] =
    &[(0.5, "0.5x"), (1.0, "1x"), (2.0, "2x")];
pub(crate) const RESOLUTION_OPTIONS: &[(u32, u32)] = &[(1280, 720), (1920, 1080)];
pub(crate) const UI_SCALE_OPTIONS: &[(f32, &str)] = &[
    (0.75, "75%"),
    (0.85, "85%"),
    (1.0, "100%"),
    (1.15, "115%"),
    (1.25, "125%"),
    (1.5, "150%"),
];

// ── Plugin ──

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuPage>()
            .add_systems(
                OnEnter(AppState::MainMenu),
                multiplayer::cleanup_network_on_enter_menu.before(systems::spawn_menu),
            )
            .add_systems(OnEnter(AppState::MainMenu), systems::spawn_menu)
            .add_systems(OnExit(AppState::MainMenu), systems::cleanup_menu)
            .add_systems(
                Update,
                (systems::handle_menu_buttons, systems::handle_selector_clicks)
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    systems::update_selector_visuals,
                    systems::page_transition_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    menu_helpers::text_input_system,
                    menu_helpers::text_input_cursor_blink,
                    systems::random_name_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    menu_helpers::menu_scroll_system,
                    systems::randomize_seed_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                multiplayer::update_lobby_ui
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    multiplayer::connect_to_host_system,
                    multiplayer::refresh_lan_hosts_system,
                    multiplayer::poll_lan_discovery_results_system,
                    multiplayer::select_discovered_host_system,
                    multiplayer::copy_session_code_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    multiplayer::update_web_client_url,
                    multiplayer::paste_code_system,
                    multiplayer::clear_code_system,
                    multiplayer::copy_reset_system,
                    multiplayer::connection_timer_system,
                    multiplayer::countdown_system,
                    multiplayer::kick_player_system,
                    multiplayer::lobby_ping_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            );
    }
}
