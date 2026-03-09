use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache, spawn_from_blueprint};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};

pub const SAVE_PATH: &str = "saves/save.json";

// ── Stable entity ID ────────────────────────────────────────────────────────

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct GameId(pub u64);

#[derive(Resource, Default)]
pub struct GameIdCounter(pub u64);

impl GameIdCounter {
    pub fn next(&mut self) -> GameId {
        self.0 += 1;
        GameId(self.0)
    }
}

// ── Save/Load request flags ─────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct SaveRequested(pub bool);

#[derive(Resource, Default)]
pub struct LoadRequested(pub bool);

#[derive(Resource, Default)]
pub struct SaveLoadStatus {
    pub message: String,
    pub timer: f32,
}

// ── Save file format ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    pub id_counter: u64,
    pub player_resources: SavedPlayerResources,
    pub fog_explored: Vec<bool>,
    pub entities: Vec<SavedEntity>,
    pub resource_nodes: Vec<SavedResourceNode>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedPlayerResources {
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
}

impl From<&PlayerResources> for SavedPlayerResources {
    fn from(r: &PlayerResources) -> Self {
        Self {
            wood: r.wood,
            copper: r.copper,
            iron: r.iron,
            gold: r.gold,
            oil: r.oil,
        }
    }
}

impl From<&SavedPlayerResources> for PlayerResources {
    fn from(r: &SavedPlayerResources) -> Self {
        Self {
            wood: r.wood,
            copper: r.copper,
            iron: r.iron,
            gold: r.gold,
            oil: r.oil,
        }
    }
}

// ── Saved entity data ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct SavedEntity {
    pub id: u64,
    pub kind: EntityKind,
    pub faction: Faction,
    pub position: [f32; 3],
    pub health_current: f32,
    pub health_max: f32,

    // Unit fields
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub move_target: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub worker_task: Option<SavedWorkerTask>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub carrying_amount: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub carrying_resource: Option<ResourceType>,

    // Building fields
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub building_state: Option<BuildingState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub building_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub construction_fraction: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub training_queue: Option<Vec<EntityKind>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub training_fraction: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub upgrade_fraction: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub upgrade_target_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rally_point: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tower_auto_attack: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub storage: Option<SavedStorage>,

    // Mob fields
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub patrol_center: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub patrol_radius: Option<f32>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct SavedWorkerTask {
    pub variant: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub target_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub depot_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gather_node_id: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedStorage {
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
}

#[derive(Serialize, Deserialize)]
pub struct SavedResourceNode {
    pub position: [f32; 3],
    pub resource_type: ResourceType,
    pub amount_remaining: u32,
}

// ── Pending load overrides (bridges frame 1 → frame 2) ─────────────────────

#[derive(Resource, Default)]
pub struct PendingLoadOverrides {
    pub entity_overrides: Vec<(u64, SavedEntity)>,
    pub gid_map: HashMap<u64, Entity>,
    pub resource_nodes: Vec<SavedResourceNode>,
}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameIdCounter>()
            .init_resource::<SaveRequested>()
            .init_resource::<LoadRequested>()
            .init_resource::<PendingLoadOverrides>()
            .init_resource::<SaveLoadStatus>()
            .add_systems(
                Update,
                (
                    assign_game_ids,
                    save_game,
                    load_game,
                    apply_load_overrides,
                    tick_status_timer,
                ),
            );
    }
}

// ── Auto-assign GameId to saveable entities ─────────────────────────────────

fn assign_game_ids(
    mut commands: Commands,
    mut counter: ResMut<GameIdCounter>,
    untagged: Query<
        Entity,
        (
            With<EntityKind>,
            Without<GameId>,
        ),
    >,
) {
    for entity in &untagged {
        commands.entity(entity).insert(counter.next());
    }
}

// ── Save game ───────────────────────────────────────────────────────────────

