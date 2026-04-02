use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_mod_outline::OutlineVolume;
use game_state::message::{ClientMessage, InputCommand, PlayerInput, ServerMessage};

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::camera;
use crate::components::*;
use crate::ground::HeightMap;
use crate::hover_material::{HoverRingMaterial, HoverRingSettings};
use crate::minimap::{MinimapInteraction, MinimapSet};
use crate::multiplayer::host_systems::execute_input_command;
use crate::multiplayer::{ClientNetState, HostNetState, NetRole};
use crate::net_bridge::EntityNetMap;
use crate::audio::{PlaySfx, SfxKind};
use crate::combat_intents::{
    apply_manual_attack_intent, apply_manual_attack_move_intent, apply_manual_hold_intent,
    apply_manual_move_intent, clear_combat_intent,
};
use crate::orders;

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
            .init_resource::<CommandMode>()
            .init_resource::<ActiveFormation>()
            .init_resource::<NextTaskId>()
            .init_resource::<SubgroupCycleState>()
            .init_resource::<DoubleClickDetector>()
            .add_systems(Startup, setup_hover_assets)
            .add_systems(OnEnter(AppState::InGame), spawn_selection_box)
            .add_systems(First, reset_ui_clicked.run_if(in_state(AppState::InGame)))
            .add_systems(
                Update,
                (track_drag, update_selection_box_visual)
                    .chain()
                    .in_set(SelectionSet)
                    .in_set(GameFlowSet::Input)
                    .after(MinimapSet)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                (
                    update_hover.after(update_selection_box_visual),
                    handle_click_select.after(update_hover),
                )
                    .in_set(SelectionSet)
                    .in_set(GameFlowSet::Input)
                    .after(MinimapSet)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(Update, update_entity_visuals)
            .add_systems(
                Update,
                handle_right_click_move
                    .in_set(GameFlowSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                handle_unit_command_hotkeys
                    .in_set(GameFlowSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                update_hover_ring
                    .in_set(GameFlowSet::Presentation)
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    tab_subgroup_cycle_system,
                    reset_subgroup_on_selection_change,
                )
                    .chain()
                    .in_set(GameFlowSet::Input)
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                PostUpdate,
                clear_ui_press_on_release.run_if(in_state(AppState::InGame)),
            );
    }
}

fn enqueue_task(
    task_queues: &mut Query<&mut TaskQueue, With<Unit>>,
    next_task_id: &mut NextTaskId,
    entity: Entity,
    task: QueuedTask,
) {
    if let Ok(mut queue) = task_queues.get_mut(entity) {
        orders::push_queued_task(&mut queue, next_task_id, task);
    }
}

fn clear_task_queue(task_queues: &mut Query<&mut TaskQueue, With<Unit>>, entity: Entity) {
    if let Ok(mut queue) = task_queues.get_mut(entity) {
        orders::clear_task_queue(&mut queue);
    }
}

