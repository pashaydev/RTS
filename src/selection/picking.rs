use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera;
use crate::components::*;
use crate::ground::HeightMap;
use crate::hover_material::{HoverRingMaterial, HoverRingSettings};
use crate::items::ItemPickup;

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
pub(crate) fn ray_aabb_dist(
    ray: &Ray3d,
    center: Vec3,
    half_xz: f32,
    y_min: f32,
    y_max: f32,
) -> Option<f32> {
    let min = Vec3::new(center.x - half_xz, y_min, center.z - half_xz);
    let max = Vec3::new(center.x + half_xz, y_max, center.z + half_xz);

    let dir = *ray.direction;
    let inv = Vec3::new(
        if dir.x.abs() > 1e-8 {
            1.0 / dir.x
        } else {
            f32::INFINITY.copysign(dir.x)
        },
        if dir.y.abs() > 1e-8 {
            1.0 / dir.y
        } else {
            f32::INFINITY.copysign(dir.y)
        },
        if dir.z.abs() > 1e-8 {
            1.0 / dir.z
        } else {
            f32::INFINITY.copysign(dir.z)
        },
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
    pub is_pickup: bool,
}

/// Persistent state for click-cycling through overlapping entities.
#[derive(Resource, Default)]
pub(crate) struct PickCycleState {
    /// Screen position of the last click (window coords).
    pub last_cursor: Option<Vec2>,
    /// Entity returned by the last pick.
    pub last_entity: Option<Entity>,
    /// How many times the user clicked roughly the same spot.
    pub cycle_index: usize,
}

impl PickCycleState {
    /// Check if the current click is close enough to the previous one to continue cycling.
    pub fn should_cycle(&self, cursor: Vec2) -> bool {
        if let Some(prev) = self.last_cursor {
            prev.distance(cursor) < 6.0
        } else {
            false
        }
    }

    pub fn advance(&mut self, cursor: Vec2, entity: Entity) {
        if self.should_cycle(cursor) && self.last_entity == Some(entity) {
            // Same entity picked at same spot — will be skipped next time
            self.cycle_index += 1;
        } else if self.should_cycle(cursor) {
            // Same spot but different entity picked (cycling worked)
            // keep cycle_index
        } else {
            // New spot
            self.cycle_index = 0;
        }
        self.last_cursor = Some(cursor);
        self.last_entity = Some(entity);
    }

    pub fn reset(&mut self) {
        self.last_cursor = None;
        self.last_entity = None;
        self.cycle_index = 0;
    }
}

/// Collected hit data before scoring.
struct PickHit {
    entity: Entity,
    ray_dist: f32,
    screen_dist: f32,
    is_unit: bool,
    is_building: bool,
    is_mob: bool,
    is_resource: bool,
    is_pickup: bool,
}

/// Pick the best entity for click selection.
///
/// Uses a composite score of screen-space proximity (how close the entity's
/// center projects to the cursor) and ray distance. Screen-space proximity
/// dominates in dense areas, making picking predictable.
///
/// `skip_entity`: when cycling, the entity to deprioritize.
pub fn pick_for_click(
    ray: &Ray3d,
    cursor_screen: Option<Vec2>,
    camera: &Camera,
    cam_gt: &GlobalTransform,
    window: &Window,
    graphics: &GraphicsSettings,
    pickables: &Query<(Entity, &GlobalTransform, &PickRadius, &InheritedVisibility)>,
    units: &Query<Entity, With<Unit>>,
    buildings: &Query<(Entity, &BuildingFootprint, &BuildingHeight), With<Building>>,
    mobs: &Query<Entity, With<Mob>>,
    resource_nodes: &Query<Entity, With<ResourceNode>>,
    pickups: &Query<Entity, With<ItemPickup>>,
    height_map: &HeightMap,
    skip_entity: Option<Entity>,
) -> Option<PickResult> {
    let mut hits: Vec<PickHit> = Vec::new();

    for (entity, gt, pick_r, inherited_vis) in pickables {
        if !inherited_vis.get() {
            continue;
        }
        let is_unit = units.contains(entity);
        let is_building = buildings.contains(entity);
        let is_mob = mobs.contains(entity);
        let is_resource = resource_nodes.contains(entity);
        let is_pickup = pickups.contains(entity);

        if !is_unit && !is_building && !is_mob && !is_resource && !is_pickup {
            continue;
        }

        let center = gt.translation();
        let ray_dist = if is_building {
            if let Ok((_, footprint, bld_h)) = buildings.get(entity) {
                let terrain_y = height_map.sample(center.x, center.z);
                ray_aabb_dist(ray, center, footprint.0, terrain_y, terrain_y + bld_h.0)
            } else {
                ray_sphere_dist(ray, center, pick_r.0)
            }
        } else {
            ray_sphere_dist(ray, center, pick_r.0)
        };

        let Some(d) = ray_dist else {
            continue;
        };

        // Compute screen-space distance from cursor to entity center projection
        let screen_dist = cursor_screen
            .and_then(|cursor| {
                camera::world_to_window_viewport(camera, cam_gt, center, window, graphics)
                    .map(|proj| cursor.distance(proj))
            })
            .unwrap_or(0.0);

        hits.push(PickHit {
            entity,
            ray_dist: d,
            screen_dist,
            is_unit,
            is_building,
            is_mob,
            is_resource,
            is_pickup,
        });
    }

    if hits.is_empty() {
        return None;
    }

    // ── Scoring ──
    // Composite score: screen-space proximity is the primary discriminator in
    // dense clusters, with ray distance as a secondary factor and type priority
    // as a final tiebreaker.

    let min_ray = hits.iter().map(|h| h.ray_dist).fold(f32::INFINITY, f32::min);
    let max_ray = hits.iter().map(|h| h.ray_dist).fold(0.0_f32, f32::max);
    let ray_range = (max_ray - min_ray).max(0.01);

    let min_screen = hits.iter().map(|h| h.screen_dist).fold(f32::INFINITY, f32::min);
    let max_screen = hits.iter().map(|h| h.screen_dist).fold(0.0_f32, f32::max);
    let screen_range = (max_screen - min_screen).max(0.01);

    // Type priority bonus (lower is better): unit=0, building=1, pickup=2, resource=3, mob=4
    let type_priority = |h: &PickHit| -> f32 {
        if h.is_unit {
            0.0
        } else if h.is_building {
            0.05
        } else if h.is_pickup {
            0.10
        } else if h.is_resource {
            0.15
        } else {
            0.20
        }
    };

    // Sort by composite score (lower is better)
    hits.sort_by(|a, b| {
        let score_a = screen_score(a.screen_dist, min_screen, screen_range)
            + ray_score(a.ray_dist, min_ray, ray_range)
            + type_priority(a)
            + if Some(a.entity) == skip_entity { 100.0 } else { 0.0 };
        let score_b = screen_score(b.screen_dist, min_screen, screen_range)
            + ray_score(b.ray_dist, min_ray, ray_range)
            + type_priority(b)
            + if Some(b.entity) == skip_entity { 100.0 } else { 0.0 };
        score_a
            .partial_cmp(&score_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let best = &hits[0];
    Some(PickResult {
        entity: best.entity,
        is_unit: best.is_unit,
        is_building: best.is_building,
        is_mob: best.is_mob,
        is_resource: best.is_resource,
        is_pickup: best.is_pickup,
    })
}

/// Normalized screen-space score (0..1). Weighted heavily — this is the primary factor.
fn screen_score(dist: f32, min: f32, range: f32) -> f32 {
    ((dist - min) / range) * 0.7
}

/// Normalized ray-distance score (0..1). Secondary factor.
fn ray_score(dist: f32, min: f32, range: f32) -> f32 {
    ((dist - min) / range) * 0.3
}

pub(crate) fn setup_hover_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
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
    pickups: Query<Entity, With<ItemPickup>>,
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
    let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics)
    else {
        return;
    };

    let cursor_screen = window.cursor_position();

    if let Some(result) = pick_for_click(
        &ray,
        cursor_screen,
        camera,
        cam_gt,
        window,
        &graphics,
        &pickables,
        &units,
        &buildings,
        &mobs,
        &resource_nodes,
        &pickups,
        &height_map,
        None, // no skip for hover
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
