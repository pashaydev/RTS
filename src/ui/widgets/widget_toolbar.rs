use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use super::core::constants::*;
use super::core::framework::{WidgetId, WidgetRegistry};
use super::core::hud::MainHudRoot;
use super::core::interactions::UiClickEvent;
use crate::components::AppState;
use crate::theme::{self, Theme};
use crate::ui::fonts::{self, UiFonts};

pub struct WidgetToolbarPlugin;

impl Plugin for WidgetToolbarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_toolbar_widget
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(
            Update,
            (widget_toolbar_system, update_toolbar_visuals).run_if(in_state(AppState::InGame)),
        );
    }
}

fn spawn_toolbar_widget(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    theme: Res<Theme>,
    root_q: Query<Entity, Added<MainHudRoot>>,
) {
    let Ok(hud_root) = root_q.single() else {
        return;
    };
    spawn_toolbar(&mut commands, hud_root, &fonts, &theme);
}

#[derive(Component)]
pub struct WidgetToolbar;

#[derive(Component)]
pub struct WidgetToolbarButton(pub WidgetId);

/// Marker for the toolbar container entity
#[derive(Component)]
pub struct ToolbarContainer;

pub fn spawn_toolbar(commands: &mut Commands, parent: Entity, fonts: &UiFonts, theme: &Theme) {
    let anchor = commands
        .spawn((
            ToolbarContainer,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(4.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(anchor);

    let toolbar = commands
        .spawn((
            WidgetToolbar,
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                padding: PAD_COMPACT,
                // border_radius: RADIUS_LG,
                ..default()
            },
            BackgroundColor(theme::BG_PANEL.with_alpha(0.85)),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.4),
                Val::Px(0.0),
                Val::Px(2.0),
                Val::Px(0.0),
                Val::Px(6.0),
            ),
        ))
        .id();
    commands.entity(anchor).add_child(toolbar);

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
                    padding: PAD_COMPACT,
                    // border_radius: RADIUS_MD,
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
        commands.entity(toolbar).add_child(btn);
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
