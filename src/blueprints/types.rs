use bevy::prelude::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::*;

// ── EntityKind — unified type enum ──

#[derive(
    Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Serialize, Deserialize,
)]
pub enum EntityKind {
    // Player Units
    Worker,
    Soldier,
    Archer,
    Tank,
    Knight,
    Mage,
    Priest,
    Cavalry,
    Scout,

    // Siege
    Catapult,
    BatteringRam,

    // Buildings
    Base,
    Barracks,
    Workshop,
    Tower,
    WatchTower,
    GuardTower,
    BallistaTower,
    BombardTower,
    Outpost,
    Gatehouse,
    WallSegment,
    WallPost,
    WallCorner,
    Floor,
    Storage,
    House,
    MageTower,
    Temple,
    Stable,
    SiegeWorks,
    Sawmill,
    Mine,
    OilRig,
    Smelter,
    Alchemist,

    // Mobs
    Goblin,

    // Summons
    SkeletonMinion,
    SpiritWolf,
    FireElemental,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EntityCategory {
    Unit,
    Building,
    Mob,
    Siege,
    Summon,
}

impl EntityKind {
    pub fn category(self) -> EntityCategory {
        match self {
            Self::Worker
            | Self::Soldier
            | Self::Archer
            | Self::Tank
            | Self::Knight
            | Self::Mage
            | Self::Priest
            | Self::Cavalry
            | Self::Scout => EntityCategory::Unit,

            Self::Catapult | Self::BatteringRam => EntityCategory::Siege,

            Self::Base
            | Self::Barracks
            | Self::Workshop
            | Self::Tower
            | Self::WatchTower
            | Self::GuardTower
            | Self::BallistaTower
            | Self::BombardTower
            | Self::Outpost
            | Self::Gatehouse
            | Self::WallSegment
            | Self::WallPost
            | Self::WallCorner
            | Self::Floor
            | Self::Storage
            | Self::House
            | Self::MageTower
            | Self::Temple
            | Self::Stable
            | Self::SiegeWorks
            | Self::Sawmill
            | Self::Mine
            | Self::OilRig
            | Self::Smelter
            | Self::Alchemist => EntityCategory::Building,

            Self::Goblin => EntityCategory::Mob,

            Self::SkeletonMinion | Self::SpiritWolf | Self::FireElemental => EntityCategory::Summon,
        }
    }

    /// Returns the armor type for this entity kind (used in the damage counter system).
    pub fn armor_type(self) -> ArmorType {
        use ArmorType::*;
        match self {
            // Light armor: workers, ranged, casters, scouts, light mobs, summons
            Self::Worker
            | Self::Archer
            | Self::Mage
            | Self::Priest
            | Self::Scout
            | Self::Goblin
            | Self::SkeletonMinion
            | Self::SpiritWolf
            | Self::FireElemental => Light,
            // Heavy armor: melee fighters
            Self::Soldier | Self::Tank | Self::Knight | Self::Cavalry => Heavy,
            // Siege armor: siege units
            Self::Catapult | Self::BatteringRam => Siege,
            // Structure armor: all buildings
            _ => Structure,
        }
    }

