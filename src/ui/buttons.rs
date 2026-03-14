use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::buildings;
use crate::components::*;
use crate::theme;

// ── Build button handler ──

pub fn handle_build_buttons(
    interactions: Query<
        (
            Entity,
            &Interaction,
            &BuildButton,
            Option<&super::actions_widget::BuildGridButton>,
        ),
        Changed<Interaction>,
    >,
    mut placement: ResMut<BuildingPlacementState>,
    all_completed: Res<AllCompletedBuildings>,
    base_state: Res<FactionBaseState>,
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for (_entity, interaction, build_btn, _grid_btn) in &interactions {
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            let kind = build_btn.0;
            let founded = base_state.is_founded(&active_player.0);

            if kind == EntityKind::Base && !founded {
                let player_res = all_resources.get(&active_player.0);
                let carried = carried_totals.get(&active_player.0);
                let bp = registry.get(kind);
                if !bp.cost.can_afford_with_carried(player_res, carried) {
                    continue;
                }

                placement.mode = PlacementMode::PlotBase;
                placement.awaiting_release = true;
                placement.hint_text = Some("Plot your first Base".to_string());
                continue;
            }

            let bp = registry.get(kind);
            let prereq_met = if let Some(ref bd) = bp.building {
                match bd.prerequisite {
                    None => true,
                    Some(prereq_kind) => {
                        if prereq_kind == EntityKind::Base {
                            founded || all_completed.has(&active_player.0, prereq_kind)
                        } else {
                            all_completed.has(&active_player.0, prereq_kind)
                        }
                    }
                }
            } else {
                true
            };
            if !prereq_met {
                continue;
            }

            if kind == EntityKind::WallSegment && founded {
                placement.mode = PlacementMode::PlotWall { start: Vec3::ZERO };
                placement.awaiting_release = false;
                placement.hint_text = Some("Click ground to start wall".to_string());
                continue;
            }

            if kind == EntityKind::Gatehouse && founded {
                placement.mode = PlacementMode::PlotGate;
                placement.awaiting_release = false;
                placement.hint_text = Some("Hover an owned wall segment to place gate".to_string());
                continue;
            }

            let player_res = all_resources.get(&active_player.0);
            let carried = carried_totals.get(&active_player.0);
            if !bp.cost.can_afford_with_carried(player_res, carried) {
                continue;
            }

            placement.mode = PlacementMode::Placing(kind);
            placement.awaiting_release = true;
            placement.hint_text = None;
        }
    }
}

// ── Train button handler ──

pub fn handle_train_buttons(
    interactions: Query<(&Interaction, &TrainButton), Changed<Interaction>>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut queues: Query<&mut TrainingQueue>,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, train_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        let kind = train_btn.0;
        let bp = registry.get(kind);
        let player_res = all_resources.get(&active_player.0);
        let carried = carried_totals.get(&active_player.0);
        if !bp.cost.can_afford_with_carried(player_res, carried) {
            continue;
        }

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queues.get_mut(building_entity) {
                let player_res_mut = all_resources.get_mut(&active_player.0);
                let (dw, dc, di, dg, do_) = bp.cost.deduct_with_carried(player_res_mut);
                let drain = SpendFromCarried {
                    faction: active_player.0,
                    amounts: [dw, dc, di, dg, do_],
                };
                if drain.has_deficit() {
                    pending_drains.drains.push(drain);
                }
                queue.queue.push(kind);
                break;
            }
        }
    }
}

// ── Upgrade button handler ──

pub fn handle_upgrade_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<UpgradeButton>)>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    selected_buildings: Query<
        (
            Entity,
            &EntityKind,
            &BuildingLevel,
            &BuildingState,
            &Faction,
        ),
        (With<Building>, With<Selected>, Without<UpgradeProgress>),
    >,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if let Ok((entity, kind, level, state, faction)) = selected_buildings.single() {
            if *state != BuildingState::Complete {
                continue;
            }
            let carried = carried_totals.get(faction);
            let player_res = all_resources.get_mut(&active_player.0);
            buildings::start_upgrade(
                &mut commands,
                entity,
                level.0,
                *kind,
                &registry,
                player_res,
                *faction,
                carried,
                &mut pending_drains,
            );
        }
    }
}

