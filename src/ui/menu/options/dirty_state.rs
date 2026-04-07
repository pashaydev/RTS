use bevy::prelude::*;

use crate::types::*;
use crate::ui::menu::helpers::*;
use crate::ui::menu::*;
use crate::ui::theme::{Theme, TEXT_PRIMARY, OVERLAY};
use crate::ui::core::components as ui_components;
use crate::ui::core::fonts::{self, UiFonts};

// ── Options Dirty-State Tracking ──

/// Captures a snapshot of current settings when entering the Options page.
pub(crate) fn capture_options_snapshot(
    page: Res<MenuPage>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::infrastructure::audio::AudioSettings>,
    mut commands: Commands,
    snapshot: Option<Res<super::super::OptionsSnapshot>>,
) {
    if !page.is_changed() {
        return;
    }
    if matches!(*page, MenuPage::Options) {
        // Only capture if we don't already have a snapshot (fresh entry)
        if snapshot.is_none() {
            commands.insert_resource(super::super::OptionsSnapshot {
                graphics: graphics.clone(),
                audio: audio_settings.clone(),
            });
        }
    } else {
        // Leaving Options page — clean up snapshot
        if snapshot.is_some() {
            commands.remove_resource::<super::super::OptionsSnapshot>();
        }
    }
}

/// Toggles Save button visibility based on whether settings have changed from snapshot.
pub(crate) fn toggle_save_button_visibility(
    page: Res<MenuPage>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::infrastructure::audio::AudioSettings>,
    snapshot: Option<Res<super::super::OptionsSnapshot>>,
    mut save_btns: Query<&mut Visibility, With<super::super::SaveSettingsButton>>,
) {
    if !matches!(*page, MenuPage::Options) {
        return;
    }
    let dirty = if let Some(ref snap) = snapshot {
        *graphics != snap.graphics || *audio_settings != snap.audio
    } else {
        false
    };
    for mut vis in &mut save_btns {
        *vis = if dirty {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Spawns/despawns the unsaved-changes confirmation popup.
pub(crate) fn manage_unsaved_changes_popup(
    popup_state: Res<super::super::ConfirmPopupState>,
    existing_popup: Query<Entity, With<super::super::UnsavedChangesPopup>>,
    mut commands: Commands,
    theme: Res<Theme>,
    fonts: Res<UiFonts>,
) {
    if !popup_state.is_changed() {
        return;
    }

    if popup_state.active {
        // Don't spawn twice
        if !existing_popup.is_empty() {
            return;
        }
        spawn_unsaved_changes_popup(&mut commands, &theme, &fonts);
    } else {
        // Despawn popup
        for entity in &existing_popup {
            commands.entity(entity).try_despawn();
        }
    }
}

fn spawn_unsaved_changes_popup(commands: &mut Commands, theme: &Theme, fonts: &UiFonts) {
    commands
        .spawn((
            super::super::UnsavedChangesPopup,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(OVERLAY),
            GlobalZIndex(100),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: Val::Px(400.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(24.0)),
                        row_gap: Val::Px(16.0),
                        border: UiRect::all(Val::Px(2.0)),
                        // border_radius: BorderRadius::all(Val::Px(12.0)),
                        ..default()
                    },
                    BackgroundColor(theme.colors.bg_surface),
                    BorderColor::all(theme.colors.accent),
                    BoxShadow::new(
                        Color::srgba(0.0, 0.0, 0.0, 0.5),
                        Val::Px(0.0),
                        Val::Px(4.0),
                        Val::Px(0.0),
                        Val::Px(20.0),
                    ),
                ))
                .with_children(|card| {
                    // Title
                    card.spawn((
                        Text::new("Unsaved Changes"),
                        fonts::heading(fonts, theme.typography.heading),
                        TextColor(TEXT_PRIMARY),
                    ));

                    // Description
                    card.spawn((
                        Text::new("You have unsaved changes.\nSave before leaving?"),
                        TextFont {
                            font: fonts.body.clone(),
                            font_size: theme.typography.body,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
                        TextLayout::new_with_justify(Justify::Center),
                    ));

                    // Buttons row
                    card.spawn(Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        column_gap: Val::Px(8.0),
                        margin: UiRect::top(Val::Px(8.0)),
                        ..default()
                    })
                    .with_children(|row| {
                        // Save & Leave
                        row.spawn((
                            MenuButton(MenuAction::SaveAndLeave),
                            Button,
                            ui_components::button_node(120.0, 40.0),
                            ui_components::filled_button_chrome(
                                theme,
                                ui_components::UiTone::Accent,
                            ),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("SAVE"),
                                fonts::heading(fonts, theme.typography.small),
                                TextColor(TEXT_PRIMARY),
                                Pickable::IGNORE,
                            ));
                        });

                        // Discard
                        row.spawn((
                            MenuButton(MenuAction::DiscardSettings),
                            Button,
                            ui_components::button_node(120.0, 40.0),
                            ui_components::filled_button_chrome(
                                theme,
                                ui_components::UiTone::Destructive,
                            ),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("DISCARD"),
                                fonts::heading(fonts, theme.typography.small),
                                TextColor(TEXT_PRIMARY),
                                Pickable::IGNORE,
                            ));
                        });

                        // Cancel
                        row.spawn((
                            MenuButton(MenuAction::CancelPopup),
                            Button,
                            ui_components::button_node(120.0, 40.0),
                            ui_components::ghost_button_chrome(
                                theme,
                                ui_components::UiTone::Neutral,
                            ),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("CANCEL"),
                                fonts::heading(fonts, theme.typography.small),
                                TextColor(theme.colors.text_secondary),
                                Pickable::IGNORE,
                            ));
                        });
                    });
                });
        });
}