fn save_game(
    mut save_req: ResMut<SaveRequested>,
    mut status: ResMut<SaveLoadStatus>,
    id_counter: Res<GameIdCounter>,
    player_resources: Option<Res<PlayerResources>>,
    fog: Res<FogOfWarMap>,
    // Entity query — split into multiple queries to stay under tuple limit
    entity_q: Query<(
        &GameId,
        &EntityKind,
        &Faction,
        &Transform,
        Option<&Health>,
        Option<&MoveTarget>,
        Option<&WorkerTask>,
        Option<&Carrying>,
    ), Without<ResourceNode>>,
    building_q: Query<(
        &GameId,
        Option<&BuildingState>,
        Option<&BuildingLevel>,
        Option<&ConstructionProgress>,
        Option<&TrainingQueue>,
        Option<&UpgradeProgress>,
        Option<&RallyPoint>,
        Option<&TowerAutoAttackEnabled>,
        Option<&StorageInventory>,
    )>,
    patrol_q: Query<(&GameId, &PatrolState)>,
    // Entity→GameId lookup for cross-references
    id_lookup: Query<(Entity, &GameId)>,
    // Resource nodes (no EntityKind)
    resource_node_q: Query<(&Transform, &ResourceNode)>,
) {
    if !save_req.0 {
        return;
    }
    save_req.0 = false;

    let Some(player_resources) = player_resources else {
        warn!("Cannot save: PlayerResources resource does not exist");
        status.message = "Save failed: missing PlayerResources".to_string();
        status.timer = 5.0;
        return;
    };

    let entity_to_gid: HashMap<Entity, u64> =
        id_lookup.iter().map(|(e, gid)| (e, gid.0)).collect();

    let mut entities: Vec<SavedEntity> = Vec::new();

    for (game_id, kind, faction, transform, health, move_target, worker_task, carrying) in
        &entity_q
    {
        let mut saved = SavedEntity {
            id: game_id.0,
            kind: *kind,
            faction: *faction,
            position: [
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
            ],
            health_current: health.map(|h| h.current).unwrap_or(100.0),
            health_max: health.map(|h| h.max).unwrap_or(100.0),
            move_target: move_target.map(|mt| [mt.0.x, mt.0.y, mt.0.z]),
            worker_task: worker_task.map(|wt| serialize_worker_task(wt, &entity_to_gid)),
            carrying_amount: carrying.and_then(|c| {
                if c.amount > 0 {
                    Some(c.amount)
                } else {
                    None
                }
            }),
            carrying_resource: carrying.and_then(|c| c.resource_type),
            building_state: None,
            building_level: None,
            construction_fraction: None,
            training_queue: None,
            training_fraction: None,
            upgrade_fraction: None,
            upgrade_target_level: None,
            rally_point: None,
            tower_auto_attack: None,
            storage: None,
            patrol_center: None,
            patrol_radius: None,
        };

        // Building data
        if let Ok((
            _,
            b_state,
            b_level,
            b_constr,
            b_train,
            b_upgrade,
            b_rally,
            b_tower,
            b_storage,
        )) = building_q.get(id_lookup.iter().find(|(_, gid)| gid.0 == game_id.0).unwrap().0)
        {
            saved.building_state = b_state.copied();
            saved.building_level = b_level.map(|l| l.0);
            saved.construction_fraction = b_constr.map(|cp| {
                cp.timer.elapsed_secs() / cp.timer.duration().as_secs_f32()
            });
            saved.training_queue = b_train.and_then(|tq| {
                if tq.queue.is_empty() {
                    None
                } else {
                    Some(tq.queue.clone())
                }
            });
            saved.training_fraction = b_train
                .and_then(|tq| tq.timer.as_ref())
                .map(|t| t.elapsed_secs() / t.duration().as_secs_f32());
            saved.upgrade_fraction = b_upgrade.map(|up| {
                up.timer.elapsed_secs() / up.timer.duration().as_secs_f32()
            });
            saved.upgrade_target_level = b_upgrade.map(|up| up.target_level);
            saved.rally_point = b_rally.map(|rp| [rp.0.x, rp.0.y, rp.0.z]);
            saved.tower_auto_attack = b_tower.map(|ta| ta.0);
            saved.storage = b_storage.map(|si| SavedStorage {
                wood: si.wood,
                copper: si.copper,
                iron: si.iron,
                gold: si.gold,
                oil: si.oil,
            });
        }

        // Patrol data
        if let Ok((_, ps)) = patrol_q.get(
            id_lookup
                .iter()
                .find(|(_, gid)| gid.0 == game_id.0)
                .unwrap()
                .0,
        ) {
            saved.patrol_center = Some([ps.center.x, ps.center.y, ps.center.z]);
            saved.patrol_radius = Some(ps.radius);
        }

        entities.push(saved);
    }

    // Save resource nodes
    let resource_nodes: Vec<SavedResourceNode> = resource_node_q
        .iter()
        .map(|(transform, rn)| SavedResourceNode {
            position: [
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
            ],
            resource_type: rn.resource_type,
            amount_remaining: rn.amount_remaining,
        })
        .collect();

    let save_file = SaveFile {
        version: 1,
        id_counter: id_counter.0,
        player_resources: (&*player_resources).into(),
        fog_explored: fog.explored.clone(),
        entities,
        resource_nodes,
    };

    match serde_json::to_string_pretty(&save_file) {
        Ok(json) => {
            std::fs::create_dir_all("saves").ok();
            match std::fs::write(SAVE_PATH, &json) {
                Ok(_) => {
                    info!("Game saved to {} ({} entities, {} resource nodes)", SAVE_PATH, save_file.entities.len(), save_file.resource_nodes.len());
                    status.message = format!("Saved! ({} entities)", save_file.entities.len());
                    status.timer = 3.0;
                }
                Err(e) => {
                    warn!("Failed to write save file: {e}");
                    status.message = format!("Save failed: {e}");
                    status.timer = 5.0;
                }
            }
        }
        Err(e) => {
            warn!("Failed to serialize save: {e}");
            status.message = format!("Serialize failed: {e}");
            status.timer = 5.0;
        }
    }
}

