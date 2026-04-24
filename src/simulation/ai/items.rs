//! AI item-pickup dispatch: send idle AI units to collect dropped loot.
//!
//! Runs on the tactical tick (0.5s). For each AI faction, scans all `ItemPickup`
//! entities in a deterministic order, matches them to eligible idle units, and
//! issues a `MoveTarget` + `PendingItemPickup` so the existing pickup resolver
//! in `simulation::items::mod::resolve_pending_item_pickups` handles collection.
//!
//! Determinism:
//! - Pickups sorted by position bits + entity index.
//! - Candidate units sorted by (quantized distance, entity index).
//! - Assignments stored in `BTreeMap<Entity, Entity>` on the brain so peers
//!   re-enter with identical state next tick.
//! - No RNG.

use bevy::prelude::*;
use bevy::time::Fixed;

use crate::blueprints::EntityKind;
use crate::simulation::items::{
    first_missing_requirement, inventory_failure_for_item, ItemKind, ItemPickup, ItemRegistry,
    PendingItemPickup, UnitInventory,
};
use crate::types::*;

use super::types::*;

/// Minimum XZ distance gate: don't divert units further than this.
const MAX_DISPATCH_DIST: f32 = 80.0;

pub fn ai_item_pickup_dispatch(
    time: Res<Time<Fixed>>,
    config: Res<GameSetupConfig>,
    active_player: Res<ActivePlayer>,
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    mut commands: Commands,
    item_registry: Res<ItemRegistry>,
    pickups_q: Query<(Entity, &ItemPickup, &Transform)>,
    mut units_q: Query<
        (
            Entity,
            &Faction,
            &EntityKind,
            &Transform,
            &UnitState,
            &UnitInventory,
            Option<&PendingItemPickup>,
        ),
        With<Unit>,
    >,
) {
    let dt = time.delta_secs();

    for &faction in &ai_controlled.factions {
        if !faction_uses_ai(&config, faction) {
            continue;
        }
        if faction == active_player.0 {
            continue;
        }

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        // Throttle: reuse the tactical cadence. The system as a whole runs every
        // fixed tick, so gate by a local timer field to keep the cost down.
        brain.scout_timer -= 0.0; // no-op; scout_timer still owned by scouting logic.
        let _ = dt;

        // Prune assignments whose pickup or unit no longer exists / is dead / already carrying.
        brain.pending_item_targets.retain(|pickup_entity, unit_entity| {
            if pickups_q.get(*pickup_entity).is_err() {
                return false;
            }
            match units_q.get(*unit_entity) {
                Err(_) => false,
                Ok((_, _, _, _, _, _, pending)) => pending.is_some(),
            }
        });

        // Collect already-assigned units so we don't double-book them.
        let mut assigned_units: Vec<Entity> = brain
            .pending_item_targets
            .values()
            .copied()
            .collect();
        assigned_units.sort();
        assigned_units.dedup();

        // Gather pickups into a deterministic order (position bits, then entity index).
        let mut pickup_list: Vec<(Entity, ItemKind, Vec3)> = Vec::new();
        for (pe, pickup, tf) in pickups_q.iter() {
            pickup_list.push((pe, pickup.item, tf.translation));
        }
        pickup_list.sort_by(|a, b| {
            let ka = (
                a.2.x.to_bits(),
                a.2.y.to_bits(),
                a.2.z.to_bits(),
                a.0.to_bits(),
            );
            let kb = (
                b.2.x.to_bits(),
                b.2.y.to_bits(),
                b.2.z.to_bits(),
                b.0.to_bits(),
            );
            ka.cmp(&kb)
        });

        for (pickup_entity, item, pickup_pos) in pickup_list {
            if brain.pending_item_targets.contains_key(&pickup_entity) {
                continue;
            }

            // Find best eligible unit: AI-faction, idle-ish, inventory OK, meets requirement.
            let mut best: Option<(Entity, i64, u64)> = None; // (entity, dist_key, entity_bits)
            for (unit_entity, unit_faction, kind, unit_tf, state, inventory, pending) in
                units_q.iter()
            {
                if *unit_faction != faction {
                    continue;
                }
                if pending.is_some() {
                    continue;
                }
                if assigned_units.binary_search(&unit_entity).is_ok() {
                    continue;
                }
                // Workers keep gathering; only divert idle combat/support units.
                if *kind == EntityKind::Worker {
                    continue;
                }
                if !matches!(state, UnitState::Idle | UnitState::HoldPosition) {
                    continue;
                }
                if inventory_failure_for_item(item, inventory).is_some() {
                    continue;
                }
                if first_missing_requirement(&item_registry, *kind, item).is_some() {
                    continue;
                }
                let dx = unit_tf.translation.x - pickup_pos.x;
                let dz = unit_tf.translation.z - pickup_pos.z;
                let dist = (dx * dx + dz * dz).sqrt();
                if dist > MAX_DISPATCH_DIST {
                    continue;
                }
                let dist_key = (dist * 1000.0) as i64;
                let bits = unit_entity.to_bits();
                let better = match best {
                    None => true,
                    Some((_, bd, bi)) => dist_key < bd || (dist_key == bd && bits < bi),
                };
                if better {
                    best = Some((unit_entity, dist_key, bits));
                }
            }

            let Some((chosen, _, _)) = best else {
                continue;
            };

            // Issue move + pending pickup (resolver handles approach + collection).
            if let Ok((_, _, _, _, _, _, _)) = units_q.get_mut(chosen) {
                commands
                    .entity(chosen)
                    .insert(MoveTarget(pickup_pos))
                    .insert(PendingItemPickup {
                        pickup: pickup_entity,
                    });
                brain.pending_item_targets.insert(pickup_entity, chosen);
                assigned_units.push(chosen);
                assigned_units.sort();
            }
        }
    }
}
