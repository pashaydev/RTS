//! Item pickup/spawn request messages and the failure-reason enum
//! consumed by the items pipeline and UI notifications.

use bevy::prelude::*;

use crate::types::Faction;

use super::components::ItemKind;

#[derive(Message, Clone, Copy, Debug)]
pub struct SpawnItemPickup {
    pub item: ItemKind,
    pub position: Vec3,
    pub owner: Option<Faction>,
    pub lifetime_secs: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemPickupFailureReason {
    NoSelectedUnits,
    TooFar,
    InventoryFull,
    CategoryConflict,
    WrongUnitType,
    NoInventorySlots,
}

impl ItemPickupFailureReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoSelectedUnits => "Select a unit to pick up items.",
            Self::TooFar => "Move a selected unit closer to the item.",
            Self::InventoryFull => "Inventory is full.",
            Self::CategoryConflict => "That unit already carries an item of this category.",
            Self::WrongUnitType => "Selected units cannot equip this item type.",
            Self::NoInventorySlots => "Selected units cannot carry items.",
        }
    }

    pub fn priority(self) -> u8 {
        match self {
            Self::NoSelectedUnits => 0,
            Self::TooFar => 1,
            Self::WrongUnitType | Self::NoInventorySlots => 2,
            Self::CategoryConflict => 3,
            Self::InventoryFull => 4,
        }
    }
}

#[derive(Message, Clone, Debug)]
pub struct RequestPickupItem {
    pub pickup: Entity,
    pub pickers: Vec<Entity>,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct RequestDropItem {
    pub unit: Entity,
    pub slot: usize,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct RequestTransferItem {
    pub from_unit: Entity,
    pub from_slot: usize,
    pub to_unit: Entity,
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
    pub info_message: Option<&'static str>,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct ItemPickupFailed {
    pub pickup: Entity,
    pub item: ItemKind,
    pub reason: ItemPickupFailureReason,
}

#[derive(Message, Clone, Debug)]
pub struct ItemTransferFailed {
    pub item: ItemKind,
    pub from_unit: Entity,
    pub to_unit: Entity,
    pub reason: ItemPickupFailureReason,
}
