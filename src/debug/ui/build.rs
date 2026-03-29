use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::debug::model::{DebugTweaks, TweakValue};
use crate::debug::state::{DebugPanelState, TweakStructureVersion};
use crate::debug::ui::components::{
    ColorPreview, DebugDayCycleText, DebugEntityCountText, DebugExpandButton, DebugFpsText,
    DebugTweakPanel, FolderHeader, SaveConfigButton, SaveConfigButtonText, TweakButton,
    TweakCycleEnum, TweakCycleText, TweakPanelBuiltVersion, TweakReadOnlyText, TweakSlider,
    TweakSliderFill, TweakSliderKnob, TweakSliderValueText, TweakToggle, TweakToggleText,
};
use crate::debug::ui::style::{
    debug_card_node, debug_control_border, debug_control_surface, debug_emphasis_border,
    debug_inverse_text, debug_pill_node, debug_row_node, debug_separator, debug_slider_fill,
    debug_text_primary, debug_text_secondary, format_tweak_float,
};
use crate::theme;

pub fn spawn_debug_content(commands: &mut Commands, parent: Entity) {
    let stats_header = commands
        .spawn((
            Text::new("RUNTIME"),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(debug_text_secondary()),
        ))
        .id();
    commands.entity(parent).add_child(stats_header);

    let fps = commands
        .spawn((
            DebugFpsText,
            Text::new("FPS: --"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    commands.entity(parent).add_child(fps);

    let ent_count = commands
        .spawn((
            DebugEntityCountText,
            Text::new("Entities: --"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    commands.entity(parent).add_child(ent_count);

    let day_cycle = commands
        .spawn((
            DebugDayCycleText,
            Text::new("Day: --"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    commands.entity(parent).add_child(day_cycle);

    let sep = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                margin: UiRect::axes(Val::ZERO, Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(debug_separator()),
        ))
        .id();
    commands.entity(parent).add_child(sep);

    let btn_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(4.0),
            width: Val::Percent(100.0),
            ..default()
        })
        .id();
    commands.entity(parent).add_child(btn_row);

    let expand_btn = commands
        .spawn((
            DebugExpandButton,
            Interaction::default(),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    let expand_text = commands
        .spawn((
            Pickable::IGNORE,
            Text::new("Inspect"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
        ))
        .id();
    commands.entity(expand_btn).add_child(expand_text);
    commands.entity(btn_row).add_child(expand_btn);

    let save_btn = commands
        .spawn((
            SaveConfigButton,
            Interaction::default(),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    let save_text = commands
        .spawn((
            SaveConfigButtonText,
            Pickable::IGNORE,
            Text::new("Save"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
        ))
        .id();
    commands.entity(save_btn).add_child(save_text);
    commands.entity(btn_row).add_child(save_btn);

    let tweak_panel = commands
        .spawn((
            DebugTweakPanel,
            TweakPanelBuiltVersion(0),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                width: Val::Percent(100.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .id();
    commands.entity(parent).add_child(tweak_panel);
}

pub fn rebuild_tweak_panel(
    tweaks: Res<DebugTweaks>,
    panel_state: Res<DebugPanelState>,
    mut structure: ResMut<TweakStructureVersion>,
    mut commands: Commands,
    mut panel_q: Query<
        (Entity, &mut TweakPanelBuiltVersion, &mut Visibility),
        With<DebugTweakPanel>,
    >,
    children_q: Query<&Children>,
) {
    let Ok((panel_entity, mut built_ver, mut panel_vis)) = panel_q.single_mut() else {
        return;
    };

    let target_vis = if panel_state.tweaks_expanded {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *panel_vis != target_vis {
        *panel_vis = target_vis;
    }

    if !panel_state.tweaks_expanded {
        return;
    }

    let folder_count = tweaks.folders.len();
    let entry_counts: Vec<usize> = tweaks.folders.values().map(|v| v.len()).collect();
    let structure_changed = folder_count != structure.last_folder_count
        || entry_counts != structure.last_entry_counts
        || panel_state.is_changed();

    if !structure_changed {
        return;
    }

    structure.last_folder_count = folder_count;
    structure.last_entry_counts = entry_counts;
    structure.version += 1;
    built_ver.0 = structure.version;

    if let Ok(children) = children_q.get(panel_entity) {
        for child in children {
            commands.entity(*child).try_despawn();
        }
    }

    commands.entity(panel_entity).with_children(|panel| {
        let mut current_section: Option<String> = None;
        for (folder_name, entries) in &tweaks.folders {
            let (section, display_name) = if let Some(idx) = folder_name.find('/') {
                (Some(&folder_name[..idx]), &folder_name[idx + 1..])
            } else {
                (None, folder_name.as_str())
            };

            let section_str = section.map(|s| s.to_string());
            if section_str != current_section {
                if let Some(ref sec) = section_str {
                    spawn_section_header(panel, sec);
                }
                current_section = section_str;
            }

            let collapsed = panel_state.collapsed_folders.contains(folder_name);
            spawn_folder_header(panel, folder_name, display_name, collapsed);

            if collapsed {
                continue;
            }

            let mut color_prefix: Option<String> = None;

            for entry in entries {
                match &entry.value {
                    TweakValue::Float {
                        value, min, max, ..
                    } => {
                        spawn_slider_row(panel, folder_name, &entry.label, *value, *min, *max);

                        if entry.label.ends_with(" R") {
                            color_prefix = Some(entry.label.trim_end_matches(" R").to_string());
                        } else if entry.label.ends_with(" B") {
                            if let Some(ref prefix) = color_prefix {
                                let expected_b = format!("{} B", prefix);
                                if entry.label == expected_b {
                                    spawn_color_preview(panel, folder_name, prefix);
                                }
                            }
                            color_prefix = None;
                        }
                    }
                    TweakValue::Bool(v) => {
                        spawn_toggle_row(panel, folder_name, &entry.label, *v);
                        color_prefix = None;
                    }
                    TweakValue::ReadOnly(text) => {
                        spawn_readonly_row(panel, folder_name, &entry.label, text);
                        color_prefix = None;
                    }
                    TweakValue::CycleEnum { options, selected } => {
                        let display = options.get(*selected).map(|s| s.as_str()).unwrap_or("--");
                        spawn_cycle_row(panel, folder_name, &entry.label, display);
                        color_prefix = None;
                    }
                    TweakValue::Button { text } => {
                        spawn_button_row(panel, folder_name, &entry.label, text);
                        color_prefix = None;
                    }
                }
            }
        }
    });
}

fn spawn_section_header(parent: &mut ChildSpawnerCommands, section: &str) {
    parent
        .spawn((
            Node {
                padding: UiRect::new(Val::Px(6.0), Val::Px(6.0), Val::Px(8.0), Val::Px(3.0)),
                margin: UiRect::top(Val::Px(8.0)),
                width: Val::Percent(100.0),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(debug_separator()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(section.to_uppercase()),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(debug_text_secondary()),
            ));
        });
}

fn spawn_folder_header(
    parent: &mut ChildSpawnerCommands,
    key: &str,
    display_name: &str,
    collapsed: bool,
) {
    let arrow = if collapsed { ">" } else { "v" };
    parent
        .spawn((
            FolderHeader(key.to_string()),
            Interaction::default(),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                margin: UiRect::top(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(if collapsed {
                debug_control_border()
            } else {
                debug_emphasis_border()
            }),
        ))
        .with_children(|header| {
            header.spawn((
                Text::new(format!("{} {}", arrow, display_name.to_uppercase())),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_primary()),
            ));
        });
}

fn spawn_slider_row(
    parent: &mut ChildSpawnerCommands,
    folder: &str,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
) {
    let pct = ((value - min) / (max - min)).clamp(0.0, 1.0) * 100.0;

    parent
        .spawn((
            debug_card_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|card| {
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|top| {
                top.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_text_primary()),
                ));

                top.spawn((
                    TweakSliderValueText {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Text::new(format_tweak_float(value)),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_inverse_text()),
                    debug_pill_node(),
                    BackgroundColor(debug_slider_fill()),
                    BorderColor::all(debug_slider_fill()),
                ));
            });

            card.spawn((
                TweakSlider {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(12.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    overflow: Overflow::clip(),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
                BorderColor::all(debug_control_border()),
            ))
            .with_children(|track| {
                track.spawn((
                    TweakSliderFill {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Node {
                        width: Val::Percent(pct),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(999.0)),
                        ..default()
                    },
                    BackgroundColor(debug_slider_fill()),
                ));

                track.spawn((
                    TweakSliderKnob {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(pct),
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        margin: UiRect::left(Val::Px(-6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(999.0)),
                        ..default()
                    },
                    BackgroundColor(debug_text_primary()),
                    BorderColor::all(Color::BLACK),
                ));
            });
        });
}

fn spawn_toggle_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, value: bool) {
    parent
        .spawn((
            debug_row_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_primary()),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));

            let bg = if value {
                crate::debug::ui::style::debug_active_surface()
            } else {
                debug_control_surface()
            };
            let text = if value { "ON" } else { "OFF" };

            row.spawn((
                TweakToggle {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    min_width: Val::Px(56.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(if value {
                    debug_emphasis_border()
                } else {
                    debug_control_border()
                }),
            ))
            .with_children(|btn| {
                btn.spawn((
                    TweakToggleText {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Pickable::IGNORE,
                    Text::new(text),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_text_primary()),
                ));
            });
        });
}

fn spawn_readonly_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, text: &str) {
    parent
        .spawn((
            debug_row_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_secondary()),
            ));

            row.spawn((
                TweakReadOnlyText {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Text::new(text),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_primary()),
            ));
        });
}

fn spawn_color_preview(parent: &mut ChildSpawnerCommands, folder: &str, prefix: &str) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new("Preview"),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_secondary()),
            ));

            row.spawn((
                ColorPreview {
                    folder: folder.to_string(),
                    prefix: prefix.to_string(),
                },
                Node {
                    width: Val::Px(88.0),
                    height: Val::Px(18.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.5, 0.5, 0.5)),
                BorderColor::all(debug_control_border()),
            ));
        });
}

fn spawn_cycle_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, display: &str) {
    parent
        .spawn((
            debug_row_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_primary()),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));

            row.spawn((
                TweakCycleEnum {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    min_width: Val::Px(124.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04)),
                BorderColor::all(debug_control_border()),
            ))
            .with_children(|btn| {
                btn.spawn((
                    TweakCycleText {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Pickable::IGNORE,
                    Text::new(display),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_text_primary()),
                ));
            });
        });
}

fn spawn_button_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, text: &str) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                TweakButton {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(debug_control_surface()),
                BorderColor::all(debug_emphasis_border()),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Pickable::IGNORE,
                    Text::new(text.to_uppercase()),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_text_primary()),
                ));
            });
        });
}
