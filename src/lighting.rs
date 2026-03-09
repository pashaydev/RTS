use std::collections::{HashMap, HashSet};

use bevy::light::{FogVolume, VolumetricFog, VolumetricLight};
use bevy::prelude::*;

use crate::components::{Building, GhostBuilding, Unit};

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EntityLightGrid>()
            .init_resource::<EntityLightConfig>()
            .add_systems(Startup, (setup_lighting, register_lighting_tweaks))
            .add_systems(
                Update,
                (advance_day_cycle, update_lighting, update_volumetric_fog).chain(),
            )
            .add_systems(
                Update,
                (update_entity_light_grid, manage_cluster_lights).chain(),
            );
    }
}

fn register_lighting_tweaks(mut tweaks: ResMut<crate::debug::DebugTweaks>) {
    let cycle = DayCycle::default();

    // Time of Day folder
    tweaks.add_float("Visuals/Time of Day", "Cycle Duration", cycle.cycle_duration, 10.0, 3600.0, 10.0);
    tweaks.add_float("Visuals/Time of Day", "Time", cycle.time, 0.0, 1.0, 0.01);
    tweaks.add_bool("Visuals/Time of Day", "Paused", cycle.paused);
    tweaks.add_readonly("Visuals/Time of Day", "Phase", &format!("{:?}", cycle.phase));

    // Sunlight folder
    tweaks.add_bool("Visuals/Sunlight", "Override", false);
    tweaks.add_float("Visuals/Sunlight", "Illuminance", 6000.0, 0.0, 15000.0, 100.0);
    tweaks.add_float("Visuals/Sunlight", "Color R", 0.85, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/Sunlight", "Color G", 0.80, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/Sunlight", "Color B", 0.70, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/Sunlight", "Pitch", -0.8, -1.5, 0.0, 0.01);
    tweaks.add_float("Visuals/Sunlight", "Yaw", SUN_YAW, -3.14, 3.14, 0.01);
    tweaks.add_bool("Visuals/Sunlight", "Shadows", true);

    // Ambient Light folder
    tweaks.add_bool("Visuals/Ambient Light", "Override", false);
    tweaks.add_float("Visuals/Ambient Light", "Brightness", 300.0, 0.0, 1000.0, 5.0);
    tweaks.add_float("Visuals/Ambient Light", "Color R", 0.9, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/Ambient Light", "Color G", 0.85, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/Ambient Light", "Color B", 0.80, 0.0, 1.0, 0.01);

    // Sky Color folder
    tweaks.add_bool("Visuals/Sky Color", "Override", false);
    tweaks.add_float("Visuals/Sky Color", "Color R", 0.6, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/Sky Color", "Color G", 0.65, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/Sky Color", "Color B", 0.75, 0.0, 1.0, 0.01);

    // Entity Lights folder
    tweaks.add_bool("Visuals/Entity Lights", "Enabled", true);
    tweaks.add_float("Visuals/Entity Lights", "Cell Size", 15.0, 5.0, 30.0, 1.0);
    tweaks.add_float("Visuals/Entity Lights", "Max Lights", 64.0, 8.0, 128.0, 4.0);
    tweaks.add_float("Visuals/Entity Lights", "Building Intensity", 150000.0, 0.0, 500000.0, 5000.0);
    tweaks.add_float("Visuals/Entity Lights", "Unit Intensity", 80000.0, 0.0, 300000.0, 5000.0);
    tweaks.add_float("Visuals/Entity Lights", "Night Factor", 1.0, 0.0, 1.0, 0.05);
    tweaks.add_float("Visuals/Entity Lights", "Day Factor", 0.3, 0.0, 1.0, 0.05);
    tweaks.add_readonly("Visuals/Entity Lights", "Active Lights", "0");
}

// ── Day/Night Cycle ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayPhase {
    Night,
    Dawn,
    Day,
    Dusk,
}

#[derive(Resource)]
pub struct DayCycle {
    pub time: f32,
    pub cycle_duration: f32,
    pub paused: bool,
    pub phase: DayPhase,
}

impl Default for DayCycle {
    fn default() -> Self {
        Self {
            time: 0.35,
            cycle_duration: 600.0,
            paused: false,
            phase: DayPhase::Day,
        }
    }
}

