mod abilities;
mod ages;
mod ai;
mod animation;
mod attention;
mod audio;
mod blueprints;
mod buildings;
mod camera;
mod combat;
mod components;
mod culling;
mod database;
mod debug;
mod entity_labels;
mod fog;
mod fog_material;
mod grass_material;
mod ground;
mod hover_material;
mod items;
mod lighting;
mod logging;
mod menu;
mod minimap;
mod mobs;
mod model_assets;
mod multiplayer;
mod net_bridge;
mod orders;
mod pathfinding;
mod pathvis;
mod pause_menu;
mod procedural_mobs;
mod resources;
mod save_load;
mod selection;
mod spatial;
mod terrain_material;
mod tree_occlusion_material;
mod theme;
mod ui;
mod unit_ai;
mod units;
mod vfx;
mod victory;
mod water_material;

use bevy::ecs::error;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::PresentMode;
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::OutlinePlugin;

use components::{AppState, GameFlowSet, GameSetupConfig};

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

    logging::configure_session_logging(
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
    );

    // Open database early so settings (graphics, audio) are loaded before window creation.
    let (db, profile, graphics, audio_settings) = database::init_early();

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
                        custom_layer: logging::make_tracing_layer,
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
        .insert_resource(GameSetupConfig::default())
        .insert_resource(theme::Theme::from_mode(graphics.theme_mode))
        .insert_resource(graphics)
        .insert_resource(db)
        .insert_resource(profile)
        .insert_resource(audio_settings)
        .add_plugins(logging::SessionLogPlugin)
        .add_plugins(database::DatabasePlugin)
        .add_plugins(menu::MenuPlugin)
        .add_plugins(blueprints::BlueprintPlugin)
        .add_plugins((
            debug::DebugPlugin,
            model_assets::ModelAssetsPlugin,
            ground::GroundPlugin,
            camera::CameraPlugin,
            lighting::LightingPlugin,
            units::UnitsPlugin,
            selection::SelectionPlugin,
            ui::UiPlugin,
            resources::ResourcesPlugin,
            buildings::BuildingsPlugin,
            pathvis::PathVisPlugin,
            vfx::VfxPlugin,
            mobs::MobsPlugin,
            items::ItemsPlugin,
        ))
        .add_plugins((
            combat::CombatPlugin,
            fog::FogPlugin,
        ))
        .add_plugins(combat::CombatIntentsPlugin)
        .add_plugins(combat::CombatBudgetPlugin)
        .add_plugins(combat::CombatSlotsPlugin)
        .add_plugins(abilities::AbilitiesPlugin)
        .add_plugins(spatial::SpatialPlugin)
        .add_plugins(pathfinding::PathfindingPlugin)
        .add_plugins(culling::CullingPlugin)
        .add_plugins(animation::AnimationPlugin)
        .add_plugins(procedural_mobs::ProceduralMobsPlugin)
        .add_plugins(minimap::MinimapPlugin)
        .add_plugins(attention::AttentionPlugin)
        .add_plugins(ai::AiPlugin)
        .add_plugins(unit_ai::UnitAiPlugin)
        .add_plugins(pause_menu::PauseMenuPlugin)
        .add_plugins(save_load::SaveLoadPlugin)
        .add_plugins(net_bridge::NetBridgePlugin)
        .add_plugins(multiplayer::MultiplayerPlugin)
        .add_plugins(victory::VictoryPlugin)
        .add_plugins(ages::AgesPlugin)
        .add_plugins(audio::GameAudioPlugin)
        .add_plugins(entity_labels::EntityLabelPlugin)
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
