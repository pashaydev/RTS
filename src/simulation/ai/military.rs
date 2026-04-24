//! AI military: unit training queues, army composition, squad staging,
//! and attack orders dispatched against enemy targets.

use bevy::prelude::*;
use bevy::time::Fixed;
use std::collections::HashMap;

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::types::*;

use super::helpers::*;
use super::types::*;
use super::AiWorldSnapshot;

/// Minimum squad size before committing to an attack (staging requirement)
const ATTACK_STAGING_MIN: usize = 4;
/// Distance from rally point within which a unit is considered "staged"
const STAGING_RADIUS: f32 = 25.0;

// ════════════════════════════════════════════════════════════════════
// System 3: Military — Army composition, squads, attacks, scouting
// ════════════════════════════════════════════════════════════════════

pub fn ai_military_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    config: Res<GameSetupConfig>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    snapshot: Res<AiWorldSnapshot>,
    mut all_resources: ResMut<AllPlayerResources>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    mut notifications: ResMut<AllyNotifications>,
    queries: (
        Query<&Faction, With<Unit>>,
        Query<(Entity, &Faction, &EntityKind, &Transform), (With<Unit>, Without<Building>)>,
        Query<
            (
                Entity,
                &Faction,
                &EntityKind,
                &Transform,
                &UnitState,
                Option<&crate::simulation::combat::UnitBrain>,
            ),
            (
                With<Unit>,
                Without<MoveTarget>,
                Without<Building>,
            ),
        >,
        Query<&Health>,
        Query<
            (
                &Faction,
                &Transform,
                Option<&crate::infrastructure::net_bridge::NetworkId>,
            ),
            (With<Building>, Without<FloorTile>),
        >,
        Query<(&Faction, &EntityKind, &BuildingState, &BuildingLevel), With<Building>>,
        Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
    ),
    building_detail_q: Query<
        (&Faction, &EntityKind, &Transform, Option<&ConstructionProgress>),
        (With<Building>, Without<FloorTile>),
    >,
) {
    let (
        all_unit_factions_q,
        units_q,
        idle_military_q,
        health_q,
        enemy_buildings_q,
        building_levels_q,
        mut train_queues,
    ) = queries;
    let dt = time.delta_secs();
    let is_idle_military = |entity| {
        idle_military_q
            .get(entity)
            .ok()
            .is_some_and(|(_, _, _, _, _, brain)| brain.map_or(true, |brain| brain.target.is_none()))
    };

    for &faction in &ai_controlled.factions {
        if !faction_uses_ai(&config, faction) {
            continue;
        }
        if faction == active_player.0 {
            continue;
        }

        let is_friendly = teams.is_allied(&faction, &active_player.0);

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.military_timer -= dt;
        if brain.military_timer > 0.0 {
            continue;
        }
        brain.military_timer = brain.effective_tick(MILITARY_TICK);

        let Some(faction_snapshot) = snapshot.factions.get(&faction) else {
            continue;
        };
        let base_pos = faction_snapshot
            .base_position
            .or(brain.base_position)
            .unwrap_or_else(|| Vec3::ZERO);
        brain.base_position = Some(base_pos);

        // Prune dead entities
        let alive: std::collections::HashSet<Entity> =
            faction_snapshot.unit_entities.iter().copied().collect();
        brain.prune_dead(&alive);

        // Count our units by type
        let unit_counts: HashMap<EntityKind, usize> = faction_snapshot.unit_counts.clone();
        let military_count = faction_snapshot.military_count;
        let mut total_own_strength: f32 = 0.0;
        for &(entity, _, _) in &faction_snapshot.military_entities {
            if let Ok(h) = health_q.get(entity) {
                total_own_strength += h.current;
            }
        }

        // Compute relative strength
        let enemy_str = brain.enemy_strength.max(1.0);
        brain.relative_strength = if enemy_str > 0.0 {
            total_own_strength / enemy_str
        } else {
            if military_count > 0 {
                10.0
            } else {
                0.0
            }
        };

        // Check for retreating posture
        if brain.posture == TacticalPosture::Normal && military_count < 4 && brain.game_time > 120.0
        {
            brain.posture = TacticalPosture::Retreating;
            brain.posture_cooldown = 20.0;
        }

        let top_state = brain.top_state;
        let personality = brain.personality;

        // ── Counter-composition training ──
        let difficulty = brain.difficulty;
        let wave_bias = brain.wave_counter_bias;
        let desired_composition: Vec<(EntityKind, usize)> = get_desired_composition_with_intel(
            top_state,
            personality,
            is_friendly,
            &snapshot,
            active_player.0,
            &brain.enemy_composition,
            difficulty,
            wave_bias,
        );

        // Find most under-represented unit type and train it
        let mut best_deficit: Option<(EntityKind, f32)> = None;
        for (kind, desired) in &desired_composition {
            let current = unit_counts.get(kind).copied().unwrap_or(0);
            if current < *desired {
                let deficit = (*desired - current) as f32 / *desired as f32;
                if best_deficit.is_none() || deficit > best_deficit.unwrap().1 {
                    best_deficit = Some((*kind, deficit));
                }
            }
        }

        if let Some((unit_kind, _)) = best_deficit {
            let bp = registry.get(unit_kind);
            let carried = carried_totals.get(&faction);
            if bp
                .cost
                .can_afford_with_carried(all_resources.get(&faction), carried)
            {
                if try_train(
                    &mut train_queues,
                    &faction,
                    unit_kind,
                    &registry,
                    &all_unit_factions_q,
                    &building_levels_q,
                ) {
                    let deficits = bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                    let drain = SpendFromCarried {
                        faction,
                        amounts: deficits,
                    };
                    if drain.has_deficit() {
                        pending_drains.drains.push(drain);
                    }
                }
            }
        }

        // ── Assign unassigned military to squads ──
        let mut unassigned: Vec<(Entity, EntityKind, Vec3)> = Vec::new();
        for &(entity, kind, pos) in &faction_snapshot.military_entities {
            if !brain.assigned_units.contains_key(&entity) {
                unassigned.push((entity, kind, pos));
            }
        }

        // State-aware squad assignment
        match top_state {
            AiTopState::Defending => {
                for (entity, _, _) in &unassigned {
                    brain.add_to_squad(*entity, SquadRole::DefenseSquad);
                    commands.entity(*entity).insert(MoveTarget(base_pos));
                }
            }
            _ => {
                for (entity, _, _) in &unassigned {
                    let defense_size = brain.squad_size(SquadRole::DefenseSquad);
                    if defense_size < DEFENSE_SQUAD_SIZE {
                        brain.add_to_squad(*entity, SquadRole::DefenseSquad);
                        commands.entity(*entity).insert(MoveTarget(base_pos));
                    } else {
                        brain.add_to_squad(*entity, SquadRole::AttackSquad);
                    }
                }
            }
        }

        // ── Scouting (Expanding+) ──
        if matches!(
            top_state,
            AiTopState::Expanding | AiTopState::LateGame | AiTopState::Militarize
        ) && brain.squad_size(SquadRole::Scout) == 0
        {
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

            let scout_candidate = attack_members
                .iter()
                .find(|&&e| {
                    units_q
                        .get(e)
                        .map_or(false, |(_, _, k, _)| *k == EntityKind::Cavalry)
                })
                .or_else(|| attack_members.first());

            if let Some(&scout_entity) = scout_candidate {
                brain.remove_from_squad(scout_entity);
                brain.add_to_squad(scout_entity, SquadRole::Scout);

                if brain.scout_route.is_empty() {
                    brain.scout_route = compute_scout_route(base_pos);
                }
            }
        }

        // Move scout
        brain.scout_timer -= MILITARY_TICK;
        if brain.scout_timer <= 0.0 {
            brain.scout_timer = SCOUT_TICK;
            let route = brain.scout_route.clone();
            let waypoint_idx = brain.next_scout_waypoint;
            if !route.is_empty() {
                if let Some(squad) = brain.get_squad(SquadRole::Scout) {
                    for &entity in &squad.members {
                        if units_q.get(entity).is_ok() {
                            let wp = route[waypoint_idx % route.len()];
                            commands.entity(entity).insert(MoveTarget(wp));
                        }
                    }
                }
                brain.next_scout_waypoint = (waypoint_idx + 1) % route.len().max(1);
            }
        }

        // ── Harassment raids (Aggressive personality, Expanding+, enemy only) ──
        if !is_friendly
            && personality == AiPersonality::Aggressive
            && matches!(top_state, AiTopState::Expanding | AiTopState::LateGame)
        {
            brain.raid_cooldown -= MILITARY_TICK;
            if brain.raid_cooldown <= 0.0 && brain.squad_size(SquadRole::Raider) == 0 {
                let attack_members: Vec<Entity> = brain
                    .get_squad(SquadRole::AttackSquad)
                    .map(|s| s.members.clone())
                    .unwrap_or_default();

                let mut raiders: Vec<Entity> = Vec::new();
                for &e in &attack_members {
                    if raiders.len() >= 3 {
                        break;
                    }
                    if units_q
                        .get(e)
                        .map_or(false, |(_, _, k, _)| *k == EntityKind::Cavalry)
                    {
                        raiders.push(e);
                    }
                }
                for &e in &attack_members {
                    if raiders.len() >= 2 {
                        break;
                    }
                    if !raiders.contains(&e) {
                        raiders.push(e);
                    }
                }

                if raiders.len() >= 2 {
                    for &e in &raiders {
                        brain.remove_from_squad(e);
                        brain.add_to_squad(e, SquadRole::Raider);
                    }

                    // Build harass input snapshots (deterministic: iterated in
                    // entity order, then sorted inside select_harass_target).
                    let mut enemy_workers: Vec<(Vec3, u64)> = Vec::new();
                    let mut enemy_military_pos: Vec<Vec3> = Vec::new();
                    for (uent, uf, ukind, utf) in units_q.iter() {
                        if !teams.is_hostile(&faction, uf) || *uf == Faction::Neutral {
                            continue;
                        }
                        if *ukind == EntityKind::Worker {
                            enemy_workers.push((utf.translation, uent.to_bits()));
                        } else {
                            enemy_military_pos.push(utf.translation);
                        }
                    }
                    let mut enemy_bld_list: Vec<HarassBuilding> = Vec::new();
                    for (bf, bk, btf, progress) in building_detail_q.iter() {
                        enemy_bld_list.push(HarassBuilding {
                            faction: *bf,
                            kind: *bk,
                            position: btf.translation,
                            construction_frac: progress.map(|p| p.timer.fraction()),
                        });
                    }

                    let target = select_harass_target(
                        &teams,
                        &faction,
                        &enemy_workers,
                        &enemy_military_pos,
                        &enemy_bld_list,
                    )
                    .or_else(|| find_enemy_resource_area(&enemy_buildings_q, &teams, &faction));

                    if let Some(target) = target {
                        for &e in &raiders {
                            if units_q.get(e).is_ok() {
                                commands.entity(e).insert(MoveTarget(target));
                            }
                        }
                    }
                    brain.raid_cooldown = 30.0;
                }
            }
        }

        // ── Friendly AI: Cooperative behavior ──
        if is_friendly {
            brain.last_cooperation_check -= MILITARY_TICK;
            if brain.last_cooperation_check <= 0.0 {
                brain.last_cooperation_check = COOPERATION_CHECK_INTERVAL;

                let mut player_army_center = Vec3::ZERO;
                let mut player_base = base_pos;
                let mut player_army_count = 0u32;
                if let Some(player_snapshot) = snapshot.factions.get(&active_player.0) {
                    if let Some(center) = player_snapshot.military_center {
                        player_army_center = center;
                        player_army_count = player_snapshot.military_count as u32;
                    }
                    if let Some(base) = player_snapshot.base_position {
                        player_base = base;
                    }
                }

                if player_army_count > 0 {
                    player_army_center /= player_army_count as f32;
                    let dist_from_player_base = player_army_center.distance(player_base);
                    if dist_from_player_base > ALLY_SUPPORT_DISTANCE {
                        brain.ally_attack_target = Some(player_army_center);
                    } else {
                        brain.ally_attack_target = None;
                    }
                }
            }
        }

        // ── Timed push (Age II completion): commit everything for 60s ──
        let timed_push_active = brain.timed_push_until.is_some();
        if timed_push_active && !is_friendly {
            // Gather every combat-capable squad member.
            let mut all_pushers: Vec<Entity> = Vec::new();
            for role in [
                SquadRole::AttackSquad,
                SquadRole::DefenseSquad,
                SquadRole::Raider,
            ] {
                if let Some(sq) = brain.get_squad(role) {
                    all_pushers.extend(&sq.members);
                }
            }
            // Pick a target the same way a raid would, but fall through to
            // strategic targeting if no exploit exists.
            let mut enemy_workers: Vec<(Vec3, u64)> = Vec::new();
            let mut enemy_military_pos: Vec<Vec3> = Vec::new();
            for (uent, uf, ukind, utf) in units_q.iter() {
                if !teams.is_hostile(&faction, uf) || *uf == Faction::Neutral {
                    continue;
                }
                if *ukind == EntityKind::Worker {
                    enemy_workers.push((utf.translation, uent.to_bits()));
                } else {
                    enemy_military_pos.push(utf.translation);
                }
            }
            let mut enemy_bld_list: Vec<HarassBuilding> = Vec::new();
            for (bf, bk, btf, progress) in building_detail_q.iter() {
                enemy_bld_list.push(HarassBuilding {
                    faction: *bf,
                    kind: *bk,
                    position: btf.translation,
                    construction_frac: progress.map(|p| p.timer.fraction()),
                });
            }
            let push_target = select_harass_target(
                &teams,
                &faction,
                &enemy_workers,
                &enemy_military_pos,
                &enemy_bld_list,
            )
            .or_else(|| {
                pick_strategic_target(
                    base_pos,
                    &brain.known_threats,
                    &enemy_buildings_q,
                    &teams,
                    &faction,
                )
            });
            if let Some(target) = push_target {
                for &e in &all_pushers {
                    if units_q.get(e).is_ok() {
                        commands.entity(e).insert(MoveTarget(target));
                    }
                }
            }
        }

        // ── Attack decision — with staging ──
        let posture = brain.posture;
        let attack_ready = brain.attack_ready;
        let last_attack_time = brain.last_attack_time;
        let game_time = brain.game_time;

        let should_attack = match top_state {
            AiTopState::Attacking => true,
            AiTopState::LateGame => {
                attack_ready && (game_time - last_attack_time) > ATTACK_MIN_INTERVAL
            }
            _ => false,
        };

        if posture == TacticalPosture::Normal && should_attack {
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

            // Staging: compute rally point and check if enough units are gathered
            let rally = base_pos + (Vec3::ZERO - base_pos).normalize_or_zero() * 30.0;
            let staged_count = attack_members
                .iter()
                .filter(|&&e| {
                    units_q.get(e).map_or(false, |(_, _, _, tf)| {
                        tf.translation.distance(rally) < STAGING_RADIUS
                    })
                })
                .count();

            let min_staged = ATTACK_STAGING_MIN.min(attack_members.len());
            let squad_ready = staged_count >= min_staged || attack_members.len() <= 2;

            if squad_ready {
                // Squad is staged — commit to attack
                let target = if is_friendly {
                    brain.ally_attack_target.or_else(|| {
                        pick_strategic_target(
                            base_pos,
                            &brain.known_threats,
                            &enemy_buildings_q,
                            &teams,
                            &faction,
                        )
                    })
                } else {
                    pick_strategic_target(
                        base_pos,
                        &brain.known_threats,
                        &enemy_buildings_q,
                        &teams,
                        &faction,
                    )
                };

                if let Some(target_pos) = target {
                    for entity in &attack_members {
                        if units_q.get(*entity).is_ok() {
                            commands.entity(*entity).insert(MoveTarget(target_pos));
                        }
                    }
                    brain.last_attack_time = game_time;
                    brain.attack_started_at = game_time;
                    brain.attack_ready = false;

                    if is_friendly {
                        notifications.push(
                            AllyNotifyKind::Attacking,
                            "Ally is launching an attack!".to_string(),
                            Some(target_pos),
                            game_time,
                        );
                    }
                }
            } else {
                // Not enough units staged — rally them to staging point
                for &entity in &attack_members {
                    if is_idle_military(entity) {
                        commands.entity(entity).insert(MoveTarget(rally));
                    }
                }
            }
        }

        // Notify when ally is ready to attack
        if is_friendly && attack_ready && (game_time - last_attack_time) > ATTACK_MIN_INTERVAL * 0.8
        {
            notifications.push(
                AllyNotifyKind::ReadyToAttack,
                "Ally army ready to push!".to_string(),
                None,
                game_time,
            );
        }

        // ── Retreat behavior: check attack squad avg HP ──
        if !is_friendly
            && (brain.posture == TacticalPosture::Normal
                || matches!(top_state, AiTopState::Attacking))
        {
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

            if attack_members.len() >= 3 {
                let mut total_hp_pct = 0.0;
                let mut count = 0u32;
                for &e in &attack_members {
                    if let Ok(h) = health_q.get(e) {
                        total_hp_pct += h.current / h.max;
                        count += 1;
                    }
                }
                if count > 0 {
                    let avg_hp_pct = total_hp_pct / count as f32;
                    if avg_hp_pct < RETREAT_HP_THRESHOLD {
                        brain.posture = TacticalPosture::Retreating;
                        brain.posture_cooldown = 20.0;
                        for &e in &attack_members {
                            if units_q.get(e).is_ok() {
                                commands.entity(e).insert(MoveTarget(base_pos));
                            }
                        }
                        if top_state == AiTopState::Attacking {
                            brain.transition_to(AiTopState::Defending);
                        }
                    }
                }
            }
        }

        // ── Rally idle attack units (when not attacking) ──
        if posture == TacticalPosture::Normal && !matches!(top_state, AiTopState::Attacking) {
            let rally = base_pos + (Vec3::ZERO - base_pos).normalize_or_zero() * 30.0;
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

            for &entity in &attack_members {
                if is_idle_military(entity) {
                    commands.entity(entity).insert(MoveTarget(rally));
                }
            }
        }

        // ── Defending: recall all squads to base ──
        if matches!(top_state, AiTopState::Defending) && posture == TacticalPosture::Normal {
            let mut recall_entities: Vec<Entity> = Vec::new();
            if let Some(squad) = brain.get_squad(SquadRole::AttackSquad) {
                recall_entities.extend(&squad.members);
            }
            if let Some(squad) = brain.get_squad(SquadRole::DefenseSquad) {
                recall_entities.extend(&squad.members);
            }
            for entity in &recall_entities {
                if is_idle_military(*entity) {
                    commands.entity(*entity).insert(MoveTarget(base_pos));
                }
            }
        }
    }
}

