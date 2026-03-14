use bevy::prelude::*;
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline, OutlineStencil, OutlineVolume};
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
    MageTower,
    Temple,
    Stable,
    SiegeWorks,
    Sawmill,
    Mine,
    OilRig,

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
            | Self::Cavalry => EntityCategory::Unit,

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
            | Self::MageTower
            | Self::Temple
            | Self::Stable
            | Self::SiegeWorks
            | Self::Sawmill
            | Self::Mine
            | Self::OilRig => EntityCategory::Building,

            Self::Goblin | Self::Skeleton | Self::Orc | Self::Demon => EntityCategory::Mob,

            Self::SkeletonMinion | Self::SpiritWolf | Self::FireElemental => EntityCategory::Summon,
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
            Self::MageTower => "Mage Tower",
            Self::Temple => "Temple",
            Self::Stable => "Stable",
            Self::SiegeWorks => "Siege Works",
            Self::Sawmill => "Sawmill",
            Self::Mine => "Mine",
            Self::OilRig => "Oil Rig",
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
        EntityKind::MageTower,
        EntityKind::Temple,
        EntityKind::Stable,
        EntityKind::SiegeWorks,
        EntityKind::Sawmill,
        EntityKind::Mine,
        EntityKind::OilRig,
        EntityKind::Goblin,
        EntityKind::Skeleton,
        EntityKind::Orc,
        EntityKind::Demon,
        EntityKind::SkeletonMinion,
        EntityKind::SpiritWolf,
        EntityKind::FireElemental,
    ];

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
            Self::MageTower => "Trains Mages and Priests.",
            Self::Temple => "Trains Priests. Provides healing aura when upgraded.",
            Self::Stable => "Trains Cavalry and Knights.",
            Self::SiegeWorks => "Trains Catapults and Battering Rams.",
            Self::Sawmill => "Processes Wood from nearby trees automatically.",
            Self::Mine => "Processes Copper, Iron, and Gold from nearby deposits.",
            Self::OilRig => "Extracts Oil from nearby deposits.",
            Self::Goblin | Self::Skeleton | Self::Orc | Self::Demon => "Enemy mob.",
            Self::SkeletonMinion | Self::SpiritWolf | Self::FireElemental => "Summoned creature.",
        }
    }

    pub fn is_building(self) -> bool {
        self.category() == EntityCategory::Building
    }

    pub fn is_mob(self) -> bool {
        self.category() == EntityCategory::Mob
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
    pub projectile_speed: Option<f32>,
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

#[derive(Clone, Debug, Default)]
pub struct ResourceCost {
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
}

impl ResourceCost {
    pub fn can_afford(&self, res: &PlayerResources) -> bool {
        res.can_afford(self.wood, self.copper, self.iron, self.gold, self.oil)
    }

    pub fn deduct(&self, res: &mut PlayerResources) {
        res.subtract(self.wood, self.copper, self.iron, self.gold, self.oil);
    }

    /// Check if stored + carried resources are enough to afford this cost.
    pub fn can_afford_with_carried(
        &self,
        stored: &PlayerResources,
        carried: &PlayerResources,
    ) -> bool {
        let costs = [self.wood, self.copper, self.iron, self.gold, self.oil];
        ResourceType::ALL
            .iter()
            .enumerate()
            .all(|(i, rt)| stored.get(*rt) + carried.get(*rt) >= costs[i])
    }

    /// Deduct from stored first, return the deficit that must come from carried workers.
    /// Returns (wood, copper, iron, gold, oil) deficits.
    pub fn deduct_with_carried(&self, stored: &mut PlayerResources) -> (u32, u32, u32, u32, u32) {
        let costs = [self.wood, self.copper, self.iron, self.gold, self.oil];
        let mut deficits = [0u32; 5];
        for (i, rt) in ResourceType::ALL.iter().enumerate() {
            let have = stored.get(*rt);
            deficits[i] = costs[i].saturating_sub(have);
            stored.amounts[rt.index()] = have.saturating_sub(costs[i]);
        }
        (
            deficits[0],
            deficits[1],
            deficits[2],
            deficits[3],
            deficits[4],
        )
    }

    pub fn cost_entries(&self) -> Vec<(ResourceType, u32)> {
        let mut entries = Vec::new();
        if self.wood > 0 {
            entries.push((ResourceType::Wood, self.wood));
        }
        if self.copper > 0 {
            entries.push((ResourceType::Copper, self.copper));
        }
        if self.iron > 0 {
            entries.push((ResourceType::Iron, self.iron));
        }
        if self.gold > 0 {
            entries.push((ResourceType::Gold, self.gold));
        }
        if self.oil > 0 {
            entries.push((ResourceType::Oil, self.oil));
        }
        entries
    }
}

#[derive(Clone, Debug)]
pub struct BuildingLevelData {
    pub cost: ResourceCost,
    pub time_secs: f32,
    pub scale_multiplier: f32,
    pub bonus: LevelBonus,
}

#[derive(Clone, Debug)]
pub enum LevelBonus {
    None,
    VisionBoost(f32),
    TrainTimeMultiplier(f32),
    TrainedStatBoost {
        hp_mult: f32,
        dmg_mult: f32,
    },
    RangeAndDamage {
        range_boost: f32,
        damage_boost: f32,
    },
    CooldownMultiplier(f32),
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
    Cylinder { radius: f32, height: f32 },
    GltfScene { pick_radius: f32 },
    GltfCharacter { pick_radius: f32 },
}

impl MeshKind {
    /// Bounding sphere radius for mouse picking, with a generous buffer.
    pub fn pick_radius(&self) -> f32 {
        let r = match *self {
            MeshKind::Capsule { radius, length } => length / 2.0 + radius,
            MeshKind::Cuboid { x, y, z } => (x * x + y * y + z * z).sqrt() / 2.0,
            MeshKind::Cylinder { radius, height } => {
                (radius * radius + (height / 2.0).powi(2)).sqrt()
            }
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

// ── Ability system ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum AbilityId {
    Fireball,
    FrostNova,
    Heal,
    HolySmite,
    ShieldBash,
    Charge,
    SummonSkeleton,
    DarkBolt,
    BoulderThrow,
}

#[derive(Clone, Debug)]
pub struct AbilitySlot {
    pub id: AbilityId,
    pub cooldown_secs: f32,
    pub mana_cost: f32,
    pub range: f32,
    pub display_name: &'static str,
}

#[derive(Component, Clone, Debug)]
pub struct Abilities {
    pub slots: Vec<AbilityInstance>,
}

#[derive(Clone, Debug)]
pub struct AbilityInstance {
    pub id: AbilityId,
    pub cooldown: Timer,
    pub mana_cost: f32,
    pub range: f32,
}

impl AbilityInstance {
    pub fn from_slot(slot: &AbilitySlot) -> Self {
        Self {
            id: slot.id,
            cooldown: Timer::from_seconds(slot.cooldown_secs, TimerMode::Once),
            mana_cost: slot.mana_cost,
            range: slot.range,
        }
    }
}

// ── Relationship components ──

#[derive(Component)]
pub struct SummonedBy(pub Entity);

#[derive(Component)]
pub struct ActiveSummons {
    pub entities: Vec<Entity>,
    pub max_count: u32,
}

// ── IsRanged marker ──

#[derive(Component)]
pub struct IsRanged;

// ── Child entity definition ──

#[derive(Clone, Debug)]
pub struct ChildDef {
    pub kind: EntityKind,
    pub offset: Vec3,
    pub count: u32,
}

// ── Upgrade path ──

#[derive(Clone, Debug)]
pub struct UpgradePath {
    pub target: EntityKind,
    pub cost: ResourceCost,
    pub time_secs: f32,
    pub requires_building: Option<EntityKind>,
}

// ── Blueprint ──

#[derive(Clone, Debug)]
pub struct Blueprint {
    pub kind: EntityKind,
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
    pub children: Vec<ChildDef>,
    pub abilities: Vec<AbilitySlot>,
    pub upgrades: Vec<UpgradePath>,
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
            EntityKind::Barracks,
            EntityKind::Workshop,
            EntityKind::Storage,
            EntityKind::Sawmill,
            EntityKind::Mine,
            EntityKind::OilRig,
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
            kind: EntityKind::Worker,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 115.0,
                damage: 6.0,
                attack_range: 1.8,
                attack_cooldown_secs: 1.2,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
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
            cost: ResourceCost {
                wood: 30,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Soldier,
        Blueprint {
            kind: EntityKind::Soldier,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 100.0,
                damage: 12.0,
                attack_range: 2.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: Some(MovementStats {
                speed: 4.5,
                y_offset: 0.9,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost {
                wood: 20,
                iron: 15,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![UpgradePath {
                target: EntityKind::Knight,
                cost: ResourceCost {
                    iron: 30,
                    gold: 20,
                    ..Default::default()
                },
                time_secs: 12.0,
                requires_building: Some(EntityKind::Stable),
            }],
        },
    );

    blueprints.insert(
        EntityKind::Archer,
        Blueprint {
            kind: EntityKind::Archer,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 100.0,
                damage: 8.0,
                attack_range: 12.0,
                attack_cooldown_secs: 1.5,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(15.0),
            }),
            movement: Some(MovementStats {
                speed: 5.5,
                y_offset: 0.75,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 18.0 }),
            cost: ResourceCost {
                wood: 25,
                iron: 10,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Tank,
        Blueprint {
            kind: EntityKind::Tank,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 100.0,
                damage: 18.0,
                attack_range: 2.5,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: Some(MovementStats {
                speed: 3.0,
                y_offset: 1.25,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost {
                copper: 20,
                iron: 50,
                gold: 15,
                oil: 5,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Knight,
        Blueprint {
            kind: EntityKind::Knight,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 18.0,
                attack_range: 2.5,
                attack_cooldown_secs: 0.8,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: Some(MovementStats {
                speed: 6.0,
                y_offset: 1.2,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 14.0 }),
            cost: ResourceCost {
                wood: 20,
                copper: 15,
                iron: 45,
                gold: 20,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![
                AbilitySlot {
                    id: AbilityId::Charge,
                    cooldown_secs: 15.0,
                    mana_cost: 0.0,
                    range: 8.0,
                    display_name: "Charge",
                },
                AbilitySlot {
                    id: AbilityId::ShieldBash,
                    cooldown_secs: 8.0,
                    mana_cost: 0.0,
                    range: 2.5,
                    display_name: "Shield Bash",
                },
            ],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Mage,
        Blueprint {
            kind: EntityKind::Mage,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 70.0,
                damage: 15.0,
                attack_range: 14.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(12.0),
            }),
            movement: Some(MovementStats {
                speed: 4.0,
                y_offset: 0.8,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost {
                wood: 10,
                gold: 40,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![
                AbilitySlot {
                    id: AbilityId::Fireball,
                    cooldown_secs: 10.0,
                    mana_cost: 30.0,
                    range: 16.0,
                    display_name: "Fireball",
                },
                AbilitySlot {
                    id: AbilityId::FrostNova,
                    cooldown_secs: 20.0,
                    mana_cost: 50.0,
                    range: 8.0,
                    display_name: "Frost Nova",
                },
            ],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Priest,
        Blueprint {
            kind: EntityKind::Priest,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 80.0,
                damage: 6.0,
                attack_range: 10.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(10.0),
            }),
            movement: Some(MovementStats {
                speed: 4.5,
                y_offset: 0.8,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 16.0 }),
            cost: ResourceCost {
                wood: 15,
                gold: 30,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![
                AbilitySlot {
                    id: AbilityId::Heal,
                    cooldown_secs: 8.0,
                    mana_cost: 25.0,
                    range: 12.0,
                    display_name: "Heal",
                },
                AbilitySlot {
                    id: AbilityId::HolySmite,
                    cooldown_secs: 12.0,
                    mana_cost: 35.0,
                    range: 10.0,
                    display_name: "Holy Smite",
                },
            ],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Cavalry,
        Blueprint {
            kind: EntityKind::Cavalry,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 14.0,
                attack_range: 2.0,
                attack_cooldown_secs: 0.9,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: Some(MovementStats {
                speed: 7.0,
                y_offset: 1.1,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 14.0 }),
            cost: ResourceCost {
                wood: 25,
                copper: 10,
                iron: 25,
                gold: 10,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    // ── Siege ──

    blueprints.insert(
        EntityKind::Catapult,
        Blueprint {
            kind: EntityKind::Catapult,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 40.0,
                attack_range: 25.0,
                attack_cooldown_secs: 5.0,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(8.0),
            }),
            movement: Some(MovementStats {
                speed: 2.0,
                y_offset: 1.0,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 28.0 }),
            cost: ResourceCost {
                wood: 80,
                iron: 60,
                gold: 20,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![AbilitySlot {
                id: AbilityId::BoulderThrow,
                cooldown_secs: 8.0,
                mana_cost: 0.0,
                range: 25.0,
                display_name: "Boulder Throw",
            }],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::BatteringRam,
        Blueprint {
            kind: EntityKind::BatteringRam,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 50.0,
                attack_range: 2.0,
                attack_cooldown_secs: 4.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: Some(MovementStats {
                speed: 2.5,
                y_offset: 0.8,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost {
                wood: 100,
                iron: 40,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    // ── Buildings ──

    blueprints.insert(
        EntityKind::Base,
        Blueprint {
            kind: EntityKind::Base,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 500.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 25.0 }),
            cost: ResourceCost {
                wood: 90,
                iron: 15,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 1.5,
                trains: vec![EntityKind::Worker],
                prerequisite: None,
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 130,
                            iron: 30,
                            ..Default::default()
                        },
                        time_secs: 20.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::VisionBoost(5.0),
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 210,
                            copper: 30,
                            iron: 80,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Barracks,
        Blueprint {
            kind: EntityKind::Barracks,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 350.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 15.0 }),
            cost: ResourceCost {
                wood: 75,
                iron: 30,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 12.0,
                half_height: 1.25,
                trains: vec![EntityKind::Worker, EntityKind::Soldier],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 110,
                            iron: 40,
                            ..Default::default()
                        },
                        time_secs: 15.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::UnlocksTraining(vec![EntityKind::Archer]),
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 170,
                            copper: 40,
                            iron: 90,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Workshop,
        Blueprint {
            kind: EntityKind::Workshop,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 400.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 15.0 }),
            cost: ResourceCost {
                wood: 90,
                copper: 25,
                iron: 55,
                gold: 15,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 18.0,
                half_height: 1.5,
                trains: vec![EntityKind::Tank],
                prerequisite: Some(EntityKind::Mine),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 120,
                            copper: 40,
                            iron: 80,
                            gold: 20,
                            ..Default::default()
                        },
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.75),
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 180,
                            copper: 70,
                            iron: 120,
                            gold: 40,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Tower,
        Blueprint {
            kind: EntityKind::Tower,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 10.0,
                attack_range: 15.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(20.0),
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost {
                wood: 45,
                copper: 10,
                iron: 35,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 10.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 70,
                            copper: 20,
                            iron: 50,
                            ..Default::default()
                        },
                        time_secs: 12.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 5.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 110,
                            copper: 40,
                            iron: 70,
                            gold: 20,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::WatchTower,
        Blueprint {
            kind: EntityKind::WatchTower,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 160.0,
                damage: 8.0,
                attack_range: 13.0,
                attack_cooldown_secs: 1.5,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(20.0),
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 18.0 }),
            cost: ResourceCost {
                wood: 35,
                iron: 15,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 8.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 55,
                            iron: 25,
                            ..Default::default()
                        },
                        time_secs: 10.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 2.0,
                            damage_boost: 3.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 85,
                            copper: 15,
                            iron: 35,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::GuardTower,
        Blueprint {
            kind: EntityKind::GuardTower,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 260.0,
                damage: 14.0,
                attack_range: 16.0,
                attack_cooldown_secs: 2.0,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(20.0),
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 22.0 }),
            cost: ResourceCost {
                wood: 60,
                copper: 20,
                iron: 45,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 11.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 85,
                            copper: 30,
                            iron: 60,
                            ..Default::default()
                        },
                        time_secs: 12.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 5.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 130,
                            copper: 55,
                            iron: 85,
                            gold: 20,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::BallistaTower,
        Blueprint {
            kind: EntityKind::BallistaTower,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 220.0,
                damage: 28.0,
                attack_range: 21.0,
                attack_cooldown_secs: 3.5,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(24.0),
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 24.0 }),
            cost: ResourceCost {
                wood: 70,
                copper: 55,
                iron: 80,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::SiegeWorks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 95,
                            copper: 70,
                            iron: 100,
                            ..Default::default()
                        },
                        time_secs: 16.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 7.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 140,
                            copper: 95,
                            iron: 130,
                            gold: 30,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::BombardTower,
        Blueprint {
            kind: EntityKind::BombardTower,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 240.0,
                damage: 22.0,
                attack_range: 14.0,
                attack_cooldown_secs: 2.8,
                aggro_range: None,
                is_ranged: true,
                projectile_speed: Some(18.0),
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost {
                wood: 85,
                copper: 45,
                iron: 65,
                gold: 35,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::MageTower),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 105,
                            copper: 60,
                            iron: 85,
                            gold: 45,
                            ..Default::default()
                        },
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 2.0,
                            damage_boost: 6.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 150,
                            copper: 85,
                            iron: 110,
                            gold: 65,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Outpost,
        Blueprint {
            kind: EntityKind::Outpost,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 140.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 30.0 }),
            cost: ResourceCost {
                wood: 20,
                iron: 10,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 6.0,
                half_height: 2.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 35,
                            iron: 20,
                            ..Default::default()
                        },
                        time_secs: 8.0,
                        scale_multiplier: 1.05,
                        bonus: LevelBonus::VisionBoost(6.0),
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 55,
                            copper: 10,
                            iron: 30,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Gatehouse,
        Blueprint {
            kind: EntityKind::Gatehouse,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 300.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 16.0 }),
            cost: ResourceCost {
                wood: 40,
                copper: 10,
                iron: 35,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::WallSegment,
        Blueprint {
            kind: EntityKind::WallSegment,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 180.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 8.0 }),
            cost: ResourceCost {
                wood: 12,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::WallPost,
        Blueprint {
            kind: EntityKind::WallPost,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 220.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost {
                wood: 16,
                ..Default::default()
            },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Storage,
        Blueprint {
            kind: EntityKind::Storage,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost {
                wood: 55,
                iron: 15,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 8.0,
                half_height: 0.15,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 75,
                            iron: 25,
                            ..Default::default()
                        },
                        time_secs: 10.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::GatherAura {
                            speed_bonus: 0.15,
                            range: 20.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 120,
                            copper: 20,
                            iron: 45,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::MageTower,
        Blueprint {
            kind: EntityKind::MageTower,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 300.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 22.0 }),
            cost: ResourceCost {
                wood: 80,
                copper: 30,
                iron: 40,
                gold: 55,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 20.0,
                half_height: 2.5,
                trains: vec![EntityKind::Mage, EntityKind::Priest],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 100,
                            copper: 40,
                            iron: 55,
                            gold: 80,
                            ..Default::default()
                        },
                        time_secs: 20.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.85),
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 140,
                            copper: 60,
                            iron: 80,
                            gold: 130,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Temple,
        Blueprint {
            kind: EntityKind::Temple,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 250.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 18.0 }),
            cost: ResourceCost {
                wood: 90,
                copper: 20,
                iron: 40,
                gold: 70,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 22.0,
                half_height: 2.0,
                trains: vec![EntityKind::Priest],
                prerequisite: Some(EntityKind::MageTower),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 115,
                            copper: 30,
                            iron: 55,
                            gold: 85,
                            ..Default::default()
                        },
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::HealAura {
                            heal_per_sec: 2.0,
                            range: 15.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 170,
                            copper: 50,
                            iron: 75,
                            gold: 130,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Stable,
        Blueprint {
            kind: EntityKind::Stable,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 300.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost {
                wood: 85,
                copper: 30,
                iron: 45,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 1.25,
                trains: vec![EntityKind::Cavalry],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 115,
                            copper: 45,
                            iron: 65,
                            ..Default::default()
                        },
                        time_secs: 16.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::UnlocksTraining(vec![EntityKind::Knight]),
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 170,
                            copper: 70,
                            iron: 90,
                            gold: 35,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::SiegeWorks,
        Blueprint {
            kind: EntityKind::SiegeWorks,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 350.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost {
                wood: 100,
                copper: 35,
                iron: 90,
                gold: 30,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 20.0,
                half_height: 1.5,
                trains: vec![EntityKind::Catapult, EntityKind::BatteringRam],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 140,
                            copper: 50,
                            iron: 110,
                            gold: 45,
                            ..Default::default()
                        },
                        time_secs: 20.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.8),
                    },
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 220,
                            copper: 80,
                            iron: 150,
                            gold: 75,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    // ── Resource Processing Buildings ──

    blueprints.insert(
        EntityKind::Sawmill,
        Blueprint {
            kind: EntityKind::Sawmill,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost {
                wood: 50,
                iron: 15,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 12.0,
                half_height: 1.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 70,
                            iron: 25,
                            ..Default::default()
                        },
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
                        cost: ResourceCost {
                            wood: 110,
                            copper: 15,
                            iron: 35,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Mine,
        Blueprint {
            kind: EntityKind::Mine,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost {
                wood: 70,
                iron: 35,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 1.2,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 80,
                            iron: 50,
                            ..Default::default()
                        },
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
                        cost: ResourceCost {
                            wood: 110,
                            copper: 40,
                            iron: 75,
                            gold: 25,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::OilRig,
        Blueprint {
            kind: EntityKind::OilRig,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 150.0,
                damage: 0.0,
                attack_range: 0.0,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
            }),
            movement: None,
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost {
                wood: 75,
                copper: 25,
                iron: 35,
                ..Default::default()
            },
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 1.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost {
                            wood: 95,
                            copper: 35,
                            iron: 45,
                            ..Default::default()
                        },
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
                        cost: ResourceCost {
                            wood: 135,
                            copper: 55,
                            iron: 65,
                            gold: 20,
                            ..Default::default()
                        },
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    // ── Mobs ──

    blueprints.insert(
        EntityKind::Goblin,
        Blueprint {
            kind: EntityKind::Goblin,
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 50.0,
                damage: 5.0,
                attack_range: 1.5,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(15.0),
                is_ranged: false,
                projectile_speed: None,
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Skeleton,
        Blueprint {
            kind: EntityKind::Skeleton,
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 80.0,
                damage: 10.0,
                attack_range: 1.8,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(18.0),
                is_ranged: false,
                projectile_speed: None,
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Orc,
        Blueprint {
            kind: EntityKind::Orc,
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 120.0,
                damage: 15.0,
                attack_range: 2.0,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(20.0),
                is_ranged: false,
                projectile_speed: None,
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::Demon,
        Blueprint {
            kind: EntityKind::Demon,
            faction: Faction::Neutral,
            combat: Some(CombatStats {
                hp: 200.0,
                damage: 25.0,
                attack_range: 2.2,
                attack_cooldown_secs: 1.2,
                aggro_range: Some(25.0),
                is_ranged: false,
                projectile_speed: None,
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    // ── Summons ──

    blueprints.insert(
        EntityKind::SkeletonMinion,
        Blueprint {
            kind: EntityKind::SkeletonMinion,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 40.0,
                damage: 6.0,
                attack_range: 1.5,
                attack_cooldown_secs: 1.0,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::SpiritWolf,
        Blueprint {
            kind: EntityKind::SpiritWolf,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 60.0,
                damage: 8.0,
                attack_range: 1.8,
                attack_cooldown_secs: 0.8,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
        },
    );

    blueprints.insert(
        EntityKind::FireElemental,
        Blueprint {
            kind: EntityKind::FireElemental,
            faction: Faction::Player1,
            combat: Some(CombatStats {
                hp: 80.0,
                damage: 12.0,
                attack_range: 3.0,
                attack_cooldown_secs: 1.5,
                aggro_range: None,
                is_ranged: false,
                projectile_speed: None,
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
            children: vec![],
            abilities: vec![],
            upgrades: vec![],
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
            entity_cmds.insert((
                Unit,
                UnitState::default(),
                TaskSource::default(),
                TaskQueue::default(),
                UnitStance::default(),
                AutoRole::default(),
            ));
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
                        caps: [500, 80, 120, 0, 0],
                        ..default()
                    },
                ));
            } else if kind == EntityKind::Storage {
                entity_cmds.insert((
                    DepositPoint,
                    StorageInventory {
                        caps: [300, 300, 300, 300, 200],
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
                            caps: [3000, 0, 0, 0, 0],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ResourceProcessor {
                            resource_types: vec![ResourceType::Wood],
                            harvest_radius: 15.0,
                            harvest_rate: 3.0,
                            max_workers: 3,
                            buffer: 0,
                            buffer_capacity: 50,
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
                    ));
                }
                EntityKind::Mine => {
                    entity_cmds.insert((
                        DepositPoint,
                        StorageInventory {
                            caps: [0, 1000, 1000, 0, 0],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ResourceProcessor {
                            resource_types: vec![ResourceType::Iron],
                            harvest_radius: 12.0,
                            harvest_rate: 2.0,
                            max_workers: 4,
                            buffer: 0,
                            buffer_capacity: 40,
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
                            caps: [0, 0, 0, 0, 500],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ResourceProcessor {
                            resource_types: vec![ResourceType::Oil],
                            harvest_radius: 12.0,
                            harvest_rate: 1.5,
                            max_workers: 0, // Fully automated
                            buffer: 0,
                            buffer_capacity: 30,
                            worker_rate_bonus: 0.0,
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
                _ => {}
            }
        }
    }

    // Combat stats
    if let Some(ref combat) = bp.combat {
        entity_cmds.insert((
            Health {
                current: combat.hp,
                max: combat.hp,
            },
            AttackDamage(combat.damage),
            AttackRange(combat.attack_range),
            AttackCooldown {
                timer: Timer::from_seconds(combat.attack_cooldown_secs, TimerMode::Repeating),
            },
        ));
        if let Some(aggro) = combat.aggro_range {
            entity_cmds.insert(AggroRange(aggro));
        }
        if combat.is_ranged {
            entity_cmds.insert(IsRanged);
        }
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
        });
    }

    // Abilities
    if !bp.abilities.is_empty() {
        entity_cmds.insert(Abilities {
            slots: bp
                .abilities
                .iter()
                .map(AbilityInstance::from_slot)
                .collect(),
        });
    }

    let entity_id = entity_cmds.id();

    // Spawn GLTF scene child for buildings with GltfScene mesh kind
    if !is_gltf_character && bp.visual.mesh_kind.is_gltf() {
        if let Some(models) = building_models {
            if let Some(scene_handle) = models.scenes.get(&(kind, 1)) {
                let cal = models.calibration.get(&kind);
                let scale = cal.map(|c| c.scale).unwrap_or(1.0);
                let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                let child = commands
                    .spawn((
                        SceneRoot(scene_handle.clone()),
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
            MeshKind::Cylinder { radius, height } => meshes.add(Cylinder::new(radius, height)),
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