    /// Returns the damage type for this entity kind (used in the damage counter system).
    pub fn damage_type(self) -> DamageType {
        use DamageType::*;
        match self {
            // Pierce: ranged physical
            Self::Archer
            | Self::Tower
            | Self::WatchTower
            | Self::GuardTower
            | Self::BallistaTower
            | Self::BombardTower => Pierce,
            // Magic: casters, magic summons
            Self::Mage | Self::Priest | Self::FireElemental => Magic,
            // Siege: siege units
            Self::Catapult | Self::BatteringRam => SiegeDmg,
            // Melee: everything else (workers, soldiers, knights, cavalry, etc.)
            _ => Melee,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Worker => "Worker",
            Self::Soldier => "Soldier",
            Self::Archer => "Archer",
            Self::Tank => "Tank",
            Self::Knight => "Knight",
            Self::Mage => "Mage",
            Self::Priest => "Priest",
            Self::Cavalry => "Cavalry",
            Self::Scout => "Scout",
            Self::Catapult => "Catapult",
            Self::BatteringRam => "Battering Ram",
            Self::Base => "Base",
            Self::Barracks => "Barracks",
            Self::Workshop => "Workshop",
            Self::Tower => "Tower",
            Self::WatchTower => "Watch Tower",
            Self::GuardTower => "Guard Tower",
            Self::BallistaTower => "Ballista Tower",
            Self::BombardTower => "Bombard Tower",
            Self::Outpost => "Outpost",
            Self::Gatehouse => "Gatehouse",
            Self::WallSegment => "Wall",
            Self::WallPost => "Wall Post",
            Self::WallCorner => "Wall Corner",
            Self::Floor => "Floor",
            Self::Storage => "Storage",
            Self::House => "House",
            Self::MageTower => "Mage Tower",
            Self::Temple => "Temple",
            Self::Stable => "Stable",
            Self::SiegeWorks => "Siege Works",
            Self::Sawmill => "Sawmill",
            Self::Mine => "Mine",
            Self::OilRig => "Oil Rig",
            Self::Smelter => "Smelter",
            Self::Alchemist => "Alchemist",
            Self::Goblin => "Goblin",
            Self::SkeletonMinion => "Skeleton Minion",
            Self::SpiritWolf => "Spirit Wolf",
            Self::FireElemental => "Fire Elemental",
        }
    }

    pub const ALL: &'static [EntityKind] = &[
        EntityKind::Worker,
        EntityKind::Soldier,
        EntityKind::Archer,
        EntityKind::Tank,
        EntityKind::Knight,
        EntityKind::Mage,
        EntityKind::Priest,
        EntityKind::Cavalry,
        EntityKind::Scout,
        EntityKind::Catapult,
        EntityKind::BatteringRam,
        EntityKind::Base,
        EntityKind::Barracks,
        EntityKind::Workshop,
        EntityKind::Tower,
        EntityKind::WatchTower,
        EntityKind::GuardTower,
        EntityKind::BallistaTower,
        EntityKind::BombardTower,
        EntityKind::Outpost,
        EntityKind::Gatehouse,
        EntityKind::WallSegment,
        EntityKind::WallPost,
        EntityKind::Floor,
        EntityKind::Storage,
        EntityKind::House,
        EntityKind::MageTower,
        EntityKind::Temple,
        EntityKind::Stable,
        EntityKind::SiegeWorks,
        EntityKind::Sawmill,
        EntityKind::Mine,
        EntityKind::OilRig,
        EntityKind::Smelter,
        EntityKind::Alchemist,
        EntityKind::Goblin,
        EntityKind::SkeletonMinion,
        EntityKind::SpiritWolf,
        EntityKind::FireElemental,
        EntityKind::WallCorner,
    ];

    /// Convert to numeric index (position in ALL array). Used for network serialization.
    pub fn to_index(self) -> u16 {
        Self::ALL
            .iter()
            .position(|k| *k == self)
            .unwrap_or(u16::MAX as usize) as u16
    }