/// Composition with counter-intelligence: blends personality defaults with
/// observed enemy composition and (optionally) the incoming night wave bias.
///
/// Easy/Medium difficulty: legacy melee-vs-ranged heuristic.
/// Hard difficulty: damage-vs-armor matrix scoring — picks the top-scoring
/// friendly unit types against the observed enemy armor histogram.
fn get_desired_composition_with_intel(
    state: AiTopState,
    personality: AiPersonality,
    is_friendly: bool,
    snapshot: &AiWorldSnapshot,
    active_player: Faction,
    enemy_composition: &HashMap<EntityKind, u32>,
    difficulty: AiDifficulty,
    wave_bias: Option<WaveCounterBias>,
) -> Vec<(EntityKind, usize)> {
    let base = get_desired_composition(state, personality, is_friendly, snapshot, active_player);

    // Start from base; apply counter-weights in place.
    let mut result = base.clone();

    if !enemy_composition.is_empty() {
        match difficulty {
            AiDifficulty::Hard => {
                apply_hard_counter_matrix(&mut result, enemy_composition);
            }
            _ => {
                apply_simple_counter_heuristic(&mut result, enemy_composition);
            }
        }
    }

    // Apply night-wave counter-prep on top. These boosts survive the day —
    // they bias training between Dusk and Dawn.
    if let Some(bias) = wave_bias {
        apply_wave_bias(&mut result, bias);
    }

    result
}

