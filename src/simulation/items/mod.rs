pub mod components;
pub mod messages;
pub mod registry;
pub mod vfx;

use bevy::math::primitives::Rectangle;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use std::f32::consts::PI;

use crate::blueprints::EntityKind;
use crate::simulation::combat::{apply_manual_move_intent, clear_combat_intent};
use crate::types::{
    AllyNotifications, AllyNotifyKind, AppState, AttackTarget, Faction, GameFlowSet, Hovered,
    MoveTarget, PickRadius, RtsCamera, TaskSource, Unit, UnitState,
};
use crate::simulation::selection::SelectionSet;

pub use components::*;
pub use messages::*;
pub use registry::*;

#[derive(Resource)]
pub struct ItemAssets {
    pub padded_vest: Handle<Image>,
    pub bronze_cuirass: Handle<Image>,
    pub plate_cuirass: Handle<Image>,
    pub crusader_helm: Handle<Image>,
    pub kettle_helm: Handle<Image>,
    pub viking_helm: Handle<Image>,
    pub jewel_ring: Handle<Image>,
    pub plain_band: Handle<Image>,
    pub wedding_band: Handle<Image>,
    pub golden_band: Handle<Image>,
    pub twin_rings: Handle<Image>,
    pub linked_rings: Handle<Image>,
    pub arming_sword: Handle<Image>,
    pub viking_blade: Handle<Image>,
    pub battle_staff: Handle<Image>,
    pub mage_crozier: Handle<Image>,
    pub yew_longbow: Handle<Image>,
    pub war_bow: Handle<Image>,
}

impl ItemAssets {
    pub fn icon(&self, item: ItemKind) -> Handle<Image> {
        match item {
            ItemKind::PaddedVest => self.padded_vest.clone(),
            ItemKind::BronzeCuirass => self.bronze_cuirass.clone(),
            ItemKind::PlateCuirass => self.plate_cuirass.clone(),
            ItemKind::CrusaderHelm => self.crusader_helm.clone(),
            ItemKind::KettleHelm => self.kettle_helm.clone(),
            ItemKind::VikingHelm => self.viking_helm.clone(),
            ItemKind::JewelRing => self.jewel_ring.clone(),
            ItemKind::PlainBand => self.plain_band.clone(),
            ItemKind::WeddingBand => self.wedding_band.clone(),
            ItemKind::GoldenBand => self.golden_band.clone(),
            ItemKind::TwinRings => self.twin_rings.clone(),
            ItemKind::LinkedRings => self.linked_rings.clone(),
            ItemKind::ArmingSword => self.arming_sword.clone(),
            ItemKind::VikingBlade => self.viking_blade.clone(),
            ItemKind::BattleStaff => self.battle_staff.clone(),
            ItemKind::MageCrozier => self.mage_crozier.clone(),
            ItemKind::YewLongbow => self.yew_longbow.clone(),
            ItemKind::WarBow => self.war_bow.clone(),
        }
    }
}

