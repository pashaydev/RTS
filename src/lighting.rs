use bevy::prelude::*;


pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_lighting, register_lighting_tweaks))
            .add_systems(Update, (advance_day_cycle, update_lighting).chain());
    }
}

fn register_lighting_tweaks(mut tweaks: ResMut<crate::debug::DebugTweaks>) {
    let cycle = DayCycle::default();

    // Day Cycle folder
    tweaks.add_float("Day Cycle", "cycle_duration", cycle.cycle_duration, 10.0, 3600.0, 10.0);
    tweaks.add_float("Day Cycle", "time", cycle.time, 0.0, 1.0, 0.01);
    tweaks.add_bool("Day Cycle", "paused", cycle.paused);
    tweaks.add_readonly("Day Cycle", "phase", &format!("{:?}", cycle.phase));

    // Sun folder
    tweaks.add_bool("Sun", "override", false);
    tweaks.add_float("Sun", "illuminance", 6000.0, 0.0, 15000.0, 100.0);
    tweaks.add_float("Sun", "color R", 0.85, 0.0, 1.0, 0.01);
    tweaks.add_float("Sun", "color G", 0.80, 0.0, 1.0, 0.01);
    tweaks.add_float("Sun", "color B", 0.70, 0.0, 1.0, 0.01);
    tweaks.add_float("Sun", "pitch", -0.8, -1.5, 0.0, 0.01);
    tweaks.add_float("Sun", "yaw", SUN_YAW, -3.14, 3.14, 0.01);
    tweaks.add_bool("Sun", "shadows", true);

    // Ambient folder
    tweaks.add_bool("Ambient", "override", false);
    tweaks.add_float("Ambient", "brightness", 300.0, 0.0, 1000.0, 5.0);
    tweaks.add_float("Ambient", "color R", 0.9, 0.0, 1.0, 0.01);
    tweaks.add_float("Ambient", "color G", 0.85, 0.0, 1.0, 0.01);
    tweaks.add_float("Ambient", "color B", 0.80, 0.0, 1.0, 0.01);

    // Sky / Fog folder
    tweaks.add_bool("Sky / Fog", "override", false);
    tweaks.add_float("Sky / Fog", "color R", 0.6, 0.0, 1.0, 0.01);
    tweaks.add_float("Sky / Fog", "color G", 0.65, 0.0, 1.0, 0.01);
    tweaks.add_float("Sky / Fog", "color B", 0.75, 0.0, 1.0, 0.01);
}

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

#[derive(Component)]
pub struct SunLight;

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
}

// Keyframe times
const KF_TIMES: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

// Sun illuminance
const KF_SUN_ILLUM: [f32; 5] = [0.0, 3000.0, 6000.0, 3000.0, 0.0];

// Sun color (RGB)
const KF_SUN_COLOR: [[f32; 3]; 5] = [
    [1.0, 0.6, 0.3],
    [1.0, 0.6, 0.3],
    [0.85, 0.8, 0.7],
    [1.0, 0.5, 0.2],
    [1.0, 0.6, 0.3],
];

// Sun pitch (radians)
const KF_SUN_PITCH: [f32; 5] = [-0.15, -0.15, -0.8, -0.15, -0.15];

// Ambient brightness
const KF_AMB_BRIGHT: [f32; 5] = [80.0, 180.0, 300.0, 180.0, 80.0];

// Ambient color (RGB)
const KF_AMB_COLOR: [[f32; 3]; 5] = [
    [0.15, 0.15, 0.3],
    [0.5, 0.45, 0.5],
    [0.9, 0.85, 0.8],
    [0.5, 0.35, 0.4],
    [0.15, 0.15, 0.3],
];

// Fog / sky color (RGB)
const KF_FOG_COLOR: [[f32; 3]; 5] = [
    [0.05, 0.05, 0.15],
    [0.5, 0.4, 0.5],
    [0.6, 0.65, 0.75],
    [0.55, 0.35, 0.4],
    [0.05, 0.05, 0.15],
];

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

fn advance_day_cycle(mut cycle: ResMut<DayCycle>, time: Res<Time>) {
    if cycle.paused {
        return;
    }
    cycle.time += time.delta_secs() / cycle.cycle_duration;
    cycle.time = cycle.time.rem_euclid(1.0);
    cycle.phase = phase_from_time(cycle.time);
}

fn update_lighting(
    cycle: Res<DayCycle>,
    overrides: Res<LightingOverrides>,
    mut sun_q: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
) {
    let t = cycle.time;

    // Sun
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

    // Ambient
    ambient.brightness = overrides
        .ambient_brightness
        .unwrap_or_else(|| sample_f32(&KF_TIMES, &KF_AMB_BRIGHT, t));
    let ac = overrides
        .ambient_color
        .unwrap_or_else(|| sample_rgb(&KF_TIMES, &KF_AMB_COLOR, t));
    ambient.color = Color::srgb(ac[0], ac[1], ac[2]);

    // Fog & sky color
    let fc = overrides
        .fog_color
        .unwrap_or_else(|| sample_rgb(&KF_TIMES, &KF_FOG_COLOR, t));
    clear.0 = Color::srgb(fc[0], fc[1], fc[2]);
}
