use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::*;

use super::helpers::push_if_missing;
use super::types::*;

// ════════════════════════════════════════════════════════════════════
// System 1: Strategy — State machine transitions & build queue planning
// ════════════════════════════════════════════════════════════════════

pub fn ai_strategy_system(
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    base_state: Res<FactionBaseState>,
    all_completed: Res<AllCompletedBuildings>,
    buildings_q: Query<(&Faction, &EntityKind, &BuildingState), With<Building>>,
    units_q: Query<(&Faction, &EntityKind), With<Unit>>,
) {
    let dt = time.delta_secs();

    for &faction in &ai_controlled.factions {
        if faction == active_player.0 {
            continue;
        }

        let relation = if teams.is_allied(&faction, &active_player.0) {
            AiRelation::Friendly
        } else {
            AiRelation::Enemy
        };

        let brain = ai_state.factions.entry(faction).or_insert_with(|| {
            let idx = Faction::PLAYERS
                .iter()
                .position(|f| *f == faction)
                .unwrap_or(0);
            let personality = match relation {
                AiRelation::Friendly => AiPersonality::Supportive,
                AiRelation::Enemy => match idx % 3 {
                    0 => AiPersonality::Balanced,
                    1 => AiPersonality::Aggressive,
                    _ => AiPersonality::Defensive,
                },
            };
            AiFactionBrain::new_with_offsets(
                idx as f32 * 0.3,
                relation,
                personality,
                AiDifficulty::Medium,
            )
        });

        brain.relation = relation;

        brain.strategy_timer -= dt;
        if brain.strategy_timer > 0.0 {
            continue;
        }
        brain.strategy_timer = brain.effective_tick(STRATEGY_TICK);
        brain.game_time += STRATEGY_TICK;

        // Cache counts
        let mut building_counts: HashMap<EntityKind, usize> = HashMap::new();
        let mut completed_building_counts: HashMap<EntityKind, usize> = HashMap::new();
        for (f, kind, state) in buildings_q.iter() {
            if *f != faction {
                continue;
            }
            *building_counts.entry(*kind).or_default() += 1;
            if *state == BuildingState::Complete {
                *completed_building_counts.entry(*kind).or_default() += 1;
            }
        }

        let mut worker_count = 0usize;
        let mut military_count = 0usize;
        for (f, kind) in units_q.iter() {
            if *f != faction {
                continue;
            }
            if *kind == EntityKind::Worker {
                worker_count += 1;
            } else {
                military_count += 1;
            }
        }

        // Count buildings under construction
        let mut under_construction = 0u8;
        for (f, _, state) in buildings_q.iter() {
            if *f == faction && *state == BuildingState::UnderConstruction {
                under_construction += 1;
            }
        }
        brain.pending_builds = under_construction;

        // ── State machine transitions (with hysteresis) ──
        let defense_interrupt = brain.defense_interrupt;
        let has_base =
            base_state.is_founded(&faction) || all_completed.has(&faction, EntityKind::Base);
        let has_storage = all_completed.has(&faction, EntityKind::Storage);
        let has_sawmill = all_completed.has(&faction, EntityKind::Sawmill);
        let has_barracks = all_completed.has(&faction, EntityKind::Barracks);
        let has_tower = all_completed.has(&faction, EntityKind::WatchTower)
            || all_completed.has(&faction, EntityKind::GuardTower);
        let has_siege_or_temple = all_completed.has(&faction, EntityKind::SiegeWorks)
            || all_completed.has(&faction, EntityKind::Temple);
        let time_in_state = brain.game_time - brain.state_entered_at;
        let rel_strength = brain.relative_strength;

        match brain.top_state {
            AiTopState::Founding => {
                if has_base {
                    brain.try_transition_to(AiTopState::EarlyEconomy);
                }
            }
            AiTopState::EarlyEconomy => {
                if defense_interrupt {
                    brain.defense_interrupt = false;
                    brain.try_transition_to(AiTopState::Defending);
                } else if has_storage && has_sawmill && worker_count >= 4 {
                    brain.try_transition_to(AiTopState::Militarize);
                }
            }
            AiTopState::Militarize => {
                if defense_interrupt {
                    brain.defense_interrupt = false;
                    brain.try_transition_to(AiTopState::Defending);
                } else if has_barracks && military_count >= 3 && has_tower {
                    brain.try_transition_to(AiTopState::Expanding);
                }
            }
            AiTopState::Expanding => {
                if defense_interrupt {
                    brain.defense_interrupt = false;
                    brain.try_transition_to(AiTopState::Defending);
                } else if brain.game_time > 600.0 && has_siege_or_temple {
                    brain.try_transition_to(AiTopState::LateGame);
                } else if rel_strength > brain.attack_strength_threshold()
                    && military_count >= brain.min_attack_army()
                    && (brain.game_time - brain.last_attack_time) > ATTACK_MIN_INTERVAL
                {
                    brain.try_transition_to(AiTopState::Attacking);
                }
            }
            AiTopState::Attacking => {
                if defense_interrupt {
                    brain.defense_interrupt = false;
                    brain.try_transition_to(AiTopState::Defending);
                } else if time_in_state > ATTACK_DURATION {
                    brain.try_transition_to(AiTopState::Expanding);
                }
            }
            AiTopState::Defending => {
                let threats_near_base = brain.known_threats.iter().any(|t| {
                    if let Some(bp) = brain.base_position {
                        t.position.distance(bp) < BASE_THREAT_RADIUS * 2.0
                            && brain.game_time - t.last_seen < 10.0
                    } else {
                        false
                    }
                });
                if !threats_near_base && time_in_state > 10.0 {
                    if military_count < 4 {
                        brain.try_transition_to(AiTopState::Militarize);
                    } else {
                        brain.try_transition_to(AiTopState::Expanding);
                    }
                }
            }
            AiTopState::LateGame => {
                if defense_interrupt {
                    brain.defense_interrupt = false;
                    brain.posture = TacticalPosture::UnderAttack;
                    brain.posture_cooldown = UNDER_ATTACK_COOLDOWN;
                } else if rel_strength > brain.attack_strength_threshold()
                    && military_count >= brain.min_attack_army()
                    && (brain.game_time - brain.last_attack_time) > ATTACK_MIN_INTERVAL
                {
                    brain.attack_ready = true;
                }
            }
        }

        // Posture management
        if brain.posture == TacticalPosture::Retreating && military_count >= 6 {
            brain.posture = TacticalPosture::Normal;
        }
        if brain.posture != TacticalPosture::Normal {
            brain.posture_cooldown -= STRATEGY_TICK;
        }

        // Set desired workers based on income rates (goal-driven)
        let base_workers: i32 = match brain.top_state {
            AiTopState::Founding => 2,
            AiTopState::EarlyEconomy => 5,
            AiTopState::Militarize => 7,
            AiTopState::Expanding => {
                // Scale workers based on income: if income is low, add more workers
                let total_income: f32 = brain.income_rates.iter().sum();
                if total_income < 5.0 {
                    10
                } else if total_income < 15.0 {
                    9
                } else {
                    8
                }
            }
            AiTopState::Attacking => brain.desired_workers as i32, // hold current
            AiTopState::Defending => 6,
            AiTopState::LateGame => {
                let total_income: f32 = brain.income_rates.iter().sum();
                if total_income < 10.0 {
                    10
                } else {
                    8
                }
            }
        };
        if brain.top_state != AiTopState::Attacking {
            brain.desired_workers = (base_workers + brain.difficulty.worker_offset()).max(2) as u8;
        }

        // ── Persistent build queue — only plan when empty or on state change ──
        if brain.build_queue.is_empty() {
            if !has_base && brain.top_state == AiTopState::Founding {
                brain.build_queue.push(BuildRequest {
                    kind: EntityKind::Base,
                    priority: 0,
                    near_position: None,
                });
            } else {
                let tc = &building_counts;
                let player_buildings = if brain.personality == AiPersonality::Supportive {
                    let mut pb: HashMap<EntityKind, usize> = HashMap::new();
                    for (f, kind, state) in buildings_q.iter() {
                        if *f == active_player.0 && *state == BuildingState::Complete {
                            *pb.entry(*kind).or_default() += 1;
                        }
                    }
                    Some(pb)
                } else {
                    None
                };
                plan_builds_for_state(brain, tc, player_buildings.as_ref());
            }
            brain.build_queue.sort_by_key(|r| r.priority);
        }

        // Set resource goal from first build queue item
        if let Some(first) = brain.build_queue.first() {
            let kind = first.kind;
            brain.resource_goal = Some(ResourceGoal {
                wood: 0,
                copper: 0,
                iron: 0,
                gold: 0,
                oil: 0,
            });
            let _ = kind;
        }

        // Attack readiness for states that use it
        match brain.top_state {
            AiTopState::Attacking | AiTopState::LateGame => {}
            _ => {
                brain.attack_ready = false;
            }
        }
    }
}