#[derive(Resource)]
struct PickupVisualAssets {
    icon_quad: Handle<Mesh>,
    stem_quad: Handle<Mesh>,
    ring_mesh: Handle<Mesh>,
    orb_mesh: Handle<Mesh>,
    halo_ring_mesh: Handle<Mesh>,
    aura_ring_mesh: Handle<Mesh>,
}

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>().clone();
        app.insert_resource(load_item_assets(&asset_server))
            .init_resource::<ItemRegistry>()
            .add_message::<SpawnItemPickup>()
            .add_message::<RequestPickupItem>()
            .add_message::<RequestDropItem>()
            .add_message::<InventoryChanged>()
            .add_message::<ItemPickupCollected>()
            .add_message::<ItemPickupFailed>()
            .add_message::<vfx::ItemVfxTrigger>()
            .add_systems(
                Startup,
                (
                    register_item_definitions,
                    setup_pickup_visual_assets,
                    vfx::setup_item_vfx_assets,
                ),
            )
            .add_systems(
                Update,
                (
                    ensure_unit_inventories,
                    refresh_item_runtime_state,
                    vfx::sync_item_vfx,
                    vfx::update_item_vfx_conditions,
                    vfx::spawn_triggered_item_vfx,
                    spawn_pickup_entities,
                    resolve_pending_item_pickups,
                    collect_item_pickups,
                    drop_inventory_items,
                    tick_pickup_collect_vfx,
                    despawn_expired_pickups,
                    notify_pickup_failures,
                )
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    animate_pickup_bob,
                    face_pickup_billboards,
                    animate_pickup_visuals,
                    vfx::animate_persistent_item_vfx,
                    vfx::animate_item_vfx_flashes,
                    vfx::animate_item_vfx_trails,
                )
                    .after(SelectionSet)
                    .in_set(GameFlowSet::Presentation)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn load_item_assets(asset_server: &AssetServer) -> ItemAssets {
    ItemAssets {
        padded_vest: asset_server.load("icons/items/armor/padded_vest.png"),
        bronze_cuirass: asset_server.load("icons/items/armor/bronze_cuirass.png"),
        plate_cuirass: asset_server.load("icons/items/armor/plate_cuirass.png"),
        crusader_helm: asset_server.load("icons/items/helmet/crusader_helm.png"),
        kettle_helm: asset_server.load("icons/items/helmet/kettle_helm.png"),
        viking_helm: asset_server.load("icons/items/helmet/viking_helm.png"),
        jewel_ring: asset_server.load("icons/items/ring/jewel_ring.png"),
        plain_band: asset_server.load("icons/items/ring/plain_band.png"),
        wedding_band: asset_server.load("icons/items/ring/wedding_band.png"),
        golden_band: asset_server.load("icons/items/ring/golden_band.png"),
        twin_rings: asset_server.load("icons/items/ring/twin_rings.png"),
        linked_rings: asset_server.load("icons/items/ring/linked_rings.png"),
        arming_sword: asset_server.load("icons/items/sword/arming_sword.png"),
        viking_blade: asset_server.load("icons/items/sword/viking_blade.png"),
        battle_staff: asset_server.load("icons/items/staff/battle_staff.png"),
        mage_crozier: asset_server.load("icons/items/staff/mage_crozier.png"),
        yew_longbow: asset_server.load("icons/items/bow/yew_longbow.png"),
        war_bow: asset_server.load("icons/items/bow/war_bow.png"),
    }
}

fn setup_pickup_visual_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(PickupVisualAssets {
        icon_quad: meshes.add(Rectangle::new(0.85, 0.85)),
        stem_quad: meshes.add(Rectangle::new(0.16, 1.65)),
        ring_mesh: meshes.add(Torus::new(0.56, 0.06)),
        orb_mesh: meshes.add(Sphere::new(0.16)),
        halo_ring_mesh: meshes.add(Torus::new(0.42, 0.035)),
        aura_ring_mesh: meshes.add(Torus::new(1.18, 0.045)),
    });
}

fn ensure_unit_inventories(
    mut commands: Commands,
    units: Query<(Entity, &EntityKind), (With<Unit>, Without<UnitInventory>)>,
) {
    for (entity, kind) in &units {
        let capacity = inferred_inventory_capacity(*kind);
        commands.entity(entity).insert((
            UnitInventory {
                capacity,
                items: Vec::new(),
            },
            ItemRuntimeState::default(),
        ));
    }
}