fn apply_simple_counter_heuristic(
    result: &mut Vec<(EntityKind, usize)>,
    enemy_composition: &HashMap<EntityKind, u32>,
) {
    let enemy_melee: u32 = enemy_composition
        .iter()
        .filter(|(k, _)| {
            matches!(
                k,
                EntityKind::Soldier | EntityKind::Knight | EntityKind::Cavalry
            )
        })
        .map(|(_, v)| v)
        .sum();
    let enemy_ranged: u32 = enemy_composition
        .iter()
        .filter(|(k, _)| matches!(k, EntityKind::Archer | EntityKind::Mage))
        .map(|(_, v)| v)
        .sum();

    if enemy_melee > enemy_ranged + 2 {
        for (kind, count) in result.iter_mut() {
            if matches!(kind, EntityKind::Archer | EntityKind::Mage) {
                *count = (*count + 2).min(*count * 2);
            }
        }
    } else if enemy_ranged > enemy_melee + 2 {
        for (kind, count) in result.iter_mut() {
            if matches!(kind, EntityKind::Knight | EntityKind::Cavalry) {
                *count = (*count + 2).min(*count * 2);
            }
        }
    }
}

/// Hard-difficulty counter: use damage/armor matrix to pick top counter units.
/// Computes a weighted enemy armor histogram, then scores each friendly unit
/// kind by summed damage multiplier vs that histogram. Top two scorers get +2.
fn apply_hard_counter_matrix(
    result: &mut Vec<(EntityKind, usize)>,
    enemy_composition: &HashMap<EntityKind, u32>,
) {
    // Sort enemy kinds for deterministic iteration (HashMap otherwise drifts).
    let mut enemy_sorted: Vec<(EntityKind, u32)> =
        enemy_composition.iter().map(|(k, v)| (*k, *v)).collect();
    enemy_sorted.sort_by_key(|(k, _)| k.to_index());

    let mut armor_hist: [u32; 4] = [0; 4]; // [Light, Heavy, Siege, Structure]
    for (kind, count) in &enemy_sorted {
        let a = armor_for_kind(*kind) as usize;
        armor_hist[a] += count;
    }

    // Score each of our candidate unit types.
    let mut scored: Vec<(EntityKind, i64)> = result
        .iter()
        .map(|(kind, _)| {
            let dmg = damage_for_kind(*kind);
            let mut score: f32 = 0.0;
            for a_idx in 0..4 {
                let armor = match a_idx {
                    0 => ArmorType::Light,
                    1 => ArmorType::Heavy,
                    2 => ArmorType::Siege,
                    _ => ArmorType::Structure,
                };
                let mult = dmg.multiplier_vs(armor);
                score += armor_hist[a_idx] as f32 * mult;
            }
            // Quantize for integer tie-break.
            ((*kind), (score * 1000.0) as i64)
        })
        .collect();
    // Stable sort by (score desc, kind index asc).
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_index().cmp(&b.0.to_index())));

    // Boost top 2 scoring kinds.
    for (boost_kind, _) in scored.iter().take(2) {
        for (kind, count) in result.iter_mut() {
            if kind == boost_kind {
                *count = *count + 2;
            }
        }
    }
}

