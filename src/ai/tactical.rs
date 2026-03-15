use bevy::prelude::*;
use std::collections::HashSet;

use crate::blueprints::EntityKind;
use crate::components::*;

use super::helpers::update_threat;
use super::types::*;

// ════════════════════════════════════════════════════════════════════
// System 4: Tactical — Fast reactions, threat detection, intel, defense
// ════════════════════════════════════════════════════════════════════

pub fn ai_tactical_system(
    mut commands: Commands,
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    mut notifications: ResMut<AllyNotifications>,
    own_entities_q: Query<
        (Entity, &Faction, &Transform, &Health),
        Or<(With<Unit>, With<Building>)>,
    >,
    enemy_units_q: Query<
        (Entity, &Faction, &EntityKind, &Transform, &Health, Option<&AttackDamage>),
        Or<(With<Unit>, With<Mob>)>,
    >,
    buildings_q: Query<(&Faction, &Transform), With<Building>>,
) {
    let dt = time.delta_secs();

    for &faction in &ai_controlled.factions {
        if faction == active_player.0 {
            continue;
        }

        let is_friendly = teams.is_allied(&faction, &active_player.0);

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.tactical_timer -= dt;
        if brain.tactical_timer > 0.0 {
            continue;
        }
        brain.tactical_timer = brain.effective_tick(TACTICAL_TICK);

        let base_pos = match brain.base_position {
            Some(p) => p,
            None => continue,
        };

        let game_time = brain.game_time;

        // ── Update enemy intelligence ──
        brain.enemy_composition.clear();
        brain.enemy_strength = 0.0;
        let mut enemies_near_base: u32 = 0;

        for (_, ef, ekind, etf, ehealth, edamage) in enemy_units_q.iter() {
            if !teams.is_hostile(&faction, ef) {
                continue;
            }
            *brain.enemy_composition.entry(*ekind).or_default() += 1;
            let unit_strength = ehealth.current * edamage.map_or(5.0, |d: &AttackDamage| d.0);
            brain.enemy_strength += unit_strength;

            if etf.translation.distance(base_pos) < BASE_THREAT_RADIUS * 1.5 {
                enemies_near_base += 1;
            }
        }

        // ── Defense interrupt ──
        if enemies_near_base >= DEFENSE_INTERRUPT_COUNT {
            brain.defense_interrupt = true;
        }

        // ── Detect damage on own entities ──
        let mut threats_detected = false;
        let mut threat_positions: Vec<Vec3> = Vec::new();

        for (entity, f, tf, health) in own_entities_q.iter() {
            if *f != faction {
                continue;
            }
            let prev = brain.prev_health.get(&entity).copied();
            brain.prev_health.insert(entity, health.current);

            if let Some(prev_hp) = prev {
                if health.current < prev_hp {
                    let pos = tf.translation;
                    for (_, ef, _, etf, _, _) in enemy_units_q.iter() {
                        if !teams.is_hostile(&faction, ef) {
                            continue;
                        }
                        if etf.translation.distance(pos) < 25.0 {
                            threat_positions.push(etf.translation);
                            threats_detected = true;
                        }
                    }
                }
            }
        }

        // ── Friendly AI: also detect threats near player's base ──
        let mut player_base_pos = None;
        if is_friendly {
            for (f, tf) in buildings_q.iter() {
                if *f == active_player.0 {
                    player_base_pos = Some(tf.translation);
                    break;
                }
            }

            if let Some(pbp) = player_base_pos {
                detect_threats_near_ally(
                    brain,
                    &enemy_units_q,
                    &teams,
                    &faction,
                    pbp,
                    game_time,
                    &mut threats_detected,
                    &mut threat_positions,
                    &mut notifications,
                );
            }
        }

        // ── Update threat map from visible enemies near base ──
        for (_, ef, _, etf, health, damage) in enemy_units_q.iter() {
            if !teams.is_hostile(&faction, ef) {
                continue;
            }
            let pos = etf.translation;
            if pos.distance(base_pos) < 100.0 {
                let strength = health.current * damage.map_or(5.0, |d: &AttackDamage| d.0);
                update_threat(&mut brain.known_threats, pos, strength, game_time);
                if pos.distance(base_pos) < BASE_THREAT_RADIUS {
                    threats_detected = true;
                    threat_positions.push(pos);
                }
            }
        }

        // ── Trigger UnderAttack ──
        if threats_detected && brain.posture == TacticalPosture::Normal {
            brain.posture = TacticalPosture::UnderAttack;
            brain.posture_cooldown = UNDER_ATTACK_COOLDOWN;

            if is_friendly {
                let threat_center = if !threat_positions.is_empty() {
                    let sum: Vec3 = threat_positions.iter().copied().sum();
                    sum / threat_positions.len() as f32
                } else {
                    base_pos
                };
                notifications.push(
                    AllyNotifyKind::UnderAttack,
                    "Ally is under attack!".to_string(),
                    Some(threat_center),
                    game_time,
                );
            }
        }

        // ── Defensive response ──
        if brain.posture == TacticalPosture::UnderAttack {
            let threat_center = if !threat_positions.is_empty() {
                let sum: Vec3 = threat_positions.iter().copied().sum();
                sum / threat_positions.len() as f32
            } else {
                base_pos
            };

            // Build set of alive entities for command guards
            let alive: HashSet<Entity> = own_entities_q
                .iter()
                .filter(|(_, f, _, _)| **f == faction)
                .map(|(e, _, _, _)| e)
                .collect();

            // Recall defense squad to threat
            defend_own_base(&mut commands, brain, threat_center, &alive);

            // Friendly AI: also defend player's base area
            if is_friendly {
                if let Some(pbp) = player_base_pos {
                    defend_ally_base(&mut commands, brain, &threat_positions, pbp, &alive);
                }
            }
        }

        // ── Posture cooldown ──
        if brain.posture == TacticalPosture::UnderAttack {
            brain.posture_cooldown -= TACTICAL_TICK;
            if brain.posture_cooldown <= 0.0 && !threats_detected {
                brain.posture = TacticalPosture::Normal;
            }
        }

        // ── Decay old threats ──
        brain
            .known_threats
            .retain(|t| game_time - t.last_seen < THREAT_DECAY_SECS);
    }
}