// ── Per-state build planning ──

fn plan_builds_for_state(
    brain: &mut AiFactionBrain,
    tc: &HashMap<EntityKind, usize>,
    player_buildings: Option<&HashMap<EntityKind, usize>>,
) {
    match brain.top_state {
        AiTopState::Founding => {
            push_if_missing(brain, tc, EntityKind::Base, 1, 0);
        }
        AiTopState::EarlyEconomy => {
            push_if_missing(brain, tc, EntityKind::Storage, 1, 0);
            push_if_missing(brain, tc, EntityKind::Sawmill, 1, 1);
        }
        AiTopState::Militarize => {
            push_if_missing(brain, tc, EntityKind::Barracks, 1, 0);
            match brain.personality {
                AiPersonality::Aggressive => {
                    push_if_missing(brain, tc, EntityKind::Barracks, 2, 1);
                }
                AiPersonality::Defensive => {
                    push_if_missing(brain, tc, EntityKind::WatchTower, 2, 1);
                }
                _ => {
                    push_if_missing(brain, tc, EntityKind::WatchTower, 1, 1);
                }
            }
        }
        AiTopState::Expanding => {
            match brain.personality {
                AiPersonality::Balanced => {
                    push_if_missing(brain, tc, EntityKind::Workshop, 1, 0);
                    push_if_missing(brain, tc, EntityKind::Stable, 1, 1);
                    push_if_missing(brain, tc, EntityKind::GuardTower, 2, 2);
                    push_if_missing(brain, tc, EntityKind::Mine, 1, 3);
                }
                AiPersonality::Aggressive => {
                    push_if_missing(brain, tc, EntityKind::Barracks, 2, 0);
                    push_if_missing(brain, tc, EntityKind::Stable, 1, 1);
                    push_if_missing(brain, tc, EntityKind::Workshop, 1, 2);
                    push_if_missing(brain, tc, EntityKind::Mine, 1, 3);
                }
                AiPersonality::Defensive => {
                    push_if_missing(brain, tc, EntityKind::GuardTower, 3, 0);
                    push_if_missing(brain, tc, EntityKind::Mine, 1, 1);
                    push_if_missing(brain, tc, EntityKind::Workshop, 1, 2);
                    push_if_missing(brain, tc, EntityKind::MageTower, 1, 3);
                }
                AiPersonality::Economic => {
                    push_if_missing(brain, tc, EntityKind::Mine, 1, 0);
                    push_if_missing(brain, tc, EntityKind::Storage, 2, 1);
                    push_if_missing(brain, tc, EntityKind::Sawmill, 2, 1);
                    push_if_missing(brain, tc, EntityKind::Workshop, 1, 2);
                    push_if_missing(brain, tc, EntityKind::Stable, 1, 3);
                }
                AiPersonality::Supportive => {
                    if let Some(pb) = player_buildings {
                        let player_has_barracks =
                            pb.get(&EntityKind::Barracks).copied().unwrap_or(0) >= 2;
                        let player_has_workshop =
                            pb.get(&EntityKind::Workshop).copied().unwrap_or(0) > 0;
                        if player_has_barracks {
                            push_if_missing(brain, tc, EntityKind::Workshop, 1, 0);
                            push_if_missing(brain, tc, EntityKind::Stable, 1, 1);
                        } else {
                            push_if_missing(brain, tc, EntityKind::Barracks, 2, 0);
                        }
                        if !player_has_workshop {
                            push_if_missing(brain, tc, EntityKind::Workshop, 1, 1);
                        }
                    }
                    push_if_missing(brain, tc, EntityKind::GuardTower, 2, 2);
                    push_if_missing(brain, tc, EntityKind::Mine, 1, 3);
                }
            }
        }
        AiTopState::Attacking => {
            // Don't queue new buildings during attack
        }
        AiTopState::Defending => {
            // Emergency defenses
            push_if_missing(brain, tc, EntityKind::WatchTower, 2, 0);
            push_if_missing(brain, tc, EntityKind::GuardTower, 2, 1);
        }
        AiTopState::LateGame => {
            match brain.personality {
                AiPersonality::Aggressive => {
                    push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 0);
                    push_if_missing(brain, tc, EntityKind::Barracks, 3, 1);
                    push_if_missing(brain, tc, EntityKind::GuardTower, 2, 2);
                    push_if_missing(brain, tc, EntityKind::OilRig, 1, 3);
                }
                AiPersonality::Defensive => {
                    push_if_missing(brain, tc, EntityKind::Temple, 1, 0);
                    push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 1);
                    push_if_missing(brain, tc, EntityKind::GuardTower, 4, 2);
                    push_if_missing(brain, tc, EntityKind::Storage, 2, 3);
                }
                AiPersonality::Economic => {
                    push_if_missing(brain, tc, EntityKind::OilRig, 1, 0);
                    push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 1);
                    push_if_missing(brain, tc, EntityKind::Temple, 1, 2);
                    push_if_missing(brain, tc, EntityKind::GuardTower, 4, 3);
                }
                _ => {
                    // Balanced & Supportive
                    push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 0);
                    push_if_missing(brain, tc, EntityKind::Temple, 1, 1);
                    push_if_missing(brain, tc, EntityKind::GuardTower, 4, 2);
                    push_if_missing(brain, tc, EntityKind::OilRig, 1, 2);
                    push_if_missing(brain, tc, EntityKind::Storage, 2, 3);
                }
            }
        }
    }
}
