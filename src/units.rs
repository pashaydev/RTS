use bevy::prelude::*;
use std::collections::HashSet;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache, spawn_from_blueprint_with_faction};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::UnitModelAssets;
use std::f32::consts::PI;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivePlayer>()
            .init_resource::<AllPlayerResources>()
            .init_resource::<AllCompletedBuildings>()
            .init_resource::<FactionBaseState>()
            .init_resource::<TeamConfig>()
            .add_systems(OnEnter(AppState::InGame), apply_game_config)
            .add_systems(OnEnter(AppState::InGame), spawn_all_players.after(crate::ground::spawn_ground))
            .add_systems(
                Update,
                (steer_avoidance, move_units).chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Applies GameSetupConfig to TeamConfig, AiControlledFactions, etc.
fn apply_game_config(
    config: Res<GameSetupConfig>,
    mut teams: ResMut<TeamConfig>,
    mut ai_controlled: ResMut<AiControlledFactions>,
) {
    let all_factions = [Faction::Player1, Faction::Player2, Faction::Player3, Faction::Player4];
    let count = (1 + config.num_ai_opponents as usize).min(4);
    let factions = &all_factions[..count];

    // Setup AI controlled factions (all except Player1)
    let mut ai_facs = HashSet::new();
    for &f in &factions[1..] {
        ai_facs.insert(f);
    }
    ai_controlled.factions = ai_facs;

    // Setup teams
    let mut team_map = std::collections::HashMap::new();
    match config.team_mode {
        TeamMode::FFA => {
            for (i, &faction) in factions.iter().enumerate() {
                team_map.insert(faction, i as u8);
            }
        }
        TeamMode::Teams => {
            // 2v2: first half team 0, second half team 1
            for (i, &faction) in factions.iter().enumerate() {
                team_map.insert(faction, if i < count / 2 { 0 } else { 1 });
            }
        }
        TeamMode::Custom => {
            // Use player_teams array directly
            for (i, &faction) in factions.iter().enumerate() {
                team_map.insert(faction, config.player_teams[i]);
            }
        }
    }
    teams.teams = team_map.clone();
    info!("apply_game_config: mode={:?}, count={}, teams={:?}", config.team_mode, count, team_map);
}

pub fn y_offset_for(kind: EntityKind, registry: &BlueprintRegistry) -> f32 {
    let bp = registry.get(kind);
    bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8)
}

fn spawn_all_players(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    unit_models: Option<Res<UnitModelAssets>>,
    mut base_state: ResMut<FactionBaseState>,
    mut all_resources: ResMut<AllPlayerResources>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    let mut positions = config.spawn_positions(map_seed.0);

    // Biome validation: nudge spawn positions away from Water/Mountain
    let half_map = config.map_size.world_size() / 2.0;
    let radius = 0.6 * half_map;
    let count = positions.len();
    let rotation_offset = (map_seed.0 % 360) as f32 * PI / 180.0;

    for (i, (_faction, (ref mut x, ref mut z))) in positions.iter_mut().enumerate() {
        let base_angle = 2.0 * PI * i as f32 / count as f32 + rotation_offset;
        let biome = biome_map.get_biome(*x, *z);
        if biome == Biome::Water || biome == Biome::Mountain {
            // Nudge angle by ±5° increments until valid
            for nudge in 1..=36 {
                for sign in &[1.0_f32, -1.0] {
                    let angle = base_angle + sign * nudge as f32 * 5.0 * PI / 180.0;
                    let nx = angle.cos() * radius;
                    let nz = angle.sin() * radius;
                    let nb = biome_map.get_biome(nx, nz);
                    if nb != Biome::Water && nb != Biome::Mountain {
                        *x = nx;
                        *z = nz;
                        break;
                    }
                }
                let b = biome_map.get_biome(*x, *z);
                if b != Biome::Water && b != Biome::Mountain {
                    break;
                }
            }
        }
    }

    for &(faction, (sx, sz)) in &positions {
        let spawn_pos = Vec3::new(sx, 0.0, sz);
        base_state.set_founded(faction, false);

        // Initialize resources for this faction with starting multiplier
        let mut res = PlayerResources::empty();
        res.add(ResourceType::Wood, (200.0 * config.starting_resources_mult) as u32);
        res.add(ResourceType::Copper, (40.0 * config.starting_resources_mult) as u32);
        res.add(ResourceType::Iron, (20.0 * config.starting_resources_mult) as u32);
        all_resources.resources.insert(faction, res);

        // Spawn 2 workers near the starting settlement area.
        let worker_offsets = [
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(-3.0, 0.0, 2.0),
        ];
        for offset in worker_offsets {
            spawn_from_blueprint_with_faction(
                &mut commands, &cache, EntityKind::Worker, spawn_pos + offset,
                &registry, None, unit_models.as_deref(), &height_map, faction,
            );
        }
    }
}

fn steer_avoidance(
    time: Res<Time>,
    mut units: Query<(Entity, &mut Transform), With<Unit>>,
) {
    let avoidance_radius = 1.8;
    let strength = 4.0;

    let positions: Vec<(Entity, Vec3)> = units
        .iter()
        .map(|(e, t)| (e, t.translation))
        .collect();

    for (entity, mut transform) in &mut units {
        let my_pos = transform.translation;
        let mut separation = Vec3::ZERO;

        for (other_e, other_pos) in &positions {
            if *other_e == entity {
                continue;
            }
            let diff = my_pos - *other_pos;
            let flat_diff = Vec3::new(diff.x, 0.0, diff.z);
            let dist = flat_diff.length();
            if dist > 0.01 && dist < avoidance_radius {
                separation += flat_diff.normalize() * (avoidance_radius - dist) / avoidance_radius;
            }
        }

        if separation.length_squared() > 0.0 {
            transform.translation += separation * strength * time.delta_secs();
        }
    }
}

fn move_units(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    height_map: Res<HeightMap>,
    mut query: Query<(Entity, &mut Transform, &MoveTarget, &UnitSpeed, &EntityKind, Option<&Carrying>, Option<&CarryCapacity>), With<Unit>>,
) {
    for (entity, mut transform, target, unit_speed, kind, carrying, capacity) in &mut query {
        let direction = target.0 - transform.translation;
        let flat_dir = Vec3::new(direction.x, 0.0, direction.z);
        let distance = flat_dir.length();

        if distance < 0.2 {
            commands.entity(entity).remove::<MoveTarget>();
        } else {
            // Encumbrance: slow down when carrying heavy loads
            let speed_mult = if let (Some(carry), Some(cap)) = (carrying, capacity) {
                if cap.0 > 0.0 && carry.weight > 0.0 {
                    let load_fraction = (carry.weight / cap.0).min(1.0);
                    1.0 - load_fraction * 0.4 // 40% slower at full load
                } else {
                    1.0
                }
            } else {
                1.0
            };

            let step = flat_dir.normalize() * unit_speed.0 * speed_mult * time.delta_secs();
            transform.translation += step;
        }
        // Snap Y to terrain
        transform.translation.y = height_map.sample(transform.translation.x, transform.translation.z) + y_offset_for(*kind, &registry);
    }
}