fn serialize_worker_task(
    wt: &WorkerTask,
    lookup: &HashMap<Entity, u64>,
) -> SavedWorkerTask {
    match wt {
        WorkerTask::Idle | WorkerTask::ManualMove => SavedWorkerTask {
            variant: "Idle".into(),
            ..default()
        },
        WorkerTask::MovingToResource(e) => SavedWorkerTask {
            variant: "MovingToResource".into(),
            target_id: lookup.get(e).copied(),
            ..default()
        },
        WorkerTask::Gathering(e) => SavedWorkerTask {
            variant: "Gathering".into(),
            target_id: lookup.get(e).copied(),
            ..default()
        },
        WorkerTask::ReturningToDeposit { depot, gather_node } => SavedWorkerTask {
            variant: "ReturningToDeposit".into(),
            depot_id: lookup.get(depot).copied(),
            gather_node_id: gather_node.and_then(|gn| lookup.get(&gn).copied()),
            ..default()
        },
        WorkerTask::Depositing { depot, gather_node } => SavedWorkerTask {
            variant: "Depositing".into(),
            depot_id: lookup.get(depot).copied(),
            gather_node_id: gather_node.and_then(|gn| lookup.get(&gn).copied()),
            ..default()
        },
        WorkerTask::MovingToBuild(e) => SavedWorkerTask {
            variant: "MovingToBuild".into(),
            target_id: lookup.get(e).copied(),
            ..default()
        },
        WorkerTask::Building(e) => SavedWorkerTask {
            variant: "Building".into(),
            target_id: lookup.get(e).copied(),
            ..default()
        },
        WorkerTask::WaitingForStorage { depot, gather_node } => SavedWorkerTask {
            variant: "WaitingForStorage".into(),
            depot_id: lookup.get(depot).copied(),
            gather_node_id: gather_node.and_then(|gn| lookup.get(&gn).copied()),
            ..default()
        },
        WorkerTask::AssignedToBuilding(e) => SavedWorkerTask {
            variant: "AssignedToBuilding".into(),
            target_id: lookup.get(e).copied(),
            ..default()
        },
    }
}

// ── Load game ───────────────────────────────────────────────────────────────

