use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_mod_outline::OutlineVolume;

use crate::blueprints::{EntityKind, EntityVisualCache};
use crate::components::*;
use crate::ground::terrain_height;
use crate::hover_material::{HoverRingMaterial, HoverRingSettings};
use crate::minimap::{MinimapInteraction, MinimapSet};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectionSet;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<HoverRingMaterial>::default())
            .init_resource::<DragState>()
            .init_resource::<InspectedEnemy>()
            .init_resource::<UiClickedThisFrame>()
            .init_resource::<UiPressActive>()
            .add_systems(Startup, (spawn_selection_box, setup_hover_assets))
            .add_systems(First, reset_ui_clicked)
            .add_systems(
                Update,
                (
                    track_drag,
                    update_selection_box_visual,
                )
                    .chain()
                    .in_set(SelectionSet)
                    .after(MinimapSet),
            )
            .add_systems(
                Update,
                (
                    update_hover.after(update_selection_box_visual),
                    handle_click_select.after(update_hover),
                )
                    .in_set(SelectionSet)
                    .after(MinimapSet),
            )
            .add_systems(
                Update,
                (
                    update_entity_visuals,
                    handle_right_click_move,
                )
                    .after(SelectionSet),
            )
            .add_systems(
                Update,
                (update_hover_ring, update_hover_tooltip)
                    .after(SelectionSet),
            )
            .add_systems(
                PostUpdate,
                clear_ui_press_on_release,
            );
    }
}

// ── Ray-sphere intersection ──

/// Returns the distance along `ray` to the closest intersection with a sphere,
/// or `None` if the ray misses. Uses a generous test — if the ray origin is
/// inside the sphere it still counts as a hit (distance 0).
fn ray_sphere_dist(ray: &Ray3d, center: Vec3, radius: f32) -> Option<f32> {
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

/// Cast a ray against all pickable entities (those with `PickRadius` + `GlobalTransform`)
/// and one of the marker components. Returns the closest hit entity.
fn pick_nearest(
    ray: &Ray3d,
    pickables: &Query<(Entity, &GlobalTransform, &PickRadius)>,
    units: &Query<Entity, With<Unit>>,
    buildings: &Query<Entity, With<Building>>,
    mobs: &Query<Entity, With<Mob>>,
    resource_nodes: &Query<Entity, With<ResourceNode>>,
) -> Option<Entity> {
    let mut best: Option<(Entity, f32)> = None;

    for (entity, gt, pick_r) in pickables {
        // Only pick entities that are units, buildings, mobs, or resource nodes
        if !units.contains(entity)
            && !buildings.contains(entity)
            && !mobs.contains(entity)
            && !resource_nodes.contains(entity)
        {
            continue;
        }

        let center = gt.translation();
        if let Some(dist) = ray_sphere_dist(ray, center, pick_r.0) {
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((entity, dist));
            }
        }
    }

    best.map(|(e, _)| e)
}

/// Categorized pick result for click selection.
#[allow(dead_code)]
struct PickResult {
    entity: Entity,
    is_unit: bool,
    is_building: bool,
    is_mob: bool,
    is_resource: bool,
}

/// Pick the best entity for click selection — prioritizes units > buildings > resources > mobs.
fn pick_for_click(
    ray: &Ray3d,
    pickables: &Query<(Entity, &GlobalTransform, &PickRadius)>,
    units: &Query<Entity, With<Unit>>,
    buildings: &Query<Entity, With<Building>>,
    mobs: &Query<Entity, With<Mob>>,
    resource_nodes: &Query<Entity, With<ResourceNode>>,
) -> Option<PickResult> {
    let mut hits: Vec<(Entity, f32, bool, bool, bool, bool)> = Vec::new();

    for (entity, gt, pick_r) in pickables {
        let is_unit = units.contains(entity);
        let is_building = buildings.contains(entity);
        let is_mob = mobs.contains(entity);
        let is_resource = resource_nodes.contains(entity);

        if !is_unit && !is_building && !is_mob && !is_resource {
            continue;
        }

        let center = gt.translation();
        if let Some(dist) = ray_sphere_dist(ray, center, pick_r.0) {
            hits.push((entity, dist, is_unit, is_building, is_mob, is_resource));
        }
    }

    if hits.is_empty() {
        return None;
    }

    // Sort by distance
    hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Among close hits (within 2 units of the closest), prefer units > buildings > resources > mobs
    let closest_dist = hits[0].1;
    let threshold = closest_dist + 2.0;
    let close_hits: Vec<_> = hits.into_iter().filter(|h| h.1 <= threshold).collect();

    // Priority: unit > building > resource > mob
    if let Some(h) = close_hits.iter().find(|h| h.2) {
        return Some(PickResult { entity: h.0, is_unit: true, is_building: false, is_mob: false, is_resource: false });
    }
    if let Some(h) = close_hits.iter().find(|h| h.3) {
        return Some(PickResult { entity: h.0, is_unit: false, is_building: true, is_mob: false, is_resource: false });
    }
    if let Some(h) = close_hits.iter().find(|h| h.5) {
        return Some(PickResult { entity: h.0, is_unit: false, is_building: false, is_mob: false, is_resource: true });
    }
    if let Some(h) = close_hits.iter().find(|h| h.4) {
        return Some(PickResult { entity: h.0, is_unit: false, is_building: false, is_mob: true, is_resource: false });
    }

    None
}

