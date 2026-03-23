use bevy::prelude::*;
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline, OutlineStencil, OutlineVolume};
use rand::Rng;
use std::collections::HashMap;

use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};

// ── EntityKind — unified type enum ──

use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
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
    Skeleton,
    Orc,
    Demon,

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

            Self::Goblin | Self::Skeleton | Self::Orc | Self::Demon => EntityCategory::Mob,

            Self::SkeletonMinion | Self::SpiritWolf | Self::FireElemental => EntityCategory::Summon,
        }
    }

    /// Returns the armor type for this entity kind (used in the damage counter system).
    pub fn armor_type(self) -> ArmorType {
        use ArmorType::*;
        match self {
            // Light armor: workers, ranged, casters, scouts, light mobs, summons
            Self::Worker | Self::Archer | Self::Mage | Self::Priest | Self::Scout
            | Self::Goblin | Self::Skeleton
            | Self::SkeletonMinion | Self::SpiritWolf | Self::FireElemental => Light,
            // Heavy armor: melee fighters, heavy mobs
            Self::Soldier | Self::Tank | Self::Knight | Self::Cavalry
            | Self::Orc | Self::Demon => Heavy,
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
            Self::Archer | Self::Skeleton
            | Self::Tower | Self::WatchTower | Self::GuardTower
            | Self::BallistaTower | Self::BombardTower => Pierce,
            // Magic: casters, demons, magic summons
            Self::Mage | Self::Priest | Self::Demon | Self::FireElemental => Magic,
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
            Self::Skeleton => "Skeleton",
            Self::Orc => "Orc",
            Self::Demon => "Demon",
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
        EntityKind::Skeleton,
        EntityKind::Orc,
        EntityKind::Demon,
        EntityKind::SkeletonMinion,
        EntityKind::SpiritWolf,
        EntityKind::FireElemental,
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
            Self::Goblin | Self::Skeleton | Self::Orc | Self::Demon => "Enemy mob.",
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

fn default_attack_profile(kind: EntityKind, combat: &CombatStats) -> AttackProfile {
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
        EntityKind::Skeleton => AttackProfile {
            windup_secs: 0.22,
            recovery_secs: 0.28,
            projectile_speed: 0.0,
            projectile_scale: 0.0,
            impact_scale: 0.65,
        },
        EntityKind::Orc => AttackProfile {
            windup_secs: 0.3,
            recovery_secs: 0.34,
            projectile_speed: 0.0,
            projectile_scale: 0.0,
            impact_scale: 0.95,
        },
        EntityKind::Demon => AttackProfile {
            windup_secs: 0.38,
            recovery_secs: 0.4,
            projectile_speed: 14.0,
            projectile_scale: 0.18,
            impact_scale: 1.1,
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

fn default_combat_fx(kind: EntityKind, combat: &CombatStats) -> CombatFxKind {
    match kind {
        EntityKind::Archer
        | EntityKind::Scout
        | EntityKind::Tower
        | EntityKind::WatchTower
        | EntityKind::GuardTower
        | EntityKind::BallistaTower => CombatFxKind::Pierce,
        EntityKind::Mage | EntityKind::Priest | EntityKind::Demon => CombatFxKind::Arcane,
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
        // 30% buffer for easier clicking
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


// ── IsRanged marker ──

#[derive(Component)]
pub struct IsRanged;

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
            EntityKind::Gatehouse,
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
}

// ── Build the registry ──

pub fn build_registry() -> BlueprintRegistry {
    let mut blueprints = HashMap::new();

    // ── Player Units ──

    blueprints.insert(
        EntityKind::Worker,
        Blueprint {

            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 80.0,
                damage: 6.0,
                attack_range: 1.8,
                attack_cooldown_secs: 1.2,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 5.0,
                y_offset: 0.8,
            }),
            gathering: Some(GatheringStats {
                gather_speed: 5.0,
                carry_weight_capacity: 20.0,
            }),
            vision: Some(VisionStats { range: 15.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 30),
            train_time_secs: 5.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.9, 0.8, 0.2),
                selected_color: Color::srgb(1.0, 1.0, 0.4),
                selected_emissive: LinearRgba::new(0.3, 0.3, 0.0, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Soldier,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 120.0,
                damage: 12.0,
                attack_range: 2.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 4.5,
                y_offset: 0.9,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 20).with(ResourceType::Iron, 15),
            train_time_secs: 8.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.8, 0.15, 0.15),
                selected_color: Color::srgb(1.0, 0.3, 0.3),
                selected_emissive: LinearRgba::new(0.3, 0.05, 0.05, 1.0),
                scale: 1.0,
            },


        },
    );

    blueprints.insert(
        EntityKind::Archer,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 100.0,
                damage: 10.0,
                attack_range: 12.0,
                attack_cooldown_secs: 1.5,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: Some(MovementStats {
                speed: 5.5,
                y_offset: 0.75,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 18.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 25).with(ResourceType::Iron, 10),
            train_time_secs: 7.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.15, 0.7, 0.2),
                selected_color: Color::srgb(0.3, 1.0, 0.4),
                selected_emissive: LinearRgba::new(0.05, 0.3, 0.05, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Tank,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 250.0,
                damage: 18.0,
                attack_range: 2.5,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 3.0,
                y_offset: 1.25,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new().with(ResourceType::Copper, 20).with(ResourceType::Iron, 50).with(ResourceType::Gold, 15).with(ResourceType::Oil, 5).with(ResourceType::Steel, 5),
            train_time_secs: 15.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.8 },
                color: Color::srgb(0.35, 0.35, 0.4),
                selected_color: Color::srgb(0.6, 0.6, 0.65),
                selected_emissive: LinearRgba::new(0.1, 0.1, 0.12, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Knight,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 18.0,
                attack_range: 2.5,
                attack_cooldown_secs: 0.8,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 6.0,
                y_offset: 1.2,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 14.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 20).with(ResourceType::Copper, 15).with(ResourceType::Iron, 45).with(ResourceType::Gold, 20).with(ResourceType::Bronze, 5),
            train_time_secs: 12.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.8 },
                color: Color::srgb(0.7, 0.7, 0.75),
                selected_color: Color::srgb(0.9, 0.9, 0.95),
                selected_emissive: LinearRgba::new(0.2, 0.2, 0.25, 1.0),
                scale: 1.0,
            },

        },
    );

    blueprints.insert(
        EntityKind::Mage,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 70.0,
                damage: 15.0,
                attack_range: 14.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: Some(MovementStats {
                speed: 4.0,
                y_offset: 0.8,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 15).with(ResourceType::Gold, 50),
            train_time_secs: 15.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.3, 0.2, 0.7),
                selected_color: Color::srgb(0.5, 0.4, 1.0),
                selected_emissive: LinearRgba::new(0.1, 0.05, 0.3, 1.0),
                scale: 1.0,
            },

        },
    );

    blueprints.insert(
        EntityKind::Priest,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 80.0,
                damage: 6.0,
                attack_range: 10.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: Some(MovementStats {
                speed: 4.5,
                y_offset: 0.8,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 16.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 15).with(ResourceType::Gold, 30),
            train_time_secs: 12.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.9, 0.85, 0.6),
                selected_color: Color::srgb(1.0, 0.95, 0.7),
                selected_emissive: LinearRgba::new(0.3, 0.28, 0.1, 1.0),
                scale: 1.0,
            },

        },
    );

    blueprints.insert(
        EntityKind::Cavalry,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 14.0,
                attack_range: 2.0,
                attack_cooldown_secs: 0.9,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 7.0,
                y_offset: 1.1,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 14.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 25).with(ResourceType::Copper, 10).with(ResourceType::Iron, 25).with(ResourceType::Gold, 10),
            train_time_secs: 10.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.55, 0.4, 0.25),
                selected_color: Color::srgb(0.75, 0.6, 0.4),
                selected_emissive: LinearRgba::new(0.15, 0.1, 0.05, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Scout,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 40.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 999.0,
                aggro_range: None,
                is_ranged: false,
            }),
            movement: Some(MovementStats {
                speed: 8.0,
                y_offset: 0.7,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 25.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 15),
            train_time_secs: 4.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.0 },
                color: Color::srgb(0.3, 0.6, 0.3),
                selected_color: Color::srgb(0.5, 0.8, 0.5),
                selected_emissive: LinearRgba::new(0.05, 0.15, 0.05, 1.0),
                scale: 0.8,
            },
        },
    );

    // ── Siege ──

    blueprints.insert(
        EntityKind::Catapult,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 40.0,
                attack_range: 25.0,
                attack_cooldown_secs: 5.0,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: Some(MovementStats {
                speed: 2.0,
                y_offset: 1.0,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 28.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 80).with(ResourceType::Iron, 60).with(ResourceType::Gold, 20).with(ResourceType::Gunpowder, 5),
            train_time_secs: 20.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::Cuboid {
                    x: 1.5,
                    y: 1.0,
                    z: 2.0,
                },
                color: Color::srgb(0.5, 0.35, 0.2),
                selected_color: Color::srgb(0.7, 0.5, 0.3),
                selected_emissive: LinearRgba::new(0.1, 0.05, 0.02, 1.0),
                scale: 1.0,
            },

        },
    );

    blueprints.insert(
        EntityKind::BatteringRam,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 50.0,
                attack_range: 2.0,
                attack_cooldown_secs: 4.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 2.5,
                y_offset: 0.8,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 100).with(ResourceType::Iron, 40).with(ResourceType::Planks, 15),
            train_time_secs: 18.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::Cuboid {
                    x: 1.0,
                    y: 0.8,
                    z: 2.5,
                },
                color: Color::srgb(0.45, 0.3, 0.15),
                selected_color: Color::srgb(0.65, 0.45, 0.25),
                selected_emissive: LinearRgba::new(0.08, 0.04, 0.01, 1.0),
                scale: 1.0,
            },



        },
    );

    // ── Buildings ──

    blueprints.insert(
        EntityKind::Base,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 500.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 25.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 90).with(ResourceType::Iron, 15),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 1.5,
                trains: vec![EntityKind::Worker],
                prerequisite: None,
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 130).with(ResourceType::Iron, 30),
                        time_secs: 20.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::VisionBoost(5.0),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 210).with(ResourceType::Copper, 30).with(ResourceType::Iron, 80),
                        time_secs: 30.0,
                        scale_multiplier: 1.15,
                        bonus: LevelBonus::TrainTimeMultiplier(0.7),
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 5.0 },
                color: Color::srgb(0.6, 0.55, 0.45),
                selected_color: Color::srgb(0.6, 0.55, 0.45),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Barracks,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 350.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 15.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 75).with(ResourceType::Iron, 30),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 12.0,
                half_height: 1.25,
                trains: vec![EntityKind::Worker, EntityKind::Soldier, EntityKind::Scout],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 110).with(ResourceType::Iron, 40),
                        time_secs: 15.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::UnlocksTraining(vec![EntityKind::Archer]),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 170).with(ResourceType::Copper, 40).with(ResourceType::Iron, 90),
                        time_secs: 25.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::TrainedStatBoost {
                            hp_mult: 1.25,
                            dmg_mult: 1.25,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 5.0 },
                color: Color::srgb(0.7, 0.3, 0.25),
                selected_color: Color::srgb(0.7, 0.3, 0.25),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Workshop,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 400.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 15.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 90).with(ResourceType::Copper, 25).with(ResourceType::Iron, 55).with(ResourceType::Gold, 15).with(ResourceType::Bronze, 10),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 18.0,
                half_height: 1.5,
                trains: vec![EntityKind::Tank],
                prerequisite: Some(EntityKind::Mine),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 120).with(ResourceType::Copper, 40).with(ResourceType::Iron, 80).with(ResourceType::Gold, 20),
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.75),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 180).with(ResourceType::Copper, 70).with(ResourceType::Iron, 120).with(ResourceType::Gold, 40),
                        time_secs: 28.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::TrainedStatBoost {
                            hp_mult: 1.3,
                            dmg_mult: 1.3,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 5.0 },
                color: Color::srgb(0.45, 0.45, 0.5),
                selected_color: Color::srgb(0.45, 0.45, 0.5),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Tower,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 10.0,
                attack_range: 15.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 45).with(ResourceType::Copper, 10).with(ResourceType::Iron, 35),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 10.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 70).with(ResourceType::Copper, 20).with(ResourceType::Iron, 50),
                        time_secs: 12.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 5.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 110).with(ResourceType::Copper, 40).with(ResourceType::Iron, 70).with(ResourceType::Gold, 20),
                        time_secs: 20.0,
                        scale_multiplier: 1.15,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 5.0,
                            damage_boost: 8.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.55, 0.55, 0.6),
                selected_color: Color::srgb(0.55, 0.55, 0.6),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::WatchTower,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 160.0,
                damage: 8.0,
                attack_range: 13.0,
                attack_cooldown_secs: 1.5,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 18.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 35).with(ResourceType::Iron, 15),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 8.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 55).with(ResourceType::Iron, 25),
                        time_secs: 10.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 2.0,
                            damage_boost: 3.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 85).with(ResourceType::Copper, 15).with(ResourceType::Iron, 35),
                        time_secs: 16.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 5.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.58, 0.56, 0.52),
                selected_color: Color::srgb(0.58, 0.56, 0.52),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::GuardTower,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 260.0,
                damage: 14.0,
                attack_range: 16.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 22.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 60).with(ResourceType::Copper, 20).with(ResourceType::Iron, 45),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 11.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 85).with(ResourceType::Copper, 30).with(ResourceType::Iron, 60),
                        time_secs: 12.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 5.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 130).with(ResourceType::Copper, 55).with(ResourceType::Iron, 85).with(ResourceType::Gold, 20),
                        time_secs: 20.0,
                        scale_multiplier: 1.15,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 5.0,
                            damage_boost: 8.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.55, 0.55, 0.6),
                selected_color: Color::srgb(0.55, 0.55, 0.6),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::BallistaTower,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 220.0,
                damage: 28.0,
                attack_range: 21.0,
                attack_cooldown_secs: 3.5,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 24.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 70).with(ResourceType::Copper, 55).with(ResourceType::Iron, 80).with(ResourceType::Steel, 10),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::SiegeWorks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 95).with(ResourceType::Copper, 70).with(ResourceType::Iron, 100),
                        time_secs: 16.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 7.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 140).with(ResourceType::Copper, 95).with(ResourceType::Iron, 130).with(ResourceType::Gold, 30),
                        time_secs: 24.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 5.0,
                            damage_boost: 10.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.5, 0.5, 0.58),
                selected_color: Color::srgb(0.5, 0.5, 0.58),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::BombardTower,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 240.0,
                damage: 22.0,
                attack_range: 14.0,
                attack_cooldown_secs: 2.8,
                aggro_range: None,
                is_ranged: true,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 85).with(ResourceType::Copper, 45).with(ResourceType::Iron, 65).with(ResourceType::Gold, 35).with(ResourceType::Gunpowder, 5),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::MageTower),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 105).with(ResourceType::Copper, 60).with(ResourceType::Iron, 85).with(ResourceType::Gold, 45),
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 2.0,
                            damage_boost: 6.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 150).with(ResourceType::Copper, 85).with(ResourceType::Iron, 110).with(ResourceType::Gold, 65),
                        time_secs: 26.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 4.0,
                            damage_boost: 9.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.58, 0.5, 0.5),
                selected_color: Color::srgb(0.58, 0.5, 0.5),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Outpost,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 140.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 30.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 20).with(ResourceType::Iron, 10),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 6.0,
                half_height: 2.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 35).with(ResourceType::Iron, 20),
                        time_secs: 8.0,
                        scale_multiplier: 1.05,
                        bonus: LevelBonus::VisionBoost(6.0),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 55).with(ResourceType::Copper, 10).with(ResourceType::Iron, 30),
                        time_secs: 12.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::VisionBoost(10.0),
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 3.5 },
                color: Color::srgb(0.5, 0.45, 0.35),
                selected_color: Color::srgb(0.5, 0.45, 0.35),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Gatehouse,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 300.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 16.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 40).with(ResourceType::Copper, 10).with(ResourceType::Iron, 35),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 10.0,
                half_height: 2.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Outpost),
                level_upgrades: vec![],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.46, 0.43, 0.4),
                selected_color: Color::srgb(0.46, 0.43, 0.4),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::WallSegment,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 180.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 8.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 12),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 4.0,
                half_height: 1.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::Cuboid {
                    x: 1.0,
                    y: 2.2,
                    z: 0.7,
                },
                color: Color::srgb(0.42, 0.25, 0.11),
                selected_color: Color::srgb(0.58, 0.36, 0.17),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::WallPost,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 220.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 16),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 5.0,
                half_height: 1.2,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::Cuboid {
                    x: 0.9,
                    y: 2.6,
                    z: 0.9,
                },
                color: Color::srgb(0.40, 0.23, 0.10),
                selected_color: Color::srgb(0.58, 0.34, 0.16),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Storage,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 55).with(ResourceType::Iron, 15),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 8.0,
                half_height: 0.15,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 75).with(ResourceType::Iron, 25),
                        time_secs: 10.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::GatherAura {
                            speed_bonus: 0.15,
                            range: 20.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 120).with(ResourceType::Copper, 20).with(ResourceType::Iron, 45),
                        time_secs: 18.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::GatherAura {
                            speed_bonus: 0.30,
                            range: 30.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.5 },
                color: Color::srgb(0.45, 0.32, 0.18),
                selected_color: Color::srgb(0.45, 0.32, 0.18),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::House,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 45)
                .with(ResourceType::Iron, 10),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 7.0,
                half_height: 0.1,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 60)
                            .with(ResourceType::Iron, 15),
                        time_secs: 10.0,
                        scale_multiplier: 1.05,
                        bonus: LevelBonus::None,
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 90)
                            .with(ResourceType::Copper, 10)
                            .with(ResourceType::Iron, 30),
                        time_secs: 16.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::None,
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.62, 0.52, 0.42),
                selected_color: Color::srgb(0.62, 0.52, 0.42),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::MageTower,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 300.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 22.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 80).with(ResourceType::Copper, 30).with(ResourceType::Iron, 40).with(ResourceType::Gold, 55),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 20.0,
                half_height: 2.5,
                trains: vec![EntityKind::Mage, EntityKind::Priest],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 100).with(ResourceType::Copper, 40).with(ResourceType::Iron, 55).with(ResourceType::Gold, 80),
                        time_secs: 20.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.85),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 140).with(ResourceType::Copper, 60).with(ResourceType::Iron, 80).with(ResourceType::Gold, 130),
                        time_secs: 30.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::TrainedStatBoost {
                            hp_mult: 1.15,
                            dmg_mult: 1.2,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 5.0 },
                color: Color::srgb(0.35, 0.25, 0.55),
                selected_color: Color::srgb(0.35, 0.25, 0.55),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Temple,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 250.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 18.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 90).with(ResourceType::Copper, 20).with(ResourceType::Iron, 40).with(ResourceType::Gold, 70),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 22.0,
                half_height: 2.0,
                trains: vec![EntityKind::Priest],
                prerequisite: Some(EntityKind::MageTower),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 115).with(ResourceType::Copper, 30).with(ResourceType::Iron, 55).with(ResourceType::Gold, 85),
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::HealAura {
                            heal_per_sec: 2.0,
                            range: 15.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 170).with(ResourceType::Copper, 50).with(ResourceType::Iron, 75).with(ResourceType::Gold, 130),
                        time_secs: 28.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::HealAura {
                            heal_per_sec: 5.0,
                            range: 20.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 5.0 },
                color: Color::srgb(0.85, 0.8, 0.65),
                selected_color: Color::srgb(0.85, 0.8, 0.65),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Stable,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 300.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 85).with(ResourceType::Copper, 30).with(ResourceType::Iron, 45),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 1.25,
                trains: vec![EntityKind::Cavalry],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 115).with(ResourceType::Copper, 45).with(ResourceType::Iron, 65),
                        time_secs: 16.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::UnlocksTraining(vec![EntityKind::Knight]),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 170).with(ResourceType::Copper, 70).with(ResourceType::Iron, 90).with(ResourceType::Gold, 35),
                        time_secs: 25.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::TrainedStatBoost {
                            hp_mult: 1.2,
                            dmg_mult: 1.2,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 5.0 },
                color: Color::srgb(0.5, 0.35, 0.2),
                selected_color: Color::srgb(0.5, 0.35, 0.2),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::SiegeWorks,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 350.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 100).with(ResourceType::Copper, 35).with(ResourceType::Iron, 90).with(ResourceType::Gold, 30),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 20.0,
                half_height: 1.5,
                trains: vec![EntityKind::Catapult, EntityKind::BatteringRam],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 140).with(ResourceType::Copper, 50).with(ResourceType::Iron, 110).with(ResourceType::Gold, 45),
                        time_secs: 20.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.8),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 220).with(ResourceType::Copper, 80).with(ResourceType::Iron, 150).with(ResourceType::Gold, 75),
                        time_secs: 30.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::TrainedStatBoost {
                            hp_mult: 1.25,
                            dmg_mult: 1.0,
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 5.5 },
                color: Color::srgb(0.4, 0.35, 0.3),
                selected_color: Color::srgb(0.4, 0.35, 0.3),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    // ── Resource Processing Buildings ──

    blueprints.insert(
        EntityKind::Sawmill,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 50).with(ResourceType::Iron, 15),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 12.0,
                half_height: 1.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 70).with(ResourceType::Iron, 25),
                        time_secs: 10.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::ProcessorUpgrade {
                            harvest_rate_boost: 1.5,
                            radius_boost: 5.0,
                            extra_worker_slots: 1,
                            unlock_resources: vec![],
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 110).with(ResourceType::Copper, 15).with(ResourceType::Iron, 35),
                        time_secs: 15.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::ProcessorUpgrade {
                            harvest_rate_boost: 1.5,
                            radius_boost: 5.0,
                            extra_worker_slots: 1,
                            unlock_resources: vec![],
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.55, 0.35, 0.15),
                selected_color: Color::srgb(0.7, 0.45, 0.2),
                selected_emissive: LinearRgba::new(0.3, 0.2, 0.05, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Mine,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 70).with(ResourceType::Iron, 35),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 1.2,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 80).with(ResourceType::Iron, 50),
                        time_secs: 12.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::ProcessorUpgrade {
                            harvest_rate_boost: 1.0,
                            radius_boost: 3.0,
                            extra_worker_slots: 1,
                            unlock_resources: vec![ResourceType::Copper],
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 110).with(ResourceType::Copper, 40).with(ResourceType::Iron, 75).with(ResourceType::Gold, 25),
                        time_secs: 20.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::ProcessorUpgrade {
                            harvest_rate_boost: 1.5,
                            radius_boost: 5.0,
                            extra_worker_slots: 1,
                            unlock_resources: vec![ResourceType::Gold],
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.45, 0.4, 0.35),
                selected_color: Color::srgb(0.55, 0.5, 0.45),
                selected_emissive: LinearRgba::new(0.15, 0.12, 0.08, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::OilRig,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 75).with(ResourceType::Copper, 25).with(ResourceType::Iron, 35),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 1.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 95).with(ResourceType::Copper, 35).with(ResourceType::Iron, 45),
                        time_secs: 12.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::ProcessorUpgrade {
                            harvest_rate_boost: 1.0,
                            radius_boost: 4.0,
                            extra_worker_slots: 0,
                            unlock_resources: vec![],
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new().with(ResourceType::Wood, 135).with(ResourceType::Copper, 55).with(ResourceType::Iron, 65).with(ResourceType::Gold, 20),
                        time_secs: 18.0,
                        scale_multiplier: 1.12,
                        bonus: LevelBonus::ProcessorUpgrade {
                            harvest_rate_boost: 1.5,
                            radius_boost: 2.0,
                            extra_worker_slots: 0,
                            unlock_resources: vec![],
                        },
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.15, 0.15, 0.15),
                selected_color: Color::srgb(0.25, 0.25, 0.25),
                selected_emissive: LinearRgba::new(0.1, 0.1, 0.1, 1.0),
                scale: 1.0,
            },



        },
    );

    // ── Production Chain Buildings ──

    blueprints.insert(
        EntityKind::Smelter,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 300.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 80)
                .with(ResourceType::Copper, 20)
                .with(ResourceType::Iron, 40),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 16.0,
                half_height: 1.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Mine),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 100)
                            .with(ResourceType::Iron, 60)
                            .with(ResourceType::Copper, 30),
                        time_secs: 14.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::UnlockRecipe {
                            recipe_index: 1,
                            extra_worker_slots: 1,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 150)
                            .with(ResourceType::Iron, 90)
                            .with(ResourceType::Gold, 30),
                        time_secs: 20.0,
                        scale_multiplier: 1.15,
                        bonus: LevelBonus::ProductionSpeedMultiplier(0.67),
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 4.0 },
                color: Color::srgb(0.6, 0.35, 0.15),
                selected_color: Color::srgb(0.8, 0.5, 0.2),
                selected_emissive: LinearRgba::new(0.15, 0.08, 0.03, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Alchemist,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 250.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 60)
                .with(ResourceType::Iron, 30)
                .with(ResourceType::Gold, 25)
                .with(ResourceType::Oil, 15),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 18.0,
                half_height: 1.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Smelter),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 80)
                            .with(ResourceType::Iron, 50)
                            .with(ResourceType::Gold, 35)
                            .with(ResourceType::Oil, 25),
                        time_secs: 16.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::ProductionSpeedMultiplier(0.75),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 120)
                            .with(ResourceType::Iron, 80)
                            .with(ResourceType::Gold, 50)
                            .with(ResourceType::Oil, 40),
                        time_secs: 22.0,
                        scale_multiplier: 1.15,
                        bonus: LevelBonus::ProductionSpeedMultiplier(0.67),
                    },
                ],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfScene { pick_radius: 3.5 },
                color: Color::srgb(0.45, 0.2, 0.2),
                selected_color: Color::srgb(0.65, 0.3, 0.3),
                selected_emissive: LinearRgba::new(0.12, 0.05, 0.05, 1.0),
                scale: 1.0,
            },



        },
    );

    // ── Mobs ──

    blueprints.insert(
        EntityKind::Goblin,
        Blueprint {
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 50.0,
                damage: 5.0,
                attack_range: 1.5,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(15.0),
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 3.5,
                y_offset: 0.65,
            }),
            gathering: None,
            vision: None,
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: Some(MobAiData {
                patrol_radius: 12.0,
            }),
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.3, 0.6, 0.15),
                selected_color: Color::srgb(0.3, 0.6, 0.15),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Skeleton,
        Blueprint {
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 80.0,
                damage: 10.0,
                attack_range: 1.8,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(18.0),
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 3.0,
                y_offset: 0.78,
            }),
            gathering: None,
            vision: None,
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: Some(MobAiData {
                patrol_radius: 15.0,
            }),
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.5 },
                color: Color::srgb(0.85, 0.82, 0.75),
                selected_color: Color::srgb(0.85, 0.82, 0.75),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Orc,
        Blueprint {
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 120.0,
                damage: 15.0,
                attack_range: 2.0,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(20.0),
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 2.5,
                y_offset: 1.05,
            }),
            gathering: None,
            vision: None,
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: Some(MobAiData {
                patrol_radius: 18.0,
            }),
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.8 },
                color: Color::srgb(0.4, 0.3, 0.15),
                selected_color: Color::srgb(0.4, 0.3, 0.15),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::Demon,
        Blueprint {
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 25.0,
                attack_range: 2.2,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(25.0),
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 3.0,
                y_offset: 1.15,
            }),
            gathering: None,
            vision: None,
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: Some(MobAiData {
                patrol_radius: 20.0,
            }),
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 2.0 },
                color: Color::srgb(0.6, 0.1, 0.1),
                selected_color: Color::srgb(0.6, 0.1, 0.1),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },



        },
    );

    // ── Summons ──

    blueprints.insert(
        EntityKind::SkeletonMinion,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 40.0,
                damage: 6.0,
                attack_range: 1.5,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 4.0,
                y_offset: 0.7,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 8.0 }),
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 1.3 },
                color: Color::srgb(0.75, 0.72, 0.65),
                selected_color: Color::srgb(0.85, 0.82, 0.75),
                selected_emissive: LinearRgba::new(0.1, 0.1, 0.08, 1.0),
                scale: 0.9,
            },



        },
    );

    blueprints.insert(
        EntityKind::SpiritWolf,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 60.0,
                damage: 8.0,
                attack_range: 1.8,
                attack_cooldown_secs: 0.8,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 7.0,
                y_offset: 0.5,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::Capsule {
                    radius: 0.3,
                    length: 0.6,
                },
                color: Color::srgba(0.5, 0.6, 0.8, 0.7),
                selected_color: Color::srgba(0.6, 0.7, 0.9, 0.8),
                selected_emissive: LinearRgba::new(0.1, 0.15, 0.25, 1.0),
                scale: 1.0,
            },



        },
    );

    blueprints.insert(
        EntityKind::FireElemental,
        Blueprint {
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 80.0,
                damage: 12.0,
                attack_range: 3.0,
                attack_cooldown_secs: 1.5,
                aggro_range: None,
                is_ranged: false,

            }),
            movement: Some(MovementStats {
                speed: 3.5,
                y_offset: 0.9,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::Capsule {
                    radius: 0.35,
                    length: 1.0,
                },
                color: Color::srgb(0.9, 0.4, 0.1),
                selected_color: Color::srgb(1.0, 0.5, 0.15),
                selected_emissive: LinearRgba::new(0.5, 0.2, 0.05, 1.0),
                scale: 1.0,
            },



        },
    );

    BlueprintRegistry { blueprints }
}