fn set_current_task(
    task_queues: &mut Query<&mut TaskQueue, With<Unit>>,
    next_task_id: &mut NextTaskId,
    entity: Entity,
    task: QueuedTask,
) {
    if let Ok(mut queue) = task_queues.get_mut(entity) {
        queue.clear_queued();
        orders::set_current_task(&mut queue, next_task_id, task);
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

/// Returns the distance along `ray` to the closest intersection with an AABB,
/// or `None` if the ray misses. The box spans `[center.x ± half_xz, y_min..y_max, center.z ± half_xz]`.
fn ray_aabb_dist(ray: &Ray3d, center: Vec3, half_xz: f32, y_min: f32, y_max: f32) -> Option<f32> {
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
struct PickResult {
    entity: Entity,
    is_unit: bool,
    is_building: bool,
    is_mob: bool,
    is_resource: bool,
}

/// Pick the best entity for click selection.
///
/// Distance should dominate target choice. Type priority is only used to break
/// near-ties so a nearby unit does not steal hover/selection from a building
/// that is more directly under the cursor.
fn pick_for_click(
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
    commands.insert_resource(HoverRingAssets { mesh: ring_mesh });
}

fn spawn_selection_box(mut commands: Commands) {
    commands.spawn((
        GameWorld,
        SelectionBox,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Px(0.0),
            height: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.29, 0.62, 1.0, 0.15)),
        BorderColor::all(Color::srgba(0.29, 0.62, 1.0, 0.6)),
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
    ui_scale: Res<UiScale>,
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
            let scale = ui_scale.0.max(0.001);
            let min_x = start.x.min(current.x);
            let min_y = start.y.min(current.y);
            let w = (current.x - start.x).abs();
            let h = (current.y - start.y).abs();

            node.left = Val::Px(min_x / scale);
            node.top = Val::Px(min_y / scale);
            node.width = Val::Px(w / scale);
            node.height = Val::Px(h / scale);
            *vis = Visibility::Visible;
        }
    } else {
        *vis = Visibility::Hidden;
    }
}

/// Raycast from cursor using ray-sphere intersection against all pickable entities.
fn update_hover(
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

fn handle_click_select(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: (ResMut<DragState>, ResMut<InspectedEnemy>),
    placement: Res<BuildingPlacementState>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    pickables: Query<(Entity, &GlobalTransform, &PickRadius, &InheritedVisibility)>,
    entity_queries: (
        Query<Entity, With<Unit>>,
        Query<(Entity, &BuildingFootprint, &BuildingHeight), With<Building>>,
        Query<Entity, With<Mob>>,
        Query<Entity, With<ResourceNode>>,
    ),
    selected: Query<Entity, With<Selected>>,
    unit_transforms: Query<&GlobalTransform, With<Unit>>,
    flags: (
        Res<MinimapInteraction>,
        Res<UiClickedThisFrame>,
        Res<UiPressActive>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    active_player: Res<ActivePlayer>,
    faction_q: Query<&Faction>,
    height_map: Res<HeightMap>,
    mut extra: (
        Res<Time<Real>>,
        ResMut<DoubleClickDetector>,
        Query<&EntityKind>,
        bevy::ecs::message::MessageWriter<PlaySfx>,
    ),
) {
    let (ref camera_q, ref windows, ref graphics) = viewport;
    let (ref mut drag, ref mut inspected) = state;
    let (ref units, ref buildings, ref mobs, ref resource_nodes) = entity_queries;
    let (ref minimap_interaction, ref ui_clicked, ref ui_press) = flags;
    let (ref time, ref mut dbl_click, ref entity_kinds, ref mut sfx) = extra;
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
                // Only select units of the active faction
                if let Ok(f) = faction_q.get(entity) {
                    if *f != active_player.0 {
                        continue;
                    }
                }
                // Workers assigned to buildings are now visible, so no need to skip them
                if let Ok(gt) = unit_transforms.get(entity) {
                    if let Some(screen_pos) =
                        camera::world_to_window_viewport(camera, cam_gt, gt.translation(), window, &graphics)
                    {
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
        let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics) else {
            return;
        };

        let pick = pick_for_click(
            &ray,
            &pickables,
            &units,
            &buildings,
            &mobs,
            &resource_nodes,
            &height_map,
        );

        if let Some(result) = pick {
            if result.is_mob {
                inspected.entity = Some(result.entity);
            } else if result.is_resource {
                // Resources can always be inspected/selected
                inspected.entity = None;
                if !shift {
                    for entity in &selected {
                        commands.entity(entity).remove::<Selected>();
                    }
                }
                commands.entity(result.entity).insert(Selected);
            } else {
                // Units and buildings: only select if they belong to active faction
                let is_own = faction_q
                    .get(result.entity)
                    .map(|f| *f == active_player.0)
                    .unwrap_or(false);

                if !is_own {
                    // Inspect enemy units/buildings instead of selecting
                    inspected.entity = Some(result.entity);
                } else {
                    inspected.entity = None;

                    // Double-click detection: select all visible same-type units
                    let now = time.elapsed_secs_f64();
                    let is_double_click = dbl_click.last_click_entity == Some(result.entity)
                        && (now - dbl_click.last_click_time) < 0.4;
                    dbl_click.last_click_entity = Some(result.entity);
                    dbl_click.last_click_time = now;

                    if is_double_click && !shift {
                        if let Ok(clicked_kind) = entity_kinds.get(result.entity) {
                            let target_kind = *clicked_kind;
                            // Deselect all
                            for entity in &selected {
                                commands.entity(entity).remove::<Selected>();
                            }
                            // Select all own units of same type visible on screen
                            let Ok((camera, cam_gt)) = camera_q.single() else {
                                return;
                            };
                            for entity in units.iter() {
                                if let Ok(f) = faction_q.get(entity) {
                                    if *f != active_player.0 {
                                        continue;
                                    }
                                }
                                if let Ok(kind) = entity_kinds.get(entity) {
                                    if *kind != target_kind {
                                        continue;
                                    }
                                }
                                if let Ok(gt) = unit_transforms.get(entity) {
                                    if camera::world_to_window_viewport(
                                        camera,
                                        cam_gt,
                                        gt.translation(),
                                        window,
                                        &graphics,
                                    )
                                    .is_some()
                                    {
                                        commands.entity(entity).insert(Selected);
                                    }
                                }
                            }
                        }
                        // Clear double-click state so triple doesn't re-trigger
                        dbl_click.last_click_entity = None;
                    } else {
                        if !shift {
                            for entity in &selected {
                                commands.entity(entity).remove::<Selected>();
                            }
                        }

                        if shift && selected.contains(result.entity) {
                            commands.entity(result.entity).remove::<Selected>();
                        } else {
                            commands.entity(result.entity).insert(Selected);
                            if let Ok(gt) = unit_transforms.get(result.entity) {
                                sfx.write(PlaySfx {
                                    kind: SfxKind::UnitSelect,
                                    position: Some(gt.translation()),
                                });
                            }
                        }
                    }
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
    added_selected: Query<(Entity, &EntityKind, Has<Mesh3d>), Added<Selected>>,
    mut removed_selected: RemovedComponents<Selected>,
    added_hovered: Query<(Entity, &EntityKind, Has<Mesh3d>), Added<Hovered>>,
    mut removed_hovered: RemovedComponents<Hovered>,
    added_army_highlighted: Query<(Entity, &EntityKind, Has<Mesh3d>), Added<ArmyOverviewHighlighted>>,
    mut removed_army_highlighted: RemovedComponents<ArmyOverviewHighlighted>,
    all_entities: Query<(
        Entity,
        &EntityKind,
        Has<Selected>,
        Has<Hovered>,
        Has<ArmyOverviewHighlighted>,
        Has<Mesh3d>,
    )>,
    mut outlines: Query<&mut OutlineVolume>,
) {
    // Outline colors
    let outline_selected = Color::srgb(0.2, 1.0, 0.3);
    let outline_hovered = Color::srgb(0.3, 0.8, 1.0);

    for (entity, kind, has_mesh) in &added_selected {
        if has_mesh {
            if let Some(mat) = cache.materials_selected.get(kind) {
                commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
            }
        }
        if let Ok(mut outline) = outlines.get_mut(entity) {
            outline.visible = true;
            outline.colour = outline_selected;
            outline.width = 4.0;
        }
    }

    for entity in removed_selected.read() {
        if let Ok((_, kind, _, has_hovered, has_army_highlighted, has_mesh)) =
            all_entities.get(entity)
        {
            if has_hovered || has_army_highlighted {
                if has_mesh {
                    if let Some(mat) = cache.materials_hovered.get(kind) {
                        commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                    }
                }
                if let Ok(mut outline) = outlines.get_mut(entity) {
                    outline.visible = true;
                    outline.colour = outline_hovered;
                    outline.width = 3.0;
                }
            } else {
                if has_mesh {
                    if let Some(mat) = cache.materials_default.get(kind) {
                        commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                    }
                }
                if let Ok(mut outline) = outlines.get_mut(entity) {
                    outline.visible = false;
                }
            }
        }
    }

    for (entity, kind, has_mesh) in &added_hovered {
        if let Ok((_, _, has_selected, _, _, _)) = all_entities.get(entity) {
            if !has_selected {
                if has_mesh {
                    if let Some(mat) = cache.materials_hovered.get(kind) {
                        commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                    }
                }
                if let Ok(mut outline) = outlines.get_mut(entity) {
                    outline.visible = true;
                    outline.colour = outline_hovered;
                    outline.width = 3.0;
                }
            }
        }
    }

    for (entity, kind, has_mesh) in &added_army_highlighted {
        if let Ok((_, _, has_selected, _, _, _)) = all_entities.get(entity) {
            if !has_selected {
                if has_mesh {
                    if let Some(mat) = cache.materials_hovered.get(kind) {
                        commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                    }
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
        if let Ok((_, kind, has_selected, _, has_army_highlighted, has_mesh)) =
            all_entities.get(entity)
        {
            if !has_selected {
                if has_army_highlighted {
                    if has_mesh {
                        if let Some(mat) = cache.materials_hovered.get(kind) {
                            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                        }
                    }
                    if let Ok(mut outline) = outlines.get_mut(entity) {
                        outline.visible = true;
                        outline.colour = outline_hovered;
                        outline.width = 3.0;
                    }
                } else {
                    if has_mesh {
                        if let Some(mat) = cache.materials_default.get(kind) {
                            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                        }
                    }
                    if let Ok(mut outline) = outlines.get_mut(entity) {
                        outline.visible = false;
                    }
                }
            }
        }
    }

    for entity in removed_army_highlighted.read() {
        if let Ok((_, kind, has_selected, has_hovered, _, has_mesh)) = all_entities.get(entity) {
            if !has_selected {
                if has_hovered {
                    if has_mesh {
                        if let Some(mat) = cache.materials_hovered.get(kind) {
                            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                        }
                    }
                    if let Ok(mut outline) = outlines.get_mut(entity) {
                        outline.visible = true;
                        outline.colour = outline_hovered;
                        outline.width = 3.0;
                    }
                } else {
                    if has_mesh {
                        if let Some(mat) = cache.materials_default.get(kind) {
                            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                        }
                    }
                    if let Ok(mut outline) = outlines.get_mut(entity) {
                        outline.visible = false;
                    }
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

fn handle_right_click_move(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_task_id: ResMut<NextTaskId>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
    ),
    selected_units: Query<
        (Entity, &EntityKind, &Faction, &Transform),
        (With<Unit>, With<Selected>),
    >,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    formation: Res<ActiveFormation>,
    pickables: Query<(Entity, &GlobalTransform, &PickRadius, &InheritedVisibility)>,
    target_queries: (
        Query<Entity, With<Mob>>,
        Query<Entity, With<ResourceNode>>,
        Query<(Entity, &GlobalTransform), (With<Building>, With<ConstructionProgress>)>,
        Query<(Entity, &ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    ),
    // Queries for contextual right-click: enemy units, all units with faction, buildings with faction
    enemy_detect: (
        Query<(Entity, &Faction), (With<Unit>, Without<Selected>)>,
        Query<(Entity, &Faction, &BuildingState), (With<Building>, Without<ConstructionProgress>)>,
    ),
    picking_extra: (
        Query<&AssignedWorkers>,
        Query<(&BuildingFootprint, &BuildingHeight), With<Building>>,
        Res<HeightMap>,
    ),
    ui_flags: (
        Res<MinimapInteraction>,
        Res<UiClickedThisFrame>,
        Res<UiPressActive>,
        bevy::ecs::message::MessageWriter<PlaySfx>,
    ),
    net_params: (
        Res<NetRole>,
        Option<Res<ClientNetState>>,
        Option<Res<HostNetState>>,
        Res<BlueprintRegistry>,
        Option<Res<EntityNetMap>>,
        Res<Time>,
        Query<&mut UnitState>,
        Query<(&mut TrainingQueue, &EntityKind, Option<&BuildingLevel>), With<Building>>,
        Query<&GlobalTransform>,
        Option<ResMut<bevy_matchbox::prelude::MatchboxSocket>>,
    ),
) {
    let (camera_q, windows) = viewport;
    let (mobs, resource_nodes, construction_q, processor_buildings) =
        target_queries;
    let (other_units, other_buildings) = enemy_detect;
    let (assigned_workers_q, building_aabb_q, height_map) = picking_extra;
    let (minimap_interaction, ui_clicked, ui_press, mut sfx) = ui_flags;
    let (
        net_role,
        client_net,
        host_net,
        registry,
        net_map,
        time,
        mut unit_states_q,
        mut training_buildings,
        all_transforms,
        mut matchbox_socket,
    ) = net_params;

    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    if minimap_interaction.clicked || ui_clicked.0 > 0 || ui_press.0 {
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

    let units_vec: Vec<(Entity, EntityKind)> = selected_units
        .iter()
        .filter(|(_, _, faction, _)| **faction == active_player.0)
        .map(|(e, k, _, _)| (e, *k))
        .collect();
    if units_vec.is_empty() {
        return;
    }

    // Contextual right-click action types
    #[derive(Clone, Copy, PartialEq)]
    enum RClickAction {
        AttackEnemy,     // enemy unit or mob
        GatherResource,  // resource node (workers)
        AssistBuild,     // construction site (workers)
        AssignProcessor, // processor building (workers)
        MoveToAlly,      // allied building — just move near it
    }

    // Find ALL hits and evaluate by depth + priority
    struct RClickHit {
        entity: Entity,
        dist: f32,
        action: RClickAction,
    }

    let mut hits: Vec<RClickHit> = Vec::new();
    for (entity, gt, pick_r, inherited_vis) in &pickables {
        if !inherited_vis.get() {
            continue;
        }

        let center = gt.translation();
        let dist = if let Ok((footprint, bld_h)) = building_aabb_q.get(entity) {
            let terrain_y = height_map.sample(center.x, center.z);
            ray_aabb_dist(&ray, center, footprint.0, terrain_y, terrain_y + bld_h.0)
        } else {
            ray_sphere_dist(&ray, center, pick_r.0)
        };
        let Some(dist) = dist else {
            continue;
        };

        // Determine the best action for this hit
        let action = if mobs.contains(entity) {
            // Mobs are always hostile
            Some(RClickAction::AttackEnemy)
        } else if let Ok((_, unit_faction)) = other_units.get(entity) {
            // Another unit — attack if hostile, ignore if allied (can't interact)
            if teams.is_hostile(&active_player.0, unit_faction) {
                Some(RClickAction::AttackEnemy)
            } else {
                None // Allied unit — no right-click action
            }
        } else if construction_q.contains(entity) {
            Some(RClickAction::AssistBuild)
        } else if processor_buildings.contains(entity) {
            // Check if it's our processor or enemy building
            if let Ok((_, _, _, proc_faction)) = processor_buildings.get(entity) {
                if teams.is_hostile(&active_player.0, proc_faction) {
                    Some(RClickAction::AttackEnemy)
                } else {
                    Some(RClickAction::AssignProcessor)
                }
            } else {
                None
            }
        } else if resource_nodes.contains(entity) {
            Some(RClickAction::GatherResource)
        } else if let Ok((_, bld_faction, bld_state)) = other_buildings.get(entity) {
            // Completed building — attack if hostile, move-to if allied
            if *bld_state != BuildingState::Complete {
                None
            } else if teams.is_hostile(&active_player.0, bld_faction) {
                Some(RClickAction::AttackEnemy)
            } else {
                Some(RClickAction::MoveToAlly)
            }
        } else {
            None
        };

        if let Some(action) = action {
            hits.push(RClickHit {
                entity,
                dist,
                action,
            });
        }
    }

    // Sort hits by depth
    hits.sort_by(|a, b| {
        a.dist
            .partial_cmp(&b.dist)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let target_action = if !hits.is_empty() {
        let closest_dist = hits[0].dist;
        let threshold = closest_dist + 2.0;
        let close_hits: Vec<_> = hits.into_iter().filter(|h| h.dist <= threshold).collect();

        // Priority tie-breaker: attack enemy > resource > construction > processor > ally
        if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::AttackEnemy)
        {
            Some((h.entity, h.action))
        }  else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::GatherResource)
        {
            Some((h.entity, h.action))
        } else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::AssistBuild)
        {
            Some((h.entity, h.action))
        } else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::AssignProcessor)
        {
            Some((h.entity, h.action))
        } else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::MoveToAlly)
        {
            Some((h.entity, h.action))
        } else {
            None
        }
    } else {
        None
    };

    let ground_point = ray
        .intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
        .map(|dist| ray.get_point(dist));

    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let input_player_id = client_net
        .as_ref()
        .map(|client| client.player_id as u32)
        .unwrap_or(0);

    // Build one network input packet for right-click actions supported by current relay path.
    let make_network_input = || -> Option<PlayerInput> {
        let net_map = net_map.as_ref()?;
        let entity_ids: Vec<u32> = units_vec
            .iter()
            .filter_map(|(entity, _)| net_map.to_net.get(entity).copied())
            .collect();
        if entity_ids.is_empty() {
            return None;
        }

        let mut commands = Vec::new();
        match target_action {
            Some((target_entity, RClickAction::AttackEnemy)) => {
                let target_id = *net_map.to_net.get(&target_entity)?;
                commands.push(InputCommand::Attack { target_id });
            }
            Some((target_entity, RClickAction::GatherResource)) => {
                let target_id = *net_map.to_net.get(&target_entity)?;
                commands.push(InputCommand::Gather { target_id });
            }
            Some(_) => {
                // Unsupported complex right-click action for network relay yet.
                return None;
            }
            None => {
                let point = ground_point?;
                commands.push(InputCommand::Move {
                    target: [point.x, point.y, point.z],
                    formation: Some(formation.formation.to_net_u8()),
                });
            }
        }

        Some(PlayerInput {
            player_id: input_player_id,
            tick: 0,
            entity_ids,
            commands,
        })
    };

    if *net_role == NetRole::Client {
        if let (Some(client), Some(ref mut socket)) =
            (client_net.as_ref(), matchbox_socket.as_mut())
        {
            if let Some(input) = make_network_input() {
                let seq = {
                    let mut s = client.seq.lock().unwrap();
                    *s += 1;
                    *s
                };
                let msg = ClientMessage::Input {
                    seq,
                    timestamp: time.elapsed_secs_f64(),
                    input,
                };
                crate::multiplayer::matchbox_transport::send_to_host(socket, &msg);
            }
        }
        // Client no longer mutates local gameplay state directly; host relays supported inputs.
        return;
    }

    if *net_role == NetRole::Host {
        if let Some(host) = host_net.as_ref() {
            if let Some(input) = make_network_input() {
                let seq = {
                    let mut s = host.seq.lock().unwrap();
                    *s += 1;
                    *s
                };
                let relay = ServerMessage::RelayedInput {
                    seq,
                    timestamp: time.elapsed_secs_f64(),
                    player_id: 0,
                    input: input.clone(),
                };
                if let Some(ref mut socket) = matchbox_socket {
                    crate::multiplayer::matchbox_transport::broadcast_reliable(socket, &relay);
                }

                // Execute locally through the same code path as client commands,
                // so host and clients have identical component setup.
                if let Some(ref nm) = net_map {
                    execute_input_command(
                        &mut commands,
                        &input,
                        time.elapsed_secs_f64(),
                        nm,
                        &mut unit_states_q,
                        &mut task_queues,
                        &mut training_buildings,
                        &mut next_task_id,
                        &all_transforms,
                        &registry,
                    );
                }
                return;
            }
        }
    }

    if let Some((target_entity, action)) = target_action {
        match action {
            RClickAction::AttackEnemy => {
                for (entity, _kind) in &units_vec {
                    if shift {
                        enqueue_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *entity,
                            QueuedTask::Attack(target_entity),
                        );
                    } else {
                        apply_manual_attack_intent(
                            &mut commands,
                            *entity,
                            target_entity,
                            time.elapsed_secs_f64(),
                        );
                        let mut ec = commands.entity(*entity);
                        ec.remove::<MoveTarget>()
                            .insert(AttackTarget(target_entity))
                            .insert(UnitState::Attacking(target_entity));
                        set_current_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *entity,
                            QueuedTask::Attack(target_entity),
                        );
                    }
                }
                sfx.write(PlaySfx { kind: SfxKind::UnitAttack, position: None });
            }
            RClickAction::AssignProcessor => {
                if let Ok((_, processor, state, proc_faction)) =
                    processor_buildings.get(target_entity)
                {
                    if *state == BuildingState::Complete
                        && *proc_faction == active_player.0
                        && processor.max_workers > 0
                    {
                        let current_count = assigned_workers_q
                            .get(target_entity)
                            .map(|aw| aw.workers.len())
                            .unwrap_or(0);
                        let mut assigned = 0;
                        for (entity, kind) in &units_vec {
                            if *kind == EntityKind::Worker
                                && current_count + assigned < processor.max_workers as usize
                            {
                                if shift {
                                    enqueue_task(
                                        &mut task_queues,
                                        &mut next_task_id,
                                        *entity,
                                        QueuedTask::AssignToProcessor(target_entity),
                                    );
                                } else {
                                    if let Ok(gt) =
                                        pickables.get(target_entity).map(|(_, gt, _, _)| gt)
                                    {
                                        clear_combat_intent(
                                            &mut commands,
                                            *entity,
                                            time.elapsed_secs_f64(),
                                        );
                                        commands
                                            .entity(*entity)
                                            .remove::<AttackTarget>()
                                            .insert(MoveTarget(gt.translation()))
                                            .insert(UnitState::AssignedGathering {
                                                building: target_entity,
                                                phase: AssignedPhase::SeekingNode,
                                            })
                                            .insert(BuildingAssignment(target_entity))
                                            .insert(TaskSource::Manual);
                                        set_current_task(
                                            &mut task_queues,
                                            &mut next_task_id,
                                            *entity,
                                            QueuedTask::AssignToProcessor(target_entity),
                                        );
                                    }
                                }
                                assigned += 1;
                            } else if let Ok(gt) =
                                pickables.get(target_entity).map(|(_, gt, _, _)| gt)
                            {
                                apply_manual_move_intent(
                                    &mut commands,
                                    *entity,
                                    gt.translation(),
                                    time.elapsed_secs_f64(),
                                );
                                commands
                                    .entity(*entity)
                                    .remove::<AttackTarget>()
                                    .insert(MoveTarget(gt.translation()))
                                    .insert(UnitState::Moving(gt.translation()))
                                    .insert(TaskSource::Manual);
                            }
                        }
                    }
                }
            }
            RClickAction::AssistBuild => {
                let construction_pos = pickables
                    .get(target_entity)
                    .map(|(_, gt, _, _)| gt.translation());
                for (entity, kind) in &units_vec {
                    if *kind == EntityKind::Worker {
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(target_entity),
                            );
                        } else {
                            clear_combat_intent(
                                &mut commands,
                                *entity,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .remove::<MoveTarget>()
                                .insert(UnitState::MovingToBuild(target_entity))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(target_entity),
                            );
                        }
                    } else if let Ok(pos) = construction_pos {
                        apply_manual_move_intent(
                            &mut commands,
                            *entity,
                            pos,
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(pos))
                            .insert(UnitState::Moving(pos))
                            .insert(TaskSource::Manual);
                    }
                }
            }
            RClickAction::GatherResource => {
                for (entity, kind) in &units_vec {
                    if *kind == EntityKind::Worker {
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Gather(target_entity),
                            );
                        } else {
                            if let Ok(gt) = pickables.get(target_entity).map(|(_, gt, _, _)| gt) {
                                clear_combat_intent(
                                    &mut commands,
                                    *entity,
                                    time.elapsed_secs_f64(),
                                );
                                commands
                                    .entity(*entity)
                                    .remove::<AttackTarget>()
                                    .insert(MoveTarget(gt.translation()))
                                    .insert(UnitState::Gathering(target_entity))
                                    .insert(TaskSource::Manual);
                                set_current_task(
                                    &mut task_queues,
                                    &mut next_task_id,
                                    *entity,
                                    QueuedTask::Gather(target_entity),
                                );
                            }
                        }
                    } else if let Ok(gt) = pickables.get(target_entity).map(|(_, gt, _, _)| gt) {
                        apply_manual_move_intent(
                            &mut commands,
                            *entity,
                            gt.translation(),
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(gt.translation()))
                            .insert(UnitState::Moving(gt.translation()))
                            .insert(TaskSource::Manual);
                    }
                }
            }
            RClickAction::MoveToAlly => {
                // Move units toward the allied building
                if let Ok(gt) = pickables.get(target_entity).map(|(_, gt, _, _)| gt) {
                    let pos = gt.translation();
                    let n = units_vec.len();
                    let spacing = 1.5;
                    let radius = if n > 1 {
                        (spacing * n as f32 / std::f32::consts::TAU).max(1.0)
                    } else {
                        0.0
                    };
                    for (i, (entity, _kind)) in units_vec.iter().enumerate() {
                        let offset = if n > 1 {
                            let angle = i as f32 / n as f32 * std::f32::consts::TAU;
                            Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                        } else {
                            Vec3::ZERO
                        };
                        let dest = pos + offset;
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        } else {
                            apply_manual_move_intent(
                                &mut commands,
                                *entity,
                                dest,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .insert(MoveTarget(dest))
                                .insert(UnitState::Moving(dest))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        }
                    }
                }
            }
        }
    } else {
        if let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) {
            let point = ray.get_point(dist);

            // Ground fallback check for clicking slightly outside construction bounds
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
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(site_entity),
                            );
                        } else {
                            clear_combat_intent(
                                &mut commands,
                                *entity,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .remove::<MoveTarget>()
                                .insert(UnitState::MovingToBuild(site_entity))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(site_entity),
                            );
                        }
                    } else {
                        apply_manual_move_intent(
                            &mut commands,
                            *entity,
                            point,
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(point))
                            .insert(UnitState::Moving(point))
                            .insert(TaskSource::Manual);
                    }
                }
            } else {
                // Ground move
                let n = units_vec.len();
                if n == 1 {
                    let (ent, _kind) = &units_vec[0];
                    if shift {
                        enqueue_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *ent,
                            QueuedTask::Move(point),
                        );
                    } else {
                        apply_manual_move_intent(
                            &mut commands,
                            *ent,
                            point,
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*ent)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(point))
                            .insert(UnitState::Moving(point))
                            .insert(TaskSource::Manual);
                        set_current_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *ent,
                            QueuedTask::Move(point),
                        );
                    }
                } else if n > 1 {
                    // Calculate centroid of selected units
                    let centroid = selected_units
                        .iter()
                        .filter(|(_, _, f, _)| **f == active_player.0)
                        .map(|(_, _, _, tf)| tf.translation)
                        .fold(Vec3::ZERO, |a, b| a + b)
                        / n as f32;
                    let facing =
                        Vec2::new(point.x - centroid.x, point.z - centroid.z).normalize_or_zero();

                    let offsets = formation_offsets(formation.formation, n, facing);

                    for (i, (entity, _kind)) in units_vec.iter().enumerate() {
                        let off = offsets.get(i).copied().unwrap_or(Vec2::ZERO);
                        let dest = point + Vec3::new(off.x, 0.0, off.y);
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        } else {
                            apply_manual_move_intent(
                                &mut commands,
                                *entity,
                                dest,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .insert(MoveTarget(dest))
                                .insert(UnitState::Moving(dest))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        }
                    }
                }
                sfx.write(PlaySfx { kind: SfxKind::UnitMove, position: Some(point) });
            }
        }
    }
}