fn reset_ui_clicked(mut ui_clicked: ResMut<UiClickedThisFrame>) {
    ui_clicked.0 = ui_clicked.0.saturating_sub(1);
}

fn clear_ui_press_on_release(
    mouse: Res<ButtonInput<MouseButton>>,
    mut ui_press: ResMut<UiPressActive>,
) {
    if mouse.just_released(MouseButton::Left) {
        ui_press.0 = false;
    }
}

fn setup_hover_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Flat plane that will show the ring shader — sized 3x3 units
    let ring_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(1.5)));
    commands.insert_resource(HoverRingAssets {
        mesh: ring_mesh,
    });

    // Spawn tooltip UI (hidden by default)
    commands.spawn((
        HoverTooltip,
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.1, 0.85)),
        Visibility::Hidden,
        GlobalTransform::default(),
        Text::new(""),
        TextFont { font_size: 13.0, ..default() },
        TextColor(Color::srgba(0.9, 0.9, 0.85, 1.0)),
    ));
}

fn spawn_selection_box(mut commands: Commands) {
    commands.spawn((
        SelectionBox,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Px(0.0),
            height: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.2, 0.4, 1.0, 0.2)),
        BorderColor::all(Color::srgba(0.3, 0.5, 1.0, 0.8)),
        Visibility::Hidden,
        GlobalTransform::default(),
    ));
}

fn track_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut drag: ResMut<DragState>,
    minimap_interaction: Res<MinimapInteraction>,
    ui_clicked: Res<UiClickedThisFrame>,
    ui_press: Res<UiPressActive>,
    ui_interactions: Query<&Interaction, With<Node>>,
) {
    if minimap_interaction.clicked || ui_clicked.0 > 0 || ui_press.0 {
        return;
    }
    // Block drag when mouse is over any UI node
    for interaction in &ui_interactions {
        if *interaction == Interaction::Pressed || *interaction == Interaction::Hovered {
            return;
        }
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        drag.start = Some(cursor);
        drag.current = Some(cursor);
        drag.dragging = false;
    }

    if mouse.pressed(MouseButton::Left) {
        drag.current = Some(cursor);
        if let Some(start) = drag.start {
            if (cursor - start).length() > 5.0 {
                drag.dragging = true;
            }
        }
    }
}

fn update_selection_box_visual(
    drag: Res<DragState>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut query: Query<(&mut Node, &mut Visibility), With<SelectionBox>>,
    placement: Res<BuildingPlacementState>,
    minimap_interaction: Res<MinimapInteraction>,
) {
    let Ok((mut node, mut vis)) = query.single_mut() else {
        return;
    };

    if drag.dragging
        && mouse.pressed(MouseButton::Left)
        && placement.mode == PlacementMode::None
        && !minimap_interaction.clicked
    {
        if let (Some(start), Some(current)) = (drag.start, drag.current) {
            let min_x = start.x.min(current.x);
            let min_y = start.y.min(current.y);
            let w = (current.x - start.x).abs();
            let h = (current.y - start.y).abs();

            node.left = Val::Px(min_x);
            node.top = Val::Px(min_y);
            node.width = Val::Px(w);
            node.height = Val::Px(h);
            *vis = Visibility::Visible;
        }
    } else {
        *vis = Visibility::Hidden;
    }
}

/// Raycast from cursor using ray-sphere intersection against all pickable entities.
fn update_hover(
    mut commands: Commands,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pickables: Query<(Entity, &GlobalTransform, &PickRadius)>,
    units: Query<Entity, With<Unit>>,
    buildings: Query<Entity, With<Building>>,
    mobs: Query<Entity, With<Mob>>,
    resource_nodes: Query<Entity, With<ResourceNode>>,
    hovered: Query<Entity, With<Hovered>>,
    placement: Res<BuildingPlacementState>,
    ui_interactions: Query<&Interaction, With<Node>>,
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
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
        return;
    };

    if let Some(entity) = pick_nearest(&ray, &pickables, &units, &buildings, &mobs, &resource_nodes) {
        commands.entity(entity).insert(Hovered);
    }
}

