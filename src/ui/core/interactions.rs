//! Hover, click, and hold-complete events; selection mode and command
//! entry plumbing shared across widgets.

use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;

use crate::infrastructure::audio::{PlaySfx, SfxKind};
use crate::types::TextInputField;

use super::framework::{WidgetDragHandle, WidgetResizeHandle};

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiInteractPhase {
    #[default]
    Idle,
    Hovered,
    Pressed,
    #[allow(dead_code)]
    Disabled,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct UiInteractState {
    pub phase: UiInteractPhase,
    pub hold_progress: f32,
    pub click_flash: f32,
    pub armed: bool,
    pub hold_complete_sent: bool,
}

impl Default for UiInteractState {
    fn default() -> Self {
        Self {
            phase: UiInteractPhase::Idle,
            hold_progress: 0.0,
            click_flash: 0.0,
            armed: false,
            hold_complete_sent: false,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct UiInteractBehavior {
    pub hold_duration_secs: f32,
    pub emit_click: bool,
}

impl Default for UiInteractBehavior {
    fn default() -> Self {
        Self {
            hold_duration_secs: 0.35,
            emit_click: true,
        }
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct UiClickEvent {
    pub entity: Entity,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct UiHoldCompleteEvent {
    pub _entity: Entity,
}

pub fn attach_interaction_state(
    mut commands: Commands,
    query: Query<
        (Entity, Has<WidgetDragHandle>, Has<WidgetResizeHandle>),
        (
            Added<Interaction>,
            Without<UiInteractState>,
            Or<(
                With<Button>,
                With<WidgetDragHandle>,
                With<WidgetResizeHandle>,
                With<TextInputField>,
            )>,
        ),
    >,
) {
    for (entity, is_drag_handle, is_resize_handle) in &query {
        let mut behavior = UiInteractBehavior::default();
        if is_drag_handle || is_resize_handle {
            behavior.emit_click = false;
        }

        commands
            .entity(entity)
            .insert((UiInteractState::default(), behavior));
    }
}

pub fn update_interaction_states(
    time: Res<Time>,
    mut click_events: MessageWriter<UiClickEvent>,
    mut hold_events: MessageWriter<UiHoldCompleteEvent>,
    mut sfx_events: MessageWriter<PlaySfx>,
    mut query: Query<(
        Entity,
        &Interaction,
        &mut UiInteractState,
        &UiInteractBehavior,
    )>,
) {
    let dt = time.delta_secs();

    for (entity, interaction, mut state, behavior) in &mut query {
        state.click_flash = (state.click_flash - dt * 8.0).max(0.0);

        match *interaction {
            Interaction::Pressed => {
                if state.phase != UiInteractPhase::Pressed {
                    state.phase = UiInteractPhase::Pressed;
                    state.armed = true;
                    state.hold_progress = 0.0;
                    state.hold_complete_sent = false;
                }

                let hold_duration = behavior.hold_duration_secs.max(0.001);
                state.hold_progress = (state.hold_progress + dt / hold_duration).clamp(0.0, 1.0);

                if !state.hold_complete_sent && state.hold_progress >= 1.0 {
                    hold_events.write(UiHoldCompleteEvent { _entity: entity });
                    state.hold_complete_sent = true;
                }
            }
            Interaction::Hovered => {
                if state.phase == UiInteractPhase::Pressed && state.armed && behavior.emit_click {
                    click_events.write(UiClickEvent { entity });
                    sfx_events.write(PlaySfx {
                        kind: SfxKind::ButtonClick,
                        position: None,
                    });
                    state.click_flash = 1.0;
                }

                // Play hover sound when transitioning from Idle → Hovered
                if state.phase == UiInteractPhase::Idle {
                    sfx_events.write(PlaySfx {
                        kind: SfxKind::ButtonHover,
                        position: None,
                    });
                }

                state.phase = UiInteractPhase::Hovered;
                state.armed = false;
                state.hold_progress = 0.0;
                state.hold_complete_sent = false;
            }
            Interaction::None => {
                state.phase = UiInteractPhase::Idle;
                state.armed = false;
                state.hold_progress = 0.0;
                state.hold_complete_sent = false;
            }
        }
    }
}