// ── Demolish button handler ──

pub fn handle_demolish_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<DemolishButton>)>,
    action_bar: Query<Entity, With<ActionBarInner>>,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingState, &Transform),
        (With<Building>, With<Selected>),
    >,
    registry: Res<BlueprintRegistry>,
    existing_confirm: Query<Entity, With<DemolishConfirmPanel>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if !existing_confirm.is_empty() {
            continue;
        }

        let Ok(bar_entity) = action_bar.single() else {
            continue;
        };

        if let Ok((_entity, kind, state, _transform)) = selected_buildings.single() {
            let bp = registry.get(*kind);
            let refund_pct = if *state == BuildingState::Complete {
                50
            } else {
                100
            };
            let refund_str = format!(
                "Refunds ~{}% (W:{} C:{})",
                refund_pct,
                bp.cost.wood * refund_pct as u32 / 100,
                bp.cost.copper * refund_pct as u32 / 100,
            );

            let panel = commands
                .spawn((
                    DemolishConfirmPanel,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Percent(100.0),
                        left: Val::Px(0.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(theme::BG_PANEL),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Demolish?"),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(theme::DESTRUCTIVE),
                    ));
                    panel.spawn((
                        Text::new(refund_str),
                        TextFont {
                            font_size: theme::FONT_SMALL,
                            ..default()
                        },
                        TextColor(theme::TEXT_SECONDARY),
                    ));
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(6.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Button,
                                StandardButton,
                                ConfirmDemolishButton,
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                    border_radius: BorderRadius::all(Val::Px(4.0)),
                                    ..default()
                                },
                                BackgroundColor(theme::DESTRUCTIVE),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Yes"),
                                    TextFont {
                                        font_size: theme::FONT_BODY,
                                        ..default()
                                    },
                                    TextColor(theme::TEXT_PRIMARY),
                                ));
                            });

                            row.spawn((
                                Button,
                                StandardButton,
                                CancelDemolishButton,
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                    border_radius: BorderRadius::all(Val::Px(4.0)),
                                    ..default()
                                },
                                BackgroundColor(theme::BTN_PRIMARY),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("No"),
                                    TextFont {
                                        font_size: theme::FONT_BODY,
                                        ..default()
                                    },
                                    TextColor(theme::TEXT_PRIMARY),
                                ));
                            });
                        });
                })
                .id();
            commands.entity(bar_entity).add_child(panel);
        }
    }
}

pub fn handle_demolish_confirm(
    mut commands: Commands,
    confirm_interactions: Query<&Interaction, (Changed<Interaction>, With<ConfirmDemolishButton>)>,
    cancel_interactions: Query<&Interaction, (Changed<Interaction>, With<CancelDemolishButton>)>,
    selected_buildings: Query<
        (Entity, &Transform, &BuildingState),
        (With<Building>, With<Selected>),
    >,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    registry: Res<BlueprintRegistry>,
    building_kinds: Query<&EntityKind, With<Building>>,
    confirm_panels: Query<Entity, With<DemolishConfirmPanel>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &confirm_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if let Ok((entity, transform, state)) = selected_buildings.single() {
            if *state == BuildingState::UnderConstruction {
                if let Ok(kind) = building_kinds.get(entity) {
                    let bp = registry.get(*kind);
                    let player_res = all_resources.get_mut(&active_player.0);
                    let refund = [
                        bp.cost.wood,
                        bp.cost.copper,
                        bp.cost.iron,
                        bp.cost.gold,
                        bp.cost.oil,
                    ];
                    for (i, &amt) in refund.iter().enumerate() {
                        player_res.amounts[i] += amt;
                    }
                }
                commands.entity(entity).try_despawn();
            } else {
                buildings::start_demolish(&mut commands, entity, transform);
            }
        }

        for panel in &confirm_panels {
            commands.entity(panel).try_despawn();
        }
    }

    for interaction in &cancel_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        for panel in &confirm_panels {
            commands.entity(panel).try_despawn();
        }
    }
}

