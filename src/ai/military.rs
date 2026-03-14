use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::components::*;

use super::helpers::*;
use super::types::*;

/// Minimum squad size before committing to an attack (staging requirement)
const ATTACK_STAGING_MIN: usize = 4;
/// Distance from rally point within which a unit is considered "staged"
const STAGING_RADIUS: f32 = 25.0;

// ════════════════════════════════════════════════════════════════════
// System 3: Military — Army composition, squads, attacks, scouting
// ════════════════════════════════════════════════════════════════════

pub fn ai_military_system(
    mut commands: Commands,
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    mut all_resources: ResMut<AllPlayerResources>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    mut notifications: ResMut<AllyNotifications>,
    queries: (
        Query<(Entity, &Faction, &EntityKind, &Transform), (With<Unit>, Without<Building>)>,
        Query<
            (Entity, &Faction, &EntityKind, &Transform, &UnitState),
            (
                With<Unit>,
                Without<AttackTarget>,
                Without<MoveTarget>,
                Without<Building>,
            ),
        >,
        Query<&Health>,
        Query<(&Faction, &Transform), With<Building>>,
        Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
    ),
) {
    let (units_q, idle_military_q, health_q, enemy_buildings_q, mut train_queues) = queries;
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

        brain.military_timer -= dt;
        if brain.military_timer > 0.0 {
            continue;
        }
        brain.military_timer = brain.effective_tick(MILITARY_TICK);

        let base_pos = match brain.base_position {
            Some(p) => p,
            None => continue,
        };

        // Prune dead entities
        let alive: std::collections::HashSet<Entity> = units_q
            .iter()
            .filter(|(_, f, _, _)| **f == faction)
            .map(|(e, _, _, _)| e)
            .collect();
        brain.prune_dead(&alive);

        // Count our units by type
        let mut unit_counts: HashMap<EntityKind, usize> = HashMap::new();
        let mut military_count = 0usize;
        let mut total_own_strength: f32 = 0.0;
        for (e, f, kind, _) in units_q.iter() {
            if *f != faction {
                continue;
            }
            *unit_counts.entry(*kind).or_default() += 1;
            if *kind != EntityKind::Worker {
                military_count += 1;
                if let Ok(h) = health_q.get(e) {
                    total_own_strength += h.current;
                }
            }
        }

        // Compute relative strength
        let enemy_str = brain.enemy_strength.max(1.0);
        brain.relative_strength = if enemy_str > 0.0 {
            total_own_strength / enemy_str
        } else {
            if military_count > 0 { 10.0 } else { 0.0 }
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
        let desired_composition: Vec<(EntityKind, usize)> =
            get_desired_composition_with_intel(
                top_state,
                personality,
                is_friendly,
                &units_q,
                &active_player,
                &brain.enemy_composition,
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
                if try_train(&mut train_queues, &faction, unit_kind, &registry) {
                    let (dw, dc, di, dg, do_) =
                        bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                    let drain = SpendFromCarried {
                        faction,
                        amounts: [dw, dc, di, dg, do_],
                    };
                    if drain.has_deficit() {
                        pending_drains.drains.push(drain);
                    }
                }
            }
        }

        // ── Assign unassigned military to squads ──
        let mut unassigned: Vec<(Entity, EntityKind, Vec3)> = Vec::new();
        for (entity, f, kind, tf) in units_q.iter() {
            if *f != faction || *kind == EntityKind::Worker {
                continue;
            }
            if !brain.assigned_units.contains_key(&entity) {
                unassigned.push((entity, *kind, tf.translation));
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
                        let wp = route[waypoint_idx % route.len()];
                        commands.entity(entity).insert(MoveTarget(wp));
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
                    if let Some(target) =
                        find_enemy_resource_area(&enemy_buildings_q, &teams, &faction)
                    {
                        for &e in &raiders {
                            commands.entity(e).insert(MoveTarget(target));
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
                let mut player_army_count = 0u32;
                let mut player_base = base_pos;
                for (_, f, kind, tf) in units_q.iter() {
                    if *f == active_player.0 {
                        if *kind != EntityKind::Worker {
                            player_army_center += tf.translation;
                            player_army_count += 1;
                        }
                    }
                }
                for (f, tf) in enemy_buildings_q.iter() {
                    if *f == active_player.0 {
                        player_base = tf.translation;
                        break;
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
                    units_q
                        .get(e)
                        .map_or(false, |(_, _, _, tf)| tf.translation.distance(rally) < STAGING_RADIUS)
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
                        commands.entity(*entity).insert(MoveTarget(target_pos));
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
                    if idle_military_q.get(entity).is_ok() {
                        commands.entity(entity).insert(MoveTarget(rally));
                    }
                }
            }
        }

        // Notify when ally is ready to attack
        if is_friendly
            && attack_ready
            && (game_time - last_attack_time) > ATTACK_MIN_INTERVAL * 0.8
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
                            commands.entity(e).insert(MoveTarget(base_pos));
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
                if idle_military_q.get(entity).is_ok() {
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
                if idle_military_q.get(*entity).is_ok() {
                    commands.entity(*entity).insert(MoveTarget(base_pos));
                }
            }
        }
    }
}

/// Composition with counter-intelligence: blend enemy composition awareness with personality
fn get_desired_composition_with_intel(
    state: AiTopState,
    personality: AiPersonality,
    is_friendly: bool,
    units_q: &Query<(Entity, &Faction, &EntityKind, &Transform), (With<Unit>, Without<Building>)>,
    active_player: &ActivePlayer,
    enemy_composition: &HashMap<EntityKind, u32>,
) -> Vec<(EntityKind, usize)> {
    let base = get_desired_composition(state, personality, is_friendly, units_q, active_player);

    if enemy_composition.is_empty() {
        return base;
    }

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

    let mut result = base.clone();

    if enemy_melee > enemy_ranged + 2 {
        for (kind, count) in result.iter_mut() {
            match kind {
                EntityKind::Archer | EntityKind::Mage => *count = (*count + 2).min(*count * 2),
                _ => {}
            }
        }
    } else if enemy_ranged > enemy_melee + 2 {
        for (kind, count) in result.iter_mut() {
            match kind {
                EntityKind::Knight | EntityKind::Cavalry => {
                    *count = (*count + 2).min(*count * 2)
                }
                _ => {}
            }
        }
    }

    result
}

fn get_desired_composition(
    state: AiTopState,
    personality: AiPersonality,
    is_friendly: bool,
    units_q: &Query<(Entity, &Faction, &EntityKind, &Transform), (With<Unit>, Without<Building>)>,
    active_player: &ActivePlayer,
) -> Vec<(EntityKind, usize)> {
    if is_friendly || personality == AiPersonality::Supportive {
        let mut player_melee = 0usize;
        let mut player_ranged = 0usize;
        for (_, f, kind, _) in units_q.iter() {
            if *f != active_player.0 || *kind == EntityKind::Worker {
                continue;
            }
            match kind {
                EntityKind::Soldier | EntityKind::Knight | EntityKind::Cavalry => player_melee += 1,
                EntityKind::Archer | EntityKind::Mage => player_ranged += 1,
                _ => {}
            }
        }
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
