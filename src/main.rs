mod blueprints;
mod infrastructure;
mod presentation;
mod simulation;
mod types;
mod ui;
mod world;

use bevy::ecs::error;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::time::Fixed;
use bevy::window::PresentMode;
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::OutlinePlugin;

use types::{AppState, GameFlowSet, GameRng, GameSetupConfig, MapSeed, SimClock, SimSet};

const GAMEPLAY_FIXED_HZ: f64 = 30.0;

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    // Resolve the executable's directory so assets/config/saves are found
    // correctly in distribution builds (especially Windows).
    // Skip when running inside a cargo `target/` dir (i.e. during development).
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .filter(|d| !d.components().any(|c| c.as_os_str() == "target"));

    if let Some(ref dir) = exe_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Build an absolute asset path from the exe directory so Bevy's
    // AssetServer works even when CWD is unexpected (Windows shortcuts,
    // UNC paths, etc.).
    let asset_path = exe_dir
        .as_ref()
        .map(|d| d.join("assets").to_string_lossy().into_owned())
        .unwrap_or_else(|| "assets".to_string());

    infrastructure::logging::configure_session_logging(
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
    );

    // Open database early so settings (graphics, audio) are loaded before window creation.
    let (db, profile, mut graphics, audio_settings) = infrastructure::database::init_early();

    // On WASM, use the browser viewport size so hover coordinates match from the start.
    // The DB-stored resolution is a desktop value that causes a mismatch until a resize event.
    #[cfg(target_arch = "wasm32")]
    let (w, h) = {
        let (mut w, mut h) = graphics.resolution;
        if let Some(window) = web_sys::window() {
            w = window
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(w as f64) as u32;
            h = window
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(h as f64) as u32;
        }
        graphics.resolution = (w, h);
        (w, h)
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (w, h) = graphics.resolution;

    App::new()
        .set_error_handler(error::warn)
        .add_plugins(
            {
                let plugins = DefaultPlugins
                    .set(LogPlugin {
                        custom_layer: infrastructure::logging::make_tracing_layer,
                        ..default()
                    })
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            title: "RTS Prototype".to_string(),
                            resolution: (w, h).into(),
                            mode: if graphics.fullscreen {
                                bevy::window::WindowMode::BorderlessFullscreen(
                                    bevy::window::MonitorSelection::Current,
                                )
                            } else {
                                bevy::window::WindowMode::Windowed
                            },
                            present_mode: if graphics.vsync {
                                PresentMode::AutoVsync
                            } else {
                                PresentMode::AutoNoVsync
                            },
                            fit_canvas_to_parent: true,
                            canvas: Some("canvas".to_string()),
                            ..default()
                        }),
                        ..default()
                    })
                    .set(AssetPlugin {
                        file_path: asset_path,
                        meta_check: bevy::asset::AssetMetaCheck::Never,
                        ..default()
                    });
                // Use conservative WebGPU defaults so Bevy generates simpler
                // shaders that browser WebGPU implementations can compile.
                // Chrome/Edge use Dawn which validates shader bindings strictly
                // during module creation. Request adapter-level limits so the
                // PBR shader's binding counts (textures, uniform buffers) pass.
                #[cfg(target_arch = "wasm32")]
                let plugins = plugins.set(bevy::render::RenderPlugin {
                    render_creation: bevy::render::settings::WgpuSettings {
                        // Functionality mode: request the adapter's actual
                        // limits & features (not the conservative WebGPU defaults).
                        priority: bevy::render::settings::WgpuSettingsPriority::Functionality,
                        // Disable advanced shader features that generate WGSL
                        // constructs Chrome's Dawn backend may not compile.
                        disabled_features: Some(
                            bevy::render::settings::WgpuFeatures::TEXTURE_BINDING_ARRAY
                                | bevy::render::settings::WgpuFeatures::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
                                | bevy::render::settings::WgpuFeatures::BUFFER_BINDING_ARRAY
                                | bevy::render::settings::WgpuFeatures::STORAGE_RESOURCE_BINDING_ARRAY
                                | bevy::render::settings::WgpuFeatures::STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING
                                | bevy::render::settings::WgpuFeatures::UNIFORM_BUFFER_BINDING_ARRAYS,
                        ),
                        ..default()
                    }
                    .into(),
                    ..default()
                });
                plugins
            },
        )
        // bevy_mod_outline shaders are incompatible with browser WebGPU
        .add_plugins(cfg_outline_plugin())
        .init_state::<AppState>()
        .insert_resource(Time::<Fixed>::from_hz(GAMEPLAY_FIXED_HZ))
        .configure_sets(
            Update,
            (
                GameFlowSet::Input,
                GameFlowSet::NetworkReceive,
                GameFlowSet::Simulation,
                GameFlowSet::NetworkBroadcast,
                GameFlowSet::Ui,
                GameFlowSet::Presentation,
                GameFlowSet::Diagnostics,
            )
                .chain(),
        )
        .configure_sets(
            FixedUpdate,
            (
                GameFlowSet::Input,
                GameFlowSet::NetworkReceive,
                GameFlowSet::Simulation,
                GameFlowSet::NetworkBroadcast,
                GameFlowSet::Ui,
                GameFlowSet::Presentation,
                GameFlowSet::Diagnostics,
            )
                .chain(),
        )
        // Lockstep: FixedUpdate's simulation set is gated on the
        // `advance_allowed` flag latched earlier in FixedFirst, so the tick
        // only runs when every peer's input for it is confirmed.
        .configure_sets(
            FixedUpdate,
            GameFlowSet::Simulation.run_if(infrastructure::multiplayer::lockstep_advance_allowed),
        )
        .configure_sets(
            Update,
            GameFlowSet::Simulation.run_if(not(infrastructure::multiplayer::is_online)),
        )
        .configure_sets(
            FixedUpdate,
            (
                SimSet::Ai,
                SimSet::Command,
                SimSet::Movement,
                SimSet::Combat,
                SimSet::Economy,
                SimSet::Spatial,
            )
                .chain()
                .in_set(GameFlowSet::Simulation),
        )
        .insert_resource(SimClock::default())
        .insert_resource(GameRng::default())
        .add_systems(
            FixedFirst,
            advance_sim_clock
                .run_if(in_state(AppState::InGame))
                .run_if(infrastructure::multiplayer::lockstep_advance_allowed)
                .after(infrastructure::multiplayer::lockstep_check_gate),
        )
        .add_systems(
            OnEnter(AppState::InGame),
            reseed_game_rng.after(crate::world::ground::resolve_map_seed),
        )
        .insert_resource(GameSetupConfig::default())
        .insert_resource(ui::theme::Theme::from_mode(graphics.theme_mode))
        .insert_resource(graphics)
        .insert_resource(db)
        .insert_resource(profile)
        .insert_resource(audio_settings)
        .add_plugins(infrastructure::InfraPlugins)
        .add_plugins(blueprints::BlueprintPlugin)
        .add_plugins(world::WorldPlugins)
        .add_plugins(simulation::SimulationPlugins)
        .add_plugins(presentation::PresentationPlugins)
        .add_plugins(ui::UiPlugin)
        .run();
}

#[cfg(not(target_arch = "wasm32"))]
fn cfg_outline_plugin() -> OutlinePlugin {
    OutlinePlugin
}

#[cfg(target_arch = "wasm32")]
fn cfg_outline_plugin() -> impl bevy::app::Plugin {
    |_app: &mut App| {}
}

pub(crate) fn advance_sim_clock(mut clock: ResMut<SimClock>) {
    clock.tick = clock.tick.wrapping_add(1);
}

fn reseed_game_rng(map_seed: Option<Res<MapSeed>>, mut rng: ResMut<GameRng>) {
    if let Some(seed) = map_seed {
        rng.reseed(seed.0);
    }
}