// ── Spawn from blueprint ──

/// Spawn with default faction from blueprint (backward compat).
pub fn spawn_from_blueprint(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    kind: EntityKind,
    pos: Vec3,
    registry: &BlueprintRegistry,
    building_models: Option<&BuildingModelAssets>,
    unit_models: Option<&UnitModelAssets>,
    height_map: &HeightMap,
) -> Entity {
    let bp = registry.get(kind);
    spawn_from_blueprint_with_faction(
        commands,
        cache,
        kind,
        pos,
        registry,
        building_models,
        unit_models,
        height_map,
        bp.faction,
    )
}

/// Spawn an entity from a blueprint with an explicit faction.
pub fn spawn_from_blueprint_with_faction(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    kind: EntityKind,
    pos: Vec3,
    registry: &BlueprintRegistry,
    building_models: Option<&BuildingModelAssets>,
    unit_models: Option<&UnitModelAssets>,
    height_map: &HeightMap,
    faction: Faction,
) -> Entity {
    let bp = registry.get(kind);

    let mesh_handle = cache
        .meshes
        .get(&kind)
        .expect("Missing mesh for entity kind")
        .clone();
    let mat_handle = cache
        .materials_default
        .get(&kind)
        .expect("Missing material for entity kind")
        .clone();

    let is_gltf = bp.visual.mesh_kind.is_gltf();
    let is_gltf_character = bp.visual.mesh_kind.is_gltf_character();

    // Compute Y position
    let y_off = bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.0);
    let building_y = if is_gltf && !is_gltf_character {
        0.0 // GLTF building models sit at ground level
    } else {
        bp.building.as_ref().map(|b| b.half_height).unwrap_or(0.0)
    };
    let y = height_map.sample(pos.x, pos.z) + y_off + building_y;

    let pick_radius = bp.visual.mesh_kind.pick_radius() * bp.visual.scale;

    let mut entity_cmds = if is_gltf {
        // GLTF buildings/characters: no Mesh3d/MeshMaterial3d on parent
        commands.spawn((
            GameWorld,
            kind,
            faction,
            PickRadius(pick_radius),
            Transform::from_translation(Vec3::new(pos.x, y, pos.z))
                .with_scale(Vec3::splat(bp.visual.scale)),
            Visibility::default(),
            OutlineVolume {
                visible: false,
                colour: Color::NONE,
                width: 3.0,
            },
            OutlineStencil::default(),
        ))
    } else {
        commands.spawn((
            GameWorld,
            kind,
            faction,
            PickRadius(pick_radius),
            Mesh3d(mesh_handle),
            MeshMaterial3d(mat_handle),
            Transform::from_translation(Vec3::new(pos.x, y, pos.z))
                .with_scale(Vec3::splat(bp.visual.scale)),
            OutlineVolume {
                visible: false,
                colour: Color::NONE,
                width: 3.0,
            },
            OutlineStencil::default(),
        ))
    };

    // Category markers
    match kind.category() {
        EntityCategory::Unit | EntityCategory::Siege | EntityCategory::Summon => {
            let stance = match kind {
                EntityKind::Worker | EntityKind::Priest => UnitStance::Defensive,
                _ => UnitStance::Aggressive,
            };
            let mut rng = rand::rng();
            entity_cmds.insert((
                Unit,
                UnitState::default(),
                TaskSource::default(),
                TaskQueue::default(),
                stance,
                StatusEffects::default(),
                Experience::default(),
                VeterancyApplied(VeterancyLevel::Recruit),
                SpawnAnimation {
                    timer: Timer::from_seconds(0.5, TimerMode::Once),
                    target_scale: Vec3::splat(bp.visual.scale),
                },
                MovementSmoothing {
                    current_speed: 0.0,
                    acceleration: 12.0,
                    deceleration: 8.0,
                    speed_variation: rng.random_range(0.93..1.07),
                },
                IdleBehavior {
                    fidget_timer: Timer::from_seconds(
                        rng.random_range(5.0..10.0),
                        TimerMode::Repeating,
                    ),
                    fidget_look_target: None,
                    fidget_elapsed: 0.0,
                    breathing_phase: rng.random_range(0.0..std::f32::consts::TAU),
                },
            ));

            // Assign abilities based on unit kind
            let abilities: Vec<AbilityId> = match kind {
                EntityKind::Knight => vec![AbilityId::KnightCharge],
                EntityKind::Mage => vec![AbilityId::MageFireball, AbilityId::MageFrostNova],
                EntityKind::Priest => vec![AbilityId::PriestHeal, AbilityId::PriestHolySmite],
                EntityKind::Catapult => vec![AbilityId::CatapultAoeBoulder],
                _ => vec![],
            };
            if !abilities.is_empty() {
                entity_cmds.insert(UnitAbilities::new(abilities));
            }
        }
        EntityCategory::Mob => {
            entity_cmds.insert((Mob, FogHideable::Mob));
        }
        EntityCategory::Building => {
            let footprint = crate::buildings::footprint_for_kind(kind);
            entity_cmds.insert((Building, BuildingLevel(1), BuildingFootprint(footprint)));
            if let Some(ref bd) = bp.building {
                let mut construction_timer =
                    Timer::from_seconds(bd.construction_time_secs, TimerMode::Once);
                construction_timer.pause();
                entity_cmds.insert((
                    BuildingState::UnderConstruction,
                    ConstructionProgress {
                        timer: construction_timer,
                    },
                    ConstructionWorkers::default(),
                ));
            }
            if kind.uses_tower_auto_attack() {
                entity_cmds.insert(TowerAutoAttackEnabled(true));
            }
            // Base and Storage are deposit points with per-resource capacities
            if kind == EntityKind::Base {
                entity_cmds.insert((
                    DepositPoint,
                    StorageInventory {
                        caps: [500, 80, 120, 0, 0, 0, 0, 0, 0, 0],
                        ..default()
                    },
                ));
            } else if kind == EntityKind::Storage {
                entity_cmds.insert((
                    DepositPoint,
                    StorageInventory {
                        caps: [300, 300, 300, 300, 200, 100, 50, 100, 100, 50],
                        ..default()
                    },
                ));
            }
            // Resource processing buildings
            match kind {
                EntityKind::Sawmill => {
                    entity_cmds.insert((
                        DepositPoint,
                        StorageInventory {
                            caps: [3000, 0, 0, 0, 0, 500, 200, 0, 0, 0],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ResourceProcessor {
                            resource_types: vec![ResourceType::Wood],
                            harvest_radius: 15.0,
                            harvest_rate: 3.0,
                            max_workers: 3,
                            buffer: 0,

                            worker_rate_bonus: 0.5,
                            harvest_timer: Timer::from_seconds(3.0, TimerMode::Repeating),
                            harvest_accumulator: 0.0,
                        },
                        ResourceRespawnConfig {
                            resource_types: vec![ResourceType::Wood],
                            respawn_timer: Timer::from_seconds(30.0, TimerMode::Repeating),
                            respawn_radius: 15.0,
                            max_nodes: 5,
                            amount_per_node: 200,
                        },
                        ProductionState::new(vec![
                            ProductionRecipe {
                                name: "Planks",
                                inputs: vec![(ResourceType::Wood, 3)],
                                outputs: vec![(ResourceType::Planks, 2)],
                                cycle_secs: 8.0,
                                requires_level: 1,
                            },
                            ProductionRecipe {
                                name: "Charcoal",
                                inputs: vec![(ResourceType::Wood, 2)],
                                outputs: vec![(ResourceType::Charcoal, 1)],
                                cycle_secs: 6.0,
                                requires_level: 2,
                            },
                        ]),
                    ));
                }
                EntityKind::Mine => {
                    entity_cmds.insert((
                        DepositPoint,
                        StorageInventory {
                            caps: [0, 1000, 1000, 0, 0, 0, 0, 0, 0, 0],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ResourceProcessor {
                            resource_types: vec![ResourceType::Iron],
                            harvest_radius: 12.0,
                            harvest_rate: 2.0,
                            max_workers: 4,
                            buffer: 0,

                            worker_rate_bonus: 0.5,
                            harvest_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
                            harvest_accumulator: 0.0,
                        },
                        ResourceRespawnConfig {
                            resource_types: vec![ResourceType::Iron],
                            respawn_timer: Timer::from_seconds(45.0, TimerMode::Repeating),
                            respawn_radius: 12.0,
                            max_nodes: 4,
                            amount_per_node: 300,
                        },
                    ));
                }
                EntityKind::OilRig => {
                    entity_cmds.insert((
                        DepositPoint,
                        StorageInventory {
                            caps: [0, 0, 0, 0, 500, 0, 0, 0, 0, 0],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ResourceProcessor {
                            resource_types: vec![ResourceType::Oil],
                            harvest_radius: 12.0,
                            harvest_rate: 1.5,
                            max_workers: 2,
                            buffer: 0,

                            worker_rate_bonus: 0.4,
                            harvest_timer: Timer::from_seconds(5.0, TimerMode::Repeating),
                            harvest_accumulator: 0.0,
                        },
                        ResourceRespawnConfig {
                            resource_types: vec![ResourceType::Oil],
                            respawn_timer: Timer::from_seconds(60.0, TimerMode::Repeating),
                            respawn_radius: 12.0,
                            max_nodes: 3,
                            amount_per_node: 500,
                        },
                    ));
                }
                EntityKind::Smelter => {
                    entity_cmds.insert((
                        DepositPoint,
                        StorageInventory {
                            caps: [0, 200, 200, 0, 0, 0, 0, 200, 200, 0],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ProductionState::new(vec![
                            ProductionRecipe {
                                name: "Bronze",
                                inputs: vec![
                                    (ResourceType::Copper, 2),
                                    (ResourceType::Iron, 1),
                                ],
                                outputs: vec![(ResourceType::Bronze, 1)],
                                cycle_secs: 8.0,
                                requires_level: 1,
                            },
                            ProductionRecipe {
                                name: "Steel",
                                inputs: vec![
                                    (ResourceType::Iron, 3),
                                    (ResourceType::Charcoal, 1),
                                ],
                                outputs: vec![(ResourceType::Steel, 1)],
                                cycle_secs: 12.0,
                                requires_level: 2,
                            },
                        ]),
                    ));
                }
                EntityKind::Alchemist => {
                    entity_cmds.insert((
                        DepositPoint,
                        StorageInventory {
                            caps: [0, 0, 0, 0, 100, 0, 100, 0, 0, 200],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ProductionState::new(vec![ProductionRecipe {
                            name: "Gunpowder",
                            inputs: vec![
                                (ResourceType::Charcoal, 1),
                                (ResourceType::Oil, 1),
                            ],
                            outputs: vec![(ResourceType::Gunpowder, 1)],
                            cycle_secs: 10.0,
                            requires_level: 1,
                        }]),
                    ));
                }
                _ => {}
            }
        }
    }

    // Combat stats
    if let Some(ref combat) = bp.combat {
        let attack_profile = default_attack_profile(kind, combat);
        let combat_fx = default_combat_fx(kind, combat);
        entity_cmds.insert((
            Health {
                current: combat.hp,
                max: combat.hp,
            },
            AttackDamage(combat.damage),
            AttackRange(combat.attack_range),
            AttackCooldown {
                ready_in: combat.attack_cooldown_secs * 0.35,
                interval: combat.attack_cooldown_secs,
            },
            attack_profile,
            combat_fx,
            kind.armor_type(),
            kind.damage_type(),
        ));
        if let Some(aggro) = combat.aggro_range {
            entity_cmds.insert(AggroRange(aggro));
        }
        if combat.is_ranged {
            entity_cmds.insert(IsRanged);
        }
    } else {
        // Buildings without combat stats still need armor type for counter system
        entity_cmds.insert(kind.armor_type());
    }

    // Movement
    if let Some(ref movement) = bp.movement {
        entity_cmds.insert((
            UnitSpeed(movement.speed),
            FootstepTimer(Timer::from_seconds(0.4, TimerMode::Repeating)),
        ));
    }

    // Gathering
    if let Some(ref gathering) = bp.gathering {
        entity_cmds.insert((
            GatherSpeed(gathering.gather_speed),
            Carrying::default(),
            CarryCapacity(gathering.carry_weight_capacity),
            GatherAccumulator::default(),
        ));
    }

    // Vision
    if let Some(ref vision) = bp.vision {
        entity_cmds.insert(VisionRange(vision.range));
    }

    // Mob AI
    if let Some(ref _ai) = bp.mob_ai {
        entity_cmds.insert(PatrolState {
            state: PatrolStateKind::Idle,
            center: Vec3::new(pos.x, height_map.sample(pos.x, pos.z), pos.z),
            radius: bp.mob_ai.as_ref().unwrap().patrol_radius,
            patrol_target: None,
            chase_elapsed: 0.0,
        });
    }


    let entity_id = entity_cmds.id();

    // Spawn GLTF scene child for buildings with GltfScene mesh kind
    if !is_gltf_character && bp.visual.mesh_kind.is_gltf() {
        if let Some(models) = building_models {
            if let Some(scene_handle) = models.scene_for(kind, 1, pos) {
                let cal = models.calibration.get(&kind);
                let scale = cal.map(|c| c.scale).unwrap_or(1.0);
                let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                let child = commands
                    .spawn((
                        SceneRoot(scene_handle),
                        BuildingSceneChild,
                        InheritOutline,
                        AsyncSceneInheritOutline::default(),
                        Transform::from_scale(Vec3::splat(scale))
                            .with_translation(Vec3::new(0.0, y_off, 0.0)),
                    ))
                    .id();
                commands.entity(entity_id).add_child(child);
            }
        }
    }

    // Summon VFX for SpiritWolf and FireElemental
    match kind {
        EntityKind::SpiritWolf => {
            commands.entity(entity_id).insert(SummonVfx {
                color: Color::srgba(0.3, 0.5, 1.0, 0.6),
                emissive: LinearRgba::new(0.2, 0.4, 1.0, 1.0),
                pulse_speed: 3.0,
                particle_timer: Timer::from_seconds(0.15, TimerMode::Repeating),
                light_entity: None,
            });
        }
        EntityKind::FireElemental => {
            commands.entity(entity_id).insert(SummonVfx {
                color: Color::srgba(1.0, 0.4, 0.1, 0.7),
                emissive: LinearRgba::new(1.5, 0.6, 0.1, 1.0),
                pulse_speed: 5.0,
                particle_timer: Timer::from_seconds(0.1, TimerMode::Repeating),
                light_entity: None,
            });
        }
        _ => {}
    }

    // Spawn GLTF scene child for character models
    if is_gltf_character {
        if let Some(models) = unit_models {
            if let Some(scene_handle) = models.scenes.get(&kind) {
                let cal = models.calibration.get(&kind);
                let scale = cal.map(|c| c.scale).unwrap_or(2.0);
                let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                let facing = cal.map(|c| c.facing_rotation).unwrap_or(0.0);
                let child = commands
                    .spawn((
                        SceneRoot(scene_handle.clone()),
                        UnitSceneChild,
                        InheritOutline,
                        AsyncSceneInheritOutline::default(),
                        Transform::from_scale(Vec3::splat(scale))
                            .with_translation(Vec3::new(0.0, y_off, 0.0))
                            .with_rotation(Quat::from_rotation_y(facing)),
                    ))
                    .id();
                commands.entity(entity_id).add_child(child);
            }
        }
    }

    entity_id
}

// ── Build visual cache from registry ──

pub fn build_visual_cache(
    registry: &BlueprintRegistry,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> EntityVisualCache {
    let mut cache = EntityVisualCache::default();

    for (kind, bp) in &registry.blueprints {
        let mesh = match bp.visual.mesh_kind {
            MeshKind::Capsule { radius, length } => meshes.add(Capsule3d::new(radius, length)),
            MeshKind::Cuboid { x, y, z } => meshes.add(Cuboid::new(x, y, z)),
            MeshKind::GltfScene { .. } => meshes.add(Cuboid::new(4.0, 0.3, 4.0)),
            MeshKind::GltfCharacter { .. } => meshes.add(Cuboid::new(0.5, 0.1, 0.5)),
        };

        let mat_default = materials.add(StandardMaterial {
            base_color: bp.visual.color,
            ..default()
        });

        let mat_selected = materials.add(StandardMaterial {
            base_color: bp.visual.selected_color,
            emissive: bp.visual.selected_emissive,
            ..default()
        });

        let hovered_emissive = LinearRgba::new(
            bp.visual.selected_emissive.red * 0.35,
            bp.visual.selected_emissive.green * 0.35,
            bp.visual.selected_emissive.blue * 0.35,
            bp.visual.selected_emissive.alpha,
        );
        let mat_hovered = materials.add(StandardMaterial {
            base_color: bp.visual.color,
            emissive: hovered_emissive,
            ..default()
        });

        cache.meshes.insert(*kind, mesh);
        cache.materials_default.insert(*kind, mat_default);
        cache.materials_selected.insert(*kind, mat_selected);
        cache.materials_hovered.insert(*kind, mat_hovered);
    }

    cache
}

// ── Plugin ──

pub struct BlueprintPlugin;

impl Plugin for BlueprintPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_blueprints);
    }
}

fn setup_blueprints(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let registry = build_registry();
    let cache = build_visual_cache(&registry, &mut meshes, &mut materials);
    commands.insert_resource(registry);
    commands.insert_resource(cache);
}