fn refresh_item_runtime_state(
    registry: Res<ItemRegistry>,
    mut units: Query<(&EntityKind, &UnitInventory, &mut ItemRuntimeState), With<Unit>>,
) {
    for (kind, inventory, mut runtime) in &mut units {
        runtime.items.clear();
        for &item in inventory.items.iter().take(inventory.capacity as usize) {
            let def = registry.get(item);
            let missing_requirement = def
                .requirements
                .iter()
                .copied()
                .find(|req| !registry::unit_meets_requirement(*kind, *req));
            runtime.items.push(ItemStateEntry {
                item,
                enabled: missing_requirement.is_none(),
                disabled_reason: missing_requirement.map(|_| ItemDisabledReason::MissingRequirement),
                cooldown_remaining: 0.0,
                active_toggled: false,
            });
        }
    }
}

fn spawn_pickup_entities(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut reader: MessageReader<SpawnItemPickup>,
    item_assets: Res<ItemAssets>,
    pickup_visuals: Res<PickupVisualAssets>,
    registry: Res<ItemRegistry>,
) {
    for msg in reader.read() {
        let def = registry.get(msg.item);
        let icon_material = materials.add(StandardMaterial {
            base_color_texture: Some(item_assets.icon(msg.item)),
            base_color: Color::WHITE,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        let beam_material = materials.add(StandardMaterial {
            base_color: def.beam_color.with_alpha(0.42),
            emissive: def.beam_color.to_linear() * 2.2,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        let aura_material = materials.add(StandardMaterial {
            base_color: def.beam_color.with_alpha(0.18),
            emissive: def.beam_color.to_linear() * 0.95,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        let ring_material = materials.add(StandardMaterial {
            base_color: def.beam_color.with_alpha(0.92),
            emissive: def.beam_color.to_linear() * 2.6,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        let root = commands
            .spawn((
                ItemPickup {
                    item: msg.item,
                    owner: msg.owner,
                    expires_at: time.elapsed_secs() + msg.lifetime_secs,
                },
                ItemPickupLabel {
                    name: msg.item.display_name().to_string(),
                    extra: msg.item.effect_summary().to_string(),
                    color: def.beam_color,
                },
                PickupBob {
                    base_y: msg.position.y + 0.9,
                    phase: msg.position.x * 0.33 + msg.position.z * 0.21,
                },
                PickRadius(0.8),
                Transform::from_translation(msg.position + Vec3::Y * 0.9),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
            .id();

        let icon_child = commands
            .spawn((
                PickupBillboard,
                Mesh3d(pickup_visuals.icon_quad.clone()),
                MeshMaterial3d(icon_material),
                Transform {
                    translation: Vec3::new(0.0, 0.18, 0.03),
                    rotation: Quat::from_rotation_y(PI),
                    scale: Vec3::splat(1.2),
                },
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();

        let beam_child = commands
            .spawn((
                PickupStem,
                Mesh3d(pickup_visuals.stem_quad.clone()),
                MeshMaterial3d(beam_material),
                Transform::from_xyz(0.0, 0.35, -0.02),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();

        let aura_child = commands
            .spawn((
                PickupBackdrop,
                Mesh3d(pickup_visuals.icon_quad.clone()),
                MeshMaterial3d(aura_material),
                Transform {
                    translation: Vec3::new(0.0, 0.14, 0.0),
                    rotation: Quat::from_rotation_y(PI),
                    scale: Vec3::new(1.55, 1.55, 1.0),
                },
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();

        let ring_child = commands
            .spawn((
                PickupAuraRing,
                Mesh3d(pickup_visuals.ring_mesh.clone()),
                MeshMaterial3d(ring_material),
                Transform::from_xyz(0.0, -0.22, 0.0)
                    .with_rotation(Quat::from_rotation_x(PI * 0.5)),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();

        commands.entity(root).add_child(icon_child);
        commands.entity(root).add_child(beam_child);
        commands.entity(root).add_child(aura_child);
        commands.entity(root).add_child(ring_child);
    }
}



fn pickup_failure_for_unit(
    pickup: &ItemPickup,
    pickup_tf: &Transform,
    radius: &PickRadius,
    unit_tf: &Transform,
    kind: EntityKind,
    inventory: &UnitInventory,
) -> Option<ItemPickupFailureReason> {
    if inventory.capacity == 0 {
        return Some(ItemPickupFailureReason::NoInventorySlots);
    }
    // Use XZ distance only — the pickup floats above ground so 3D distance
    // would unfairly penalize ground-level units.
    let dx = unit_tf.translation.x - pickup_tf.translation.x;
    let dz = unit_tf.translation.z - pickup_tf.translation.z;
    let xz_dist = (dx * dx + dz * dz).sqrt();
    if xz_dist > radius.0 + 1.25 {
        return Some(ItemPickupFailureReason::TooFar);
    }
    if inventory.items.len() >= inventory.capacity as usize {
        return Some(ItemPickupFailureReason::InventoryFull);
    }
    if inventory
        .items
        .iter()
        .any(|existing| existing.category() == pickup.item.category())
    {
        return Some(ItemPickupFailureReason::CategoryConflict);
    }

    let allowed = match pickup.item.category() {
        ItemCategory::Bow => matches!(kind, EntityKind::Archer | EntityKind::Scout),
        ItemCategory::Staff => matches!(kind, EntityKind::Mage | EntityKind::Priest),
        ItemCategory::Sword => matches!(
            kind,
            EntityKind::Soldier | EntityKind::Tank | EntityKind::Knight | EntityKind::Cavalry
        ),
        ItemCategory::Armor | ItemCategory::Helmet => kind != EntityKind::Worker,
        ItemCategory::Ring => true,
    };

    if !allowed {
        return Some(ItemPickupFailureReason::WrongUnitType);
    }

    None
}

struct PickupCandidate {
    entity: Entity,
    distance_sq: f32,
    failure: Option<ItemPickupFailureReason>,
    can_pick_if_close: bool,
}

fn best_unit_for_pickup(candidates: &[PickupCandidate]) -> Result<Entity, ItemPickupFailureReason> {
    if candidates.is_empty() {
        return Err(ItemPickupFailureReason::NoSelectedUnits);
    }

    if let Some(best) = candidates
        .iter()
        .filter(|candidate| candidate.failure.is_none())
        .min_by(|a, b| a.distance_sq.total_cmp(&b.distance_sq))
    {
        return Ok(best.entity);
    }

    Err(candidates
        .iter()
        .filter_map(|candidate| candidate.failure)
        .min_by_key(|reason| reason.priority())
        .unwrap_or(ItemPickupFailureReason::NoSelectedUnits))
}

fn best_unit_to_approach(candidates: &[PickupCandidate]) -> Option<Entity> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate.can_pick_if_close
                && candidate.failure == Some(ItemPickupFailureReason::TooFar)
        })
        .min_by(|a, b| a.distance_sq.total_cmp(&b.distance_sq))
        .map(|candidate| candidate.entity)
}

fn can_pickup_if_close(
    pickup: &ItemPickup,
    kind: EntityKind,
    inventory: &UnitInventory,
) -> bool {
    if inventory.capacity == 0 {
        return false;
    }
    if inventory.items.len() >= inventory.capacity as usize {
        return false;
    }
    if inventory
        .items
        .iter()
        .any(|existing| existing.category() == pickup.item.category())
    {
        return false;
    }

    match pickup.item.category() {
        ItemCategory::Bow => matches!(kind, EntityKind::Archer | EntityKind::Scout),
        ItemCategory::Staff => matches!(kind, EntityKind::Mage | EntityKind::Priest),
        ItemCategory::Sword => matches!(
            kind,
            EntityKind::Soldier | EntityKind::Tank | EntityKind::Knight | EntityKind::Cavalry
        ),
        ItemCategory::Armor | ItemCategory::Helmet => kind != EntityKind::Worker,
        ItemCategory::Ring => true,
    }
}

fn collect_pickup_for_unit(
    commands: &mut Commands,
    changed: &mut MessageWriter<InventoryChanged>,
    collected: &mut MessageWriter<ItemPickupCollected>,
    unit: Entity,
    pickup_entity: Entity,
    item: ItemKind,
    inventory: &mut UnitInventory,
) {
    inventory.items.push(item);
    changed.write(InventoryChanged { unit });
    collected.write(ItemPickupCollected {
        pickup: pickup_entity,
        collector: unit,
        item,
    });
    commands.entity(unit).remove::<PendingItemPickup>();
    commands.entity(pickup_entity).insert(PickupCollectVfx {
        timer: Timer::from_seconds(0.18, TimerMode::Once),
    });
}

fn collect_item_pickups(
    mut commands: Commands,
    mut requests: MessageReader<RequestPickupItem>,
    mut changed: MessageWriter<InventoryChanged>,
    mut collected: MessageWriter<ItemPickupCollected>,
    mut failed: MessageWriter<ItemPickupFailed>,
    pickups: Query<(&ItemPickup, &Transform, &PickRadius)>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &EntityKind,
            &mut UnitInventory,
            &mut UnitState,
            &mut TaskSource,
        ),
        With<Unit>,
    >,
) {
    for msg in requests.read() {
        let Ok((pickup, pickup_tf, radius)) = pickups.get(msg.pickup) else {
            continue;
        };

        let mut candidates = Vec::with_capacity(msg.pickers.len());
        for &picker in &msg.pickers {
            let Ok((_, unit_tf, kind, inventory, _, _)) = units.get_mut(picker) else {
                continue;
            };
            let dx = unit_tf.translation.x - pickup_tf.translation.x;
            let dz = unit_tf.translation.z - pickup_tf.translation.z;
            candidates.push(PickupCandidate {
                entity: picker,
                distance_sq: dx * dx + dz * dz,
                failure: pickup_failure_for_unit(
                    pickup,
                    pickup_tf,
                    radius,
                    unit_tf,
                    *kind,
                    &inventory,
                ),
                can_pick_if_close: can_pickup_if_close(pickup, *kind, &inventory),
            });
        }

        match best_unit_for_pickup(&candidates) {
            Ok(best_picker) => {
                let Ok((_, _, _, mut inventory, _, _)) = units.get_mut(best_picker) else {
                    continue;
                };
                collect_pickup_for_unit(
                    &mut commands,
                    &mut changed,
                    &mut collected,
                    best_picker,
                    msg.pickup,
                    pickup.item,
                    &mut inventory,
                );
            }
            Err(reason) => {
                if let Some(best_picker) = best_unit_to_approach(&candidates) {
                    let Ok((unit, _, _, _, mut state, mut source)) = units.get_mut(best_picker)
                    else {
                        continue;
                    };
                    // Project to ground level so pathfinding works correctly
                    let ground_target = Vec3::new(
                        pickup_tf.translation.x,
                        0.0,
                        pickup_tf.translation.z,
                    );
                    clear_combat_intent(&mut commands, unit, 0.0);
                    apply_manual_move_intent(
                        &mut commands,
                        unit,
                        ground_target,
                        0.0,
                    );
                    commands.entity(unit).insert((
                        PendingItemPickup { pickup: msg.pickup },
                        MoveTarget(ground_target),
                        UnitState::Moving(ground_target),
                    ));
                    commands.entity(unit).remove::<AttackTarget>();
                    *state = UnitState::Moving(ground_target);
                    *source = TaskSource::Manual;
                    continue;
                }

                failed.write(ItemPickupFailed {
                    pickup: msg.pickup,
                    item: pickup.item,
                    reason,
                });
            }
        }
    }
}

fn resolve_pending_item_pickups(
    mut commands: Commands,
    time: Res<Time>,
    mut changed: MessageWriter<InventoryChanged>,
    mut collected: MessageWriter<ItemPickupCollected>,
    mut failed: MessageWriter<ItemPickupFailed>,
    pickups: Query<(&ItemPickup, &Transform, &PickRadius)>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &EntityKind,
            &mut UnitInventory,
            &mut UnitState,
            &mut TaskSource,
            Option<&MoveTarget>,
            &PendingItemPickup,
        ),
        With<Unit>,
    >,
) {
    for (unit, unit_tf, kind, mut inventory, mut state, mut source, move_target, pending) in
        &mut units
    {
        let Ok((pickup, pickup_tf, radius)) = pickups.get(pending.pickup) else {
            commands.entity(unit).remove::<PendingItemPickup>();
            continue;
        };

        match pickup_failure_for_unit(pickup, pickup_tf, radius, unit_tf, *kind, &inventory) {
            None => {
                collect_pickup_for_unit(
                    &mut commands,
                    &mut changed,
                    &mut collected,
                    unit,
                    pending.pickup,
                    pickup.item,
                    &mut inventory,
                );
                commands.entity(unit).remove::<MoveTarget>();
                *state = UnitState::Idle;
                *source = TaskSource::Auto;
            }
            Some(ItemPickupFailureReason::TooFar) => {
                let ground_target = Vec3::new(
                    pickup_tf.translation.x,
                    0.0,
                    pickup_tf.translation.z,
                );
                if move_target.map_or(true, |target| {
                    let dx = target.0.x - ground_target.x;
                    let dz = target.0.z - ground_target.z;
                    dx * dx + dz * dz > 0.25
                }) {
                    clear_combat_intent(&mut commands, unit, time.elapsed_secs_f64());
                    apply_manual_move_intent(
                        &mut commands,
                        unit,
                        ground_target,
                        time.elapsed_secs_f64(),
                    );
                    commands.entity(unit).insert(MoveTarget(ground_target));
                    commands.entity(unit).remove::<AttackTarget>();
                    *state = UnitState::Moving(ground_target);
                    *source = TaskSource::Manual;
                }
            }
            Some(reason) => {
                failed.write(ItemPickupFailed {
                    pickup: pending.pickup,
                    item: pickup.item,
                    reason,
                });
                commands.entity(unit).remove::<PendingItemPickup>();
            }
        }
    }
}

fn animate_pickup_visuals(
    time: Res<Time>,
    pickup_roots: Query<Has<Hovered>, With<ItemPickup>>,
    mut backdrops: Query<(&ChildOf, &mut Transform), (With<PickupBackdrop>, Without<PickupStem>, Without<PickupAuraRing>)>,
    mut stems: Query<(&ChildOf, &mut Transform), (With<PickupStem>, Without<PickupBackdrop>, Without<PickupAuraRing>)>,
    mut rings: Query<(&ChildOf, &mut Transform), (With<PickupAuraRing>, Without<PickupBackdrop>, Without<PickupStem>)>,
) {
    let t = time.elapsed_secs();

    for (parent, mut transform) in &mut backdrops {
        let is_hovered = pickup_roots.get(parent.parent()).unwrap_or(false);
        let hover_scale = if is_hovered { 1.8 } else { 1.55 };
        transform.translation.y = 0.14 + (t * 1.5).sin() * 0.02;
        transform.scale = Vec3::new(hover_scale, hover_scale, 1.0);
    }

    for (parent, mut transform) in &mut stems {
        let is_hovered = pickup_roots.get(parent.parent()).unwrap_or(false);
        transform.translation.y = 0.35;
        transform.scale = Vec3::new(1.0, if is_hovered { 1.08 } else { 1.0 }, 1.0);
    }

    for (parent, mut transform) in &mut rings {
        let is_hovered = pickup_roots.get(parent.parent()).unwrap_or(false);
        let ring_scale = if is_hovered { 1.18 } else { 1.0 };
        transform.rotation =
            Quat::from_rotation_y(-t * 0.9) * Quat::from_rotation_x(PI * 0.5);
        transform.scale = Vec3::new(ring_scale, ring_scale, 1.0);
    }
}

fn animate_pickup_bob(
    time: Res<Time>,
    mut pickups: Query<(&PickupBob, &mut Transform), With<ItemPickup>>,
) {
    for (bob, mut transform) in &mut pickups {
        transform.translation.y = bob.base_y + (time.elapsed_secs() * 1.8 + bob.phase).sin() * 0.08;
    }
}


fn face_pickup_billboards(
    camera_q: Query<&GlobalTransform, With<RtsCamera>>,
    pickup_roots: Query<&GlobalTransform, With<ItemPickup>>,
    mut billboards: Query<
        (&ChildOf, &mut Transform, Has<PickupStem>),
        Or<(With<PickupBillboard>, With<PickupBackdrop>, With<PickupStem>)>,
    >,
) {
    let Ok(camera_tf) = camera_q.single() else {
        return;
    };
    for (parent, mut transform, is_stem) in &mut billboards {
        let Ok(root_tf) = pickup_roots.get(parent.parent()) else {
            continue;
        };
        let to_camera = camera_tf.translation() - root_tf.translation();
        let yaw = to_camera.x.atan2(to_camera.z);
        if is_stem {
            // Stem is a vertical quad — keep it upright but face the camera
            transform.rotation = Quat::from_rotation_y(yaw);
        } else {
            transform.rotation = Quat::from_rotation_y(yaw);
        }
    }
}

fn tick_pickup_collect_vfx(
    mut commands: Commands,
    time: Res<Time>,
    mut pickups: Query<(Entity, &mut PickupCollectVfx, Option<&Children>)>,
    mut child_transforms: Query<&mut Transform>,
) {
    for (entity, mut vfx, children) in &mut pickups {
        vfx.timer.tick(time.delta());
        let pct = 1.0 - vfx.timer.fraction_remaining();
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut tf) = child_transforms.get_mut(child) {
                    tf.scale = Vec3::splat(1.0 + pct * 0.45);
                }
            }
        }
        if vfx.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

fn despawn_expired_pickups(
    mut commands: Commands,
    time: Res<Time>,
    pickups: Query<(Entity, &ItemPickup), Without<PickupCollectVfx>>,
) {
    for (entity, pickup) in &pickups {
        if time.elapsed_secs() >= pickup.expires_at {
            commands.entity(entity).try_despawn();
        }
    }
}

fn drop_inventory_items(
    mut drop_requests: MessageReader<RequestDropItem>,
    mut pickup_spawns: MessageWriter<SpawnItemPickup>,
    mut changed: MessageWriter<InventoryChanged>,
    units: Query<(&Transform, Option<&Faction>)>,
    mut inventories: Query<&mut UnitInventory, With<Unit>>,
) {
    for msg in drop_requests.read() {
        let Ok(mut inventory) = inventories.get_mut(msg.unit) else {
            continue;
        };
        if msg.slot >= inventory.items.len() {
            continue;
        }
        let item = inventory.items.remove(msg.slot);
        let Ok((transform, faction)) = units.get(msg.unit) else {
            continue;
        };
        pickup_spawns.write(SpawnItemPickup {
            item,
            position: transform.translation + Vec3::new(0.45, 0.0, 0.35),
            owner: faction.copied(),
            lifetime_secs: 45.0,
        });
        changed.write(InventoryChanged { unit: msg.unit });
    }
}

fn notify_pickup_failures(
    time: Res<Time>,
    mut notifications: ResMut<AllyNotifications>,
    mut failures: MessageReader<ItemPickupFailed>,
    pickups: Query<&Transform, With<ItemPickup>>,
) {
    for failure in failures.read() {
        let world_pos = pickups.get(failure.pickup).ok().map(|tf| tf.translation);
        notifications.push(
            AllyNotifyKind::ItemPickupFail,
            failure.reason.label().to_string(),
            world_pos,
            time.elapsed_secs(),
        );
    }
}
