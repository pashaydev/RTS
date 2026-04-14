use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use super::core::framework::{WidgetId, WidgetRegistry};
use super::core::interactions::UiClickEvent;
use crate::types::AppState;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::theme::{self, Theme};

pub struct WidgetToolbarPlugin;

impl Plugin for WidgetToolbarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (widget_toolbar_system, update_toolbar_visuals).run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
pub struct WidgetToolbarButton(pub WidgetId);

/// Spawn toolbar toggle buttons into an existing parent container.
/// Called by the header bar during its spawn.
pub fn spawn_toolbar_buttons(
    commands: &mut Commands,
    parent: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    for &id in WidgetId::ALL {
        let hotkey_name = match id.hotkey() {
            KeyCode::F1 => "F1",
            KeyCode::F2 => "F2",
            KeyCode::F3 => "F3",
            KeyCode::F4 => "F4",
            KeyCode::F5 => "F5",
            KeyCode::F6 => "F6",
            KeyCode::F7 => "F7",
            KeyCode::F8 => "F8",
            KeyCode::F9 => "F9",
            KeyCode::F10 => "F10",
            _ => "?",
        };

        let btn = commands
            .spawn((
                Button,
                WidgetToolbarButton(id),
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(2.0),
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(format!("{} {}", id.icon(), hotkey_name)),
                    fonts::toolbar(fonts),
                    TextColor(theme.colors.text_secondary),
                ));
            })
            .id();
        commands.entity(parent).add_child(btn);
    }
}

/// Reads hotkey presses + button clicks, toggles widget visibility
pub fn widget_toolbar_system(
    mut click_events: MessageReader<UiClickEvent>,
    mut registry: ResMut<WidgetRegistry>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<&WidgetToolbarButton>,
) {
    for event in click_events.read() {
        if let Ok(btn) = buttons.get(event.entity) {
            registry.toggle(btn.0);
        }
    }

    // Hotkeys
    for &id in WidgetId::ALL {
        if keys.just_pressed(id.hotkey()) {
            registry.toggle(id);
        }
    }
}

/// Update toolbar button visuals based on widget visibility
pub fn update_toolbar_visuals(
    registry: Res<WidgetRegistry>,
    added_buttons: Query<Entity, Added<WidgetToolbarButton>>,
    mut buttons: Query<(&WidgetToolbarButton, &mut BackgroundColor)>,
) {
    if !registry.is_changed() && added_buttons.is_empty() {
        return;
    }
    for (btn, mut bg) in &mut buttons {
        if registry.is_visible(btn.0) {
            *bg = BackgroundColor(theme::HIGHLIGHT_SUBTLE);
        } else {
            *bg = BackgroundColor(Color::NONE);
        }
    }
}
