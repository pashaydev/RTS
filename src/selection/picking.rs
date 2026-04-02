use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera;
use crate::components::*;
use crate::ground::HeightMap;
use crate::hover_material::{HoverRingMaterial, HoverRingSettings};

// ── Ray-sphere intersection ──

/// Returns the distance along `ray` to the closest intersection with a sphere,
/// or `None` if the ray misses. Uses a generous test — if the ray origin is
/// inside the sphere it still counts as a hit (distance 0).
pub(crate) fn ray_sphere_dist(ray: &Ray3d, center: Vec3, radius: f32) -> Option<f32> {
    let oc = ray.origin - center;
    let b = oc.dot(*ray.direction);
    let c = oc.dot(oc) - radius * radius;

    // Inside the sphere
    if c < 0.0 {
        return Some(0.0);
    }

    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }

    let t = -b - discriminant.sqrt();
    if t < 0.0 {
        // Sphere is behind the ray but we might be inside — already handled above
        None
    } else {
        Some(t)
    }
}

/// Returns the distance along `ray` to the closest intersection with an AABB,
/// or `None` if the ray misses. The box spans `[center.x ± half_xz, y_min..y_max, center.z ± half_xz]`.
pub(crate) fn ray_aabb_dist(ray: &Ray3d, center: Vec3, half_xz: f32, y_min: f32, y_max: f32) -> Option<f32> {
    let min = Vec3::new(center.x - half_xz, y_min, center.z - half_xz);
    let max = Vec3::new(center.x + half_xz, y_max, center.z + half_xz);

    let dir = *ray.direction;
    let inv = Vec3::new(
        if dir.x.abs() > 1e-8 { 1.0 / dir.x } else { f32::INFINITY.copysign(dir.x) },
        if dir.y.abs() > 1e-8 { 1.0 / dir.y } else { f32::INFINITY.copysign(dir.y) },
        if dir.z.abs() > 1e-8 { 1.0 / dir.z } else { f32::INFINITY.copysign(dir.z) },
    );

    let t1 = (min.x - ray.origin.x) * inv.x;
    let t2 = (max.x - ray.origin.x) * inv.x;
    let t3 = (min.y - ray.origin.y) * inv.y;
    let t4 = (max.y - ray.origin.y) * inv.y;
    let t5 = (min.z - ray.origin.z) * inv.z;
    let t6 = (max.z - ray.origin.z) * inv.z;

    let tmin = t1.min(t2).max(t3.min(t4)).max(t5.min(t6));
    let tmax = t1.max(t2).min(t3.max(t4)).min(t5.max(t6));

    if tmax < 0.0 || tmin > tmax {
        return None;
    }
    Some(if tmin < 0.0 { 0.0 } else { tmin })
}

/// Categorized pick result for click selection.
#[allow(dead_code)]
pub(crate) struct PickResult {
    pub entity: Entity,
    pub is_unit: bool,
    pub is_building: bool,
    pub is_mob: bool,
    pub is_resource: bool,
}