fn handle_click_select(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: (ResMut<DragState>, ResMut<InspectedEnemy>),
    placement: Res<BuildingPlacementState>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pickables: Query<(Entity, &GlobalTransform, &PickRadius)>,
    entity_queries: (
        Query<Entity, With<Unit>>,
        Query<Entity, With<Building>>,
        Query<Entity, With<Mob>>,
        Query<Entity, With<ResourceNode>>,
    ),
    selected: Query<Entity, With<Selected>>,
    unit_transforms: Query<&GlobalTransform, With<Unit>>,
    flags: (Res<MinimapInteraction>, Res<UiClickedThisFrame>, Res<UiPressActive>),
    ui_interactions: Query<&Interaction, With<Node>>,
) {
    let (ref mut drag, ref mut inspected) = state;
    let (ref units, ref buildings, ref mobs, ref resource_nodes) = entity_queries;
    let (ref minimap_interaction, ref ui_clicked, ref ui_press) = flags;
    if !mouse.just_released(MouseButton::Left) {
        return;
    }

    if placement.mode != PlacementMode::None {
        return;
    }

    // Block selection when clicking on any UI element
    let mut ui_blocking = minimap_interaction.clicked || ui_clicked.0 > 0 || ui_press.0;
    if !ui_blocking {
        for interaction in &ui_interactions {
            if *interaction == Interaction::Pressed || *interaction == Interaction::Hovered {
                ui_blocking = true;
                break;
            }
        }
    }
    if ui_blocking {
        drag.start = None;
        drag.current = None;
        drag.dragging = false;
        return;
    }

    let was_dragging = drag.dragging;
    let drag_start = drag.start;
    let drag_end = drag.current;

    drag.start = None;
    drag.current = None;
    drag.dragging = false;

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };

    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    if was_dragging {
        if let (Some(start), Some(end)) = (drag_start, drag_end) {
            let min_x = start.x.min(end.x);
            let max_x = start.x.max(end.x);
            let min_y = start.y.min(end.y);
            let max_y = start.y.max(end.y);

            if !shift {
                for entity in &selected {
                    commands.entity(entity).remove::<Selected>();
                }
            }

            inspected.entity = None;

            for entity in units.iter() {
                if let Ok(gt) = unit_transforms.get(entity) {
                    if let Ok(screen_pos) = camera.world_to_viewport(cam_gt, gt.translation()) {
                        if screen_pos.x >= min_x
                            && screen_pos.x <= max_x
                            && screen_pos.y >= min_y
                            && screen_pos.y <= max_y
                        {
                            commands.entity(entity).insert(Selected);
                        }
                    }
                }
            }
        }
    } else {
        let Some(cursor) = window.cursor_position() else {
            return;
        };
        let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
            return;
        };

        let pick = pick_for_click(&ray, &pickables, &units, &buildings, &mobs, &resource_nodes);

        if let Some(result) = pick {
            if result.is_mob {
                inspected.entity = Some(result.entity);
            } else {
                inspected.entity = None;

                if !shift {
                    for entity in &selected {
                        commands.entity(entity).remove::<Selected>();
                    }
                }

                if shift && selected.contains(result.entity) {
                    commands.entity(result.entity).remove::<Selected>();
                } else {
                    commands.entity(result.entity).insert(Selected);
                }
            }
        } else {
            inspected.entity = None;
            if !shift {
                for entity in &selected {
                    commands.entity(entity).remove::<Selected>();
                }
            }
        }
    }
}