    /// Convert from numeric index back to EntityKind. Returns None if out of range.
    pub fn from_index(idx: u16) -> Option<EntityKind> {
        Self::ALL.get(idx as usize).copied()
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Worker => "Basic worker unit. Gathers resources and constructs buildings.",
            Self::Soldier => "Infantry unit. Can be upgraded to Knight.",
            Self::Archer => "Ranged unit with long attack range.",
            Self::Tank => "Heavy armored unit with high damage.",
            Self::Knight => "Elite melee unit with Charge and Shield Bash abilities.",
            Self::Mage => "Ranged caster with Fireball and Frost Nova.",
            Self::Priest => "Support caster with Heal and Holy Smite.",
            Self::Cavalry => "Fast mounted unit for flanking.",
            Self::Scout => "Fast recon unit with high vision. No combat ability.",
            Self::Catapult => "Long-range siege unit with AoE Boulder Throw.",
            Self::BatteringRam => "Melee siege unit with massive anti-structure damage.",
            Self::Base => "Main headquarters. Unlocks all other buildings.",
            Self::Barracks => "Trains Workers, Soldiers, and Archers.",
            Self::Workshop => "Trains heavy Tanks.",
            Self::Tower => "Defensive structure. Auto-attacks nearby enemies.",
            Self::WatchTower => "Cheap early defensive tower for light pressure.",
            Self::GuardTower => "Durable general-purpose defensive tower.",
            Self::BallistaTower => "Long-range anti-armor and anti-siege tower.",
            Self::BombardTower => "Splash-damage tower for breaking up swarms.",
            Self::Outpost => "Vision structure. Reveals nearby territory but does not attack.",
            Self::Gatehouse => "Fortified wall gateway for controlled chokepoints.",
            Self::WallSegment => "Defensive wall segment. Best placed in long runs.",
            Self::WallPost => "Wall junction support piece.",
            Self::WallCorner => "Corner wall piece. Auto-placed at wall bends.",
            Self::Floor => "Grid-based foundation tile. Flattens terrain and supports tidy base layouts.",
            Self::Storage => "Resource depot. Increases storage capacity.",
            Self::House => {
                "Housing building. Increases max units by +4 at level 1, +6 at level 2, and +8 at level 3."
            }
            Self::MageTower => "Trains Mages and Priests.",
            Self::Temple => "Trains Priests. Provides healing aura when upgraded.",
            Self::Stable => "Trains Cavalry and Knights.",
            Self::SiegeWorks => "Trains Catapults and Battering Rams.",
            Self::Sawmill => "Harvests Wood and produces Planks and Charcoal. Assign workers for best output.",
            Self::Mine => "Extracts Copper, Iron, and Gold from nearby deposits. Assign workers for best output.",
            Self::OilRig => "Extracts Oil from nearby deposits.",
            Self::Smelter => "Smelts Bronze and Steel from raw ores. Assign workers to deliver inputs.",
            Self::Alchemist => "Produces Gunpowder from Charcoal and Oil. Required for siege upgrades.",
            Self::Goblin => "Enemy mob.",
            Self::SkeletonMinion | Self::SpiritWolf | Self::FireElemental => "Summoned creature.",
        }
    }

    pub fn uses_tower_auto_attack(self) -> bool {
        matches!(
            self,
            Self::Tower
                | Self::WatchTower
                | Self::GuardTower
                | Self::BallistaTower
                | Self::BombardTower
        )
    }
}

pub(crate) fn random_unit_display_name(kind: EntityKind, rng: &mut impl Rng) -> String {
    const PREFIXES: &[&str] = &[
        "Ash", "Black", "Bright", "Cold", "Dawn", "Deep", "Ember", "Flint", "Golden", "Gray",
        "Iron", "Oak", "Red", "Stone", "Storm", "Swift", "Vale", "Wolf",
    ];
    const SUFFIXES: &[&str] = &[
        "arrow", "bane", "blade", "brand", "brook", "crest", "fall", "fang", "field", "forge",
        "guard", "heart", "helm", "mark", "runner", "shade", "song", "watch",
    ];
    const FIRST_NAMES: &[&str] = &[
        "Alden", "Bren", "Cass", "Darian", "Elric", "Fen", "Gareth", "Hale", "Ivor", "Jora",
        "Kellan", "Lyra", "Mara", "Nora", "Orin", "Perrin", "Rhea", "Soren", "Talia", "Vera",
    ];

    let title = match kind {
        EntityKind::Worker => Some("the Builder"),
        EntityKind::Soldier => Some("the Bold"),
        EntityKind::Archer => Some("the Keen"),
        EntityKind::Tank => Some("the Wall"),
        EntityKind::Knight => Some("the Valiant"),
        EntityKind::Mage => Some("the Wise"),
        EntityKind::Priest => Some("the Kindly"),
        EntityKind::Cavalry => Some("the Swift"),
        EntityKind::Scout => Some("the Far-Seer"),
        EntityKind::Catapult => Some("the Breaker"),
        EntityKind::BatteringRam => Some("the Hammer"),
        EntityKind::SkeletonMinion => Some("the Bound"),
        EntityKind::SpiritWolf => Some("the Wild"),
        EntityKind::FireElemental => Some("the Burning"),
        _ => None,
    };

    if rng.random_bool(0.55) {
        let first = FIRST_NAMES[rng.random_range(0..FIRST_NAMES.len())];
        if let Some(title) = title {
            format!("{first} {title}")
        } else {
            first.to_string()
        }
    } else {
        let prefix = PREFIXES[rng.random_range(0..PREFIXES.len())];
        let suffix = SUFFIXES[rng.random_range(0..SUFFIXES.len())];
        format!("{prefix}{suffix}")
    }
}

