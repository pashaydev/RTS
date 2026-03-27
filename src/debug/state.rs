use bevy::prelude::*;

#[derive(Resource)]
pub struct DebugViewState {
    pub fps_overlay: bool,
    pub inspector: bool,
    pub frustum_culling: bool,
}

impl Default for DebugViewState {
    fn default() -> Self {
        Self {
            fps_overlay: false,
            inspector: false,
            frustum_culling: false,
        }
    }
}

#[derive(Resource, Default)]
pub struct DebugPanelState {
    pub tweaks_expanded: bool,
    pub collapsed_folders: Vec<String>,
    pub seen_folders: Vec<String>,
}

#[derive(Resource)]
pub struct FpsTracker {
    pub frame_count: u32,
    pub elapsed: f32,
    pub fps: f32,
    pub frame_time_ms: f32,
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self {
            frame_count: 0,
            elapsed: 0.0,
            fps: 0.0,
            frame_time_ms: 0.0,
        }
    }
}

#[derive(Resource, Default)]
pub struct TweakStructureVersion {
    pub version: u64,
    pub last_folder_count: usize,
    pub last_entry_counts: Vec<usize>,
}

#[derive(Resource, Default)]
pub struct ActiveSlider {
    pub folder: Option<String>,
    pub label: Option<String>,
}

impl ActiveSlider {
    pub fn is_dragging(&self, folder: &str, label: &str) -> bool {
        self.folder.as_deref() == Some(folder) && self.label.as_deref() == Some(label)
    }
}

#[derive(Resource, Default)]
pub struct DebugButtonPressed {
    pub pressed: Vec<(String, String)>,
}

#[derive(Resource)]
pub struct SaveConfigFeedback(pub Timer);

#[derive(Resource)]
pub struct ConfigApplied;

#[derive(Resource, Default)]
pub struct DebugSpawnState {
    pub click_to_spawn: bool,
    pub status_text: String,
    pub status_timer: f32,
}
