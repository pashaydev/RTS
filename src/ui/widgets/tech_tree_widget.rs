use bevy::prelude::*;

use super::core::framework::WidgetId;
use super::core::hud::MainHudRoot;
use super::core::shared::format_cost;
use crate::blueprints::{BlueprintRegistry, EntityKind, LevelBonus};
use crate::types::*;
use crate::ui::theme::{self, Theme};

pub struct TechTreeWidgetPlugin;

impl Plugin for TechTreeWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_tech_tree_widget
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(Update, update_tech_tree.run_if(in_state(AppState::InGame)));
    }
}

widget_spawn_system!(spawn_tech_tree_widget, WidgetId::TechTree);

#[derive(Component)]
pub struct TechTreeContent;

pub fn update_tech_tree(
    mut commands: Commands,
    active_player: Res<ActivePlayer>,
    base_state: Res<FactionBaseState>,
    icons: Res<IconAssets>,
    theme: Res<Theme>,
    widget_q: Query<(&super::widget_framework::Widget, &Children)>,
    content_q: Query<Entity, With<super::widget_framework::WidgetContent>>,
    existing: Query<Entity, With<TechTreeContent>>,
    all_completed: Res<AllCompletedBuildings>,
    blueprint_reg: Res<BlueprintRegistry>,
    registry: Res<super::widget_framework::WidgetRegistry>,
    all_building_levels: Query<(&Faction, &EntityKind, &BuildingLevel), With<Building>>,
) {
    use super::widget_framework::WidgetId;

    if !registry.is_visible(WidgetId::TechTree) {
        return;
    }

    let Some(content) =
        super::widget_framework::find_widget_content(WidgetId::TechTree, &widget_q, &content_q)
    else {
        return;
    };

    // Clear existing
    for entity in &existing {
        commands.entity(entity).try_despawn();
    }

    let completed = all_completed.completed_for(&active_player.0);
    let founded = base_state.is_founded(&active_player.0);
    let building_kinds = blueprint_reg.building_kinds();

    // Collect current building levels for the active player
    let mut current_levels: std::collections::HashMap<EntityKind, u8> =
        std::collections::HashMap::new();
    for (faction, kind, level) in &all_building_levels {
        if *faction == active_player.0 {
            let entry = current_levels.entry(*kind).or_insert(0);
            *entry = (*entry).max(level.0);
        }
    }

    let container = commands
        .spawn((
            TechTreeContent,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(container);

    for kind in &building_kinds {
        if founded && *kind == EntityKind::Base {
            continue;
        }

        let bp = blueprint_reg.get(*kind);
        let building_data = bp.building.as_ref();

        let is_built = completed.contains(kind);
        let prereq = building_data.and_then(|b| b.prerequisite);
        let prereq_met = match (founded, *kind, prereq) {
            (false, EntityKind::Base, _) => true,
            (false, _, _) => false,
            (true, _, None) => true,
            (true, _, Some(p)) => {
                if p == EntityKind::Base {
                    founded || completed.contains(&p)
                } else {
                    completed.contains(&p)
                }
            }
        };

        let (status_color, text_color) = if is_built {
            (theme.colors.success, theme.colors.text_primary)
        } else if prereq_met {
            (theme.colors.warning, theme.colors.text_primary)
        } else {
            (theme.colors.text_disabled, theme.colors.text_disabled)
        };

        let indent = if prereq.is_none() { 0.0 } else { 16.0 };
        let cur_level = current_levels.get(kind).copied().unwrap_or(1);

        // Card container
        let card = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(1.0),
                padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)),
                margin: UiRect::left(Val::Px(indent)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            })
            .insert(BorderColor::all(theme.colors.separator.with_alpha(0.3)))
            .id();
        commands.entity(container).add_child(card);

        // ── Header row: dot + icon + name + trained units ──
        let header = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(4.0),
                row_gap: Val::Px(2.0),
                ..default()
            })
            .id();
        commands.entity(card).add_child(header);

        // Status dot
        let status_dot = commands
            .spawn((
                Node {
                    width: Val::Px(6.0),
                    height: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(status_color),
            ))
            .id();
        commands.entity(header).add_child(status_dot);

        // Icon
        let icon = commands
            .spawn((
                ImageNode::new(icons.entity_icon(*kind)),
                Node {
                    width: Val::Px(20.0),
                    height: Val::Px(20.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(header).add_child(icon);

        // Name
        let name = commands
            .spawn((
                Text::new(kind.display_name()),
                TextFont {
                    font_size: theme::FONT_CAPTION,
                    ..default()
                },
                TextColor(text_color),
            ))
            .id();
        commands.entity(header).add_child(name);

        // Trained units icons
        if let Some(bd) = building_data {
            if !bd.trains.is_empty() {
                let trains_row = commands
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(1.0),
                        margin: UiRect::left(Val::Px(4.0)),
                        ..default()
                    })
                    .id();
                commands.entity(header).add_child(trains_row);

                for train_kind in &bd.trains {
                    let train_icon = commands
                        .spawn((
                            ImageNode::new(icons.entity_icon(*train_kind)),
                            Node {
                                width: Val::Px(12.0),
                                height: Val::Px(12.0),
                                ..default()
                            },
                        ))
                        .id();
                    commands.entity(trains_row).add_child(train_icon);
                }
            }
        }

        // ── Details section ──
        if let Some(bd) = building_data {
            let details = commands
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(1.0),
                    margin: UiRect::left(Val::Px(30.0)),
                    ..default()
                })
                .id();
            commands.entity(card).add_child(details);

            // Build time + cost
            let cost_str = format_cost(&bp.cost);
            let info_str = if cost_str.is_empty() {
                format!("Build: {:.0}s", bd.construction_time_secs)
            } else {
                format!("Build: {:.0}s | {}", bd.construction_time_secs, cost_str)
            };
            let info_text = commands
                .spawn((
                    Text::new(info_str),
                    TextFont {
                        font_size: theme::FONT_TINY,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ))
                .id();
            commands.entity(details).add_child(info_text);

            // Prerequisite info
            if let Some(p) = bd.prerequisite {
                let prereq_text = commands
                    .spawn((
                        Text::new(format!("Requires: {}", p.display_name())),
                        TextFont {
                            font_size: theme::FONT_TINY,
                            ..default()
                        },
                        TextColor(theme.colors.warning),
                    ))
                    .id();
                commands.entity(details).add_child(prereq_text);
            }

            // Level upgrades
            for (i, upgrade) in bd.level_upgrades.iter().enumerate() {
                let target_level = (i + 2) as u8;
                let is_completed = is_built && cur_level >= target_level;
                let is_available = is_built && cur_level == target_level - 1;

                let upgrade_color = if is_completed {
                    theme.colors.success
                } else if is_available {
                    theme.colors.accent
                } else {
                    theme.colors.text_disabled
                };

                let upgrade_cost = format_cost(&upgrade.cost);
                let upgrade_str = format!(
                    "L{}: {} | {:.0}s",
                    target_level, upgrade_cost, upgrade.time_secs
                );

                let upgrade_row = commands
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(0.0),
                        margin: UiRect::top(Val::Px(1.0)),
                        ..default()
                    })
                    .id();
                commands.entity(details).add_child(upgrade_row);

                let cost_text = commands
                    .spawn((
                        Text::new(upgrade_str),
                        TextFont {
                            font_size: theme::FONT_TINY,
                            ..default()
                        },
                        TextColor(upgrade_color),
                    ))
                    .id();
                commands.entity(upgrade_row).add_child(cost_text);

                let bonus_desc = describe_level_bonus(&upgrade.bonus);
                if !bonus_desc.is_empty() {
                    let bonus_text = commands
                        .spawn((
                            Text::new(format!("  {}", bonus_desc)),
                            TextFont {
                                font_size: theme::FONT_TINY,
                                ..default()
                            },
                            TextColor(upgrade_color.with_alpha(0.8)),
                        ))
                        .id();
                    commands.entity(upgrade_row).add_child(bonus_text);
                }
            }
        }
    }

    // Legend
    let legend = commands
        .spawn((
            TechTreeContent,
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                margin: UiRect::top(Val::Px(4.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(legend);

    for (color, label) in [
        (theme.colors.success, "Built"),
        (theme.colors.warning, "Available"),
        (theme.colors.text_disabled, "Locked"),
    ] {
        let item = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                ..default()
            })
            .id();
        commands.entity(legend).add_child(item);

        let dot = commands
            .spawn((
                Node {
                    width: Val::Px(6.0),
                    height: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(color),
            ))
            .id();
        commands.entity(item).add_child(dot);

        let text = commands
            .spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_TINY,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ))
            .id();
        commands.entity(item).add_child(text);
    }
}

