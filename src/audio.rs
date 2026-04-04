use bevy::audio::Volume;
use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::blueprints::EntityKind;
use crate::components::{AppState, RtsCamera};

// ── Settings (persisted) ──

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub music_volume: f32, // 0.0 – 1.0 UI slider value
    pub sfx_volume: f32,   // 0.0 – 1.0 UI slider value
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            music_volume: 0.5,
            sfx_volume: 0.7,
        }
    }
}

// AudioSettings is loaded from SQLite via database::init_early() and
// saved back by sync_settings_to_db when the resource changes.

// ── Channel markers ──

/// Marker for background music entities.
#[derive(Component)]
pub struct MusicChannel;

/// Marker for one-shot SFX entities.
#[derive(Component)]
pub struct SfxChannel;

/// Current music gameplay intensity.
#[derive(Resource, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MusicState {
    #[default]
    Menu,
    Peaceful,
    Battle,
    Victory,
    Defeat,
}

/// Fade direction for music transitions.
#[derive(Component)]
struct FadeIn {
    elapsed: f32,
    duration: f32,
    target_volume: f32,
}

#[derive(Component)]
struct FadeOut {
    elapsed: f32,
    duration: f32,
}

// ── SFX event ──

/// Fire this event from any system to trigger a sound effect.
/// The audio system will look up the appropriate clip and play it.
#[derive(bevy::ecs::message::Message)]
#[allow(dead_code)]
pub struct PlaySfx {
    pub kind: SfxKind,
    pub position: Option<Vec3>,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SfxKind {
    // Combat
    MeleeHit,
    ArrowFire,
    ArrowImpact,
    MagicCast,
    HealCast,
    CatapultLaunch,
    RamHit,
    KnightCharge,
    // Units
    UnitSelect,
    UnitMove,
    UnitAttack,
    UnitDeath,
    TrainingComplete,
    // Workers
    WorkerGather,
    // Buildings
    ConstructionHammer,
    ConstructionComplete,
    BuildingUpgrade,
    BuildingDemolish,
    BuildingDestroyed,
    // UI
    ButtonClick,
    ButtonHover,
    AgeAdvance,
    ResourceWarning,
    UnderAttack,
}

// ── Preloaded handles ──

#[derive(Resource)]
pub struct SfxLibrary {
    pub worker_gather: Handle<AudioSource>,
    pub melee_hit: Handle<AudioSource>,
    pub arrow_fire: Handle<AudioSource>,
    pub arrow_impact: Handle<AudioSource>,
    pub magic_cast: Handle<AudioSource>,
    pub heal_cast: Handle<AudioSource>,
    pub catapult_launch: Handle<AudioSource>,
    pub ram_hit: Handle<AudioSource>,
    pub knight_charge: Handle<AudioSource>,
    pub unit_select: Handle<AudioSource>,
    pub unit_move: Handle<AudioSource>,
    pub unit_attack: Handle<AudioSource>,
    pub unit_death: Handle<AudioSource>,
    pub training_complete: Handle<AudioSource>,
    pub construction_hammer: Handle<AudioSource>,
    pub construction_complete: Handle<AudioSource>,
    pub building_upgrade: Handle<AudioSource>,
    pub building_demolish: Handle<AudioSource>,
    pub building_destroyed: Handle<AudioSource>,
    pub button_click: Handle<AudioSource>,
    pub button_hover: Handle<AudioSource>,
    pub age_advance: Handle<AudioSource>,
    pub resource_warning: Handle<AudioSource>,
    pub under_attack: Handle<AudioSource>,
}

impl SfxLibrary {
    fn load(s: &AssetServer) -> Self {
        Self {
            // Workers
            worker_gather: s.load("audio/sfx/units/worker_gather.ogg"),
            // Combat
            melee_hit: s.load("audio/sfx/combat/melee_hit.ogg"),
            arrow_fire: s.load("audio/sfx/combat/arrow_fire.ogg"),
            arrow_impact: s.load("audio/sfx/combat/arrow_impact.ogg"),
            magic_cast: s.load("audio/sfx/combat/magic_cast.ogg"),
            heal_cast: s.load("audio/sfx/combat/heal_cast.ogg"),
            catapult_launch: s.load("audio/sfx/combat/catapult_launch.ogg"),
            ram_hit: s.load("audio/sfx/combat/ram_hit.ogg"),
            knight_charge: s.load("audio/sfx/combat/knight_charge.ogg"),
            // Units
            unit_select: s.load("audio/sfx/units/unit_select.ogg"),
            unit_move: s.load("audio/sfx/units/unit_move.ogg"),
            unit_attack: s.load("audio/sfx/units/unit_attack.ogg"),
            unit_death: s.load("audio/sfx/units/unit_death.ogg"),
            training_complete: s.load("audio/sfx/units/training_complete.ogg"),
            // Buildings
            construction_hammer: s.load("audio/sfx/buildings/construction_hammer.ogg"),
            construction_complete: s.load("audio/sfx/buildings/construction_complete.ogg"),
            building_upgrade: s.load("audio/sfx/buildings/building_upgrade.ogg"),
            building_demolish: s.load("audio/sfx/buildings/building_demolish.ogg"),
            building_destroyed: s.load("audio/sfx/buildings/building_destroyed.ogg"),
            // UI
            button_click: s.load("audio/sfx/ui/button_click.ogg"),
            button_hover: s.load("audio/sfx/ui/button_hover.ogg"),
            age_advance: s.load("audio/sfx/ui/age_advance.ogg"),
            resource_warning: s.load("audio/sfx/ui/resource_warning.ogg"),
            under_attack: s.load("audio/sfx/ui/under_attack.ogg"),
        }
    }