fn update_entity_visuals(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    added_selected: Query<(Entity, &EntityKind), Added<Selected>>,
    mut removed_selected: RemovedComponents<Selected>,
    added_hovered: Query<(Entity, &EntityKind), Added<Hovered>>,
    mut removed_hovered: RemovedComponents<Hovered>,
    all_entities: Query<(Entity, &EntityKind, Has<Selected>, Has<Hovered>)>,
    mut outlines: Query<&mut OutlineVolume>,
) {
    // Outline colors
    let outline_selected = Color::srgb(0.2, 1.0, 0.3);
    let outline_hovered = Color::srgb(0.3, 0.8, 1.0);

    for (entity, kind) in &added_selected {
        if let Some(mat) = cache.materials_selected.get(kind) {
            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
        }
        if let Ok(mut outline) = outlines.get_mut(entity) {
            outline.visible = true;
            outline.colour = outline_selected;
            outline.width = 4.0;
        }
    }

    for entity in removed_selected.read() {
        if let Ok((_, kind, _, has_hovered)) = all_entities.get(entity) {
            if has_hovered {
                if let Some(mat) = cache.materials_hovered.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
                if let Ok(mut outline) = outlines.get_mut(entity) {
                    outline.visible = true;
                    outline.colour = outline_hovered;
                    outline.width = 3.0;
                }
            } else {
                if let Some(mat) = cache.materials_default.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
                if let Ok(mut outline) = outlines.get_mut(entity) {
                    outline.visible = false;
                }
            }
        }
    }

    for (entity, kind) in &added_hovered {
        if let Ok((_, _, has_selected, _)) = all_entities.get(entity) {
            if !has_selected {
                if let Some(mat) = cache.materials_hovered.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
                if let Ok(mut outline) = outlines.get_mut(entity) {
                    outline.visible = true;
                    outline.colour = outline_hovered;
                    outline.width = 3.0;
                }
            }
        }
    }

    for entity in removed_hovered.read() {
        if let Ok((_, kind, has_selected, _)) = all_entities.get(entity) {
            if !has_selected {
                if let Some(mat) = cache.materials_default.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
                if let Ok(mut outline) = outlines.get_mut(entity) {
                    outline.visible = false;
                }
            }
        }
    }
}

/// Spawn/despawn a hover ring decal on the ground under the hovered entity.
fn update_hover_ring(
    mut commands: Commands,
    hovered: Query<(Entity, &Transform), With<Hovered>>,
    existing_rings: Query<(Entity, &MeshMaterial3d<HoverRingMaterial>), With<HoverRing>>,
    ring_assets: Res<HoverRingAssets>,
    mut hover_materials: ResMut<Assets<HoverRingMaterial>>,
    time: Res<Time>,
) {
    // Despawn old rings
    for (ring, _) in &existing_rings {
        commands.entity(ring).despawn();
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
            Transform::from_translation(Vec3::new(pos.x, terrain_height(pos.x, pos.z) + 0.1, pos.z)),
        ));
    }
}

/// Update tooltip position and text based on hovered entity.
fn update_hover_tooltip(
    mut tooltip_q: Query<(&mut Node, &mut Visibility, &mut Text), With<HoverTooltip>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    hovered_entities: Query<Entity, With<Hovered>>,
    entity_kinds: Query<&EntityKind>,
    resource_nodes: Query<&ResourceNode>,
    healths: Query<&Health>,
    building_levels: Query<&BuildingLevel>,
) {
    let Ok((mut node, mut vis, mut text)) = tooltip_q.single_mut() else {
        return;
    };

    let Ok(window) = windows.single() else {
        *vis = Visibility::Hidden;
        return;
    };

    let Some(cursor) = window.cursor_position() else {
        *vis = Visibility::Hidden;
        return;
    };

    let Ok(entity) = hovered_entities.single() else {
        *vis = Visibility::Hidden;
        return;
    };

    // Build tooltip text
    let mut label = String::new();

    if let Ok(kind) = entity_kinds.get(entity) {
        label.push_str(kind.display_name());
        if let Ok(level) = building_levels.get(entity) {
            label.push_str(&format!(" (Lv {})", level.0));
        }
    } else if let Ok(rn) = resource_nodes.get(entity) {
        label.push_str(&format!("{} ({})", rn.resource_type.display_name(), rn.amount_remaining));
    }

    if label.is_empty() {
        *vis = Visibility::Hidden;
        return;
    }

    if let Ok(health) = healths.get(entity) {
        label.push_str(&format!("\nHP: {}/{}", health.current as u32, health.max as u32));
    }

    *text = Text::new(label);

    // Position tooltip near cursor with offset
    node.left = Val::Px(cursor.x + 16.0);
    node.top = Val::Px(cursor.y - 10.0);
    *vis = Visibility::Visible;
}