fn phase_from_time(t: f32) -> DayPhase {
    match t {
        t if t < 0.20 => DayPhase::Night,
        t if t < 0.30 => DayPhase::Dawn,
        t if t < 0.70 => DayPhase::Day,
        t if t < 0.80 => DayPhase::Dusk,
        _ => DayPhase::Night,
    }
}

// ── Sun / Ambient / Fog markers ──

#[derive(Component)]
pub struct SunLight;

#[derive(Component)]
pub struct AtmosphericFogVolume;

#[derive(Resource, Default)]
pub struct LightingOverrides {
    pub sun_illuminance: Option<f32>,
    pub sun_color: Option<[f32; 3]>,
    pub sun_pitch: Option<f32>,
    pub sun_yaw: Option<f32>,
    pub shadows_enabled: Option<bool>,
    pub ambient_brightness: Option<f32>,
    pub ambient_color: Option<[f32; 3]>,
    pub fog_color: Option<[f32; 3]>,
    pub vol_density: Option<f32>,
    pub vol_color: Option<[f32; 3]>,
    pub vol_ambient_intensity: Option<f32>,
    pub vol_light_intensity: Option<f32>,
}

// ── Keyframe data ──

const KF_TIMES: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

const KF_SUN_ILLUM: [f32; 5] = [0.0, 3000.0, 6000.0, 3000.0, 0.0];

const KF_SUN_COLOR: [[f32; 3]; 5] = [
    [1.0, 0.6, 0.3],
    [1.0, 0.6, 0.3],
    [0.85, 0.8, 0.7],
    [1.0, 0.5, 0.2],
    [1.0, 0.6, 0.3],
];

const KF_SUN_PITCH: [f32; 5] = [-0.15, -0.15, -0.8, -0.15, -0.15];

const KF_AMB_BRIGHT: [f32; 5] = [80.0, 180.0, 300.0, 180.0, 80.0];

const KF_AMB_COLOR: [[f32; 3]; 5] = [
    [0.15, 0.15, 0.3],
    [0.5, 0.45, 0.5],
    [0.9, 0.85, 0.8],
    [0.5, 0.35, 0.4],
    [0.15, 0.15, 0.3],
];

const KF_FOG_COLOR: [[f32; 3]; 5] = [
    [0.05, 0.05, 0.15],
    [0.5, 0.4, 0.5],
    [0.6, 0.65, 0.75],
    [0.55, 0.35, 0.4],
    [0.05, 0.05, 0.15],
];

// ── Interpolation helpers ──

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn sample_f32(times: &[f32; 5], values: &[f32; 5], t: f32) -> f32 {
    let t = t.rem_euclid(1.0);
    for i in 0..4 {
        if t <= times[i + 1] {
            let seg = (t - times[i]) / (times[i + 1] - times[i]);
            let s = smoothstep(seg);
            return values[i] * (1.0 - s) + values[i + 1] * s;
        }
    }
    values[4]
}

fn sample_rgb(times: &[f32; 5], values: &[[f32; 3]; 5], t: f32) -> [f32; 3] {
    let t = t.rem_euclid(1.0);
    for i in 0..4 {
        if t <= times[i + 1] {
            let seg = (t - times[i]) / (times[i + 1] - times[i]);
            let s = smoothstep(seg);
            return [
                values[i][0] * (1.0 - s) + values[i + 1][0] * s,
                values[i][1] * (1.0 - s) + values[i + 1][1] * s,
                values[i][2] * (1.0 - s) + values[i + 1][2] * s,
            ];
        }
    }
    values[4]
}

const SUN_YAW: f32 = 0.3;

// ── Setup ──

fn setup_lighting(mut commands: Commands) {
    commands.insert_resource(DayCycle::default());
    commands.insert_resource(LightingOverrides::default());

    // Directional light (sun)
    commands.spawn((
        SunLight,
        DirectionalLight {
            illuminance: 6000.0,
            shadows_enabled: true,
            color: Color::srgb(0.85, 0.8, 0.7),
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, SUN_YAW, 0.0)),
        VolumetricLight,
    ));

    // Ambient light
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.9, 0.85, 0.8),
        brightness: 300.0,
        affects_lightmapped_meshes: true,
    });

    // Clear color (sky)
    commands.insert_resource(ClearColor(Color::srgb(0.6, 0.65, 0.75)));
}

