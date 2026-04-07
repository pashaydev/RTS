use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::blueprints::EntityKind;
use crate::types::Faction;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ItemCategory {
    Armor,
    Helmet,
    Ring,
    Sword,
    Staff,
    Bow,
}

impl ItemCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Armor => "Armor",
            Self::Helmet => "Helmet",
            Self::Ring => "Ring",
            Self::Sword => "Sword",
            Self::Staff => "Staff",
            Self::Bow => "Bow",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    PaddedVest,
    BronzeCuirass,
    PlateCuirass,
    CrusaderHelm,
    KettleHelm,
    VikingHelm,
    JewelRing,
    PlainBand,
    WeddingBand,
    GoldenBand,
    TwinRings,
    LinkedRings,
    ArmingSword,
    VikingBlade,
    BattleStaff,
    MageCrozier,
    YewLongbow,
    WarBow,
}


impl ItemKind {
    pub const ALL: &'static [Self] = &[
        Self::PaddedVest,
        Self::BronzeCuirass,
        Self::PlateCuirass,
        Self::CrusaderHelm,
        Self::KettleHelm,
        Self::VikingHelm,
        Self::JewelRing,
        Self::PlainBand,
        Self::WeddingBand,
        Self::GoldenBand,
        Self::TwinRings,
        Self::LinkedRings,
        Self::ArmingSword,
        Self::VikingBlade,
        Self::BattleStaff,
        Self::MageCrozier,
        Self::YewLongbow,
        Self::WarBow,
    ];

    pub fn category(self) -> ItemCategory {
        match self {
            Self::PaddedVest | Self::BronzeCuirass | Self::PlateCuirass => ItemCategory::Armor,
            Self::CrusaderHelm | Self::KettleHelm | Self::VikingHelm => ItemCategory::Helmet,
            Self::JewelRing
            | Self::PlainBand
            | Self::WeddingBand
            | Self::GoldenBand
            | Self::TwinRings
            | Self::LinkedRings => ItemCategory::Ring,
            Self::ArmingSword | Self::VikingBlade => ItemCategory::Sword,
            Self::BattleStaff | Self::MageCrozier => ItemCategory::Staff,
            Self::YewLongbow | Self::WarBow => ItemCategory::Bow,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::PaddedVest => "Padded Vest",
            Self::BronzeCuirass => "Bronze Cuirass",
            Self::PlateCuirass => "Plate Cuirass",
            Self::CrusaderHelm => "Crusader Helm",
            Self::KettleHelm => "Kettle Helm",
            Self::VikingHelm => "Viking Helm",
            Self::JewelRing => "Jewel Ring",
            Self::PlainBand => "Plain Band",
            Self::WeddingBand => "Wedding Band",
            Self::GoldenBand => "Golden Band",
            Self::TwinRings => "Twin Rings",
            Self::LinkedRings => "Linked Rings",
            Self::ArmingSword => "Arming Sword",
            Self::VikingBlade => "Viking Blade",
            Self::BattleStaff => "Battle Staff",
            Self::MageCrozier => "Mage Crozier",
            Self::YewLongbow => "Yew Longbow",
            Self::WarBow => "War Bow",
        }
    }

    pub fn effect_summary(self) -> &'static str {
        match self {
            Self::PaddedVest => "Reduce ranged hit damage.",
            Self::BronzeCuirass => "First melee hit is softened.",
            Self::PlateCuirass => "Prevents one-shot burst threshold.",
            Self::CrusaderHelm => "Shorter silence, stun, and slow.",
            Self::KettleHelm => "Ignores high-ground ranged bonus.",
            Self::VikingHelm => "Move burst after a kill.",
            Self::JewelRing => "Reveals hidden neutral ambushers.",
            Self::PlainBand => "Adds one consumable-only carry slot.",
            Self::WeddingBand => "Nearby ally pair gains armor.",
            Self::GoldenBand => "Neutral kills restore energy.",
            Self::TwinRings => "First ability gets shorter cooldown.",
            Self::LinkedRings => "Overheal becomes a small shield.",
            Self::ArmingSword => "Every third attack causes bleed.",
            Self::VikingBlade => "Bonus execute damage on low HP.",
            Self::BattleStaff => "Basic attacks splash magic damage.",
            Self::MageCrozier => "First spell after bracing gains range.",
            Self::YewLongbow => "Stationary shot gets bonus range.",
            Self::WarBow => "Attack-move shots apply a slow.",
        }
    }

}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ItemRequirement {
    NotWorker,
    BowUser,
    StaffUser,
    SwordUser,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ItemDisabledReason {
    WrongUnitType,
    CategoryConflict,
    InventoryFull,
    OnCooldown,
    MissingRequirement,
}

impl ItemDisabledReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::WrongUnitType => "Wrong unit type",
            Self::CategoryConflict => "Category already equipped",
            Self::InventoryFull => "Inventory full",
            Self::OnCooldown => "On cooldown",
            Self::MissingRequirement => "Requirement not met",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemStateEntry {
    pub item: ItemKind,
    pub enabled: bool,
    pub disabled_reason: Option<ItemDisabledReason>,
    pub cooldown_remaining: f32,
    pub active_toggled: bool,
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct UnitInventory {
    pub capacity: u8,
    pub items: Vec<ItemKind>,
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct ItemRuntimeState {
    pub items: Vec<ItemStateEntry>,
}

#[derive(Component, Clone, Debug)]
pub struct ItemPickup {
    pub item: ItemKind,
    pub owner: Option<Faction>,
    pub expires_at: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct PendingItemPickup {
    pub pickup: Entity,
}

#[derive(Component, Clone, Debug)]
pub struct ItemPickupLabel {
    pub name: String,
    pub extra: String,
    pub color: Color,
}

#[derive(Component)]
pub struct PickupBillboard;

#[derive(Component)]
pub struct PickupBackdrop;

#[derive(Component)]
pub struct PickupStem;

#[derive(Component)]
pub struct PickupAuraRing;

#[derive(Component)]
pub struct PickupCollectVfx {
    pub timer: Timer,
}

#[derive(Component)]
pub struct PickupBob {
    pub base_y: f32,
    pub phase: f32,
}


pub fn inferred_inventory_capacity(kind: EntityKind) -> u8 {
    match kind {
        EntityKind::Worker | EntityKind::Soldier | EntityKind::Archer | EntityKind::Scout => 1,
        EntityKind::Tank | EntityKind::Knight | EntityKind::Priest | EntityKind::Cavalry => 2,
        EntityKind::Mage | EntityKind::Catapult | EntityKind::BatteringRam => 3,
        _ => 0,
    }
}