fn handle_right_click_move(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    selected_units: Query<(Entity, &EntityKind), (With<Unit>, With<Selected>)>,
    pickables: Query<(Entity, &GlobalTransform, &PickRadius)>,
    mobs: Query<Entity, With<Mob>>,
    resource_nodes: Query<Entity, With<ResourceNode>>,
    construction_q: Query<(Entity, &GlobalTransform), (With<Building>, With<ConstructionProgress>)>,
    minimap_interaction: Res<MinimapInteraction>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    if minimap_interaction.clicked {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
        return;
    };

    let units_vec: Vec<(Entity, EntityKind)> = selected_units.iter().map(|(e, k)| (e, *k)).collect();
    if units_vec.is_empty() {
        return;
    }

    // Find closest mob, construction site, or resource node under cursor
    let mut best_mob: Option<(Entity, f32)> = None;
    let mut best_resource: Option<(Entity, f32)> = None;
    let mut best_construction: Option<(Entity, f32)> = None;

    for (entity, gt, pick_r) in &pickables {
        let center = gt.translation();
        if let Some(dist) = ray_sphere_dist(&ray, center, pick_r.0) {
            if mobs.contains(entity) {
                if best_mob.is_none() || dist < best_mob.unwrap().1 {
                    best_mob = Some((entity, dist));
                }
            } else if construction_q.contains(entity) {
                if best_construction.is_none() || dist < best_construction.unwrap().1 {
                    best_construction = Some((entity, dist));
                }
            } else if resource_nodes.contains(entity) {
                if best_resource.is_none() || dist < best_resource.unwrap().1 {
                    best_resource = Some((entity, dist));
                }
            }
        }
    }

    if let Some((mob_entity, _)) = best_mob {
        for (entity, kind) in &units_vec {
            let mut ec = commands.entity(*entity);
            ec.remove::<MoveTarget>()
                .insert(AttackTarget(mob_entity));
            if *kind == EntityKind::Worker {
                ec.insert(WorkerTask::Idle);
            }
        }
    } else if let Some((construction_entity, _)) = best_construction {
        let construction_pos = pickables.get(construction_entity).map(|(_, gt, _)| gt.translation());
        for (entity, kind) in &units_vec {
            if *kind == EntityKind::Worker {
                commands
                    .entity(*entity)
                    .remove::<AttackTarget>()
                    .remove::<MoveTarget>()
                    .insert(WorkerTask::MovingToBuild(construction_entity));
            } else if let Ok(pos) = construction_pos {
                commands
                    .entity(*entity)
                    .remove::<AttackTarget>()
                    .insert(MoveTarget(pos));
            }
        }
    } else if let Some((resource_entity, _)) = best_resource {
        // Only workers can gather; non-workers move to the resource instead
        let resource_pos = pickables.get(resource_entity).map(|(_, gt, _)| gt.translation());
        for (entity, kind) in &units_vec {
            if *kind == EntityKind::Worker {
                commands
                    .entity(*entity)
                    .remove::<AttackTarget>()
                    .remove::<MoveTarget>()
                    .insert(WorkerTask::MovingToResource(resource_entity));
            } else if let Ok(pos) = resource_pos {
                commands
                    .entity(*entity)
                    .remove::<AttackTarget>()
                    .insert(MoveTarget(pos));
            }
        }
    } else {
        if let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) {
            let point = ray.get_point(dist);

            // Check if ground click is near a construction site (fallback for missed pick sphere)
            let nearby_construction_radius = 5.0;
            let mut nearest_site: Option<(Entity, f32)> = None;
            for (site_entity, site_gt) in &construction_q {
                let site_pos = site_gt.translation();
                let d = point.distance(Vec3::new(site_pos.x, point.y, site_pos.z));
                if d < nearby_construction_radius {
                    if nearest_site.is_none() || d < nearest_site.unwrap().1 {
                        nearest_site = Some((site_entity, d));
                    }
                }
            }

            let has_workers = units_vec.iter().any(|(_, k)| *k == EntityKind::Worker);
            if let Some((site_entity, _)) = nearest_site.filter(|_| has_workers) {
                for (entity, kind) in &units_vec {
                    if *kind == EntityKind::Worker {
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .remove::<MoveTarget>()
                            .insert(WorkerTask::MovingToBuild(site_entity));
                    } else {
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(point));
                    }
                }
            } else {
                let n = units_vec.len();

                if n == 1 {
                    let (ent, kind) = &units_vec[0];
                    let mut ec = commands.entity(*ent);
                    ec.remove::<AttackTarget>()
                        .insert(MoveTarget(point));
                    if *kind == EntityKind::Worker {
                        ec.insert(WorkerTask::Idle);
                    }
                } else if n > 1 {
                    let spacing = 1.5;
                    let radius = (spacing * n as f32 / std::f32::consts::TAU).max(1.0);
                    for (i, (entity, kind)) in units_vec.iter().enumerate() {
                        let angle = i as f32 / n as f32 * std::f32::consts::TAU;
                        let offset =
                            Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                        let mut ec = commands.entity(*entity);
                        ec.remove::<AttackTarget>()
                            .insert(MoveTarget(point + offset));
                        if *kind == EntityKind::Worker {
                            ec.insert(WorkerTask::Idle);
                        }
                    }
                }
            }
        }
    }
}
