use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline, OutlineStencil, OutlineVolume};
use rand::Rng;

use crate::blueprints::types::*;
use crate::blueprints::EntityVisualCache;
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};

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
    let ground_y = if kind.category() == EntityCategory::Building
        && crate::buildings::uses_terrain_foundation(kind)
    {
        let footprint = crate::buildings::footprint_for_kind(kind);
        height_map.foundation_target_height_shaped(pos.x, pos.z, footprint)
    } else {
        height_map.sample(pos.x, pos.z)
    };
    let y = ground_y + y_off + building_y;

    let pick_radius = bp.visual.mesh_kind.pick_radius() * bp.visual.scale;

    let culling_bounds = CullingBounds::new(pick_radius.max(2.0));

    let mut entity_cmds = if is_gltf {
        // GLTF buildings/characters: no Mesh3d/MeshMaterial3d on parent
        commands.spawn((
            GameWorld,
            kind,
            faction,
            PickRadius(pick_radius),
            culling_bounds,
            CullReason::default(),
            Transform::from_translation(Vec3::new(pos.x, y, pos.z))
                .with_scale(Vec3::splat(bp.visual.scale)),
            Visibility::default(),
        ))
    } else {
        commands.spawn((
            GameWorld,
            kind,
            faction,
            PickRadius(pick_radius),
            culling_bounds,
            CullReason::default(),
            Mesh3d(mesh_handle),
            MeshMaterial3d(mat_handle),
            Transform::from_translation(Vec3::new(pos.x, y, pos.z))
                .with_scale(Vec3::splat(bp.visual.scale)),
        ))
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        entity_cmds.insert((
            OutlineVolume {
                visible: false,
                colour: Color::NONE,
                width: 3.0,
            },
            OutlineStencil::default(),
        ));
    }

    // Category markers
    match kind.category() {
        EntityCategory::Unit | EntityCategory::Siege | EntityCategory::Summon => {
            let stance = match kind {
                EntityKind::Worker | EntityKind::Priest => UnitStance::Defensive,
                _ => UnitStance::Aggressive,
            };
            let tactical_role = match kind {
                EntityKind::Archer | EntityKind::Mage => TacticalRole::RangedKiter,
                EntityKind::Tank | EntityKind::Knight => TacticalRole::Frontline,
                EntityKind::Priest => TacticalRole::Healer,
                EntityKind::Cavalry => TacticalRole::Flanker,
                EntityKind::Catapult => TacticalRole::SiegeSupport,
                _ => TacticalRole::Standard,
            };
            let mut rng = rand::rng();
            entity_cmds.insert((
                Unit,
                UnitDisplayName(random_unit_display_name(kind, &mut rng)),
                UnitState::default(),
                TaskSource::default(),
                TaskQueue::default(),
                stance,
                tactical_role,
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
            if bp.visual.mesh_kind.is_procedural_mob() {
                let visual_kind = match kind {
                    EntityKind::Goblin => MobVisualKind::Goblin,
                    EntityKind::Skeleton => MobVisualKind::Skeleton,
                    EntityKind::Orc => MobVisualKind::Orc,
                    EntityKind::Demon => MobVisualKind::Demon,
                    _ => MobVisualKind::Goblin,
                };
                entity_cmds.insert(ProceduralMob {
                    visual_kind,
                    phase: 0.0,
                    base_y_offset: bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.3),
                    base_scale: Vec3::splat(bp.visual.scale),
                    base_translation: Vec3::ZERO,
                    attack_timer: None,
                    initialized: false,
                    pulse_ring_spawned: false,
                    dying_progress: 0.0,
                });
            }
        }
        EntityCategory::Building => {
            let footprint = crate::buildings::footprint_for_kind(kind);
            let bld_height = crate::buildings::building_height_for_kind(kind);
            entity_cmds.insert((
                Building,
                BuildingLevel(1),
                BuildingFootprint(footprint),
                BuildingHeight(bld_height),
            ));
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
                        caps: [500, 80, 120, 0, 0, 0, 0, 0, 0, 0, 0],
                        ..default()
                    },
                ));
            } else if kind == EntityKind::Storage {
                entity_cmds.insert((
                    DepositPoint,
                    StorageInventory {
                        caps: [300, 300, 300, 300, 200, 300, 100, 50, 100, 100, 50],
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
                            caps: [3000, 0, 0, 0, 0, 0, 500, 200, 0, 0, 0],
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
                        {
                            let mut prod = ProductionState::new(vec![
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
                            ]);
                            prod.active_recipe = None; // Planks off by default
                            prod
                        },
                    ));
                }
                EntityKind::Mine => {
                    entity_cmds.insert((
                        DepositPoint,
                        StorageInventory {
                            caps: [0, 1000, 1000, 0, 0, 0, 0, 0, 0, 0, 0],
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
                            caps: [0, 0, 0, 0, 500, 0, 0, 0, 0, 0, 0],
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
                            caps: [0, 200, 200, 0, 0, 0, 0, 0, 200, 200, 0],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ProductionState::new(vec![
                            ProductionRecipe {
                                name: "Bronze",
                                inputs: vec![(ResourceType::Copper, 2), (ResourceType::Iron, 1)],
                                outputs: vec![(ResourceType::Bronze, 1)],
                                cycle_secs: 8.0,
                                requires_level: 1,
                            },
                            ProductionRecipe {
                                name: "Steel",
                                inputs: vec![(ResourceType::Iron, 3), (ResourceType::Charcoal, 1)],
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
                            caps: [0, 0, 0, 0, 100, 0, 0, 100, 0, 0, 200],
                            ..default()
                        },
                        AssignedWorkers::default(),
                        ProductionState::new(vec![ProductionRecipe {
                            name: "Gunpowder",
                            inputs: vec![(ResourceType::Charcoal, 1), (ResourceType::Oil, 1)],
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
        let attack_timing = default_attack_timing(kind, combat);
        let targeting_profile = default_targeting_profile(kind);
        let threat_value = default_threat_value(kind);
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
            attack_timing,
            targeting_profile,
            threat_value,
            ReservedIncomingDamage::default(),
        ));
        if let Some(aggro) = combat.aggro_range {
            entity_cmds.insert(AggroRange(aggro));
        }
        if combat.is_ranged {
            entity_cmds.insert(IsRanged);
        }
    } else {
        // Buildings without combat stats still need armor type + threat value for targeting
        entity_cmds.insert((kind.armor_type(), default_threat_value(kind)));
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
                let mut child = commands.spawn((
                    SceneRoot(scene_handle),
                    BuildingSceneChild,
                    models.child_transform(kind, 1.0),
                ));
                #[cfg(not(target_arch = "wasm32"))]
                child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                let child = child.id();
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
                _pulse_speed: 3.0,
                particle_timer: Timer::from_seconds(0.15, TimerMode::Repeating),
                light_entity: None,
            });
        }
        EntityKind::FireElemental => {
            commands.entity(entity_id).insert(SummonVfx {
                color: Color::srgba(1.0, 0.4, 0.1, 0.7),
                emissive: LinearRgba::new(1.5, 0.6, 0.1, 1.0),
                _pulse_speed: 5.0,
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
                let mut child = commands.spawn((
                    SceneRoot(scene_handle.clone()),
                    UnitSceneChild,
                    Transform::from_scale(Vec3::splat(scale))
                        .with_translation(Vec3::new(0.0, y_off, 0.0))
                        .with_rotation(Quat::from_rotation_y(facing)),
                ));
                #[cfg(not(target_arch = "wasm32"))]
                child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                let child = child.id();
                commands.entity(entity_id).add_child(child);
            }
        }
    }

    entity_id
}

// ── Build visual cache from registry ──