pub fn handle_scuttle_unit_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ScuttleUnitButton>)>,
    selected_units: Query<(Entity, &EntityKind, &Faction), (With<Unit>, With<Selected>)>,
    active_player: Res<ActivePlayer>,
    mut health_q: Query<&mut Health, With<Unit>>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        *cmd_mode = CommandMode::Normal;

        for (entity, kind, faction) in &selected_units {
            if *faction != active_player.0 || *kind != EntityKind::Worker {
                continue;
            }
            if let Ok(mut hp) = health_q.get_mut(entity) {
                hp.current = 0.0;
            }
        }
    }
}

// ── Rally point button handler ──

pub fn handle_rally_point_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<RallyPointButton>)>,
    mut rally_mode: ResMut<RallyPointMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            ui_press.0 = true;
            rally_mode.0 = !rally_mode.0;
        }
    }

    if rally_mode.0 && mouse.just_pressed(MouseButton::Left) {
        let Ok(window) = windows.single() else { return };
        let Some(cursor) = window.cursor_position() else {
            return;
        };
        let Ok((camera, cam_gt)) = camera_q.single() else {
            return;
        };
        let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
            return;
        };
        let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
            return;
        };
        let world_pos = ray.get_point(dist);

        for entity in &selected_buildings {
            commands.entity(entity).insert(RallyPoint(world_pos));
        }
        rally_mode.0 = false;
    }

    if rally_mode.0 && mouse.just_pressed(MouseButton::Right) {
        rally_mode.0 = false;
    }
}

// ── Toggle auto-attack handler ──

pub fn handle_toggle_auto_attack(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ToggleAutoAttackButton>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut auto_attacks: Query<&mut TowerAutoAttackEnabled>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        for entity in &selected_buildings {
            if let Ok(mut aa) = auto_attacks.get_mut(entity) {
                aa.0 = !aa.0;
            }
        }
    }
}

// ── Assign/Unassign worker to processor building ──

pub fn handle_assign_worker_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<AssignWorkerButton>)>,
    selected_buildings: Query<(Entity, &ResourceProcessor), (With<Building>, With<Selected>)>,
    idle_workers: Query<(Entity, &UnitState, &Faction), (With<Unit>, With<GatherSpeed>)>,
    assigned_workers_q: Query<&AssignedWorkers>,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for (building_entity, processor) in &selected_buildings {
            let current_count = assigned_workers_q
                .get(building_entity)
                .map(|aw| aw.workers.len())
                .unwrap_or(0);
            if current_count >= processor.max_workers as usize {
                continue;
            }

            let slots_available = processor.max_workers as usize - current_count;
            let mut assigned = 0;
            for (worker_entity, state, faction) in &idle_workers {
                if *faction != active_player.0 {
                    continue;
                }
                if *state != UnitState::Idle {
                    continue;
                }
                if assigned >= slots_available {
                    break;
                }
                crate::resources::assign_worker_to_processor(
                    &mut commands,
                    worker_entity,
                    building_entity,
                );
                // Also add to AssignedWorkers
                commands
                    .entity(building_entity)
                    .entry::<AssignedWorkers>()
                    .and_modify(move |mut aw| {
                        if !aw.workers.contains(&worker_entity) {
                            aw.workers.push(worker_entity);
                        }
                    })
                    .or_insert(AssignedWorkers {
                        workers: vec![worker_entity],
                    });
                assigned += 1;
            }
        }
    }
}

pub fn handle_unassign_worker_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<UnassignWorkerButton>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    assigned_workers_q: Query<&AssignedWorkers>,
    _unit_states: Query<&UnitState, With<Unit>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for building_entity in &selected_buildings {
            if let Ok(aw) = assigned_workers_q.get(building_entity) {
                let workers_to_unassign: Vec<Entity> = aw.workers.clone();
                for worker_entity in workers_to_unassign {
                    crate::resources::unassign_worker_from_processor(&mut commands, worker_entity);
                }
                // Clear the building's assigned workers list
                commands
                    .entity(building_entity)
                    .entry::<AssignedWorkers>()
                    .and_modify(|mut aw| {
                        aw.workers.clear();
                    });
            }
        }
    }
}

pub fn handle_unassign_specific_worker_button(
    mut commands: Commands,
    interactions: Query<(&Interaction, &UnassignSpecificWorkerButton), Changed<Interaction>>,
    mut assigned_workers_q: Query<&mut AssignedWorkers>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        let worker = btn.0;
        crate::resources::unassign_worker_from_processor(&mut commands, worker);

        // Remove from all buildings' AssignedWorkers
        for mut aw in &mut assigned_workers_q {
            aw.workers.retain(|&w| w != worker);
        }
    }
}

