//! Drag-selection box, click/double-click detection, subgroup cycling,
//! and multi-unit selection logic.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::OutlineVolume;

use crate::blueprints::{EntityKind, EntityVisualCache};
use crate::infrastructure::audio::{PlaySfx, SfxKind};
use crate::presentation::camera;
use crate::presentation::minimap::MinimapInteraction;
use crate::simulation::items::ItemPickup;
use crate::types::*;
use crate::world::ground::HeightMap;

use super::picking::{pick_for_click, PickCycleState};

// ── UI state helpers ──

pub(crate) fn reset_ui_clicked(mut ui_clicked: ResMut<UiClickedThisFrame>) {
    ui_clicked.0 = ui_clicked.0.saturating_sub(1);
}

pub(crate) fn clear_ui_press_on_release(
    mouse: Res<ButtonInput<MouseButton>>,
    mut ui_press: ResMut<UiPressActive>,
) {
    if mouse.just_released(MouseButton::Left) {
        ui_press.0 = false;
    }
}

// ── Selection box ──

pub(crate) fn spawn_selection_box(mut commands: Commands) {
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

pub(crate) fn track_drag(
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

pub(crate) fn update_selection_box_visual(
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

// ── Click / box selection ──

pub(crate) fn handle_click_select(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: (
        ResMut<DragState>,
        ResMut<InspectedEnemy>,
        ResMut<PickCycleState>,
    ),
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
        Query<Entity, With<ItemPickup>>,
    ),
    selection_queries: (
        Query<Entity, With<Selected>>,
        Query<Entity, (With<Selected>, With<Unit>)>,
        Query<&GlobalTransform, With<Unit>>,
    ),
    assigned_workers: Query<(), With<BuildingAssignment>>,
    flags: (
        Res<MinimapInteraction>,
        Res<UiClickedThisFrame>,
        Res<UiPressActive>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    ownership: (Res<ActivePlayer>, Query<&Faction>),
    height_map: Res<HeightMap>,
    mut extra: (
        Res<Time<Real>>,
        ResMut<DoubleClickDetector>,
        Query<&EntityKind>,
        bevy::ecs::message::MessageWriter<PlaySfx>,
    ),
) {
    let (ref camera_q, ref windows, ref graphics) = viewport;
    let (ref mut drag, ref mut inspected, ref mut pick_cycle) = state;
    let (ref units, ref buildings, ref mobs, ref resource_nodes, ref pickups) = entity_queries;
    let (ref selected, ref _selected_units, ref unit_transforms) = selection_queries;
    let (ref minimap_interaction, ref ui_clicked, ref ui_press) = flags;
    let (ref active_player, ref faction_q) = ownership;
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
                for entity in selected.iter() {
                    commands.entity(entity).remove::<Selected>();
                }
            }

            inspected.entity = None;

            for entity in units.iter() {
                if assigned_workers.contains(entity) {
                    continue;
                }
                // Only select units of the active faction
                if let Ok(f) = faction_q.get(entity) {
                    if *f != active_player.0 {
                        continue;
                    }
                }
                // Workers assigned to buildings are now visible, so no need to skip them
                if let Ok(gt) = unit_transforms.get(entity) {
                    if let Some(screen_pos) = camera::world_to_window_viewport(
                        camera,
                        cam_gt,
                        gt.translation(),
                        window,
                        &graphics,
                    ) {
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
        let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics)
        else {
            return;
        };

        let cursor_screen = window.cursor_position();

        // Determine skip entity for click-cycling: if clicking the same spot,
        // deprioritize the entity we picked last time so the next one surfaces.
        let skip = cursor_screen.and_then(|cursor| {
            if pick_cycle.should_cycle(cursor) {
                pick_cycle.last_entity
            } else {
                None
            }
        });

        let pick = pick_for_click(
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
            &assigned_workers,
            &height_map,
            skip,
        );

        if let Some(result) = pick {
            // Update cycle state so next click at same spot picks a different entity
            if let Some(cursor) = cursor_screen {
                pick_cycle.advance(cursor, result.entity);
            }

            if result.is_pickup {
                inspected.entity = Some(result.entity);
            } else if result.is_mob {
                inspected.entity = Some(result.entity);
            } else if result.is_resource {
                // Resources can always be inspected/selected
                inspected.entity = None;
                if !shift {
                    for entity in selected.iter() {
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
                            for entity in selected.iter() {
                                commands.entity(entity).remove::<Selected>();
                            }
                            // Select all own units of same type visible on screen
                            let Ok((camera, cam_gt)) = camera_q.single() else {
                                return;
                            };
                            for entity in units.iter() {
                                if assigned_workers.contains(entity) {
                                    continue;
                                }
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
                            for entity in selected.iter() {
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
            pick_cycle.reset();
            inspected.entity = None;
            if !shift {
                for entity in selected.iter() {
                    commands.entity(entity).remove::<Selected>();
                }
            }
        }
    }
}

// ── Entity visuals (selection/hover outlines + materials) ──

pub(crate) fn update_entity_visuals(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    added_selected: Query<(Entity, &EntityKind, Has<Mesh3d>), Added<Selected>>,
    mut removed_selected: RemovedComponents<Selected>,
    added_hovered: Query<(Entity, &EntityKind, Has<Mesh3d>), Added<Hovered>>,
    mut removed_hovered: RemovedComponents<Hovered>,
    added_army_highlighted: Query<
        (Entity, &EntityKind, Has<Mesh3d>),
        Added<ArmyOverviewHighlighted>,
    >,
    mut removed_army_highlighted: RemovedComponents<ArmyOverviewHighlighted>,
    all_entities: Query<(
        Entity,
        &EntityKind,
        Has<Selected>,
        Has<Hovered>,
        Has<ArmyOverviewHighlighted>,
        Has<Mesh3d>,
    )>,
    #[cfg(not(target_arch = "wasm32"))] mut outlines: Query<&mut OutlineVolume>,
) {
    // Outline colors
    #[cfg(not(target_arch = "wasm32"))]
    let outline_selected = Color::srgb(0.2, 1.0, 0.3);
    #[cfg(not(target_arch = "wasm32"))]
    let outline_hovered = Color::srgb(0.3, 0.8, 1.0);

    macro_rules! set_outline {
        ($entity:expr, visible: $vis:expr $(, colour: $col:expr, width: $w:expr)?) => {
            #[cfg(not(target_arch = "wasm32"))]
            if let Ok(mut outline) = outlines.get_mut($entity) {
                outline.visible = $vis;
                $(outline.colour = $col; outline.width = $w;)?
            }
        };
    }

    for (entity, kind, has_mesh) in &added_selected {
        if has_mesh {
            if let Some(mat) = cache.materials_selected.get(kind) {
                commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
            }
        }
        set_outline!(entity, visible: true, colour: outline_selected, width: 4.0);
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
                set_outline!(entity, visible: true, colour: outline_hovered, width: 3.0);
            } else {
                if has_mesh {
                    if let Some(mat) = cache.materials_default.get(kind) {
                        commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                    }
                }
                set_outline!(entity, visible: false);
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
                set_outline!(entity, visible: true, colour: outline_hovered, width: 3.0);
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
                set_outline!(entity, visible: true, colour: outline_hovered, width: 3.0);
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
                    set_outline!(entity, visible: true, colour: outline_hovered, width: 3.0);
                } else {
                    if has_mesh {
                        if let Some(mat) = cache.materials_default.get(kind) {
                            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                        }
                    }
                    set_outline!(entity, visible: false);
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
                    set_outline!(entity, visible: true, colour: outline_hovered, width: 3.0);
                } else {
                    if has_mesh {
                        if let Some(mat) = cache.materials_default.get(kind) {
                            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                        }
                    }
                    set_outline!(entity, visible: false);
                }
            }
        }
    }
}

// ── Tab subgroup cycling ──

pub(crate) fn tab_subgroup_cycle_system(
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

pub(crate) fn reset_subgroup_on_selection_change(
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
