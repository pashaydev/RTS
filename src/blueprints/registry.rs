//! Hardcoded blueprint definitions for every unit, building, mob, and siege
//! piece — stats, costs, visuals, and behavioral profiles.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::types::*;
use crate::types::*;

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
                y_offset: 1.6,
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
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.0 },
                color: Color::srgb(0.9, 0.8, 0.2),
                selected_color: Color::srgb(1.0, 1.0, 0.4),
                selected_emissive: LinearRgba::new(0.3, 0.3, 0.0, 1.0),
                scale: 2.0,
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
                y_offset: 1.8,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 12.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 20)
                .with(ResourceType::Iron, 15),
            train_time_secs: 8.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.0 },
                color: Color::srgb(0.8, 0.15, 0.15),
                selected_color: Color::srgb(1.0, 0.3, 0.3),
                selected_emissive: LinearRgba::new(0.3, 0.05, 0.05, 1.0),
                scale: 2.0,
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
                y_offset: 1.5,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 18.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 25)
                .with(ResourceType::Iron, 10),
            train_time_secs: 7.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.0 },
                color: Color::srgb(0.15, 0.7, 0.2),
                selected_color: Color::srgb(0.3, 1.0, 0.4),
                selected_emissive: LinearRgba::new(0.05, 0.3, 0.05, 1.0),
                scale: 2.0,
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
                y_offset: 2.5,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Copper, 20)
                .with(ResourceType::Iron, 50)
                .with(ResourceType::Gold, 15)
                .with(ResourceType::Oil, 5)
                .with(ResourceType::Steel, 5),
            train_time_secs: 15.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.6 },
                color: Color::srgb(0.35, 0.35, 0.4),
                selected_color: Color::srgb(0.6, 0.6, 0.65),
                selected_emissive: LinearRgba::new(0.1, 0.1, 0.12, 1.0),
                scale: 2.0,
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
                y_offset: 2.4,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 14.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 20)
                .with(ResourceType::Copper, 15)
                .with(ResourceType::Iron, 45)
                .with(ResourceType::Gold, 20)
                .with(ResourceType::Bronze, 5),
            train_time_secs: 12.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.6 },
                color: Color::srgb(0.7, 0.7, 0.75),
                selected_color: Color::srgb(0.9, 0.9, 0.95),
                selected_emissive: LinearRgba::new(0.2, 0.2, 0.25, 1.0),
                scale: 2.0,
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
                y_offset: 1.6,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 20.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 15)
                .with(ResourceType::Gold, 50),
            train_time_secs: 15.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.0 },
                color: Color::srgb(0.3, 0.2, 0.7),
                selected_color: Color::srgb(0.5, 0.4, 1.0),
                selected_emissive: LinearRgba::new(0.1, 0.05, 0.3, 1.0),
                scale: 2.0,
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
                y_offset: 1.6,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 16.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 15)
                .with(ResourceType::Gold, 30),
            train_time_secs: 12.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.0 },
                color: Color::srgb(0.9, 0.85, 0.6),
                selected_color: Color::srgb(1.0, 0.95, 0.7),
                selected_emissive: LinearRgba::new(0.3, 0.28, 0.1, 1.0),
                scale: 2.0,
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
                y_offset: 2.2,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 14.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 25)
                .with(ResourceType::Copper, 10)
                .with(ResourceType::Iron, 25)
                .with(ResourceType::Gold, 10),
            train_time_secs: 10.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.0 },
                color: Color::srgb(0.55, 0.4, 0.25),
                selected_color: Color::srgb(0.75, 0.6, 0.4),
                selected_emissive: LinearRgba::new(0.15, 0.1, 0.05, 1.0),
                scale: 2.0,
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
                y_offset: 1.4,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 25.0 }),
            cost: ResourceCost::new().with(ResourceType::Wood, 15),
            train_time_secs: 4.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 2.0 },
                color: Color::srgb(0.3, 0.6, 0.3),
                selected_color: Color::srgb(0.5, 0.8, 0.5),
                selected_emissive: LinearRgba::new(0.05, 0.15, 0.05, 1.0),
                scale: 1.6,
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
                y_offset: 2.0,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 28.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 80)
                .with(ResourceType::Iron, 60)
                .with(ResourceType::Gold, 20)
                .with(ResourceType::Gunpowder, 5),
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
                scale: 2.0,
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
                y_offset: 1.6,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 100)
                .with(ResourceType::Iron, 40)
                .with(ResourceType::Planks, 15),
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
                scale: 2.0,
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 90)
                .with(ResourceType::Iron, 15)
                .with(ResourceType::Stone, 20),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 1.5,
                trains: vec![EntityKind::Worker],
                prerequisite: None,
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 130)
                            .with(ResourceType::Iron, 30),
                        time_secs: 20.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::VisionBoost(5.0),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 210)
                            .with(ResourceType::Copper, 30)
                            .with(ResourceType::Iron, 80),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 75)
                .with(ResourceType::Iron, 30),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 12.0,
                half_height: 1.25,
                trains: vec![EntityKind::Worker, EntityKind::Soldier, EntityKind::Scout],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 110)
                            .with(ResourceType::Iron, 40),
                        time_secs: 15.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::UnlocksTraining(vec![EntityKind::Archer]),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 170)
                            .with(ResourceType::Copper, 40)
                            .with(ResourceType::Iron, 90),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 90)
                .with(ResourceType::Copper, 25)
                .with(ResourceType::Iron, 55)
                .with(ResourceType::Gold, 15)
                .with(ResourceType::Bronze, 10),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 18.0,
                half_height: 1.5,
                trains: vec![EntityKind::Tank],
                prerequisite: Some(EntityKind::Mine),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 120)
                            .with(ResourceType::Copper, 40)
                            .with(ResourceType::Iron, 80)
                            .with(ResourceType::Gold, 20),
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.75),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 180)
                            .with(ResourceType::Copper, 70)
                            .with(ResourceType::Iron, 120)
                            .with(ResourceType::Gold, 40),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 45)
                .with(ResourceType::Copper, 10)
                .with(ResourceType::Iron, 35),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 10.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 70)
                            .with(ResourceType::Copper, 20)
                            .with(ResourceType::Iron, 50),
                        time_secs: 12.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 5.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 110)
                            .with(ResourceType::Copper, 40)
                            .with(ResourceType::Iron, 70)
                            .with(ResourceType::Gold, 20),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 35)
                .with(ResourceType::Iron, 15)
                .with(ResourceType::Stone, 15),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 8.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 55)
                            .with(ResourceType::Iron, 25),
                        time_secs: 10.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 2.0,
                            damage_boost: 3.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 85)
                            .with(ResourceType::Copper, 15)
                            .with(ResourceType::Iron, 35),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 60)
                .with(ResourceType::Copper, 20)
                .with(ResourceType::Iron, 45)
                .with(ResourceType::Stone, 25),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 11.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 85)
                            .with(ResourceType::Copper, 30)
                            .with(ResourceType::Iron, 60),
                        time_secs: 12.0,
                        scale_multiplier: 1.1,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 5.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 130)
                            .with(ResourceType::Copper, 55)
                            .with(ResourceType::Iron, 85)
                            .with(ResourceType::Gold, 20),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 70)
                .with(ResourceType::Copper, 55)
                .with(ResourceType::Iron, 80)
                .with(ResourceType::Steel, 10)
                .with(ResourceType::Stone, 30),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::SiegeWorks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 95)
                            .with(ResourceType::Copper, 70)
                            .with(ResourceType::Iron, 100),
                        time_secs: 16.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 3.0,
                            damage_boost: 7.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 140)
                            .with(ResourceType::Copper, 95)
                            .with(ResourceType::Iron, 130)
                            .with(ResourceType::Gold, 30),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 85)
                .with(ResourceType::Copper, 45)
                .with(ResourceType::Iron, 65)
                .with(ResourceType::Gold, 35)
                .with(ResourceType::Gunpowder, 5)
                .with(ResourceType::Stone, 25),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 3.0,
                trains: vec![],
                prerequisite: Some(EntityKind::MageTower),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 105)
                            .with(ResourceType::Copper, 60)
                            .with(ResourceType::Iron, 85)
                            .with(ResourceType::Gold, 45),
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::RangeAndDamage {
                            range_boost: 2.0,
                            damage_boost: 6.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 150)
                            .with(ResourceType::Copper, 85)
                            .with(ResourceType::Iron, 110)
                            .with(ResourceType::Gold, 65),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 20)
                .with(ResourceType::Iron, 10)
                .with(ResourceType::Stone, 8),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 6.0,
                half_height: 2.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 35)
                            .with(ResourceType::Iron, 20),
                        time_secs: 8.0,
                        scale_multiplier: 1.05,
                        bonus: LevelBonus::VisionBoost(6.0),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 55)
                            .with(ResourceType::Copper, 10)
                            .with(ResourceType::Iron, 30),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 40)
                .with(ResourceType::Copper, 10)
                .with(ResourceType::Iron, 35)
                .with(ResourceType::Stone, 20),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 12)
                .with(ResourceType::Stone, 8),
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
                mesh_kind: MeshKind::GltfScene { pick_radius: 2.5 },
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 16)
                .with(ResourceType::Stone, 10),
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
                mesh_kind: MeshKind::GltfScene { pick_radius: 2.0 },
                color: Color::srgb(0.40, 0.23, 0.10),
                selected_color: Color::srgb(0.58, 0.34, 0.16),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },
        },
    );

    blueprints.insert(
        EntityKind::WallCorner,
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
            vision: Some(VisionStats { range: 8.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 10)
                .with(ResourceType::Stone, 8),
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
                mesh_kind: MeshKind::GltfScene { pick_radius: 2.5 },
                color: Color::srgb(0.42, 0.25, 0.11),
                selected_color: Color::srgb(0.58, 0.36, 0.17),
                selected_emissive: LinearRgba::NONE,
                scale: 1.0,
            },
        },
    );

    blueprints.insert(
        EntityKind::Floor,
        Blueprint {
            faction: Faction::Player1,
            combat: None,
            movement: None,
            gathering: None,
            vision: None,
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 4)
                .with(ResourceType::Stone, 2),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 0.0,
                half_height: 0.04,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![],
            }),
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::Cuboid {
                    x: 3.0,
                    y: 0.10,
                    z: 3.0,
                },
                color: Color::srgb(0.56, 0.47, 0.35),
                selected_color: Color::srgb(0.72, 0.62, 0.46),
                selected_emissive: LinearRgba::new(0.08, 0.06, 0.03, 1.0),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 55)
                .with(ResourceType::Iron, 15)
                .with(ResourceType::Stone, 10),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 8.0,
                half_height: 0.15,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 75)
                            .with(ResourceType::Iron, 25),
                        time_secs: 10.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::GatherAura {
                            speed_bonus: 0.15,
                            range: 20.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 120)
                            .with(ResourceType::Copper, 20)
                            .with(ResourceType::Iron, 45),
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
                .with(ResourceType::Iron, 10)
                .with(ResourceType::Stone, 10),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 80)
                .with(ResourceType::Copper, 30)
                .with(ResourceType::Iron, 40)
                .with(ResourceType::Gold, 55),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 20.0,
                half_height: 2.5,
                trains: vec![EntityKind::Mage, EntityKind::Priest],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 100)
                            .with(ResourceType::Copper, 40)
                            .with(ResourceType::Iron, 55)
                            .with(ResourceType::Gold, 80),
                        time_secs: 20.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.85),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 140)
                            .with(ResourceType::Copper, 60)
                            .with(ResourceType::Iron, 80)
                            .with(ResourceType::Gold, 130),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 90)
                .with(ResourceType::Copper, 20)
                .with(ResourceType::Iron, 40)
                .with(ResourceType::Gold, 70),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 22.0,
                half_height: 2.0,
                trains: vec![EntityKind::Priest],
                prerequisite: Some(EntityKind::MageTower),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 115)
                            .with(ResourceType::Copper, 30)
                            .with(ResourceType::Iron, 55)
                            .with(ResourceType::Gold, 85),
                        time_secs: 18.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::HealAura {
                            heal_per_sec: 2.0,
                            range: 15.0,
                        },
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 170)
                            .with(ResourceType::Copper, 50)
                            .with(ResourceType::Iron, 75)
                            .with(ResourceType::Gold, 130),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 85)
                .with(ResourceType::Copper, 30)
                .with(ResourceType::Iron, 45),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 1.25,
                trains: vec![EntityKind::Cavalry],
                prerequisite: Some(EntityKind::Barracks),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 115)
                            .with(ResourceType::Copper, 45)
                            .with(ResourceType::Iron, 65),
                        time_secs: 16.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::UnlocksTraining(vec![EntityKind::Knight]),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 170)
                            .with(ResourceType::Copper, 70)
                            .with(ResourceType::Iron, 90)
                            .with(ResourceType::Gold, 35),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 100)
                .with(ResourceType::Copper, 35)
                .with(ResourceType::Iron, 90)
                .with(ResourceType::Gold, 30),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 20.0,
                half_height: 1.5,
                trains: vec![EntityKind::Catapult, EntityKind::BatteringRam],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 140)
                            .with(ResourceType::Copper, 50)
                            .with(ResourceType::Iron, 110)
                            .with(ResourceType::Gold, 45),
                        time_secs: 20.0,
                        scale_multiplier: 1.08,
                        bonus: LevelBonus::TrainTimeMultiplier(0.8),
                    },
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 220)
                            .with(ResourceType::Copper, 80)
                            .with(ResourceType::Iron, 150)
                            .with(ResourceType::Gold, 75),
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
            movement: Some(MovementStats {
                speed: 0.0,
                y_offset: 0.35,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 10.0 }),
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 50)
                .with(ResourceType::Iron, 15),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 12.0,
                half_height: 1.0,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 70)
                            .with(ResourceType::Iron, 25),
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
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 110)
                            .with(ResourceType::Copper, 15)
                            .with(ResourceType::Iron, 35),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 70)
                .with(ResourceType::Iron, 35),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 15.0,
                half_height: 1.2,
                trains: vec![],
                prerequisite: Some(EntityKind::Base),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 80)
                            .with(ResourceType::Iron, 50),
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
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 110)
                            .with(ResourceType::Copper, 40)
                            .with(ResourceType::Iron, 75)
                            .with(ResourceType::Gold, 25),
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
            cost: ResourceCost::new()
                .with(ResourceType::Wood, 75)
                .with(ResourceType::Copper, 25)
                .with(ResourceType::Iron, 35),
            train_time_secs: 0.0,
            building: Some(BuildingData {
                construction_time_secs: 14.0,
                half_height: 1.5,
                trains: vec![],
                prerequisite: Some(EntityKind::Workshop),
                level_upgrades: vec![
                    BuildingLevelData {
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 95)
                            .with(ResourceType::Copper, 35)
                            .with(ResourceType::Iron, 45),
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
                        cost: ResourceCost::new()
                            .with(ResourceType::Wood, 135)
                            .with(ResourceType::Copper, 55)
                            .with(ResourceType::Iron, 65)
                            .with(ResourceType::Gold, 20),
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
                y_offset: 0.8,
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
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 3.0 },
                color: Color::srgb(0.3, 0.6, 0.15),
                selected_color: Color::srgb(0.3, 0.6, 0.15),
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
                y_offset: 1.4,
            }),
            gathering: None,
            vision: Some(VisionStats { range: 8.0 }),
            cost: ResourceCost::default(),
            train_time_secs: 0.0,
            building: None,
            mob_ai: None,
            visual: VisualDef {
                mesh_kind: MeshKind::GltfCharacter { pick_radius: 2.6 },
                color: Color::srgb(0.75, 0.72, 0.65),
                selected_color: Color::srgb(0.85, 0.82, 0.75),
                selected_emissive: LinearRgba::new(0.1, 0.1, 0.08, 1.0),
                scale: 1.8,
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
                y_offset: 1.0,
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
                scale: 2.0,
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
                y_offset: 1.8,
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
                scale: 2.0,
            },
        },
    );

    BlueprintRegistry { blueprints }
}