    pub fn handle_for(&self, kind: SfxKind) -> Handle<AudioSource> {
        match kind {
            SfxKind::WorkerGather => self.worker_gather.clone(),
            SfxKind::MeleeHit => self.melee_hit.clone(),
            SfxKind::ArrowFire => self.arrow_fire.clone(),
            SfxKind::ArrowImpact => self.arrow_impact.clone(),
            SfxKind::MagicCast => self.magic_cast.clone(),
            SfxKind::HealCast => self.heal_cast.clone(),
            SfxKind::CatapultLaunch => self.catapult_launch.clone(),
            SfxKind::RamHit => self.ram_hit.clone(),
            SfxKind::KnightCharge => self.knight_charge.clone(),
            SfxKind::UnitSelect => self.unit_select.clone(),
            SfxKind::UnitMove => self.unit_move.clone(),
            SfxKind::UnitAttack => self.unit_attack.clone(),
            SfxKind::UnitDeath => self.unit_death.clone(),
            SfxKind::TrainingComplete => self.training_complete.clone(),
            SfxKind::ConstructionHammer => self.construction_hammer.clone(),
            SfxKind::ConstructionComplete => self.construction_complete.clone(),
            SfxKind::BuildingUpgrade => self.building_upgrade.clone(),
            SfxKind::BuildingDemolish => self.building_demolish.clone(),
            SfxKind::BuildingDestroyed => self.building_destroyed.clone(),
            SfxKind::ButtonClick => self.button_click.clone(),
            SfxKind::ButtonHover => self.button_hover.clone(),
            SfxKind::AgeAdvance => self.age_advance.clone(),
            SfxKind::ResourceWarning => self.resource_warning.clone(),
            SfxKind::UnderAttack => self.under_attack.clone(),
        }
    }
}

/// Helper: map an EntityKind to the SFX played on attack.
#[allow(dead_code)]
pub fn attack_sfx_for(kind: EntityKind) -> SfxKind {
    match kind {
        EntityKind::Archer => SfxKind::ArrowFire,
        EntityKind::Mage | EntityKind::FireElemental => SfxKind::MagicCast,
        EntityKind::Priest => SfxKind::HealCast,
        EntityKind::Catapult => SfxKind::CatapultLaunch,
        EntityKind::BatteringRam => SfxKind::RamHit,
        EntityKind::Knight => SfxKind::KnightCharge,
        EntityKind::BallistaTower => SfxKind::ArrowFire,
        EntityKind::BombardTower => SfxKind::CatapultLaunch,
        _ => SfxKind::MeleeHit,
    }
}

#[derive(Resource)]
pub struct MusicTracks {
    pub menu: Handle<AudioSource>,
    pub peaceful: Handle<AudioSource>,
    pub battle: Handle<AudioSource>,
    pub victory: Handle<AudioSource>,
    pub defeat: Handle<AudioSource>,
}

impl MusicTracks {
    fn load(s: &AssetServer) -> Self {
        Self {
            menu: s.load("audio/music/menu_theme.ogg"),
            peaceful: s.load("audio/music/peaceful_theme.ogg"),
            battle: s.load("audio/music/battle_theme.ogg"),
            victory: s.load("audio/music/victory_theme.ogg"),
            defeat: s.load("audio/music/defeat_theme.ogg"),
        }
    }

