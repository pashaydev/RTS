//! Component replication registry (scaffold).
//!
//! The current host → client state sync in `host_systems::host_broadcast_state_sync`
//! uses one hand-rolled tuple query with an `Option<&T>` slot per replicated
//! component. Adding a new replicated component means editing that query, the
//! `EntitySnapshot` struct, the wire protocol, and the client applier in lock
//! step — four surfaces for one change.
//!
//! The target architecture is a registry: each replicated component
//! implements [`Replicated`] and registers itself once via `app.replicate::<T>()`.
//! The broadcast loop walks the registry; the wire protocol carries typed
//! per-component deltas; clients dispatch to the matching applier.
//!
//! This file intentionally ships only the trait + registry types as a
//! landing zone. The live broadcast path is untouched. Follow-up work will:
//!
//! 1. Implement [`Replicated`] for `Health`, `UnitState`, `MoveTarget`,
//!    `AttackTarget`, `Carrying`, `UnitStance`.
//! 2. Replace `EntitySnapshot`'s per-field `Option<…>` soup with a
//!    `Vec<ComponentDelta>` carrying typed payloads.
//! 3. Rewrite `host_broadcast_state_sync` to loop over the registry instead
//!    of the tuple query.
//! 4. Move the client-side apply logic into matching per-component handlers.

use bevy::ecs::component::Component;
use bevy::prelude::*;
use std::any::TypeId;
use std::collections::HashMap;

/// Marker trait for a component that participates in host → client state sync.
///
/// `Delta` is the wire payload the host sends when the component changes. For
/// components that are small and cheap to re-send in full, `Delta = Self` is
/// fine; for fields like `GlobalTransform` a compact delta type is preferable.
pub trait Replicated: Component + Clone {
    type Delta: Clone + Send + Sync + 'static;

    /// Compute the delta between two snapshots of the same component, or
    /// `None` if the component has not changed meaningfully this tick.
    fn diff(prev: &Self, cur: &Self) -> Option<Self::Delta>;

    /// Apply an incoming delta to this component on the client.
    fn apply(&mut self, delta: &Self::Delta);
}

/// Registry of opt-in replicated components. Populated at plugin build via
/// `app.replicate::<T>()`; consumed by the broadcast / apply loops.
#[derive(Resource, Default)]
pub struct ReplicationRegistry {
    /// Stable insertion order so broadcast output is deterministic across
    /// runs — required for multiplayer desync detection and replay.
    pub order: Vec<TypeId>,
    pub entries: HashMap<TypeId, ReplicationEntry>,
}

/// Per-component dispatch table. Concrete function pointers are filled in
/// when a component is registered; the broadcast / apply systems use them
/// without needing to know the underlying type at the call site.
pub struct ReplicationEntry {
    pub type_name: &'static str,
    // TODO(pass-6-followup): add erased `diff` / `apply` function pointers.
}

/// Extension trait so gameplay plugins can call `app.replicate::<Health>()`.
pub trait ReplicateAppExt {
    fn replicate<T: Replicated>(&mut self) -> &mut Self;
}

impl ReplicateAppExt for App {
    fn replicate<T: Replicated>(&mut self) -> &mut Self {
        self.init_resource::<ReplicationRegistry>();
        let id = TypeId::of::<T>();
        let mut reg = self
            .world_mut()
            .get_resource_mut::<ReplicationRegistry>()
            .expect("ReplicationRegistry just inserted");
        if !reg.entries.contains_key(&id) {
            reg.order.push(id);
            reg.entries.insert(
                id,
                ReplicationEntry {
                    type_name: std::any::type_name::<T>(),
                },
            );
        }
        self
    }
}
