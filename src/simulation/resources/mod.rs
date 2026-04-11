mod processing;
mod spawning;
pub(crate) mod terrain;
mod trees;
mod workers;

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::types::*;

pub use processing::{assign_worker_to_processor, unassign_worker_from_processor};
pub use terrain::reveal_explored_grass;

#[derive(Message, Clone, Copy, Debug)]
pub struct CarryingDelta {
    pub faction: Faction,
    pub resource_type: ResourceType,
    pub delta: i32,
}

#[derive(Resource, Default)]
pub struct CarriedTotalsDirty(pub bool);

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeGrowthConfig>()
            .init_resource::<TreeLeafMaterials>()
            .init_resource::<CarriedResourceTotals>()
            .init_resource::<CarriedTotalsDirty>()
            .init_resource::<PendingCarriedDrains>()
            .init_resource::<GrassDebugSettings>()
            .init_resource::<GrassRebuildState>()
            .init_resource::<GrassRenderSettings>()
            .add_message::<CarryingDelta>()
            .add_systems(
                Startup,
                (create_resource_node_materials, create_carry_visual_assets),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    mark_carried_totals_dirty,
                    (spawning::spawn_resource_nodes, terrain::spawn_decorations)
                        .after(crate::world::ground::spawn_ground)
                        .run_if(not(resource_exists::<crate::infrastructure::save_load::PendingLoad>)),
                ),
            )
            .add_systems(
                FixedUpdate,
                workers::worker_ai_system,
            )
            .add_systems(
                FixedUpdate,
                (
                    processing::resource_processor_system,
                    processing::production_chain_system,
                    spawning::deplete_resource_nodes,
                )
                    .chain()
                    .in_set(GameFlowSet::Simulation)
                    .in_set(OverlayLifecycleSet::Manage)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (drain_carried_from_workers, update_carry_visuals)
                    .chain()
                    .in_set(GameFlowSet::Simulation)
                    .after(spawning::deplete_resource_nodes)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedLast,
                sync_carried_totals.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    trees::spawn_saplings_system,
                    trees::grow_saplings_system,
                    trees::grow_trees_system,
                )
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                trees::update_tree_occlusion_uniform
                    .in_set(GameFlowSet::Presentation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                trees::fix_tree_alpha_mode
                    .in_set(GameFlowSet::Presentation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    processing::processor_worker_visual_system,
                    processing::reconcile_processor_assignments,
                    processing::enforce_processor_worker_limit,
                    processing::lock_assigned_workers_from_user_interaction,
                    spawning::resource_respawn_system,
                    spawning::grow_resource_system,
                    workers::update_resource_popups,
                )
                    .in_set(GameFlowSet::Simulation)
                    .in_set(OverlayLifecycleSet::Manage)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                terrain::rebuild_dense_grass
                    .in_set(GameFlowSet::Presentation)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<GrassInstanceAssets>),
            )
            .add_systems(
                Update,
                terrain::build_decoration_chunks
                    .in_set(GameFlowSet::Presentation)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<PendingDecorationPlacements>),
            )
            .add_systems(
                Update,
                (reveal_explored_grass, terrain::reveal_explored_decorations)
                    .in_set(GameFlowSet::Presentation)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FogOfWarMap>),
            );
    }
}

fn create_resource_node_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ResourceNodeMaterials {
        wood: materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.45, 0.1),
            ..default()
        }),
        copper: materials.add(StandardMaterial {
            base_color: Color::srgb(0.72, 0.45, 0.2),
            ..default()
        }),
        iron: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.55, 0.58),
            ..default()
        }),
        _gold: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.8, 0.2),
            emissive: LinearRgba::new(0.4, 0.35, 0.05, 1.0),
            ..default()
        }),
        oil: materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.08, 0.1),
            ..default()
        }),
        stone: materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.60, 0.56),
            ..default()
        }),
    });
}

fn create_carry_visual_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use std::collections::HashMap;

    let cube_mesh = meshes.add(Cuboid::new(0.3, 0.3, 0.3));
    let sphere_mesh = meshes.add(Sphere::new(0.2));

    let mut mats = HashMap::new();
    for rt in [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ] {
        let color = rt.carry_color();
        let mat = materials.add(StandardMaterial {
            base_color: color,
            ..default()
        });
        mats.insert(rt, mat);
    }

    commands.insert_resource(CarryVisualAssets {
        cube_mesh,
        sphere_mesh,
        materials: mats,
    });

    // Storage pile assets
    let pile_cube = meshes.add(Cuboid::new(0.4, 0.4, 0.4));
    let pile_sphere = meshes.add(Sphere::new(0.25));
    let pile_cylinder = meshes.add(Cylinder::new(0.2, 0.5));

    let mut pile_mats = HashMap::new();
    for rt in [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ] {
        let color = rt.carry_color();
        let mat = materials.add(StandardMaterial {
            base_color: color,
            ..default()
        });
        pile_mats.insert(rt, mat);
    }

    commands.insert_resource(StoragePileAssets {
        cube_mesh: pile_cube,
        sphere_mesh: pile_sphere,
        cylinder_mesh: pile_cylinder,
        materials: pile_mats,
    });
}