fn describe_level_bonus(bonus: &LevelBonus) -> String {
    match bonus {
        LevelBonus::None => String::new(),
        LevelBonus::VisionBoost(v) => format!("+{:.0}% vision range", v * 100.0),
        LevelBonus::TrainTimeMultiplier(m) => {
            let pct = ((1.0 - m) * 100.0).abs();
            if *m < 1.0 {
                format!("{:.0}% faster training", pct)
            } else {
                format!("{:.0}% slower training", pct)
            }
        }
        LevelBonus::TrainedStatBoost { hp_mult, dmg_mult } => {
            format!(
                "Trained units: +{:.0}% HP, +{:.0}% DMG",
                (hp_mult - 1.0) * 100.0,
                (dmg_mult - 1.0) * 100.0
            )
        }
        LevelBonus::RangeAndDamage {
            range_boost,
            damage_boost,
        } => {
            format!("+{:.0} range, +{:.0} damage", range_boost, damage_boost)
        }
        LevelBonus::GatherAura { speed_bonus, range } => {
            format!(
                "Gather aura: +{:.0}% speed, {:.0} range",
                speed_bonus * 100.0,
                range
            )
        }
        LevelBonus::HealAura {
            heal_per_sec,
            range,
        } => {
            format!("Heal aura: {:.1} HP/s, {:.0} range", heal_per_sec, range)
        }
        LevelBonus::UnlocksTraining(kinds) => {
            let names: Vec<&str> = kinds.iter().map(|k| k.display_name()).collect();
            format!("Unlocks: {}", names.join(", "))
        }
        LevelBonus::ProcessorUpgrade {
            harvest_rate_boost,
            radius_boost,
            extra_worker_slots,
            unlock_resources,
        } => {
            let mut parts = vec![];
            if *harvest_rate_boost > 0.0 {
                parts.push(format!("+{:.0}% harvest", harvest_rate_boost * 100.0));
            }
            if *radius_boost > 0.0 {
                parts.push(format!("+{:.0} radius", radius_boost));
            }
            if *extra_worker_slots > 0 {
                parts.push(format!("+{} workers", extra_worker_slots));
            }
            if !unlock_resources.is_empty() {
                let names: Vec<&str> = unlock_resources.iter().map(|r| r.display_name()).collect();
                parts.push(format!("Unlocks: {}", names.join(", ")));
            }
            parts.join(", ")
        }
        LevelBonus::UnlockRecipe {
            recipe_index,
            extra_worker_slots,
        } => {
            let mut s = format!("Unlock recipe #{}", recipe_index + 1);
            if *extra_worker_slots > 0 {
                s += &format!(", +{} workers", extra_worker_slots);
            }
            s
        }
        LevelBonus::ProductionSpeedMultiplier(m) => {
            let pct = ((1.0 - m) * 100.0).abs();
            format!("{:.0}% faster production", pct)
        }
    }
}