// ── Cancel training queue item handler ──

pub fn handle_cancel_train(
    interactions: Query<(&Interaction, &CancelTrainButton), Changed<Interaction>>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut queues: Query<&mut TrainingQueue>,
    registry: Res<BlueprintRegistry>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, cancel_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queues.get_mut(building_entity) {
                let idx = cancel_btn.0;
                if idx < queue.queue.len() {
                    let removed_kind = queue.queue.remove(idx);
                    let bp = registry.get(removed_kind);
                    let player_res = all_resources.get_mut(&active_player.0);
                    let refund = [
                        bp.cost.wood,
                        bp.cost.copper,
                        bp.cost.iron,
                        bp.cost.gold,
                        bp.cost.oil,
                    ];
                    for (i, &amt) in refund.iter().enumerate() {
                        player_res.amounts[i] += amt;
                    }
                    if idx == 0 {
                        queue.timer = None;
                    }
                }
                break;
            }
        }
    }
}

// ── Unit card click ──

pub fn handle_unit_card_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &UnitCardRef), (Changed<Interaction>, With<Button>)>,
    selected: Query<Entity, With<Selected>>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, card_ref) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_press.0 = true;
        for entity in &selected {
            commands.entity(entity).remove::<Selected>();
        }
        commands.entity(card_ref.0).insert(Selected);
    }
}

// ── Button hover visuals ──

pub fn button_hover_visual(
    mut query: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            Option<&mut BorderColor>,
            Has<UnitCardRef>,
        ),
        (
            Changed<Interaction>,
            With<StandardButton>,
            Without<ButtonAnimState>,
        ),
    >,
) {
    for (interaction, mut bg, border_color, is_mini_card) in &mut query {
        if is_mini_card {
            if let Some(mut bc) = border_color {
                match interaction {
                    Interaction::Hovered => {
                        *bg = BackgroundColor(theme::BG_ELEVATED);
                        *bc = BorderColor::all(Color::srgba(0.29, 0.62, 1.0, 0.5));
                    }
                    Interaction::Pressed => {
                        *bg = BackgroundColor(theme::BTN_PRESSED);
                        *bc = BorderColor::all(theme::ACCENT);
                    }
                    Interaction::None => {
                        *bg = BackgroundColor(theme::BG_SURFACE);
                        *bc = BorderColor::all(Color::NONE);
                    }
                }
            }
        } else {
            *bg = match interaction {
                Interaction::Pressed => BackgroundColor(theme::BTN_PRESSED),
                Interaction::Hovered => BackgroundColor(theme::BTN_HOVER),
                Interaction::None => BackgroundColor(theme::BTN_PRIMARY),
            };
        }
    }
}

/// Smooth lerp-based button animation
pub fn animated_button_hover_system(
    time: Res<Time>,
    mut query: Query<(
        &Interaction,
        &mut ButtonAnimState,
        &ButtonStyle,
        &mut BackgroundColor,
        &mut Transform,
    )>,
) {
    let dt = time.delta_secs();
    let speed = 14.0_f32;
    let alpha = 1.0 - (-speed * dt).exp();

    for (interaction, mut anim, style, mut bg, mut transform) in &mut query {
        match interaction {
            Interaction::Hovered => {
                anim.scale_target = 1.04;
                match style {
                    ButtonStyle::Filled => {
                        anim.bg_target = [0.25, 0.25, 0.25, 0.94];
                    }
                    ButtonStyle::Ghost => {
                        anim.bg_target = [0.29, 0.62, 1.0, 0.08];
                    }
                    ButtonStyle::Destructive => {
                        anim.bg_target = [0.80, 0.27, 0.27, 0.08];
                    }
                }
            }
            Interaction::Pressed => {
                anim.scale_target = 0.96;
                match style {
                    ButtonStyle::Filled => {
                        anim.bg_target = [0.12, 0.12, 0.12, 0.94];
                    }
                    ButtonStyle::Ghost => {
                        anim.bg_target = [0.29, 0.62, 1.0, 0.14];
                    }
                    ButtonStyle::Destructive => {
                        anim.bg_target = [0.80, 0.27, 0.27, 0.14];
                    }
                }
            }
            Interaction::None => {
                anim.scale_target = 1.0;
                match style {
                    ButtonStyle::Filled => {
                        anim.bg_target = [0.17, 0.17, 0.17, 0.94];
                    }
                    ButtonStyle::Ghost | ButtonStyle::Destructive => {
                        anim.bg_target = [0.0, 0.0, 0.0, 0.0];
                    }
                }
            }
        }

        for i in 0..4 {
            anim.bg_current[i] += (anim.bg_target[i] - anim.bg_current[i]) * alpha;
        }
        anim.scale_current += (anim.scale_target - anim.scale_current) * alpha;

        *bg = BackgroundColor(Color::srgba(
            anim.bg_current[0],
            anim.bg_current[1],
            anim.bg_current[2],
            anim.bg_current[3],
        ));
        transform.scale = Vec3::splat(anim.scale_current);
    }
}

