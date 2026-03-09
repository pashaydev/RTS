use bevy::prelude::*;

use crate::theme;
use super::widget_framework::{WidgetId, WidgetRegistry};

#[derive(Component)]
pub struct WidgetToolbar;

#[derive(Component)]
pub struct WidgetToolbarButton(pub WidgetId);

/// Marker for the toolbar container entity
#[derive(Component)]
pub struct ToolbarContainer;

pub fn spawn_toolbar(commands: &mut Commands, parent: Entity) {
    let toolbar = commands
        .spawn((
            ToolbarContainer,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(4.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-250.0)), // rough center offset
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.07, 0.07, 0.85)),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.4),
                Val::Px(0.0),
                Val::Px(2.0),
                Val::Px(0.0),
                Val::Px(6.0),
            ),
        ))
        .id();
    commands.entity(parent).add_child(toolbar);

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
                    column_gap: Val::Px(3.0),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(format!("{} {}", id.icon(), hotkey_name)),
                    TextFont { font_size: 9.0, ..default() },
                    TextColor(theme::TEXT_SECONDARY),
                ));
            })
            .id();
        commands.entity(toolbar).add_child(btn);
    }
}

/// Reads hotkey presses + button clicks, toggles widget visibility
pub fn widget_toolbar_system(
    mut registry: ResMut<WidgetRegistry>,
    keys: Res<ButtonInput<KeyCode>>,
    interactions: Query<(&Interaction, &WidgetToolbarButton), Changed<Interaction>>,
) {
    // Button clicks
    for (interaction, btn) in &interactions {
        if *interaction == Interaction::Pressed {
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
    mut buttons: Query<(&WidgetToolbarButton, &mut BackgroundColor)>,
) {
    if !registry.is_changed() {
        return;
    }
    for (btn, mut bg) in &mut buttons {
        if registry.is_visible(btn.0) {
            *bg = BackgroundColor(Color::srgba(0.29, 0.62, 1.0, 0.15));
        } else {
            *bg = BackgroundColor(Color::NONE);
        }
    }
}
