pub(crate) mod helpers;
mod multiplayer;
pub(crate) mod options;
mod pages;
mod systems;

use bevy::prelude::*;

use crate::components::*;
use crate::ui::core::text_input;

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
    LoadGame,
}

#[derive(Component)]
pub(crate) struct MenuRoot;

#[derive(Component)]
pub(crate) struct MenuCamera;

#[derive(Component)]
pub(crate) struct MenuContentRoot;

#[derive(Component)]
pub(crate) struct MenuButton(pub(crate) MenuAction);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    NewGame,
    LoadGame,
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
    CancelHost,
    Disconnect,
    LoadSave(i64),
    DeleteSave(i64),
}

#[allow(dead_code)]
#[derive(Component)]
pub(crate) struct SlotCardContainer(pub(crate) usize);

#[derive(Component)]
pub(crate) struct LobbyStatusText;

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

/// Inserted to signal that the menu content should be rebuilt without a page change.
#[derive(Resource)]
pub(crate) struct MenuDirty;

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
pub(crate) const STARTING_RES_OPTIONS: &[(f32, &str)] = &[(0.5, "0.5x"), (1.0, "1x"), (2.0, "2x")];
pub(crate) const BRIGHTNESS_OPTIONS: &[(f32, &str)] = &[
    (0.80, "80%"),
    (0.90, "90%"),
    (1.00, "100%"),
    (1.10, "110%"),
    (1.25, "125%"),
    (1.50, "150%"),
];
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

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum MenuSet {
    Input,
    Refresh,
    Networking,
    Visuals,
}

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuPage>()
            .init_resource::<MenuNavFocus>()
            .init_resource::<helpers::SliderDragState>()
            .configure_sets(
                Update,
                (
                    MenuSet::Input,
                    MenuSet::Refresh,
                    MenuSet::Networking,
                    MenuSet::Visuals,
                )
                    .chain()
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                OnEnter(AppState::MainMenu),
                multiplayer::cleanup_network_on_enter_menu.before(systems::spawn_menu),
            )
            .add_systems(
                OnEnter(AppState::MainMenu),
                systems::apply_window_settings_on_menu_enter,
            )
            .add_systems(OnEnter(AppState::MainMenu), systems::spawn_menu)
            .add_systems(
                Update,
                (
                    systems::handle_menu_buttons,
                    systems::handle_selector_clicks,
                    systems::volume_slider_system,
                    systems::menu_keyboard_nav,
                    systems::menu_selector_keyboard_nav,
                    systems::reset_nav_focus_on_page_change,
                )
                    .in_set(MenuSet::Input),
            )
            .add_systems(
                Update,
                (systems::refresh_menu_page, systems::rebuild_dirty_menu).in_set(MenuSet::Refresh),
            )
            .add_systems(
                Update,
                (text_input::text_input_system, systems::random_name_system).in_set(MenuSet::Input),
            )
            .add_systems(
                Update,
                (
                    text_input::scroll_panel_system,
                    systems::randomize_seed_system,
                )
                    .in_set(MenuSet::Input),
            )
            .add_systems(
                Update,
                (
                    multiplayer::update_lobby_ui,
                    systems::sync_range_slider_visuals,
                )
                    .in_set(MenuSet::Visuals),
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
                    .in_set(MenuSet::Networking),
            )
            .add_systems(
                Update,
                (
                    systems::update_selector_visuals,
                    text_input::animate_text_input_chrome,
                    text_input::text_input_cursor_blink,
                    text_input::text_input_render_system,
                    multiplayer::update_web_client_url,
                    multiplayer::paste_code_system,
                    multiplayer::clear_code_system,
                    multiplayer::copy_reset_system,
                    multiplayer::connection_timer_system,
                    multiplayer::countdown_system,
                    multiplayer::kick_player_system,
                    multiplayer::lobby_ping_system,
                    systems::menu_nav_focus_visuals,
                )
                    .in_set(MenuSet::Visuals),
            );
    }
}