fn apply_wave_bias(result: &mut Vec<(EntityKind, usize)>, bias: WaveCounterBias) {
    let boosted: &[EntityKind] = match bias {
        WaveCounterBias::Runner => &[EntityKind::Soldier],
        WaveCounterBias::Armored => &[EntityKind::Archer, EntityKind::BallistaTower],
        WaveCounterBias::Champion => &[EntityKind::Mage, EntityKind::Priest],
    };
    for (kind, count) in result.iter_mut() {
        if boosted.contains(kind) {
            *count = (*count as f32 * 1.5).ceil() as usize;
        }
    }
}

/// Map unit kind → its primary damage type (mirrors blueprint defaults).
fn damage_for_kind(kind: EntityKind) -> DamageType {
    match kind {
        EntityKind::Archer | EntityKind::Scout | EntityKind::BallistaTower => DamageType::Pierce,
        EntityKind::Mage | EntityKind::Priest | EntityKind::MageTower => DamageType::Magic,
        EntityKind::Catapult | EntityKind::BatteringRam | EntityKind::BombardTower => {
            DamageType::SiegeDmg
        }
        _ => DamageType::Melee,
    }
}

/// Map unit kind → its primary armor type (mirrors blueprint defaults).
fn armor_for_kind(kind: EntityKind) -> ArmorType {
    match kind {
        EntityKind::Knight | EntityKind::Tank | EntityKind::Cavalry => ArmorType::Heavy,
        EntityKind::Catapult | EntityKind::BatteringRam => ArmorType::Siege,
        _ => ArmorType::Light,
    }
}