// ── Stat bundles ──

#[derive(Clone, Debug)]
pub struct CombatStats {
    pub hp: f32,
    pub damage: f32,
    pub attack_range: f32,
    pub attack_cooldown_secs: f32,
    pub aggro_range: Option<f32>,
    pub is_ranged: bool,
}

pub(crate) fn default_attack_profile(kind: EntityKind, combat: &CombatStats) -> AttackProfile {
    let mut profile = match kind {
        EntityKind::Worker => AttackProfile {
            windup_secs: 0.28,
            recovery_secs: 0.36,
            projectile_speed: 0.0,
            projectile_scale: 0.0,
            impact_scale: 0.65,
        },
        EntityKind::Archer | EntityKind::Scout | EntityKind::Tower | EntityKind::WatchTower => {
            AttackProfile {
                windup_secs: 0.18,
                recovery_secs: 0.25,
                projectile_speed: 24.0,
                projectile_scale: 0.11,
                impact_scale: 0.55,
            }
        }
        EntityKind::BallistaTower | EntityKind::Catapult | EntityKind::BatteringRam => {
            AttackProfile {
                windup_secs: 0.35,
                recovery_secs: 0.45,
                projectile_speed: 18.0,
                projectile_scale: 0.2,
                impact_scale: 1.1,
            }
        }
        EntityKind::Mage | EntityKind::Priest => AttackProfile {
            windup_secs: 0.32,
            recovery_secs: 0.3,
            projectile_speed: 16.0,
            projectile_scale: 0.16,
            impact_scale: 0.85,
        },
        EntityKind::Goblin => AttackProfile {
            windup_secs: 0.16,
            recovery_secs: 0.24,
            projectile_speed: 0.0,
            projectile_scale: 0.0,
            impact_scale: 0.55,
        },
        _ if combat.is_ranged => AttackProfile {
            windup_secs: 0.22,
            recovery_secs: 0.28,
            projectile_speed: 18.0,
            projectile_scale: 0.14,
            impact_scale: 0.7,
        },
        _ => AttackProfile {
            windup_secs: 0.24,
            recovery_secs: 0.3,
            projectile_speed: 0.0,
            projectile_scale: 0.0,
            impact_scale: 0.75,
        },
    };

    if combat.is_ranged && profile.projectile_speed <= 0.0 {
        profile.projectile_speed = 16.0;
        profile.projectile_scale = 0.14;
    }

    profile
}

