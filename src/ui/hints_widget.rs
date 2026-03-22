//! Onboarding hints — contextual tips for the first 3 minutes + idle worker notification.

use bevy::prelude::*;
use std::collections::HashSet;

use crate::components::*;

/// Tracks onboarding hint state.
#[derive(Resource)]
pub struct HintState {
    pub shown_hints: HashSet<u8>,
    pub active_hint: Option<String>,
    pub hint_timer: Timer,
}

impl Default for HintState {
    fn default() -> Self {
        Self {
            shown_hints: HashSet::new(),
            active_hint: None,
            hint_timer: Timer::from_seconds(8.0, TimerMode::Once),
        }
    }
}

#[derive(Component)]
pub struct HintOverlay;

#[derive(Component)]
pub struct IdleWorkerButton;

const HINTS: &[(f32, u8, &str)] = &[
    (5.0, 0, "Train Workers at your Base to gather resources"),
    (30.0, 1, "Build a Sawmill near trees for wood production"),
    (60.0, 2, "Build a Barracks to train military units"),
    (100.0, 3, "Scout the map — right-click with a unit to move"),
    (150.0, 4, "Build Houses to increase your unit cap"),
];

pub fn hints_system(
    mut commands: Commands,
    time: Res<Time>,
    mut hint_state: ResMut<HintState>,
    overlay: Query<Entity, With<HintOverlay>>,
) {
    let elapsed = time.elapsed_secs();

    // Only show hints in first 3 minutes
    if elapsed > 180.0 {
        // Clean up any existing overlay
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        hint_state.active_hint = None;
        return;
    }

    // Check if a new hint should trigger
    for &(trigger_time, hint_id, hint_text) in HINTS {
        if elapsed >= trigger_time && !hint_state.shown_hints.contains(&hint_id) {
            hint_state.shown_hints.insert(hint_id);
            hint_state.active_hint = Some(hint_text.to_string());
            hint_state.hint_timer.reset();

            // Remove old overlay
            for entity in &overlay {
                commands.entity(entity).despawn();
            }

            // Spawn hint overlay
            commands.spawn((
                HintOverlay,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(10.0),
                    left: Val::Percent(25.0),
                    width: Val::Percent(50.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.15, 0.25, 0.85)),
                GlobalZIndex(50),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(hint_text),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.9, 0.5)),
                ));
            });
            break;
        }
    }

    // Auto-dismiss hint after timer
    if hint_state.active_hint.is_some() {
        hint_state.hint_timer.tick(time.delta());
        if hint_state.hint_timer.is_finished() {
            hint_state.active_hint = None;
            for entity in &overlay {
                commands.entity(entity).despawn();
            }
        }
    }
}

pub fn idle_worker_notification_system(
    mut commands: Commands,
    idle_workers: Query<(Entity, &Faction, &UnitState), With<Unit>>,
    active_player: Res<ActivePlayer>,
    existing_button: Query<Entity, With<IdleWorkerButton>>,
) {
    let idle_count = idle_workers
        .iter()
        .filter(|(_, f, state)| **f == active_player.0 && **state == UnitState::Idle)
        .count();

    // Remove existing button if no idle workers
    if idle_count == 0 {
        for entity in &existing_button {
            commands.entity(entity).despawn();
        }
        return;
    }

    // Only spawn the button if it doesn't exist
    if !existing_button.is_empty() {
        return;
    }

    commands.spawn((
        IdleWorkerButton,
        Button,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            right: Val::Px(10.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.8, 0.6, 0.1, 0.9)),
        GlobalZIndex(40),
    ))
    .with_children(|parent| {
        parent.spawn((
            Text::new(format!("Idle Workers: {}", idle_count)),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
    });
}