fn get_desired_composition(
    state: AiTopState,
    personality: AiPersonality,
    is_friendly: bool,
    snapshot: &AiWorldSnapshot,
    active_player: Faction,
) -> Vec<(EntityKind, usize)> {
    if is_friendly || personality == AiPersonality::Supportive {
        let player_units = snapshot.factions.get(&active_player);
        let player_melee = player_units.map_or(0, |player| {
            player
                .unit_counts
                .iter()
                .filter(|(kind, _)| {
                    matches!(
                        kind,
                        EntityKind::Soldier | EntityKind::Knight | EntityKind::Cavalry
                    )
                })
                .map(|(_, count)| *count)
                .sum()
        });
        let player_ranged = player_units.map_or(0, |player| {
            player
                .unit_counts
                .iter()
                .filter(|(kind, _)| matches!(kind, EntityKind::Archer | EntityKind::Mage))
                .map(|(_, count)| *count)
                .sum()
        });
        let player_prefers_melee = player_melee > player_ranged;

        return match state {
            AiTopState::Founding | AiTopState::EarlyEconomy | AiTopState::Militarize => {
                if player_prefers_melee {
                    vec![(EntityKind::Archer, 3), (EntityKind::Soldier, 2)]
                } else {
                    vec![(EntityKind::Soldier, 3), (EntityKind::Archer, 2)]
                }
            }
            AiTopState::Expanding | AiTopState::Attacking | AiTopState::Defending => {
                if player_prefers_melee {
                    vec![
                        (EntityKind::Archer, 4),
                        (EntityKind::Mage, 2),
                        (EntityKind::Soldier, 2),
                        (EntityKind::Priest, 1),
                    ]
                } else {
                    vec![
                        (EntityKind::Soldier, 3),
                        (EntityKind::Knight, 2),
                        (EntityKind::Archer, 2),
                        (EntityKind::Priest, 1),
                    ]
                }
            }
            AiTopState::LateGame => {
                if player_prefers_melee {
                    vec![
                        (EntityKind::Archer, 4),
                        (EntityKind::Mage, 3),
                        (EntityKind::Priest, 2),
                        (EntityKind::Soldier, 2),
                        (EntityKind::Catapult, 1),
                    ]
                } else {
                    vec![
                        (EntityKind::Knight, 3),
                        (EntityKind::Cavalry, 2),
                        (EntityKind::Soldier, 3),
                        (EntityKind::Priest, 2),
                        (EntityKind::BatteringRam, 1),
                    ]
                }
            }
        };
    }

    match personality {
        AiPersonality::Aggressive => match state {
            AiTopState::Founding | AiTopState::EarlyEconomy | AiTopState::Militarize => {
                vec![(EntityKind::Soldier, 4), (EntityKind::Archer, 1)]
            }
            AiTopState::Expanding | AiTopState::Attacking | AiTopState::Defending => vec![
                (EntityKind::Soldier, 5),
                (EntityKind::Knight, 3),
                (EntityKind::Archer, 2),
            ],
            AiTopState::LateGame => vec![
                (EntityKind::Soldier, 4),
                (EntityKind::Knight, 3),
                (EntityKind::Cavalry, 3),
                (EntityKind::Catapult, 2),
            ],
        },
        AiPersonality::Defensive => match state {
            AiTopState::Founding | AiTopState::EarlyEconomy | AiTopState::Militarize => {
                vec![(EntityKind::Soldier, 2), (EntityKind::Archer, 3)]
            }
            AiTopState::Expanding | AiTopState::Attacking | AiTopState::Defending => vec![
                (EntityKind::Soldier, 3),
                (EntityKind::Archer, 4),
                (EntityKind::Mage, 2),
                (EntityKind::Priest, 1),
            ],
            AiTopState::LateGame => vec![
                (EntityKind::Soldier, 4),
                (EntityKind::Archer, 4),
                (EntityKind::Mage, 3),
                (EntityKind::Priest, 2),
                (EntityKind::Catapult, 1),
            ],
        },
        AiPersonality::Economic => match state {
            AiTopState::Founding | AiTopState::EarlyEconomy | AiTopState::Militarize => {
                vec![(EntityKind::Soldier, 2), (EntityKind::Archer, 1)]
            }
            AiTopState::Expanding | AiTopState::Attacking | AiTopState::Defending => vec![
                (EntityKind::Soldier, 4),
                (EntityKind::Archer, 3),
                (EntityKind::Knight, 2),
            ],
            AiTopState::LateGame => vec![
                (EntityKind::Soldier, 4),
                (EntityKind::Knight, 3),
                (EntityKind::Mage, 3),
                (EntityKind::Cavalry, 2),
                (EntityKind::Catapult, 2),
                (EntityKind::BatteringRam, 1),
            ],
        },
        _ => match state {
            AiTopState::Founding | AiTopState::EarlyEconomy | AiTopState::Militarize => {
                vec![(EntityKind::Soldier, 3), (EntityKind::Archer, 2)]
            }
            AiTopState::Expanding | AiTopState::Attacking | AiTopState::Defending => vec![
                (EntityKind::Soldier, 4),
                (EntityKind::Archer, 3),
                (EntityKind::Knight, 2),
                (EntityKind::Mage, 1),
            ],
            AiTopState::LateGame => vec![
                (EntityKind::Soldier, 3),
                (EntityKind::Archer, 3),
                (EntityKind::Knight, 3),
                (EntityKind::Mage, 2),
                (EntityKind::Cavalry, 2),
                (EntityKind::Catapult, 1),
                (EntityKind::BatteringRam, 1),
            ],
        },
    }
}