pub(crate) fn default_attack_timing(kind: EntityKind, combat: &CombatStats) -> AttackTiming {
    match kind {
        EntityKind::Worker => AttackTiming {
            _attack_point_secs: 0.22,
            _backswing_secs: 0.26,
            _turn_rate_rad_per_sec: 10.0,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
        EntityKind::Soldier => AttackTiming {
            _attack_point_secs: 0.24,
            _backswing_secs: 0.30,
            _turn_rate_rad_per_sec: 9.0,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
        EntityKind::Archer | EntityKind::Scout => AttackTiming {
            _attack_point_secs: 0.20,
            _backswing_secs: 0.36,
            _turn_rate_rad_per_sec: 8.0,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
        EntityKind::Tank => AttackTiming {
            _attack_point_secs: 0.34,
            _backswing_secs: 0.42,
            _turn_rate_rad_per_sec: 6.5,
            minimum_range: 0.0,
            _can_move_during_backswing: false,
        },
        EntityKind::Knight => AttackTiming {
            _attack_point_secs: 0.26,
            _backswing_secs: 0.28,
            _turn_rate_rad_per_sec: 8.5,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
        EntityKind::Mage | EntityKind::Priest => AttackTiming {
            _attack_point_secs: 0.30,
            _backswing_secs: 0.34,
            _turn_rate_rad_per_sec: 7.0,
            minimum_range: 0.0,
            _can_move_during_backswing: false,
        },
        EntityKind::Cavalry => AttackTiming {
            _attack_point_secs: 0.22,
            _backswing_secs: 0.24,
            _turn_rate_rad_per_sec: 9.5,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
        EntityKind::Catapult => AttackTiming {
            _attack_point_secs: 0.42,
            _backswing_secs: 0.55,
            _turn_rate_rad_per_sec: 4.0,
            minimum_range: 5.0,
            _can_move_during_backswing: false,
        },
        EntityKind::BatteringRam => AttackTiming {
            _attack_point_secs: 0.36,
            _backswing_secs: 0.46,
            _turn_rate_rad_per_sec: 5.0,
            minimum_range: 0.0,
            _can_move_during_backswing: false,
        },
        EntityKind::Tower | EntityKind::WatchTower => AttackTiming {
            _attack_point_secs: 0.16,
            _backswing_secs: 0.22,
            _turn_rate_rad_per_sec: f32::INFINITY,
            minimum_range: 0.0,
            _can_move_during_backswing: false,
        },
        EntityKind::GuardTower => AttackTiming {
            _attack_point_secs: 0.18,
            _backswing_secs: 0.24,
            _turn_rate_rad_per_sec: f32::INFINITY,
            minimum_range: 0.0,
            _can_move_during_backswing: false,
        },
        EntityKind::BallistaTower => AttackTiming {
            _attack_point_secs: 0.26,
            _backswing_secs: 0.34,
            _turn_rate_rad_per_sec: f32::INFINITY,
            minimum_range: 0.0,
            _can_move_during_backswing: false,
        },
        EntityKind::BombardTower => AttackTiming {
            _attack_point_secs: 0.34,
            _backswing_secs: 0.46,
            _turn_rate_rad_per_sec: f32::INFINITY,
            minimum_range: 3.5,
            _can_move_during_backswing: false,
        },
        EntityKind::Goblin => AttackTiming {
            _attack_point_secs: 0.16,
            _backswing_secs: 0.24,
            _turn_rate_rad_per_sec: 9.0,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
        _ if combat.is_ranged => AttackTiming {
            _attack_point_secs: 0.22,
            _backswing_secs: 0.32,
            _turn_rate_rad_per_sec: 7.5,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
        _ => AttackTiming {
            _attack_point_secs: 0.24,
            _backswing_secs: 0.30,
            _turn_rate_rad_per_sec: 8.0,
            minimum_range: 0.0,
            _can_move_during_backswing: true,
        },
    }
}

pub(crate) fn default_targeting_profile(kind: EntityKind) -> TargetingProfile {
    match kind {
        EntityKind::Worker | EntityKind::Scout => TargetingProfile {
            distance_weight: 2.4,
            low_hp_weight: 0.8,
            threat_weight: 0.7,
            counter_weight: 0.2,
            building_penalty: 4.0,
            reserved_damage_penalty: 0.4,
        },
        EntityKind::Soldier | EntityKind::Tank => TargetingProfile {
            distance_weight: 1.7,
            low_hp_weight: 1.2,
            threat_weight: 1.4,
            counter_weight: 1.3,
            building_penalty: 2.5,
            reserved_damage_penalty: 0.8,
        },
        EntityKind::Archer => TargetingProfile {
            distance_weight: 1.1,
            low_hp_weight: 2.0,
            threat_weight: 1.2,
            counter_weight: 1.7,
            building_penalty: 3.5,
            reserved_damage_penalty: 2.1,
        },
        EntityKind::Knight | EntityKind::Cavalry => TargetingProfile {
            distance_weight: 1.3,
            low_hp_weight: 1.0,
            threat_weight: 1.6,
            counter_weight: 1.1,
            building_penalty: 2.0,
            reserved_damage_penalty: 0.7,
        },
        EntityKind::Mage => TargetingProfile {
            distance_weight: 1.0,
            low_hp_weight: 1.3,
            threat_weight: 1.8,
            counter_weight: 1.6,
            building_penalty: 3.0,
            reserved_damage_penalty: 1.2,
        },
        EntityKind::Priest => TargetingProfile {
            distance_weight: 1.0,
            low_hp_weight: 1.4,
            threat_weight: 1.0,
            counter_weight: 1.2,
            building_penalty: 4.0,
            reserved_damage_penalty: 1.0,
        },
        EntityKind::Catapult => TargetingProfile {
            distance_weight: 0.7,
            low_hp_weight: 0.4,
            threat_weight: 1.8,
            counter_weight: 0.9,
            building_penalty: -1.0,
            reserved_damage_penalty: 0.2,
        },
        EntityKind::BatteringRam => TargetingProfile {
            distance_weight: 0.9,
            low_hp_weight: 0.1,
            threat_weight: 0.8,
            counter_weight: 0.4,
            building_penalty: -3.5,
            reserved_damage_penalty: 0.1,
        },
        EntityKind::Tower
        | EntityKind::WatchTower
        | EntityKind::GuardTower
        | EntityKind::BallistaTower
        | EntityKind::BombardTower => TargetingProfile {
            distance_weight: 0.8,
            low_hp_weight: 1.5,
            threat_weight: 1.9,
            counter_weight: 1.1,
            building_penalty: 99.0,
            reserved_damage_penalty: 1.8,
        },
        _ if matches!(
            kind.category(),
            EntityCategory::Mob | EntityCategory::Summon
        ) =>
        {
            TargetingProfile {
                distance_weight: 1.5,
                low_hp_weight: 1.0,
                threat_weight: 1.1,
                counter_weight: 0.9,
                building_penalty: 3.5,
                reserved_damage_penalty: 0.6,
            }
        }
        _ => TargetingProfile {
            distance_weight: 1.5,
            low_hp_weight: 1.0,
            threat_weight: 1.0,
            counter_weight: 1.0,
            building_penalty: 3.0,
            reserved_damage_penalty: 0.8,
        },
    }
}

pub(crate) fn default_threat_value(kind: EntityKind) -> ThreatValue {
    ThreatValue(match kind {
        EntityKind::Worker => 0.35,
        EntityKind::Scout => 0.15,
        EntityKind::Soldier => 0.90,
        EntityKind::Archer => 1.15,
        EntityKind::Tank => 1.45,
        EntityKind::Knight => 1.30,
        EntityKind::Mage => 1.55,
        EntityKind::Priest => 1.10,
        EntityKind::Cavalry => 1.20,
        EntityKind::Catapult => 2.10,
        EntityKind::BatteringRam => 1.90,
        EntityKind::Goblin => 0.55,
        EntityKind::Tower | EntityKind::WatchTower => 1.20,
        EntityKind::GuardTower => 1.45,
        EntityKind::BallistaTower => 1.70,
        EntityKind::BombardTower => 1.85,
        _ => 0.0,
    })
}

pub(crate) fn default_combat_fx(kind: EntityKind, combat: &CombatStats) -> CombatFxKind {
    match kind {
        EntityKind::Archer
        | EntityKind::Scout
        | EntityKind::Tower
        | EntityKind::WatchTower
        | EntityKind::GuardTower
        | EntityKind::BallistaTower => CombatFxKind::Pierce,
        EntityKind::Mage | EntityKind::Priest => CombatFxKind::Arcane,
        EntityKind::Catapult | EntityKind::BatteringRam | EntityKind::BombardTower => {
            CombatFxKind::Siege
        }
        EntityKind::Goblin => CombatFxKind::Shadow,
        _ if combat.is_ranged => CombatFxKind::Pierce,
        _ => CombatFxKind::Slash,
    }
}

#[derive(Clone, Debug)]
pub struct MovementStats {
    pub speed: f32,
    pub y_offset: f32,
}

#[derive(Clone, Debug)]
pub struct GatheringStats {
    pub gather_speed: f32,
    pub carry_weight_capacity: f32,
}

#[derive(Clone, Debug)]
pub struct VisionStats {
    pub range: f32,
}

#[derive(Clone, Debug)]
pub struct ResourceCost {
    pub amounts: [u32; ResourceType::COUNT],
}

impl Default for ResourceCost {
    fn default() -> Self {
        Self {
            amounts: [0; ResourceType::COUNT],
        }
    }
}

impl ResourceCost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, rt: ResourceType, amt: u32) -> Self {
        self.amounts[rt.index()] = amt;
        self
    }

    pub fn get(&self, rt: ResourceType) -> u32 {
        self.amounts[rt.index()]
    }

    pub fn set(&mut self, rt: ResourceType, amt: u32) {
        self.amounts[rt.index()] = amt;
    }

    pub fn can_afford(&self, res: &PlayerResources) -> bool {
        res.can_afford_cost(self)
    }

    pub fn deduct(&self, res: &mut PlayerResources) {
        res.subtract_cost(self);
    }

    /// Check if stored + carried resources are enough to afford this cost.
    pub fn can_afford_with_carried(
        &self,
        stored: &PlayerResources,
        carried: &PlayerResources,
    ) -> bool {
        ResourceType::ALL
            .iter()
            .all(|rt| stored.get(*rt) + carried.get(*rt) >= self.amounts[rt.index()])
    }

    /// Deduct from stored first, return the deficits that must come from carried workers.
    pub fn deduct_with_carried(&self, stored: &mut PlayerResources) -> [u32; ResourceType::COUNT] {
        let mut deficits = [0u32; ResourceType::COUNT];
        for rt in ResourceType::ALL.iter() {
            let i = rt.index();
            let have = stored.get(*rt);
            deficits[i] = self.amounts[i].saturating_sub(have);
            stored.amounts[i] = have.saturating_sub(self.amounts[i]);
        }
        deficits
    }

    pub fn cost_entries(&self) -> Vec<(ResourceType, u32)> {
        ResourceType::ALL
            .iter()
            .filter_map(|rt| {
                let a = self.amounts[rt.index()];
                if a > 0 {
                    Some((*rt, a))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct BuildingLevelData {
    pub cost: ResourceCost,
    pub time_secs: f32,
    pub scale_multiplier: f32,
    pub bonus: LevelBonus,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum LevelBonus {
    None,
    VisionBoost(f32),
    TrainTimeMultiplier(f32),
    TrainedStatBoost {
        #[allow(dead_code)]
        hp_mult: f32,
        #[allow(dead_code)]
        dmg_mult: f32,
    },
    RangeAndDamage {
        range_boost: f32,
        damage_boost: f32,
    },
    GatherAura {
        speed_bonus: f32,
        range: f32,
    },
    HealAura {
        heal_per_sec: f32,
        range: f32,
    },
    UnlocksTraining(Vec<EntityKind>),
    ProcessorUpgrade {
        harvest_rate_boost: f32,
        radius_boost: f32,
        extra_worker_slots: u8,
        unlock_resources: Vec<ResourceType>,
    },
    /// Unlock a production recipe at a given index and optionally add worker slots.
    UnlockRecipe {
        #[allow(dead_code)]
        recipe_index: usize,
        extra_worker_slots: u8,
    },
    /// Production speed multiplier (e.g. 0.67 = 33% faster).
    ProductionSpeedMultiplier(f32),
}

#[derive(Clone, Debug)]
pub struct BuildingData {
    pub construction_time_secs: f32,
    pub half_height: f32,
    pub trains: Vec<EntityKind>,
    pub prerequisite: Option<EntityKind>,
    pub level_upgrades: Vec<BuildingLevelData>,
}

#[derive(Clone, Debug)]
pub struct MobAiData {
    pub patrol_radius: f32,
}

// ── Visual definition ──

#[derive(Clone, Debug)]
pub struct VisualDef {
    pub mesh_kind: MeshKind,
    pub color: Color,
    pub selected_color: Color,
    pub selected_emissive: LinearRgba,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub enum MeshKind {
    Capsule { radius: f32, length: f32 },
    Cuboid { x: f32, y: f32, z: f32 },
    GltfScene { pick_radius: f32 },
    GltfCharacter { pick_radius: f32 },
}

impl MeshKind {
    /// Bounding sphere radius for mouse picking, with a generous buffer.
    pub fn pick_radius(&self) -> f32 {
        let r = match *self {
            MeshKind::Capsule { radius, length } => length / 2.0 + radius,
            MeshKind::Cuboid { x, y, z } => (x * x + y * y + z * z).sqrt() / 2.0,
            MeshKind::GltfScene { pick_radius } => return pick_radius,
            MeshKind::GltfCharacter { pick_radius } => return pick_radius,
        };
        r * 1.3
    }

    pub fn is_gltf(&self) -> bool {
        matches!(
            self,
            MeshKind::GltfScene { .. } | MeshKind::GltfCharacter { .. }
        )
    }

    pub fn is_gltf_character(&self) -> bool {
        matches!(self, MeshKind::GltfCharacter { .. })
    }
}

// ── Blueprint ──

#[derive(Clone, Debug)]
pub struct Blueprint {
    pub faction: Faction,
    pub combat: Option<CombatStats>,
    pub movement: Option<MovementStats>,
    pub gathering: Option<GatheringStats>,
    pub vision: Option<VisionStats>,
    pub cost: ResourceCost,
    pub train_time_secs: f32,
    pub building: Option<BuildingData>,
    pub mob_ai: Option<MobAiData>,
    pub visual: VisualDef,
}

// ── Blueprint Registry ──

#[derive(Resource)]
pub struct BlueprintRegistry {
    pub blueprints: HashMap<EntityKind, Blueprint>,
}

impl BlueprintRegistry {
    pub fn get(&self, kind: EntityKind) -> &Blueprint {
        self.blueprints
            .get(&kind)
            .unwrap_or_else(|| panic!("No blueprint registered for {:?}", kind))
    }

    /// All building EntityKinds that are currently defined, in order.
    pub fn building_kinds(&self) -> Vec<EntityKind> {
        // Return in a stable display order
        let order = [
            EntityKind::Base,
            EntityKind::Outpost,
            EntityKind::WallSegment,
            EntityKind::WallCorner,
            EntityKind::Gatehouse,
            EntityKind::Floor,
            EntityKind::WatchTower,
            EntityKind::GuardTower,
            EntityKind::BallistaTower,
            EntityKind::BombardTower,
            EntityKind::House,
            EntityKind::Barracks,
            EntityKind::Workshop,
            EntityKind::Storage,
            EntityKind::Sawmill,
            EntityKind::Mine,
            EntityKind::OilRig,
            EntityKind::Smelter,
            EntityKind::Alchemist,
            EntityKind::MageTower,
            EntityKind::Temple,
            EntityKind::Stable,
            EntityKind::SiegeWorks,
        ];
        order
            .iter()
            .copied()
            .filter(|k| self.blueprints.contains_key(k))
            .collect()
    }
}

// ── Entity Visual Cache ──

#[derive(Resource, Default)]
pub struct EntityVisualCache {
    pub meshes: HashMap<EntityKind, Handle<Mesh>>,
    pub materials_default: HashMap<EntityKind, Handle<StandardMaterial>>,
    pub materials_selected: HashMap<EntityKind, Handle<StandardMaterial>>,
    pub materials_hovered: HashMap<EntityKind, Handle<StandardMaterial>>,
    pub floor_piece_meshes: HashMap<FloorPieceKind, Handle<Mesh>>,
    /// Flat plane mesh for the floor brush indicator (no side walls / shadow issues).
    pub floor_brush_indicator: Option<Handle<Mesh>>,
}
