//! Item definition registry: requirements, restrictions, beam colors,
//! and per-item metadata loaded at startup.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;

use super::components::{ItemCategory, ItemKind, ItemRequirement};

#[derive(Clone)]
pub struct ItemDefinition {
    pub item: ItemKind,
    pub category: ItemCategory,
    pub requirements: &'static [ItemRequirement],
    pub beam_color: Color,
}

#[derive(Resource, Default)]
pub struct ItemRegistry {
    defs: HashMap<ItemKind, ItemDefinition>,
}

impl ItemRegistry {
    pub fn insert(&mut self, def: ItemDefinition) {
        self.defs.insert(def.item, def);
    }

    pub fn get(&self, item: ItemKind) -> &ItemDefinition {
        self.defs.get(&item).expect("item definition must exist")
    }
}

pub fn register_item_definitions(mut registry: ResMut<ItemRegistry>) {
    use ItemKind::*;
    use ItemRequirement::*;

    let defs = [
        ItemDefinition {
            item: PaddedVest,
            category: ItemCategory::Armor,
            requirements: &[NotWorker],
            beam_color: Color::srgb(0.80, 0.66, 0.35),
        },
        ItemDefinition {
            item: BronzeCuirass,
            category: ItemCategory::Armor,
            requirements: &[NotWorker],
            beam_color: Color::srgb(0.76, 0.56, 0.26),
        },
        ItemDefinition {
            item: PlateCuirass,
            category: ItemCategory::Armor,
            requirements: &[NotWorker],
            beam_color: Color::srgb(0.72, 0.72, 0.82),
        },
        ItemDefinition {
            item: CrusaderHelm,
            category: ItemCategory::Helmet,
            requirements: &[NotWorker],
            beam_color: Color::srgb(0.80, 0.80, 0.88),
        },
        ItemDefinition {
            item: KettleHelm,
            category: ItemCategory::Helmet,
            requirements: &[NotWorker],
            beam_color: Color::srgb(0.72, 0.66, 0.44),
        },
        ItemDefinition {
            item: VikingHelm,
            category: ItemCategory::Helmet,
            requirements: &[NotWorker],
            beam_color: Color::srgb(0.87, 0.72, 0.40),
        },
        ItemDefinition {
            item: JewelRing,
            category: ItemCategory::Ring,
            requirements: &[],
            beam_color: Color::srgb(0.50, 0.85, 1.0),
        },
        ItemDefinition {
            item: PlainBand,
            category: ItemCategory::Ring,
            requirements: &[],
            beam_color: Color::srgb(0.88, 0.78, 0.45),
        },
        ItemDefinition {
            item: WeddingBand,
            category: ItemCategory::Ring,
            requirements: &[],
            beam_color: Color::srgb(1.0, 0.78, 0.60),
        },
        ItemDefinition {
            item: GoldenBand,
            category: ItemCategory::Ring,
            requirements: &[],
            beam_color: Color::srgb(1.0, 0.84, 0.35),
        },
        ItemDefinition {
            item: TwinRings,
            category: ItemCategory::Ring,
            requirements: &[],
            beam_color: Color::srgb(0.88, 0.88, 1.0),
        },
        ItemDefinition {
            item: LinkedRings,
            category: ItemCategory::Ring,
            requirements: &[],
            beam_color: Color::srgb(0.58, 0.78, 1.0),
        },
        ItemDefinition {
            item: ArmingSword,
            category: ItemCategory::Sword,
            requirements: &[SwordUser],
            beam_color: Color::srgb(0.92, 0.42, 0.36),
        },
        ItemDefinition {
            item: VikingBlade,
            category: ItemCategory::Sword,
            requirements: &[SwordUser],
            beam_color: Color::srgb(1.0, 0.52, 0.34),
        },
        ItemDefinition {
            item: BattleStaff,
            category: ItemCategory::Staff,
            requirements: &[StaffUser],
            beam_color: Color::srgb(0.48, 0.72, 1.0),
        },
        ItemDefinition {
            item: MageCrozier,
            category: ItemCategory::Staff,
            requirements: &[StaffUser],
            beam_color: Color::srgb(0.74, 0.56, 1.0),
        },
        ItemDefinition {
            item: YewLongbow,
            category: ItemCategory::Bow,
            requirements: &[BowUser],
            beam_color: Color::srgb(0.44, 0.86, 0.54),
        },
        ItemDefinition {
            item: WarBow,
            category: ItemCategory::Bow,
            requirements: &[BowUser],
            beam_color: Color::srgb(0.40, 0.75, 0.45),
        },
    ];

    for def in defs {
        registry.insert(def);
    }
}

pub fn unit_meets_requirement(kind: EntityKind, req: ItemRequirement) -> bool {
    match req {
        ItemRequirement::NotWorker => kind != EntityKind::Worker,
        ItemRequirement::BowUser => {
            matches!(kind, EntityKind::Archer | EntityKind::Scout)
        }
        ItemRequirement::StaffUser => {
            matches!(kind, EntityKind::Mage | EntityKind::Priest)
        }
        ItemRequirement::SwordUser => {
            matches!(
                kind,
                EntityKind::Soldier | EntityKind::Tank | EntityKind::Knight | EntityKind::Cavalry
            )
        }
    }
}

pub fn first_missing_requirement(
    registry: &ItemRegistry,
    kind: EntityKind,
    item: ItemKind,
) -> Option<ItemRequirement> {
    registry
        .get(item)
        .requirements
        .iter()
        .copied()
        .find(|req| !unit_meets_requirement(kind, *req))
}

pub fn requirement_failure_message(req: ItemRequirement) -> &'static str {
    match req {
        ItemRequirement::NotWorker => {
            "Workers can carry this item, but only non-worker units gain its effect."
        }
        ItemRequirement::BowUser => "Only Archers and Scouts gain this bow's effect.",
        ItemRequirement::StaffUser => "Only Mages and Priests gain this staff's effect.",
        ItemRequirement::SwordUser => {
            "Only Soldiers, Tanks, Knights, and Cavalry gain this sword's effect."
        }
    }
}

pub fn item_effect_requirement_message(
    registry: &ItemRegistry,
    kind: EntityKind,
    item: ItemKind,
) -> Option<&'static str> {
    first_missing_requirement(registry, kind, item).map(requirement_failure_message)
}