// ── Day cycle advancement ──

fn advance_day_cycle(mut cycle: ResMut<DayCycle>, time: Res<Time>) {
    if cycle.paused {
        return;
    }
    cycle.time += time.delta_secs() / cycle.cycle_duration;
    cycle.time = cycle.time.rem_euclid(1.0);
    cycle.phase = phase_from_time(cycle.time);
}

// ── Sun / Ambient / Sky update ──

fn update_lighting(
    cycle: Res<DayCycle>,
    overrides: Res<LightingOverrides>,
    mut sun_q: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
) {
    let t = cycle.time;

    if let Ok((mut sun, mut sun_tf)) = sun_q.single_mut() {
        sun.illuminance = overrides
            .sun_illuminance
            .unwrap_or_else(|| sample_f32(&KF_TIMES, &KF_SUN_ILLUM, t));

        let sc = overrides
            .sun_color
            .unwrap_or_else(|| sample_rgb(&KF_TIMES, &KF_SUN_COLOR, t));
        sun.color = Color::srgb(sc[0], sc[1], sc[2]);

        let pitch = overrides
            .sun_pitch
            .unwrap_or_else(|| sample_f32(&KF_TIMES, &KF_SUN_PITCH, t));
        let yaw = overrides.sun_yaw.unwrap_or(SUN_YAW);
        *sun_tf = Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, pitch, yaw, 0.0));

        if let Some(shadows) = overrides.shadows_enabled {
            sun.shadows_enabled = shadows;
        }
    }

    ambient.brightness = overrides
        .ambient_brightness
        .unwrap_or_else(|| sample_f32(&KF_TIMES, &KF_AMB_BRIGHT, t));
    let ac = overrides
        .ambient_color
        .unwrap_or_else(|| sample_rgb(&KF_TIMES, &KF_AMB_COLOR, t));
    ambient.color = Color::srgb(ac[0], ac[1], ac[2]);

    let fc = overrides
        .fog_color
        .unwrap_or_else(|| sample_rgb(&KF_TIMES, &KF_FOG_COLOR, t));
    clear.0 = Color::srgb(fc[0], fc[1], fc[2]);
}

// ── Volumetric fog keyframes ──

const KF_VOL_DENSITY: [f32; 5] = [0.002, 0.005, 0.003, 0.006, 0.002];

const KF_VOL_COLOR: [[f32; 3]; 5] = [
    [0.3, 0.35, 0.5],
    [0.85, 0.7, 0.5],
    [0.75, 0.7, 0.6],
    [0.9, 0.6, 0.4],
    [0.3, 0.35, 0.5],
];

const KF_VOL_AMBIENT: [f32; 5] = [0.02, 0.06, 0.05, 0.06, 0.02];

const KF_VOL_LIGHT_INT: [f32; 5] = [0.3, 1.5, 1.0, 1.5, 0.3];

fn update_volumetric_fog(
    cycle: Res<DayCycle>,
    overrides: Res<LightingOverrides>,
    mut fog_vol_q: Query<&mut FogVolume, With<AtmosphericFogVolume>>,
    mut cam_fog_q: Query<&mut VolumetricFog>,
) {
    let t = cycle.time;

    if let Ok(mut fog_vol) = fog_vol_q.single_mut() {
        fog_vol.density_factor = overrides
            .vol_density
            .unwrap_or_else(|| sample_f32(&KF_TIMES, &KF_VOL_DENSITY, t));

        let vc = overrides
            .vol_color
            .unwrap_or_else(|| sample_rgb(&KF_TIMES, &KF_VOL_COLOR, t));
        fog_vol.fog_color = Color::srgb(vc[0], vc[1], vc[2]);

        fog_vol.light_intensity = overrides
            .vol_light_intensity
            .unwrap_or_else(|| sample_f32(&KF_TIMES, &KF_VOL_LIGHT_INT, t));
    }

    if let Ok(mut vol_fog) = cam_fog_q.single_mut() {
        vol_fog.ambient_intensity = overrides
            .vol_ambient_intensity
            .unwrap_or_else(|| sample_f32(&KF_TIMES, &KF_VOL_AMBIENT, t));
    }
}