/// Pick the best entity for click selection.
///
/// Distance should dominate target choice. Type priority is only used to break
/// near-ties so a nearby unit does not steal hover/selection from a building
/// that is more directly under the cursor.
pub fn pick_for_click(
    ray: &Ray3d,
    pickables: &Query<(Entity, &GlobalTransform, &PickRadius, &InheritedVisibility)>,
    units: &Query<Entity, With<Unit>>,
    buildings: &Query<(Entity, &BuildingFootprint, &BuildingHeight), With<Building>>,
    mobs: &Query<Entity, With<Mob>>,
    resource_nodes: &Query<Entity, With<ResourceNode>>,
    height_map: &HeightMap,
) -> Option<PickResult> {
    let mut hits: Vec<(Entity, f32, bool, bool, bool, bool)> = Vec::new();

    for (entity, gt, pick_r, inherited_vis) in pickables {
        // Skip entities hidden by fog of war
        if !inherited_vis.get() {
            continue;
        }
        let is_unit = units.contains(entity);
        let is_building = buildings.contains(entity);
        let is_mob = mobs.contains(entity);
        let is_resource = resource_nodes.contains(entity);

        if !is_unit && !is_building && !is_mob && !is_resource {
            continue;
        }

        let center = gt.translation();
        let dist = if is_building {
            if let Ok((_, footprint, bld_h)) = buildings.get(entity) {
                let terrain_y = height_map.sample(center.x, center.z);
                ray_aabb_dist(ray, center, footprint.0, terrain_y, terrain_y + bld_h.0)
            } else {
                ray_sphere_dist(ray, center, pick_r.0)
            }
        } else {
            ray_sphere_dist(ray, center, pick_r.0)
        };
        if let Some(d) = dist {
            hits.push((entity, d, is_unit, is_building, is_mob, is_resource));
        }
    }

    if hits.is_empty() {
        return None;
    }

    // Sort by distance
    hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Only apply type priority to genuine near-ties. The previous 2.0-unit
    // threshold let nearby units win over buildings too aggressively.
    let closest_dist = hits[0].1;
    let threshold = closest_dist + 0.35;
    let close_hits: Vec<_> = hits.into_iter().filter(|h| h.1 <= threshold).collect();

    // Priority: unit > building > resource > mob
    if let Some(h) = close_hits.iter().find(|h| h.2) {
        return Some(PickResult {
            entity: h.0,
            is_unit: true,
            is_building: false,
            is_mob: false,
            is_resource: false,
        });
    }
    if let Some(h) = close_hits.iter().find(|h| h.3) {
        return Some(PickResult {
            entity: h.0,
            is_unit: false,
            is_building: true,
            is_mob: false,
            is_resource: false,
        });
    }
    if let Some(h) = close_hits.iter().find(|h| h.5) {
        return Some(PickResult {
            entity: h.0,
            is_unit: false,
            is_building: false,
            is_mob: false,
            is_resource: true,
        });
    }
    if let Some(h) = close_hits.iter().find(|h| h.4) {
        return Some(PickResult {
            entity: h.0,
            is_unit: false,
            is_building: false,
            is_mob: true,
            is_resource: false,
        });
    }

    None
}

pub(crate) fn setup_hover_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Flat plane that will show the ring shader — sized 3x3 units
    let ring_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(1.5)));
    commands.insert_resource(HoverRingAssets { mesh: ring_mesh });
}

/// Raycast from cursor using ray-sphere intersection against all pickable entities.
pub(crate) fn update_hover(
    mut commands: Commands,
    camera_q: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    graphics: Res<GraphicsSettings>,
    pickables: Query<(Entity, &GlobalTransform, &PickRadius, &InheritedVisibility)>,
    units: Query<Entity, With<Unit>>,
    buildings: Query<(Entity, &BuildingFootprint, &BuildingHeight), With<Building>>,
    mobs: Query<Entity, With<Mob>>,
    resource_nodes: Query<Entity, With<ResourceNode>>,
    hovered: Query<Entity, With<Hovered>>,
    placement: Res<BuildingPlacementState>,
    ui_interactions: Query<&Interaction, With<Node>>,
    height_map: Res<HeightMap>,
) {
    // Remove previous hover
    for entity in &hovered {
        commands.entity(entity).remove::<Hovered>();
    }

    if placement.mode != PlacementMode::None {
        return;
    }

    for interaction in &ui_interactions {
        if *interaction == Interaction::Hovered || *interaction == Interaction::Pressed {
            return;
        }
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics) else {
        return;
    };

    if let Some(result) = pick_for_click(
        &ray,
        &pickables,
        &units,
        &buildings,
        &mobs,
        &resource_nodes,
        &height_map,
    ) {
        commands.entity(result.entity).insert(Hovered);
    }
}

/// Spawn/despawn a hover ring decal on the ground under the hovered entity.
pub(crate) fn update_hover_ring(
    mut commands: Commands,
    hovered: Query<(Entity, &Transform), With<Hovered>>,
    existing_rings: Query<(Entity, &MeshMaterial3d<HoverRingMaterial>), With<HoverRing>>,
    ring_assets: Res<HoverRingAssets>,
    mut hover_materials: ResMut<Assets<HoverRingMaterial>>,
    height_map: Res<HeightMap>,
    time: Res<Time>,
) {
    // Despawn old rings
    for (ring, _) in &existing_rings {
        commands.entity(ring).try_despawn();
    }

    // Spawn ring for current hovered entity
    for (_entity, transform) in &hovered {
        let pos = transform.translation;
        let mat = hover_materials.add(HoverRingMaterial {
            settings: HoverRingSettings {
                time: time.elapsed_secs(),
                ..default()
            },
        });
        commands.spawn((
            HoverRing,
            Mesh3d(ring_assets.mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(Vec3::new(
                pos.x,
                height_map.sample(pos.x, pos.z) + 0.1,
                pos.z,
            )),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}