fn load_game(
    mut load_req: ResMut<LoadRequested>,
    mut commands: Commands,
    mut player_resources: Option<ResMut<PlayerResources>>,
    mut fog: ResMut<FogOfWarMap>,
    mut id_counter: ResMut<GameIdCounter>,
    mut status: ResMut<SaveLoadStatus>,
    mut pending: ResMut<PendingLoadOverrides>,
    // Despawn all entities with EntityKind (units, buildings, mobs)
    existing_entities: Query<Entity, With<EntityKind>>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    height_map: Res<HeightMap>,
    building_models: Option<Res<BuildingModelAssets>>,
    unit_models: Option<Res<UnitModelAssets>>,
) {
    if !load_req.0 {
        return;
    }
    load_req.0 = false;

    let Some(mut player_resources) = player_resources else {
        warn!("Cannot load: PlayerResources resource does not exist");
        status.message = "Load failed: missing PlayerResources".to_string();
        status.timer = 5.0;
        return;
    };

    let json = match std::fs::read_to_string(SAVE_PATH) {
        Ok(s) => s,
        Err(e) => {
            warn!("Could not read save file: {e}");
            status.message = format!("Load failed: {e}");
            status.timer = 5.0;
            return;
        }
    };

    let save: SaveFile = match serde_json::from_str(&json) {
        Ok(s) => s,
        Err(e) => {
            warn!("Could not parse save file: {e}");
            status.message = format!("Parse failed: {e}");
            status.timer = 5.0;
            return;
        }
    };

    // 1. Despawn all existing game entities (units, buildings, mobs)
    let despawned = existing_entities.iter().count();
    for entity in &existing_entities {
        commands.entity(entity).despawn();
    }

    // 2. Restore global resources
    *player_resources = (&save.player_resources).into();
    id_counter.0 = save.id_counter;
    if save.fog_explored.len() == fog.explored.len() {
        fog.explored.copy_from_slice(&save.fog_explored);
    }

    // 3. Spawn all entities via blueprint
    let mut gid_map: HashMap<u64, Entity> = HashMap::new();
    let mut entity_overrides: Vec<(u64, SavedEntity)> = Vec::new();

    let entity_count = save.entities.len();
    for saved in save.entities {
        let pos = Vec3::new(saved.position[0], 0.0, saved.position[2]);
        let entity = spawn_from_blueprint(
            &mut commands,
            &cache,
            saved.kind,
            pos,
            &registry,
            building_models.as_deref(),
            unit_models.as_deref(),
            &height_map,
        );
        let game_id = GameId(saved.id);
        commands.entity(entity).insert(game_id);
        gid_map.insert(saved.id, entity);
        entity_overrides.push((saved.id, saved));
    }

    // Store overrides and resource nodes for next frame
    pending.entity_overrides = entity_overrides;
    pending.gid_map = gid_map;
    pending.resource_nodes = save.resource_nodes;

    info!(
        "Load: despawned {despawned}, spawning {entity_count} entities. Overrides pending."
    );
    status.message = format!("Loaded! ({entity_count} entities)");
    status.timer = 3.0;
}

// ── Apply overrides (runs frame after load) ─────────────────────────────────