// ══════════════════════════════════════════════════════════════════════
// Entity Cluster Lighting
// ══════════════════════════════════════════════════════════════════════

#[derive(Component)]
pub struct EntityClusterLight {
    pub cell: IVec2,
    pub fade: f32,
    pub stale_frames: u8,
}

pub struct ClusterCell {
    pub centroid: Vec3,
    pub entity_count: u32,
    pub has_building: bool,
    pub has_unit: bool,
}

#[derive(Resource)]
pub struct EntityLightGrid {
    pub cell_size: f32,
    pub max_lights: usize,
    pub cells: HashMap<IVec2, ClusterCell>,
}

impl Default for EntityLightGrid {
    fn default() -> Self {
        Self {
            cell_size: 15.0,
            max_lights: 64,
            cells: HashMap::new(),
        }
    }
}

#[derive(Resource)]
pub struct EntityLightConfig {
    pub enabled: bool,
    pub building_base_intensity: f32,
    pub unit_base_intensity: f32,
    pub night_factor: f32,
    pub day_factor: f32,
}

impl Default for EntityLightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            building_base_intensity: 150000.0,
            unit_base_intensity: 80000.0,
            night_factor: 1.0,
            day_factor: 0.3,
        }
    }
}

fn entity_light_factor(day_time: f32, config: &EntityLightConfig) -> f32 {
    let night = config.night_factor;
    let day = config.day_factor;
    match day_time {
        t if t < 0.20 => night,
        t if t < 0.30 => {
            let s = (t - 0.20) / 0.10;
            night - smoothstep(s) * (night - day)
        }
        t if t < 0.70 => day,
        t if t < 0.80 => {
            let s = (t - 0.70) / 0.10;
            day + smoothstep(s) * (night - day)
        }
        _ => night,
    }
}

fn update_entity_light_grid(
    mut grid: ResMut<EntityLightGrid>,
    units: Query<&Transform, (With<Unit>, Without<GhostBuilding>)>,
    buildings: Query<&Transform, (With<Building>, Without<GhostBuilding>)>,
) {
    grid.cells.clear();
    let inv = 1.0 / grid.cell_size;

    for tf in &units {
        let pos = tf.translation;
        let cell = IVec2::new((pos.x * inv).floor() as i32, (pos.z * inv).floor() as i32);
        let entry = grid.cells.entry(cell).or_insert(ClusterCell {
            centroid: Vec3::ZERO,
            entity_count: 0,
            has_building: false,
            has_unit: false,
        });
        entry.centroid += pos;
        entry.entity_count += 1;
        entry.has_unit = true;
    }

    for tf in &buildings {
        let pos = tf.translation;
        let cell = IVec2::new((pos.x * inv).floor() as i32, (pos.z * inv).floor() as i32);
        let entry = grid.cells.entry(cell).or_insert(ClusterCell {
            centroid: Vec3::ZERO,
            entity_count: 0,
            has_building: false,
            has_unit: false,
        });
        entry.centroid += pos;
        entry.entity_count += 1;
        entry.has_building = true;
    }

    for cell in grid.cells.values_mut() {
        cell.centroid /= cell.entity_count as f32;
    }
}

const FADE_IN_SPEED: f32 = 4.0;
const FADE_OUT_SPEED: f32 = 3.0;
const STALE_THRESHOLD: u8 = 3;
const BUILDING_LIGHT_COLOR: (f32, f32, f32) = (1.0, 0.85, 0.6);
const UNIT_LIGHT_COLOR: (f32, f32, f32) = (0.9, 0.85, 0.75);
const BUILDING_RANGE: f32 = 25.0;
const UNIT_RANGE: f32 = 15.0;
const BUILDING_Y_OFFSET: f32 = 4.0;
const UNIT_Y_OFFSET: f32 = 3.0;

