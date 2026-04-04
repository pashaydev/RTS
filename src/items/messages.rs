use bevy::prelude::*;

use crate::components::Faction;

use super::components::ItemKind;

#[derive(Message, Clone, Copy, Debug)]
pub struct SpawnItemPickup {
    pub item: ItemKind,
    pub position: Vec3,
    pub owner: Option<Faction>,
    pub lifetime_secs: f32,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct RequestPickupItem {
    pub picker: Entity,
    pub pickup: Entity,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct InventoryChanged {
    pub unit: Entity,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct ItemPickupCollected {
    pub pickup: Entity,
    pub collector: Entity,
    pub item: ItemKind,
}