/// Action bar fade-out/fade-in transition system
pub fn action_bar_transition_system(
    mut commands: Commands,
    time: Res<Time>,
    mut fade_outs: Query<(Entity, &mut ActionBarFadeOut, &mut Transform)>,
    mut fade_ins: Query<
        (
            Entity,
            &mut ActionBarFadeIn,
            &mut Transform,
            &mut Visibility,
        ),
        Without<ActionBarFadeOut>,
    >,
) {
    let dt = time.delta_secs();

    for (entity, mut fade, mut transform) in &mut fade_outs {
        fade.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = fade.timer.fraction();
        let scale = 1.0 - t * 0.05;
        transform.scale = Vec3::splat(scale);
        transform.translation.y = -t * 8.0 + fade.initial_offset;

        if fade.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }

    for (entity, mut fade, mut transform, mut vis) in &mut fade_ins {
        fade.delay.tick(std::time::Duration::from_secs_f32(dt));
        if !fade.delay.is_finished() {
            continue;
        }
        if !fade.started {
            fade.started = true;
            *vis = Visibility::Inherited;
        }
        fade.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = fade.timer.fraction();
        let scale = 0.95 + t * 0.05;
        transform.scale = Vec3::splat(scale);
        transform.translation.y = (1.0 - t) * 8.0;

        if fade.timer.is_finished() {
            commands.entity(entity).remove::<ActionBarFadeIn>();
            transform.scale = Vec3::ONE;
            transform.translation.y = 0.0;
        }
    }
}

// ── Live display updates ──

pub fn update_hp_bars(
    mut hp_fills: Query<(&HpBarFill, &mut Node, &mut BackgroundColor)>,
    healths: Query<&Health>,
) {
    for (hp_bar, mut node, mut bg) in &mut hp_fills {
        if let Ok(health) = healths.get(hp_bar.0) {
            let pct = (health.current / health.max).clamp(0.0, 1.0) * 100.0;
            node.width = Val::Percent(pct);
            *bg = BackgroundColor(super::shared::hp_color(health.current, health.max));
        }
    }
}