fn manage_cluster_lights(
    mut commands: Commands,
    grid: Res<EntityLightGrid>,
    cycle: Res<DayCycle>,
    config: Res<EntityLightConfig>,
    time: Res<Time>,
    mut existing: Query<(Entity, &mut EntityClusterLight, &mut PointLight, &mut Transform)>,
) {
    if !config.enabled {
        for (entity, _, _, _) in &existing {
            commands.entity(entity).despawn();
        }
        return;
    }

    let factor = entity_light_factor(cycle.time, &config);
    let dt = time.delta_secs();

    // Rank cells by entity count, take top max_lights
    let mut ranked: Vec<(IVec2, &ClusterCell)> = grid.cells.iter().map(|(k, v)| (*k, v)).collect();
    ranked.sort_by(|a, b| b.1.entity_count.cmp(&a.1.entity_count));
    ranked.truncate(grid.max_lights);
    let active_cells: HashSet<IVec2> = ranked.iter().map(|(k, _)| *k).collect();

    let mut matched_cells: HashSet<IVec2> = HashSet::new();

    for (entity, mut cluster, mut light, mut tf) in &mut existing {
        if let Some(cell_data) = active_cells
            .contains(&cluster.cell)
            .then(|| grid.cells.get(&cluster.cell))
            .flatten()
        {
            // Cell is still active — reset stale counter, fade in
            matched_cells.insert(cluster.cell);
            cluster.stale_frames = 0;
            cluster.fade = (cluster.fade + dt * FADE_IN_SPEED).min(1.0);

            let (color, base_intensity, range, y_offset) = if cell_data.has_building {
                (
                    Color::srgb(BUILDING_LIGHT_COLOR.0, BUILDING_LIGHT_COLOR.1, BUILDING_LIGHT_COLOR.2),
                    config.building_base_intensity,
                    BUILDING_RANGE,
                    BUILDING_Y_OFFSET,
                )
            } else {
                (
                    Color::srgb(UNIT_LIGHT_COLOR.0, UNIT_LIGHT_COLOR.1, UNIT_LIGHT_COLOR.2),
                    config.unit_base_intensity,
                    UNIT_RANGE,
                    UNIT_Y_OFFSET,
                )
            };

            light.color = color;
            light.intensity = base_intensity * factor * cluster.fade;
            light.range = range;
            light.shadows_enabled = false;

            let target = Vec3::new(
                cell_data.centroid.x,
                cell_data.centroid.y + y_offset,
                cell_data.centroid.z,
            );
            tf.translation = tf.translation.lerp(target, 0.1);
        } else {
            // Cell no longer active — fade out after stale threshold
            cluster.stale_frames = cluster.stale_frames.saturating_add(1);
            if cluster.stale_frames >= STALE_THRESHOLD {
                cluster.fade = (cluster.fade - dt * FADE_OUT_SPEED).max(0.0);
                light.intensity = light.intensity * cluster.fade;
                if cluster.fade <= 0.01 {
                    commands.entity(entity).despawn();
                }
            }
        }
    }

    // Spawn lights for new cells
    for (cell, cell_data) in &ranked {
        if matched_cells.contains(cell) {
            continue;
        }
        let (color, base_intensity, range, y_offset) = if cell_data.has_building {
            (
                Color::srgb(BUILDING_LIGHT_COLOR.0, BUILDING_LIGHT_COLOR.1, BUILDING_LIGHT_COLOR.2),
                config.building_base_intensity,
                BUILDING_RANGE,
                BUILDING_Y_OFFSET,
            )
        } else {
            (
                Color::srgb(UNIT_LIGHT_COLOR.0, UNIT_LIGHT_COLOR.1, UNIT_LIGHT_COLOR.2),
                config.unit_base_intensity,
                UNIT_RANGE,
                UNIT_Y_OFFSET,
            )
        };

        commands.spawn((
            EntityClusterLight {
                cell: *cell,
                fade: 0.0, // starts faded out, will fade in next frame
                stale_frames: 0,
            },
            PointLight {
                color,
                intensity: base_intensity * factor * 0.0, // starts at 0
                range,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(Vec3::new(
                cell_data.centroid.x,
                cell_data.centroid.y + y_offset,
                cell_data.centroid.z,
            )),
        ));
    }
}
