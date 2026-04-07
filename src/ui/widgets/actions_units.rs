use bevy::prelude::*;

use super::core::constants::*;
use super::core::shared::{widget_content_stack, widget_wrap_row};
use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme::Theme;

pub(super) fn spawn_units_action_bar(
    commands: &mut Commands,
    parent: Entity,
    selected_units: &Query<
        (
            &EntityKind,
            Option<&Carrying>,
            Option<&CarryCapacity>,
            Option<&UnitState>,
        ),
        (With<Unit>, With<Selected>),
    >,
    layout_bucket: u8,
    formation: &ActiveFormation,
    theme: &Theme,
) {
    let container = commands
        .spawn((widget_content_stack(), Interaction::None))
        .id();
    commands.entity(parent).add_child(container);

    let unit_count = selected_units.iter().count();
    let worker_count = selected_units
        .iter()
        .filter(|(k, ..)| **k == EntityKind::Worker)
        .count();

    let label_text = if worker_count == unit_count && worker_count > 0 {
        format!(
            "{} Worker{}",
            worker_count,
            if worker_count > 1 { "s" } else { "" }
        )
    } else {
        format!(
            "{} unit{} selected",
            unit_count,
            if unit_count > 1 { "s" } else { "" }
        )
    };

    let label = commands
        .spawn((
            Text::new(label_text),
            TextFont {
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ))
        .id();
    commands.entity(container).add_child(label);

    if unit_count == 1 {
        if let Some((kind, carrying, capacity, worker_state)) = selected_units.iter().next() {
            if *kind == EntityKind::Worker {
                if let (Some(carry), Some(cap)) = (carrying, capacity) {
                    if carry.amount > 0 {
                        let rt_name = carry
                            .resource_type
                            .map(|rt| rt.display_name())
                            .unwrap_or("Nothing");
                        let carry_text =
                            format!("Carrying: {:.1}/{:.0} {}", carry.weight, cap.0, rt_name);
                        let carry_label = commands
                            .spawn((
                                Text::new(carry_text),
                                TextFont {
                                    font_size: theme.typography.medium,
                                    ..default()
                                },
                                TextColor(theme.colors.warning),
                            ))
                            .id();
                        commands.entity(container).add_child(carry_label);

                        let bar_bg = commands
                            .spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    max_width: Val::Px(220.0),
                                    height: Val::Px(6.0),
                                    // border_radius: RADIUS_SM,
                                    ..default()
                                },
                                BackgroundColor(crate::theme::BG_RECESSED.with_alpha(0.8)),
                            ))
                            .id();
                        commands.entity(container).add_child(bar_bg);

                        let fill_frac = (carry.weight / cap.0).min(1.0);
                        let fill = commands
                            .spawn((
                                Node {
                                    width: Val::Percent(fill_frac * 100.0),
                                    height: Val::Percent(100.0),
                                    // border_radius: RADIUS_SM,
                                    ..default()
                                },
                                BackgroundColor(crate::theme::PRESTIGE),
                            ))
                            .id();
                        commands.entity(bar_bg).add_child(fill);
                    }

                    if let Some(state) = worker_state {
                        let state_text = match state {
                            UnitState::Idle => "Idle",
                            UnitState::Moving(_) => "Moving",
                            UnitState::Gathering(_) => "Gathering",
                            UnitState::ReturningToDeposit { .. } => "Returning to depot",
                            UnitState::Depositing { .. } => "Depositing",
                            UnitState::MovingToPlot(_) => "Going to plot building",
                            UnitState::MovingToBuild(_) => "Moving to build",
                            UnitState::Building(_) => "Building",
                            UnitState::WaitingForStorage { .. } => "Storage full!",
                            UnitState::AssignedGathering { .. } => "Working at building",
                            UnitState::Attacking(_) => "Attacking",
                            UnitState::AttackMoving(_) => "Attack moving",
                            UnitState::Patrolling { .. } => "Patrolling",
                            UnitState::HoldPosition => "Holding position",
                        };
                        let state_label = commands
                            .spawn((
                                Text::new(state_text),
                                TextFont {
                                    font_size: theme.typography.body,
                                    ..default()
                                },
                                TextColor(theme.colors.text_secondary),
                            ))
                            .id();
                        commands.entity(container).add_child(state_label);
                    }
                }
            }
        }
    }

    // --- Command buttons row (for all units) ---
    let cmd_row = commands
        .spawn(Node {
            margin: UiRect::top(Val::Px(6.0)),
            ..widget_wrap_row(4.0, 4.0)
        })
        .id();
    commands.entity(container).add_child(cmd_row);

    let cmd_min_width = match layout_bucket {
        0 => Val::Percent(48.0),
        1 => Val::Px(116.0),
        _ => Val::Px(128.0),
    };

    struct CmdBtn {
        label: &'static str,
        tooltip: &'static str,
    }
    let cmd_defs = [
        CmdBtn {
            label: "Attack (F)",
            tooltip: "Attack-Move (F)\nClick a location to move while engaging enemies",
        },
        CmdBtn {
            label: "Patrol (P)",
            tooltip: "Patrol (P)\nClick a location to patrol between current position and target",
        },
        CmdBtn {
            label: "Hold (H)",
            tooltip: "Hold Position (H)\nStop and hold current position",
        },
        CmdBtn {
            label: "Stop (X)",
            tooltip: "Stop (X)\nClear all orders",
        },
        CmdBtn {
            label: "Stance (V)",
            tooltip: "Cycle Stance (V)\nCycle between Passive / Defensive / Aggressive",
        },
    ];

    for (i, def) in cmd_defs.iter().enumerate() {
        let mut btn = commands.spawn((
            Button,
            ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
            ButtonStyle::Filled,
            ActionTooltipTrigger {
                text: def.tooltip.to_string(),
            },
            Node {
                min_width: cmd_min_width,
                flex_grow: if layout_bucket == 0 { 1.0 } else { 0.0 },
                padding: PAD_BUTTON,
                // border_radius: RADIUS_MD,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ));
        match i {
            0 => {
                btn.insert(CommandModeButton(CommandMode::AttackMove));
            }
            1 => {
                btn.insert(CommandModeButton(CommandMode::Patrol));
            }
            2 => {
                btn.insert(HoldPositionButton);
            }
            3 => {
                btn.insert(StopButton);
            }
            4 => {
                btn.insert(CycleStanceButton);
            }
            _ => {}
        }
        let btn_id = btn
            .with_children(|b| {
                b.spawn((
                    Text::new(def.label),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_primary),
                ));
            })
            .id();
        commands.entity(cmd_row).add_child(btn_id);
    }

    // "Drop Cargo" button — shown when any selected worker is carrying resources
    let any_carrying = worker_count > 0
        && selected_units
            .iter()
            .any(|(k, c, _, _)| *k == EntityKind::Worker && c.map_or(false, |c| c.amount > 0));
    if any_carrying {
        let drop_btn = commands
            .spawn((
                Button,
                DropCargoButton,
                ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                ButtonStyle::Filled,
                ActionTooltipTrigger {
                    text: "Drop Cargo\nDiscard carried resources on the ground".to_string(),
                },
                Node {
                    margin: UiRect::top(Val::Px(6.0)),
                    align_self: AlignSelf::FlexStart,
                    padding: PAD_BUTTON,
                    // border_radius: RADIUS_MD,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("Drop Cargo"),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.warning),
                ));
            })
            .id();
        commands.entity(container).add_child(drop_btn);
    }

    // --- Formation toggle button ---
    if unit_count > 1 {
        let form_label = format!("Formation: {} (G)", formation.formation.display_name());
        let form_btn = commands
            .spawn((
                Button,
                CycleFormationButton,
                ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                ButtonStyle::Filled,
                ActionTooltipTrigger {
                    text: "Cycle Formation (G)\nLine → Grid → Chess".to_string(),
                },
                Node {
                    margin: UiRect::top(Val::Px(6.0)),
                    align_self: AlignSelf::FlexStart,
                    padding: PAD_BUTTON,
                    // border_radius: RADIUS_MD,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(form_label),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
            })
            .id();
        commands.entity(container).add_child(form_btn);
    }

    // --- Ability buttons (shown when a unit with abilities is selected) ---
    {
        let mut shown_abilities = std::collections::HashSet::new();
        for (kind, _, _, _) in selected_units.iter() {
            let unit_abilities: Vec<AbilityId> = match *kind {
                EntityKind::Knight => vec![AbilityId::KnightCharge],
                EntityKind::Mage => vec![AbilityId::MageFireball, AbilityId::MageFrostNova],
                EntityKind::Priest => vec![AbilityId::PriestHeal, AbilityId::PriestHolySmite],
                EntityKind::Catapult => vec![AbilityId::CatapultAoeBoulder],
                _ => vec![],
            };
            for a in unit_abilities {
                shown_abilities.insert(a);
            }
        }

        if !shown_abilities.is_empty() {
            let ability_label = commands
                .spawn((
                    Text::new("Abilities"),
                    TextFont {
                        font_size: theme.typography.body,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                    Node {
                        margin: UiRect::top(Val::Px(8.0)),
                        ..default()
                    },
                ))
                .id();
            commands.entity(container).add_child(ability_label);

            let ability_row = commands
                .spawn(Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    ..widget_wrap_row(4.0, 4.0)
                })
                .id();
            commands.entity(container).add_child(ability_row);

            // Sort abilities for consistent display
            let mut abilities_sorted: Vec<AbilityId> = shown_abilities.into_iter().collect();
            abilities_sorted.sort_by_key(|a| format!("{:?}", a));

            for ability in abilities_sorted {
                let label = format!("{} ({})", ability.display_name(), ability.hotkey());
                let btn = commands
                    .spawn((
                        Button,
                        AbilityButton(ability),
                        ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                        ButtonStyle::Filled,
                        ActionTooltipTrigger {
                            text: format!(
                                "{}\n{}\nCooldown: {:.0}s",
                                ability.display_name(),
                                ability.description(),
                                ability.cooldown_secs()
                            ),
                        },
                        Node {
                            min_width: cmd_min_width,
                            flex_grow: if layout_bucket == 0 { 1.0 } else { 0.0 },
                            padding: PAD_BUTTON,
                            // border_radius: RADIUS_MD,
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(label),
                            TextFont {
                                font_size: theme.typography.body,
                                ..default()
                            },
                            TextColor(crate::theme::TEXT_SECONDARY),
                        ));
                    })
                    .id();
                commands.entity(ability_row).add_child(btn);
            }
        }
    }

    if worker_count > 0 && worker_count == unit_count {
        let scuttle_btn = commands
            .spawn((
                Button,
                ScuttleUnitButton,
                ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                ButtonStyle::Destructive,
                ActionTooltipTrigger {
                    text: "Scuttle selected worker(s)\nDestroys the unit and loses any carried resources".to_string(),
                },
                Node {
                    margin: UiRect::top(Val::Px(6.0)),
                    align_self: AlignSelf::FlexStart,
                    padding: PAD_BUTTON,
                    // border_radius: RADIUS_MD,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("Scuttle Worker"),
                    TextFont { font_size: theme.typography.body, ..default() },
                    TextColor(theme.colors.destructive),
                ));
            })
            .id();
        commands.entity(container).add_child(scuttle_btn);
    }
}