/// Detect threats near an allied player's base and record them.
fn detect_threats_near_ally(
    brain: &mut AiFactionBrain,
    enemy_units_q: &Query<
        (Entity, &Faction, &EntityKind, &Transform, &Health, Option<&AttackDamage>),
        Or<(With<Unit>, With<Mob>)>,
    >,
    teams: &TeamConfig,
    faction: &Faction,
    player_base_pos: Vec3,
    game_time: f32,
    threats_detected: &mut bool,
    threat_positions: &mut Vec<Vec3>,
    notifications: &mut ResMut<AllyNotifications>,
) {
    for (_, ef, _, etf, health, damage) in enemy_units_q.iter() {
        if !teams.is_hostile(faction, ef) {
            continue;
        }
        let pos = etf.translation;
        if pos.distance(player_base_pos) < BASE_THREAT_RADIUS * 1.5 {
            let strength = health.current * damage.map_or(5.0, |d: &AttackDamage| d.0);
            update_threat(&mut brain.known_threats, pos, strength, game_time);
            *threats_detected = true;
            threat_positions.push(pos);

            notifications.push(
                AllyNotifyKind::EnemySpotted,
                "Ally spotted enemies near your base!".to_string(),
                Some(pos),
                game_time,
            );
        }
    }
}

/// Recall defense and attack squads to own base threat center.
fn defend_own_base(commands: &mut Commands, brain: &AiFactionBrain, threat_center: Vec3, alive: &HashSet<Entity>) {
    let mut recall_entities: Vec<Entity> = Vec::new();
    if let Some(squad) = brain.get_squad(SquadRole::DefenseSquad) {
        recall_entities.extend(&squad.members);
    }
    if let Some(squad) = brain.get_squad(SquadRole::AttackSquad) {
        recall_entities.extend(&squad.members);
    }

    for entity in &recall_entities {
        if alive.contains(entity) {
            commands.entity(*entity).insert(MoveTarget(threat_center));
        }
    }
}

/// Send defense squad to defend an allied player's base from nearby threats.
fn defend_ally_base(
    commands: &mut Commands,
    brain: &AiFactionBrain,
    threat_positions: &[Vec3],
    player_base_pos: Vec3,
    alive: &HashSet<Entity>,
) {
    let player_threats: Vec<Vec3> = threat_positions
        .iter()
        .filter(|p| p.distance(player_base_pos) < BASE_THREAT_RADIUS * 2.0)
        .copied()
        .collect();

    if !player_threats.is_empty() {
        let player_threat_center: Vec3 =
            player_threats.iter().copied().sum::<Vec3>() / player_threats.len() as f32;
        if let Some(squad) = brain.get_squad(SquadRole::DefenseSquad) {
            for &entity in &squad.members {
                if alive.contains(&entity) {
                    commands
                        .entity(entity)
                        .insert(MoveTarget(player_threat_center));
                }
            }
        }
    }
}
