mod dirty_state;
mod page;
mod systems;

use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor};
use std::collections::BTreeSet;

use crate::types::*;

pub(crate) use dirty_state::*;
pub(crate) use page::*;
pub(crate) use systems::*;

// ── Resolution Helpers ──

fn resolution_label(w: u32, h: u32) -> String {
    format!("{w}x{h}")
}

pub(crate) fn resolution_index(resolutions: &[(u32, u32)], resolution: (u32, u32)) -> usize {
    resolutions
        .iter()
        .position(|&r| r == resolution)
        .unwrap_or_else(|| {
            // Find closest by pixel count
            let target = resolution.0 as u64 * resolution.1 as u64;
            resolutions
                .iter()
                .enumerate()
                .min_by_key(|(_, &(w, h))| {
                    ((w as u64 * h as u64) as i64 - target as i64).unsigned_abs()
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        })
}

pub(crate) fn step_resolution_index(
    resolutions: &[(u32, u32)],
    current_index: usize,
    delta: isize,
) -> usize {
    let max_index = resolutions.len().saturating_sub(1) as isize;
    (current_index as isize + delta).clamp(0, max_index) as usize
}

/// Check if a resolution has a standard display aspect ratio (16:9, 16:10, 21:9, 4:3, 5:4, 32:9).
fn is_standard_aspect_ratio(w: u32, h: u32) -> bool {
    if h == 0 {
        return false;
    }
    let ratio = w as f64 / h as f64;
    const STANDARD_RATIOS: &[f64] = &[
        16.0 / 9.0,  // 1.778 — 1920x1080, 2560x1440, 3840x2160
        16.0 / 10.0, // 1.600 — 1920x1200, 2560x1600
        21.0 / 9.0,  // 2.333 — 3440x1440, 2560x1080
        4.0 / 3.0,   // 1.333 — 1024x768, 1600x1200
        5.0 / 4.0,   // 1.250 — 1280x1024
        32.0 / 9.0,  // 3.556 — 5120x1440 (super ultrawide)
    ];
    // Allow ~3% tolerance to account for rounding after scale factor division
    STANDARD_RATIOS.iter().any(|&std| (ratio - std).abs() / std < 0.03)
}

// ── Marker for the resolution row (greyed out when fullscreen) ──

#[derive(Component)]
pub(crate) struct ResolutionRow;

/// Inserted after monitor resolutions have been queried to avoid re-running.
#[derive(Resource)]
pub(crate) struct ResolutionsPopulated;

// ── Populate Available Resolutions from Monitor ──

pub(crate) fn populate_available_resolutions(
    mut commands: Commands,
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
    mut resolutions: ResMut<AvailableResolutions>,
) {
    let Ok(monitor) = monitors.single() else {
        return;
    };

    commands.insert_resource(ResolutionsPopulated);

    let scale = monitor.scale_factor.max(1.0);
    let native_w = (monitor.physical_width as f64 / scale).round() as u32;
    let native_h = (monitor.physical_height as f64 / scale).round() as u32;

    let mut res_set: BTreeSet<(u64, u32, u32)> = BTreeSet::new();

    // Collect unique resolutions from monitor video modes, converting to logical pixels.
    for vm in &monitor.video_modes {
        let w = (vm.physical_size.x as f64 / scale).round() as u32;
        let h = (vm.physical_size.y as f64 / scale).round() as u32;
        if w >= 1024 && h >= 720 && is_standard_aspect_ratio(w, h) {
            res_set.insert((w as u64 * h as u64, w, h));
        }
    }

    // Always include the native logical resolution (skip aspect ratio check).
    if native_w >= 1024 && native_h >= 720 {
        res_set.insert((native_w as u64 * native_h as u64, native_w, native_h));
    }

    // Add common resolutions that fit within the native size as fallbacks.
    // On HiDPI displays, video modes divided by scale factor often produce
    // non-standard values, leaving very few options. These ensure a usable list.
    const COMMON: &[(u32, u32)] = &[
        (1280, 720),
        (1280, 800),
        (1366, 768),
        (1440, 900),
        (1600, 900),
        (1600, 1000),
        (1680, 1050),
        (1920, 1080),
        (1920, 1200),
        (2560, 1440),
        (2560, 1600),
        (3440, 1440),
        (3840, 2160),
    ];
    for &(w, h) in COMMON {
        if w <= native_w && h <= native_h {
            res_set.insert((w as u64 * h as u64, w, h));
        }
    }

    if !res_set.is_empty() {
        resolutions.0 = res_set.into_iter().map(|(_, w, h)| (w, h)).collect();
    }
}

// ── Detect native resolution for first-launch default ──

pub(crate) fn detect_native_resolution(
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
    mut graphics: ResMut<GraphicsSettings>,
    resolutions: Res<AvailableResolutions>,
) {
    // Only override the default 1280x720 — if the user already changed it, respect that
    if graphics.resolution != (1280, 720) {
        return;
    }

    let Ok(monitor) = monitors.single() else {
        return;
    };

    let scale = monitor.scale_factor.max(1.0);
    let native_w = (monitor.physical_width as f64 / scale).round() as u32;
    let native_h = (monitor.physical_height as f64 / scale).round() as u32;

    // Pick the closest available resolution to the native one
    let idx = resolution_index(&resolutions.0, (native_w, native_h));
    if idx < resolutions.0.len() {
        graphics.resolution = resolutions.0[idx];
    }
}
