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
    StartMultiplayer,
    BackToMultiplayer,
    CopySessionCode,
    CancelHost,
    Disconnect,
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
pub(crate) struct CopyCodeButton;

#[derive(Component)]
pub(crate) struct CopyCodeLabel;

#[derive(Component)]
pub(crate) struct HostIpList;

#[derive(Component)]
pub(crate) struct HostIpListPopulated;

#[derive(Component)]
pub(crate) struct WebClientUrlText;

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
                (
                    multiplayer::update_lobby_ui,
                    multiplayer::connect_to_host_system,
                    multiplayer::copy_session_code_system,
                    multiplayer::update_web_client_url,
                )
                    .run_if(in_state(AppState::MainMenu)),
            );
    }
}
