//! Pre-night warning banner + mid-night HUD counter.
//!
//! Two visible UI elements driven off the night-wave system:
//!
//! - **Banner**: top-center fade-in/out toast triggered by [`WaveAlert`]
//!   messages emitted on Day→Dusk. Auto-fades after 5 seconds.
//! - **Counter**: persistent line below the banner showing the active wave's
//!   `spawned/total · killed K` while a Night is in progress; hidden during
//!   Day. Reads [`NightWaveState.active`] each frame.

use bevy::prelude::*;

use super::core::hud::MainHudRoot;
use crate::simulation::mobs::NightWaveState;
use crate::types::{AppState, WaveAlert};
use crate::ui::theme::Theme;

pub struct WaveBannerWidgetPlugin;

impl Plugin for WaveBannerWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_wave_banner_root
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(
            Update,
            (consume_wave_alerts, fade_wave_banner, update_wave_counter)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
struct WaveBannerRoot;

#[derive(Component)]
struct WaveBannerText;

#[derive(Component)]
struct WaveBannerToast {
    spawn_time: f32,
}

#[derive(Component)]
struct WaveCounterText;

fn spawn_wave_banner_root(
    mut commands: Commands,
    theme: Res<Theme>,
    root_q: Query<Entity, Added<MainHudRoot>>,
) {
    let Ok(hud_root) = root_q.single() else {
        return;
    };

    let container = commands
        .spawn((
            WaveBannerRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(110.0),
                left: Val::Percent(50.0),
                width: Val::Px(420.0),
                margin: UiRect::left(Val::Px(-210.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .id();
    commands.entity(hud_root).add_child(container);

    // The mid-night counter line — text content updated each frame.
    let counter = commands
        .spawn((
            WaveCounterText,
            Text::new(""),
            TextFont {
                font_size: theme.typography.medium,
                ..default()
            },
            TextColor(Color::srgb(1.0, 0.85, 0.5)),
        ))
        .id();
    commands.entity(container).add_child(counter);
}

fn consume_wave_alerts(
    mut commands: Commands,
    time: Res<Time>,
    theme: Res<Theme>,
    mut alerts: MessageReader<WaveAlert>,
    container_q: Query<Entity, With<WaveBannerRoot>>,
) {
    let Ok(container) = container_q.single() else {
        return;
    };
    for alert in alerts.read() {
        let msg = format!(
            "Night {} approaching\n{} attackers from the {}",
            alert.night,
            alert.incoming_count,
            alert.direction.label()
        );
        commands.entity(container).with_children(|parent| {
            parent
                .spawn((
                    WaveBannerToast {
                        spawn_time: time.elapsed_secs(),
                    },
                    Node {
                        padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                        margin: UiRect::bottom(Val::Px(4.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.92)),
                    BorderColor::all(Color::srgb(1.0, 0.5, 0.2)),
                ))
                .with_children(|toast| {
                    toast.spawn((
                        WaveBannerText,
                        Text::new(msg.clone()),
                        TextFont {
                            font_size: theme.typography.large,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.55, 0.2)),
                    ));
                });
        });
    }
}

fn fade_wave_banner(
    mut commands: Commands,
    time: Res<Time>,
    toasts: Query<(Entity, &WaveBannerToast)>,
) {
    let now = time.elapsed_secs();
    for (entity, toast) in toasts.iter() {
        if now - toast.spawn_time > 5.0 {
            commands.entity(entity).try_despawn();
        }
    }
}

fn update_wave_counter(
    wave: Res<NightWaveState>,
    mut counter_q: Query<&mut Text, With<WaveCounterText>>,
) {
    let Ok(mut text) = counter_q.single_mut() else {
        return;
    };
    let new_text = match wave.active.as_ref() {
        Some(active) => format!(
            "Wave {}: {}/{} · killed {}",
            active.night, active.spawned, active.total, active.killed
        ),
        None => String::new(),
    };
    if text.0 != new_text {
        text.0 = new_text;
    }
}