fn apply_load_overrides(
    mut commands: Commands,
    mut pending: ResMut<PendingLoadOverrides>,
    mut resource_nodes: Query<(&Transform, &mut ResourceNode)>,
) {
    if pending.entity_overrides.is_empty() && pending.resource_nodes.is_empty() {
        return;
    }

    let overrides = std::mem::take(&mut pending.entity_overrides);
    let saved_rn = std::mem::take(&mut pending.resource_nodes);
    let map = &pending.gid_map;

    // Apply entity overrides
    for (gid, saved) in overrides {
        let Some(&entity) = map.get(&gid) else {
            continue;
        };

        // Health
        commands.entity(entity).insert(Health {
            current: saved.health_current,
            max: saved.health_max,
        });

        // Faction override (blueprint default may differ)
        commands.entity(entity).insert(saved.faction);

        // Move target
        if let Some(mt) = saved.move_target {
            commands
                .entity(entity)
                .insert(MoveTarget(Vec3::new(mt[0], mt[1], mt[2])));
        }

        // Worker task
        if let Some(ref wt) = saved.worker_task {
            commands.entity(entity).insert(restore_worker_task(wt, map));
        }

        // Carrying
        if let Some(amount) = saved.carrying_amount {
            commands.entity(entity).insert(Carrying {
                amount,
                weight: amount as f32,
                resource_type: saved.carrying_resource,
            });
        }

        // Building state
        if let Some(state) = saved.building_state {
            commands.entity(entity).insert(state);
        }

        // Building level
        if let Some(level) = saved.building_level {
            commands.entity(entity).insert(BuildingLevel(level));
        }

        // Construction progress
        if let Some(fraction) = saved.construction_fraction {
            if let Some(state) = saved.building_state {
                if state == BuildingState::UnderConstruction {
                    // Re-create the timer at the right fraction
                    let duration = 10.0; // default construction time
                    let mut timer = Timer::from_seconds(duration, TimerMode::Once);
                    timer.tick(std::time::Duration::from_secs_f32(duration * fraction));
                    commands
                        .entity(entity)
                        .insert(ConstructionProgress { timer });
                }
            }
        }

        // Training queue
        if let Some(ref queue) = saved.training_queue {
            let timer = saved.training_fraction.map(|frac| {
                let duration = 5.0; // default train time
                let mut t = Timer::from_seconds(duration, TimerMode::Once);
                t.tick(std::time::Duration::from_secs_f32(duration * frac));
                t
            });
            commands.entity(entity).insert(TrainingQueue {
                queue: queue.clone(),
                timer,
            });
        }

        // Upgrade progress
        if let (Some(fraction), Some(target_level)) =
            (saved.upgrade_fraction, saved.upgrade_target_level)
        {
            let duration = 15.0; // default upgrade time
            let mut timer = Timer::from_seconds(duration, TimerMode::Once);
            timer.tick(std::time::Duration::from_secs_f32(duration * fraction));
            commands.entity(entity).insert(UpgradeProgress {
                timer,
                target_level,
            });
        }

        // Rally point
        if let Some(rp) = saved.rally_point {
            commands
                .entity(entity)
                .insert(RallyPoint(Vec3::new(rp[0], rp[1], rp[2])));
        }

        // Tower auto attack
        if let Some(auto) = saved.tower_auto_attack {
            commands
                .entity(entity)
                .insert(TowerAutoAttackEnabled(auto));
        }

        // Storage inventory
        if let Some(ref st) = saved.storage {
            commands.entity(entity).insert(StorageInventory {
                wood: st.wood,
                copper: st.copper,
                iron: st.iron,
                gold: st.gold,
                oil: st.oil,
                ..default()
            });
        }

        // Patrol state
        if let (Some(center), Some(radius)) = (saved.patrol_center, saved.patrol_radius) {
            commands.entity(entity).insert(PatrolState {
                state: PatrolStateKind::Idle,
                center: Vec3::new(center[0], center[1], center[2]),
                radius,
                patrol_target: None,
            });
        }
    }

    // Apply resource node amount updates by position matching
    for saved_node in saved_rn {
        let saved_pos = Vec3::new(
            saved_node.position[0],
            saved_node.position[1],
            saved_node.position[2],
        );

        for (transform, mut rn) in resource_nodes.iter_mut() {
            if rn.resource_type == saved_node.resource_type {
                let dist = transform.translation.distance_squared(saved_pos);
                if dist < 1.0 {
                    rn.amount_remaining = saved_node.amount_remaining;
                    break;
                }
            }
        }
    }
}

fn restore_worker_task(saved: &SavedWorkerTask, map: &HashMap<u64, Entity>) -> WorkerTask {
    let resolve = |id: Option<u64>| id.and_then(|i| map.get(&i).copied());

    match saved.variant.as_str() {
        "MovingToResource" => resolve(saved.target_id)
            .map(WorkerTask::MovingToResource)
            .unwrap_or(WorkerTask::Idle),
        "Gathering" => resolve(saved.target_id)
            .map(WorkerTask::Gathering)
            .unwrap_or(WorkerTask::Idle),
        "ReturningToDeposit" => {
            if let Some(depot) = resolve(saved.depot_id) {
                WorkerTask::ReturningToDeposit {
                    depot,
                    gather_node: resolve(saved.gather_node_id),
                }
            } else {
                WorkerTask::Idle
            }
        }
        "Depositing" => {
            if let Some(depot) = resolve(saved.depot_id) {
                WorkerTask::Depositing {
                    depot,
                    gather_node: resolve(saved.gather_node_id),
                }
            } else {
                WorkerTask::Idle
            }
        }
        "MovingToBuild" => resolve(saved.target_id)
            .map(WorkerTask::MovingToBuild)
            .unwrap_or(WorkerTask::Idle),
        "Building" => resolve(saved.target_id)
            .map(WorkerTask::Building)
            .unwrap_or(WorkerTask::Idle),
        "WaitingForStorage" => {
            if let Some(depot) = resolve(saved.depot_id) {
                WorkerTask::WaitingForStorage {
                    depot,
                    gather_node: resolve(saved.gather_node_id),
                }
            } else {
                WorkerTask::Idle
            }
        }
        _ => WorkerTask::Idle,
    }
}

// ── Status timer ────────────────────────────────────────────────────────────

fn tick_status_timer(mut status: ResMut<SaveLoadStatus>, time: Res<Time>) {
    if status.timer > 0.0 {
        status.timer -= time.delta_secs();
        if status.timer <= 0.0 {
            status.message.clear();
        }
    }
}