pub fn update_training_queue_display(
    selected_buildings: Query<&TrainingQueue, (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<TrainingProgressBar>>,
) {
    let Ok(queue) = selected_buildings.single() else {
        return;
    };
    for mut node in &mut progress_bars {
        let fraction = queue.timer.as_ref().map_or(0.0, |t| t.fraction());
        node.width = Val::Percent(fraction * 100.0);
    }
}

pub fn update_train_cost_colors(
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    registry: Res<BlueprintRegistry>,
    mut cost_texts: Query<(&TrainCostText, &mut TextColor)>,
) {
    let player_res = all_resources.get(&active_player.0);
    let carried = carried_totals.get(&active_player.0);
    for (cost_text, mut color) in &mut cost_texts {
        let bp = registry.get(cost_text.kind);
        if bp.cost.can_afford_with_carried(player_res, carried) {
            *color = TextColor(theme::TEXT_SECONDARY);
        } else {
            *color = TextColor(theme::DESTRUCTIVE);
        }
    }
}

pub fn update_construction_progress_display(
    selected_buildings: Query<(Entity, &ConstructionProgress), (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<ConstructionProgressBar>>,
    mut worker_texts: Query<(&mut Text, &mut TextColor), With<ConstructionWorkerCountText>>,
    workers: Query<&UnitState, With<Unit>>,
) {
    let Ok((building_entity, progress)) = selected_buildings.single() else {
        return;
    };
    for mut node in &mut progress_bars {
        node.width = Val::Percent(progress.timer.fraction() * 100.0);
    }

    let builder_count = workers
        .iter()
        .filter(|state| matches!(state, UnitState::Building(e) if *e == building_entity))
        .count();

    for (mut text, mut color) in &mut worker_texts {
        if builder_count == 0 {
            **text = "Waiting for workers...".to_string();
            *color = TextColor(Color::srgb(0.9, 0.5, 0.3));
        } else {
            let pct = (progress.timer.fraction() * 100.0) as u32;
            **text = format!(
                "{}% ({} worker{})",
                pct,
                builder_count,
                if builder_count == 1 { "" } else { "s" }
            );
            *color = TextColor(Color::srgb(0.6, 0.8, 0.5));
        }
    }
}

pub fn update_upgrade_progress_display(
    selected_buildings: Query<&UpgradeProgress, (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<UpgradeProgressBar>>,
) {
    let Ok(progress) = selected_buildings.single() else {
        return;
    };
    for mut node in &mut progress_bars {
        node.width = Val::Percent(progress.timer.fraction() * 100.0);
    }
}

// ── Action button tooltips ──

pub fn show_action_tooltips(
    mut commands: Commands,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    triggers: Query<(Entity, &Interaction, &ActionTooltipTrigger), Changed<Interaction>>,
    existing_tooltips: Query<(Entity, &ActionTooltip)>,
) {
    for (entity, interaction, trigger) in &triggers {
        match interaction {
            Interaction::Hovered => {
                // Check if tooltip already exists for this trigger
                let has_tooltip = existing_tooltips.iter().any(|(_, tt)| tt.owner == entity);
                if has_tooltip {
                    continue;
                }

                // Position near cursor
                let (cx, cy) = windows
                    .single()
                    .ok()
                    .and_then(|w| w.cursor_position())
                    .map(|p| (p.x, p.y))
                    .unwrap_or((0.0, 0.0));
                let scale = ui_scale.0.max(0.001);
                let (ui_w, ui_h) = windows
                    .single()
                    .map(|w| (w.width() / scale, w.height() / scale))
                    .unwrap_or((1920.0 / scale, 1080.0 / scale));
                let left = ((cx + 12.0) / scale).clamp(6.0, (ui_w - 240.0).max(6.0));
                let top = ((cy + 14.0) / scale).clamp(6.0, (ui_h - 140.0).max(6.0));

                commands
                    .spawn((
                        ActionTooltip { owner: entity },
                        Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(left),
                            top: Val::Px(top),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(8.0)),
                            row_gap: Val::Px(2.0),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            max_width: Val::Px(220.0),
                            min_width: Val::Px(140.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.96)),
                        BorderColor::all(Color::srgba(0.25, 0.25, 0.30, 0.6)),
                        BoxShadow::new(
                            Color::srgba(0.0, 0.0, 0.0, 0.6),
                            Val::Px(0.0),
                            Val::Px(4.0),
                            Val::Px(0.0),
                            Val::Px(12.0),
                        ),
                        GlobalZIndex(100),
                    ))
                    .with_children(|tt| {
                        spawn_tooltip_content(tt, &trigger.text);
                    });
            }
            _ => {
                // Remove tooltip owned by this trigger
                for (tooltip_entity, tt) in &existing_tooltips {
                    if tt.owner == entity {
                        commands.entity(tooltip_entity).try_despawn();
                    }
                }
            }
        }
    }
}

/// Clean up orphaned tooltips whose owner trigger no longer exists or is no longer hovered.
pub fn cleanup_action_tooltips(
    mut commands: Commands,
    tooltips: Query<(Entity, &ActionTooltip)>,
    triggers: Query<&Interaction, With<ActionTooltipTrigger>>,
) {
    for (tooltip_entity, tt) in &tooltips {
        let should_remove = match triggers.get(tt.owner) {
            Ok(interaction) => *interaction != Interaction::Hovered,
            Err(_) => true, // owner despawned
        };
        if should_remove {
            commands.entity(tooltip_entity).try_despawn();
        }
    }
}

fn spawn_tooltip_content(tt: &mut ChildSpawnerCommands, text: &str) {
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }

        // First line = title
        if i == 0 {
            tt.spawn((
                Text::new(*line),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(theme::TEXT_PRIMARY),
            ));
            tt.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::axes(Val::Px(0.0), Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.30, 0.30, 0.35, 0.4)),
            ));
            continue;
        }

        let (color, font_size) = if line.starts_with("Not enough") {
            (theme::DESTRUCTIVE, theme::FONT_SMALL)
        } else if line.starts_with("Requires:") {
            (theme::WARNING, theme::FONT_SMALL)
        } else if line.starts_with("Cost:") {
            (theme::TEXT_SECONDARY, theme::FONT_SMALL)
        } else if line.starts_with("HP:") || line.starts_with("DMG:") {
            (theme::STAT_DMG, theme::FONT_SMALL)
        } else if *line == "Drag & Drop to create"
            || *line == "Click to train"
            || *line == "Click to place"
        {
            tt.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::axes(Val::Px(0.0), Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.30, 0.30, 0.35, 0.4)),
            ));
            (Color::srgba(0.45, 0.65, 1.0, 0.7), theme::FONT_CAPTION)
        } else if line.starts_with("Build time:") || line.starts_with("Train:") {
            (theme::TEXT_SECONDARY, theme::FONT_SMALL)
        } else {
            (Color::srgba(0.65, 0.65, 0.65, 0.9), theme::FONT_SMALL)
        };

        tt.spawn((
            Text::new(*line),
            TextFont {
                font_size,
                ..default()
            },
            TextColor(color),
        ));
    }
}