    fn track_for(&self, state: &MusicState) -> Handle<AudioSource> {
        match state {
            MusicState::Menu => self.menu.clone(),
            MusicState::Peaceful => self.peaceful.clone(),
            MusicState::Battle => self.battle.clone(),
            MusicState::Victory => self.victory.clone(),
            MusicState::Defeat => self.defeat.clone(),
        }
    }
}

// ── Systems ──

const FADE_DURATION: f32 = 2.0;
const MIN_AUDIBLE_DB: f32 = -50.0;

fn slider_to_volume(slider_value: f32) -> Volume {
    let slider_value = slider_value.clamp(0.0, 1.0);
    if slider_value <= 0.0 {
        Volume::SILENT
    } else {
        Volume::Decibels(MIN_AUDIBLE_DB * (1.0 - slider_value))
    }
}

fn setup_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(SfxLibrary::load(&asset_server));
    commands.insert_resource(MusicTracks::load(&asset_server));
}

/// Sync MusicState to AppState transitions.
fn sync_music_state_to_app(app_state: Res<State<AppState>>, mut music: ResMut<MusicState>) {
    if !app_state.is_changed() {
        return;
    }
    let new = match app_state.get() {
        AppState::MainMenu => MusicState::Menu,
        AppState::InGame => MusicState::Peaceful,
    };
    if *music != new {
        *music = new;
    }
}

/// When MusicState changes, cross-fade to the new track.
fn change_music_track(
    mut commands: Commands,
    music_state: Res<MusicState>,
    tracks: Option<Res<MusicTracks>>,
    settings: Res<AudioSettings>,
    existing: Query<Entity, With<MusicChannel>>,
) {
    if !music_state.is_changed() {
        return;
    }
    let Some(tracks) = tracks else { return };

    // Fade out all existing music entities.
    for entity in existing.iter() {
        commands
            .entity(entity)
            .insert(FadeOut {
                elapsed: 0.0,
                duration: FADE_DURATION,
            })
            .remove::<FadeIn>();
    }

    // Spawn new track with zero volume + fade-in.
    let handle = tracks.track_for(&music_state);
    let target_vol = slider_to_volume(settings.music_volume).to_linear();
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings {
            mode: bevy::audio::PlaybackMode::Loop,
            volume: Volume::Linear(0.0),
            ..default()
        },
        MusicChannel,
        FadeIn {
            elapsed: 0.0,
            duration: FADE_DURATION,
            target_volume: target_vol,
        },
    ));
}

fn fade_in_system(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut AudioSink, &mut FadeIn), With<MusicChannel>>,
) {
    for (entity, mut sink, mut fade) in q.iter_mut() {
        fade.elapsed += time.delta_secs();
        let t = (fade.elapsed / fade.duration).min(1.0);
        let vol = t * fade.target_volume;
        sink.set_volume(Volume::Linear(vol));
        if t >= 1.0 {
            commands.entity(entity).remove::<FadeIn>();
        }
    }
}