/// Hotkey-based unit commands:
/// - `A` → enter attack-move mode (next left-click issues attack-move)
/// - `P` → enter patrol mode (next left-click issues patrol to position)
/// - `H` → hold position (instant, clears move/attack targets)
/// - `S` → stop (instant, clears all orders)
/// - `X` → scuttle selected workers (instant self-destruct)
/// - `Escape` → cancel command mode
///
/// In attack-move/patrol mode, left-click on ground executes the command.
fn handle_unit_command_hotkeys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cmd_mode: ResMut<CommandMode>,
    mut next_task_id: ResMut<NextTaskId>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    selected_units: Query<(Entity, &EntityKind, &Faction), (With<Unit>, With<Selected>)>,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    mut unit_abilities: Query<&mut UnitAbilities>,
    active_player: Res<ActivePlayer>,
    mut formation: ResMut<ActiveFormation>,
    time: Res<Time>,
    ui_clicked: Res<UiClickedThisFrame>,
    ui_press: Res<UiPressActive>,
    placement: Res<BuildingPlacementState>,
) {
    let (ref camera_q, ref windows, ref graphics) = viewport;
    if placement.mode != PlacementMode::None {
        return;
    }

    let has_selected = selected_units.iter().any(|(_, _, f)| *f == active_player.0);

    // Escape cancels command mode
    if keys.just_pressed(KeyCode::Escape) {
        *cmd_mode = CommandMode::Normal;
        return;
    }

    if has_selected {
        // --- 1. Instant Commands (Hold & Stop) ---
        if keys.just_pressed(KeyCode::KeyH) {
            for (entity, _, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                apply_manual_hold_intent(&mut commands, entity, time.elapsed_secs_f64());
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>()
                    .insert(UnitState::HoldPosition)
                    .insert(TaskSource::Manual);
                set_current_task(
                    &mut task_queues,
                    &mut next_task_id,
                    entity,
                    QueuedTask::HoldPosition,
                );
            }
            *cmd_mode = CommandMode::Normal;
            return;
        }

        if keys.just_pressed(KeyCode::KeyX) {
            for (entity, _kind, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>()
                    .insert(UnitState::Idle)
                    .insert(TaskSource::Auto);
                clear_task_queue(&mut task_queues, entity);
            }
            *cmd_mode = CommandMode::Normal;
            return;
        }

        // --- 2. Stance Cycle (V) ---
        if keys.just_pressed(KeyCode::KeyV) {
            for (entity, _, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                commands
                    .entity(entity)
                    .entry::<UnitStance>()
                    .and_modify(|mut stance| {
                        *stance = stance.cycle();
                    });
            }
            return;
        }

        // --- Formation toggle (G) ---
        if keys.just_pressed(KeyCode::KeyG) {
            formation.formation = formation.formation.cycle();
            return;
        }

        // --- 3. Enter Command Modes ---
        if keys.just_pressed(KeyCode::KeyF) {
            *cmd_mode = CommandMode::AttackMove;
            return;
        }

        if keys.just_pressed(KeyCode::KeyP) {
            *cmd_mode = CommandMode::Patrol;
            return;
        }

        // --- Ability hotkeys (Q = first ability, W = second ability) ---
        let ability_hotkey = if keys.just_pressed(KeyCode::KeyQ) {
            Some(0usize)
        } else if keys.just_pressed(KeyCode::KeyW) {
            Some(1usize)
        } else {
            None
        };
        if let Some(slot) = ability_hotkey {
            // Find first selected unit with abilities
            for (entity, _kind, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                if let Ok(abilities) = unit_abilities.get(entity) {
                    if let Some(&ability) = abilities.abilities.get(slot) {
                        if ability.targeting() == AbilityTargeting::NoTarget {
                            // Execute immediately
                            if let Ok(mut ab) = unit_abilities.get_mut(entity) {
                                if ab.is_ready(ability) {
                                    ab.trigger_cooldown(ability);
                                    commands.entity(entity).insert(CastingAbility {
                                        ability,
                                        target_pos: None,
                                        target_entity: None,
                                        cast_timer: Timer::from_seconds(0.3, TimerMode::Once),
                                    });
                                }
                            }
                        } else {
                            *cmd_mode = CommandMode::AbilityTarget(ability);
                        }
                        return;
                    }
                }
            }
        }
    }

    // --- 3. Execution (Left-Click in Command Mode) ---
    // Execution does NOT require Alt to be held, only that we are already in a mode.
    if *cmd_mode == CommandMode::Normal || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if ui_clicked.0 > 0 || ui_press.0 {
        return;
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
    let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };
    let point = ray.get_point(dist);

    let units_vec: Vec<(Entity, EntityKind)> = selected_units
        .iter()
        .filter(|(_, _, f)| **f == active_player.0)
        .map(|(e, k, _)| (e, *k))
        .collect();

    if units_vec.is_empty() {
        *cmd_mode = CommandMode::Normal;
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    match *cmd_mode {
        CommandMode::AttackMove => {
            let n = units_vec.len();
            let spacing = 1.5;
            let radius = if n > 1 {
                (spacing * n as f32 / std::f32::consts::TAU).max(1.0)
            } else {
                0.0
            };
            for (i, (entity, _kind)) in units_vec.iter().enumerate() {
                let offset = if n > 1 {
                    let angle = i as f32 / n as f32 * std::f32::consts::TAU;
                    Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                } else {
                    Vec3::ZERO
                };
                let dest = point + offset;
                if shift {
                    enqueue_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::AttackMove(dest),
                    );
                } else {
                    apply_manual_attack_move_intent(
                        &mut commands,
                        *entity,
                        dest,
                        time.elapsed_secs_f64(),
                    );
                    commands
                        .entity(*entity)
                        .remove::<AttackTarget>()
                        .insert(MoveTarget(dest))
                        .insert(UnitState::AttackMoving(dest))
                        .insert(TaskSource::Manual);
                    set_current_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::AttackMove(dest),
                    );
                }
            }
        }
        CommandMode::Patrol => {
            for (entity, _kind) in &units_vec {
                if shift {
                    enqueue_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::Patrol(point),
                    );
                } else {
                    clear_combat_intent(
                        &mut commands,
                        *entity,
                        time.elapsed_secs_f64(),
                    );
                    commands
                        .entity(*entity)
                        .remove::<AttackTarget>()
                        .insert(MoveTarget(point))
                        .insert(UnitState::Patrolling {
                            target: point,
                            origin: point,
                        })
                        .insert(TaskSource::Manual);
                    set_current_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::Patrol(point),
                    );
                }
            }
        }
        CommandMode::AbilityTarget(ability) => {
            for (entity, _kind) in &units_vec {
                if let Ok(mut abilities) = unit_abilities.get_mut(*entity) {
                    if abilities.is_ready(ability) {
                        abilities.trigger_cooldown(ability);
                        commands.entity(*entity).insert(CastingAbility {
                            ability,
                            target_pos: Some(point),
                            target_entity: None,
                            cast_timer: Timer::from_seconds(0.3, TimerMode::Once),
                        });
                    }
                }
            }
        }
        CommandMode::Normal => {}
    }

    *cmd_mode = CommandMode::Normal;
}