fn mark_carried_totals_dirty(mut dirty: ResMut<CarriedTotalsDirty>) {
    dirty.0 = true;
}

pub fn emit_carrying_delta(
    writer: &mut bevy::ecs::message::MessageWriter<CarryingDelta>,
    faction: Faction,
    resource_type: ResourceType,
    delta: i32,
) {
    if delta != 0 {
        writer.write(CarryingDelta {
            faction,
            resource_type,
            delta,
        });
    }
}

/// Full rebuild fallback for save-load, client sync, or recovery.
fn rebuild_carried_totals_full(
    workers: Query<(&Carrying, &Faction), With<Unit>>,
    mut totals: ResMut<CarriedResourceTotals>,
) {
    totals.per_faction.clear();
    for (carrying, faction) in &workers {
        if let Some(rt) = carrying.resource_type {
            if carrying.amount > 0 {
                totals
                    .per_faction
                    .entry(*faction)
                    .or_insert_with(PlayerResources::empty)
                    .add(rt, carrying.amount);
            }
        }
    }
}

pub(crate) fn sync_carried_totals(
    workers: Query<(&Carrying, &Faction), With<Unit>>,
    mut totals: ResMut<CarriedResourceTotals>,
    mut dirty: ResMut<CarriedTotalsDirty>,
    mut deltas: bevy::ecs::message::MessageReader<CarryingDelta>,
) {
    if dirty.0 {
        rebuild_carried_totals_full(workers, totals);
        dirty.0 = false;
        for _ in deltas.read() {}
        return;
    }

    for delta in deltas.read() {
        let entry = totals
            .per_faction
            .entry(delta.faction)
            .or_insert_with(PlayerResources::empty);
        let idx = delta.resource_type.index();
        let current = entry.amounts[idx] as i64;
        let next = (current + delta.delta as i64).max(0) as u32;
        entry.amounts[idx] = next;
    }
}

/// Drain carried resources from workers for pending spend requests.
fn drain_carried_from_workers(
    mut drains: ResMut<PendingCarriedDrains>,
    mut workers: Query<(&mut Carrying, &Faction), With<Unit>>,
    mut carry_deltas: bevy::ecs::message::MessageWriter<CarryingDelta>,
) {
    for msg in drains.drains.iter_mut() {
        if !msg.has_deficit() {
            continue;
        }
        for (mut carrying, faction) in &mut workers {
            if *faction != msg.faction {
                continue;
            }
            if !msg.has_deficit() {
                break;
            }
            let Some(rt) = carrying.resource_type else {
                continue;
            };
            let needed = msg.get(rt);
            if needed == 0 || carrying.amount == 0 {
                continue;
            }
            let take = needed.min(carrying.amount);
            carrying.amount -= take;
            carrying.weight = carrying.amount as f32 * rt.weight();
            emit_carrying_delta(&mut carry_deltas, *faction, rt, -(take as i32));
            msg.sub(rt, take);
            if carrying.amount == 0 {
                carrying.resource_type = None;
                carrying.weight = 0.0;
            }
        }
    }
    drains.drains.clear();
}

fn update_carry_visuals(
    mut commands: Commands,
    carry_assets: Option<Res<CarryVisualAssets>>,
    mut workers: Query<
        (Entity, &Carrying, &CarryCapacity, Option<&CarryVisual>),
        (With<Unit>, With<GatherSpeed>),
    >,
) {
    let Some(assets) = carry_assets else { return };

    for (entity, carrying, capacity, carry_visual) in &mut workers {
        if carrying.amount > 0 && carry_visual.is_none() {
            // Spawn carry visual
            if let Some(rt) = carrying.resource_type {
                let mesh = match rt {
                    ResourceType::Wood => assets.cube_mesh.clone(),
                    _ => assets.sphere_mesh.clone(),
                };
                let mat = assets.materials.get(&rt).cloned().unwrap_or_default();
                let scale_factor = 0.5 + 0.5 * (carrying.weight / capacity.0).min(1.0);

                let child = commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(mat),
                        Transform::from_translation(Vec3::new(0.0, 0.8, -0.3))
                            .with_scale(Vec3::splat(scale_factor)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ))
                    .id();
                commands.entity(entity).add_child(child);
                commands.entity(entity).insert(CarryVisual(child));
            }
        } else if carrying.amount == 0 {
            if let Some(visual) = carry_visual {
                commands.entity(visual.0).try_despawn();
                commands.entity(entity).remove::<CarryVisual>();
            }
        } else if let Some(visual) = carry_visual {
            // Update scale based on current weight
            let scale_factor = 0.5 + 0.5 * (carrying.weight / capacity.0).min(1.0);
            commands.entity(visual.0).try_insert(
                Transform::from_translation(Vec3::new(0.0, 0.8, -0.3))
                    .with_scale(Vec3::splat(scale_factor)),
            );
        }
    }
}
