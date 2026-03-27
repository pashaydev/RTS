//! Onboarding hints — contextual tips for the first 3 minutes.

use bevy::prelude::*;
use std::collections::HashSet;

use super::core::hud::MainHudRoot;
use crate::components::*;

pub struct HintsWidgetPlugin;

impl Plugin for HintsWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HintState>().add_systems(
            Update,
            hints_system.run_if(in_state(AppState::InGame)),
        );
    }
}

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
    root_q: Query<Entity, With<MainHudRoot>>,
) {
    let elapsed = time.elapsed_secs();

    // Only show hints in first 3 minutes
    if elapsed > 180.0 {
        // Clean up any existing overlay
        for entity in &overlay {
            commands.entity(entity).try_despawn();
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
                commands.entity(entity).try_despawn();
            }

            // Spawn hint overlay
            let hint_overlay = commands
                .spawn((
                    HintOverlay,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(40.0),
                        left: Val::Percent(25.0),
                        width: Val::Percent(50.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(Val::Px(4.0)),
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
                })
                .id();
            if let Ok(hud_root) = root_q.single() {
                commands.entity(hud_root).add_child(hint_overlay);
            }
            break;
        }
    }

    // Auto-dismiss hint after timer
    if hint_state.active_hint.is_some() {
        hint_state.hint_timer.tick(time.delta());
        if hint_state.hint_timer.is_finished() {
            hint_state.active_hint = None;
            for entity in &overlay {
                commands.entity(entity).try_despawn();
            }
        }
    }
}