// ── Unit command button handlers ──

pub fn handle_attack_move_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<AttackMoveButton>)>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            ui_press.0 = true;
            *cmd_mode = CommandMode::AttackMove;
        }
    }
}

pub fn handle_patrol_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<PatrolButton>)>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            ui_press.0 = true;
            *cmd_mode = CommandMode::Patrol;
        }
    }
}

pub fn handle_hold_position_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<HoldPositionButton>)>,
    mut commands: Commands,
    selected_units: Query<(Entity, &Faction), (With<Unit>, With<Selected>)>,
    active_player: Res<ActivePlayer>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        *cmd_mode = CommandMode::Normal;
        for (entity, faction) in &selected_units {
            if *faction != active_player.0 {
                continue;
            }
            commands
                .entity(entity)
                .remove::<MoveTarget>()
                .remove::<AttackTarget>()
                .insert(UnitState::HoldPosition)
                .insert(TaskSource::Manual);
            commands
                .entity(entity)
                .entry::<TaskQueue>()
                .and_modify(|mut tq| tq.queue.clear());
        }
    }
}

pub fn handle_stop_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<StopButton>)>,
    mut commands: Commands,
    selected_units: Query<(Entity, &Faction), (With<Unit>, With<Selected>)>,
    active_player: Res<ActivePlayer>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        *cmd_mode = CommandMode::Normal;
        for (entity, faction) in &selected_units {
            if *faction != active_player.0 {
                continue;
            }
            commands
                .entity(entity)
                .remove::<MoveTarget>()
                .remove::<AttackTarget>()
                .insert(UnitState::Idle)
                .insert(TaskSource::Auto);
            commands
                .entity(entity)
                .entry::<TaskQueue>()
                .and_modify(|mut tq| tq.queue.clear());
        }
    }
}

pub fn handle_cycle_stance_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<CycleStanceButton>)>,
    mut commands: Commands,
    selected_units: Query<(Entity, &Faction), (With<Unit>, With<Selected>)>,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        for (entity, faction) in &selected_units {
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
    }
}

pub fn clear_stale_inspected(
    mut inspected: ResMut<InspectedEnemy>,
    mob_query: Query<Entity, With<Mob>>,
    unit_query: Query<Entity, With<Unit>>,
    building_query: Query<Entity, With<Building>>,
) {
    if let Some(e) = inspected.entity {
        let exists =
            mob_query.get(e).is_ok() || unit_query.get(e).is_ok() || building_query.get(e).is_ok();
        if !exists {
            inspected.entity = None;
        }
    }
}