fn fade_out_system(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut AudioSink, &mut FadeOut), With<MusicChannel>>,
) {
    for (entity, mut sink, mut fade) in q.iter_mut() {
        fade.elapsed += time.delta_secs();
        let t = (fade.elapsed / fade.duration).min(1.0);
        let current = sink.volume();
        // Lerp from current volume towards 0.
        let vol = match current {
            Volume::Linear(v) => v * (1.0 - t),
            _ => 1.0 - t,
        };
        sink.set_volume(Volume::Linear(vol.max(0.0)));
        if t >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Apply volume changes from AudioSettings to currently playing music.
fn sync_music_volume(
    settings: Res<AudioSettings>,
    mut steady_music: Query<
        &mut AudioSink,
        (With<MusicChannel>, Without<FadeIn>, Without<FadeOut>),
    >,
    mut fading_in_music: Query<(&mut AudioSink, &mut FadeIn), With<MusicChannel>>,
) {
    if !settings.is_changed() {
        return;
    }

    for mut sink in steady_music.iter_mut() {
        sink.set_volume(slider_to_volume(settings.music_volume));
    }

    for (mut sink, mut fade) in fading_in_music.iter_mut() {
        fade.target_volume = slider_to_volume(settings.music_volume).to_linear();
        let t = (fade.elapsed / fade.duration).clamp(0.0, 1.0);
        sink.set_volume(Volume::Linear(t * fade.target_volume));
    }
}

/// Apply volume changes from AudioSettings to already-playing one-shot SFX.
fn sync_sfx_volume(settings: Res<AudioSettings>, mut q: Query<&mut AudioSink, With<SfxChannel>>) {
    if !settings.is_changed() {
        return;
    }

    for mut sink in q.iter_mut() {
        sink.set_volume(slider_to_volume(settings.sfx_volume));
    }
}

/// Maximum distance (in world units) at which positional SFX are audible.
const SFX_MAX_DISTANCE: f32 = 120.0;
/// Distance at which positional SFX start to attenuate (full volume up to this).
const SFX_FULL_VOLUME_DISTANCE: f32 = 30.0;

/// Play one-shot SFX when PlaySfx events arrive.
/// Sounds with a `position` are attenuated by distance from the camera
/// and culled entirely beyond `SFX_MAX_DISTANCE`.
fn play_sfx_system(
    mut commands: Commands,
    mut events: MessageReader<PlaySfx>,
    sfx_lib: Option<Res<SfxLibrary>>,
    settings: Res<AudioSettings>,
    camera_q: Query<&Transform, With<RtsCamera>>,
) {
    let Some(sfx_lib) = sfx_lib else { return };
    let camera_pos = camera_q.iter().next().map(|t| t.translation);

    for ev in events.read() {
        // Distance-based attenuation for positional sounds
        let distance_scale = if let (Some(sfx_pos), Some(cam_pos)) = (ev.position, camera_pos) {
            let dist = sfx_pos.distance(cam_pos);
            if dist > SFX_MAX_DISTANCE {
                continue; // Too far away, skip entirely
            }
            if dist <= SFX_FULL_VOLUME_DISTANCE {
                1.0
            } else {
                // Linear falloff from full volume to 0 between FULL and MAX distance
                1.0 - (dist - SFX_FULL_VOLUME_DISTANCE)
                    / (SFX_MAX_DISTANCE - SFX_FULL_VOLUME_DISTANCE)
            }
        } else {
            1.0 // No position = UI sound, play at full volume
        };

        let base_vol = slider_to_volume(settings.sfx_volume).to_linear();
        let final_vol = Volume::Linear(base_vol * distance_scale);

        let handle = sfx_lib.handle_for(ev.kind);
        commands.spawn((
            AudioPlayer(handle),
            PlaybackSettings {
                mode: bevy::audio::PlaybackMode::Despawn,
                volume: final_vol,
                ..default()
            },
            SfxChannel,
        ));
    }
}

/// Cleanup all audio entities when leaving InGame.
fn cleanup_audio_on_exit(mut commands: Commands, q: Query<Entity, With<MusicChannel>>) {
    for entity in q.iter() {
        commands.entity(entity).despawn();
    }
}

// ── Plugin ──

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        // AudioSettings is already inserted by main() via database::init_early().
        app.init_resource::<MusicState>()
            .add_message::<PlaySfx>()
            .add_systems(Startup, setup_audio)
            .add_systems(
                Update,
                (
                    sync_music_state_to_app,
                    change_music_track,
                    fade_in_system,
                    fade_out_system,
                    sync_music_volume,
                    sync_sfx_volume,
                    play_sfx_system,
                ),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_audio_on_exit);
    }
}