// ── Tab Subgroup Cycling ──

fn tab_subgroup_cycle_system(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut subgroup: ResMut<SubgroupCycleState>,
    selected: Query<(Entity, &EntityKind), (With<Unit>, With<Selected>)>,
    all_unit_kinds: Query<&EntityKind, With<Unit>>,
) {
    if !keys.just_pressed(KeyCode::Tab) {
        return;
    }
    if selected.is_empty() {
        return;
    }

    if !subgroup.active {
        // Entering subgroup mode: snapshot selection and build kind list
        let mut kinds_set: Vec<EntityKind> = Vec::new();
        subgroup.original_selection.clear();
        for (entity, kind) in &selected {
            subgroup.original_selection.push(entity);
            if !kinds_set.contains(kind) {
                kinds_set.push(*kind);
            }
        }
        // Only cycle if there are multiple types
        if kinds_set.len() <= 1 {
            return;
        }
        kinds_set.sort_by_key(|k| format!("{:?}", k));
        subgroup.subgroup_kinds = kinds_set;
        subgroup.current_index = 0;
        subgroup.active = true;
    } else {
        // Advance to next subgroup
        subgroup.current_index += 1;
        if subgroup.current_index >= subgroup.subgroup_kinds.len() {
            // Wrapped around: restore full selection and deactivate
            subgroup.current_index = 0;
            subgroup.active = false;

            // Restore original selection
            for (entity, _) in &selected {
                commands.entity(entity).remove::<Selected>();
            }
            for &entity in &subgroup.original_selection {
                if all_unit_kinds.get(entity).is_ok() {
                    commands.entity(entity).try_insert(Selected);
                }
            }
            return;
        }
    }

    // Select only units of the current subgroup kind from the original selection
    let target_kind = subgroup.subgroup_kinds[subgroup.current_index];
    // Deselect all
    for (entity, _) in &selected {
        commands.entity(entity).remove::<Selected>();
    }
    // Select matching from original
    for &entity in &subgroup.original_selection {
        if let Ok(kind) = all_unit_kinds.get(entity) {
            if *kind == target_kind {
                commands.entity(entity).try_insert(Selected);
            }
        }
    }
}

fn reset_subgroup_on_selection_change(
    mut subgroup: ResMut<SubgroupCycleState>,
    selected: Query<Entity, (With<Unit>, With<Selected>)>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if !subgroup.active {
        return;
    }
    // Don't reset if Tab was just pressed (we just changed it)
    if keys.just_pressed(KeyCode::Tab) {
        return;
    }

    // Check if selection changed externally (click, box select, group recall)
    let current: Vec<Entity> = selected.iter().collect();
    let expected_kind = &subgroup.subgroup_kinds.get(subgroup.current_index);
    if expected_kind.is_none() {
        subgroup.active = false;
        subgroup.original_selection.clear();
        return;
    }

    // If any selected entity is not in the original selection, reset
    let any_external = current
        .iter()
        .any(|e| !subgroup.original_selection.contains(e));
    if any_external {
        subgroup.active = false;
        subgroup.original_selection.clear();
    }
}
