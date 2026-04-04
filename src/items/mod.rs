pub mod components;
pub mod messages;
pub mod registry;

use bevy::math::primitives::Rectangle;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use std::f32::consts::PI;

use crate::blueprints::EntityKind;
use crate::components::{AppState, GameFlowSet, PickRadius, RtsCamera, Unit};
use crate::selection::SelectionSet;

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
    beam_quad: Handle<Mesh>,
}

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>().clone();
        app.insert_resource(load_item_assets(&asset_server))
            .init_resource::<ItemRegistry>()
            .add_message::<SpawnItemPickup>()
            .add_message::<RequestPickupItem>()
            .add_message::<InventoryChanged>()
            .add_message::<ItemPickupCollected>()
            .add_systems(Startup, (register_item_definitions, setup_pickup_visual_assets))
            .add_systems(
                Update,
                (
                    ensure_unit_inventories,
                    refresh_item_runtime_state,
                    spawn_pickup_entities,
                    collect_item_pickups,
                    tick_pickup_collect_vfx,
                    despawn_expired_pickups,
                )
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    animate_pickup_bob,
                    face_pickup_billboards,
                    animate_pickup_beams,
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
        beam_quad: meshes.add(Rectangle::new(0.18, 2.3)),
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
            base_color: def.beam_color.with_alpha(0.22),
            emissive: def.beam_color.to_linear() * 2.5,
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
                Transform::from_rotation(Quat::from_rotation_y(PI)),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
            .id();

        let beam_child = commands
            .spawn((
                PickupBeam,
                Mesh3d(pickup_visuals.beam_quad.clone()),
                MeshMaterial3d(beam_material),
                Transform::from_xyz(0.0, 0.45, -0.03),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
            .id();

        commands.entity(root).add_child(icon_child);
        commands.entity(root).add_child(beam_child);
    }
}

fn collect_item_pickups(
    mut commands: Commands,
    mut requests: MessageReader<RequestPickupItem>,
    mut changed: MessageWriter<InventoryChanged>,
    mut collected: MessageWriter<ItemPickupCollected>,
    pickups: Query<(&ItemPickup, &Transform, &PickRadius)>,
    mut units: Query<(&Transform, &EntityKind, &mut UnitInventory), With<Unit>>,
) {
    for msg in requests.read() {
        let Ok((pickup, pickup_tf, radius)) = pickups.get(msg.pickup) else {
            continue;
        };
        let Ok((unit_tf, kind, mut inventory)) = units.get_mut(msg.picker) else {
            continue;
        };
        if inventory.capacity == 0 {
            continue;
        }
        if unit_tf.translation.distance(pickup_tf.translation) > radius.0 + 1.25 {
            continue;
        }
        if inventory.items.len() >= inventory.capacity as usize {
            continue;
        }
        if inventory
            .items
            .iter()
            .any(|existing| existing.category() == pickup.item.category())
        {
            continue;
        }
        let allowed = match pickup.item.category() {
            ItemCategory::Bow => matches!(kind, EntityKind::Archer | EntityKind::Scout),
            ItemCategory::Staff => matches!(kind, EntityKind::Mage | EntityKind::Priest),
            ItemCategory::Sword => matches!(
                kind,
                EntityKind::Soldier | EntityKind::Tank | EntityKind::Knight | EntityKind::Cavalry
            ),
            ItemCategory::Armor | ItemCategory::Helmet => *kind != EntityKind::Worker,
            ItemCategory::Ring => true,
        };
        if !allowed {
            continue;
        }

        inventory.items.push(pickup.item);
        changed.write(InventoryChanged { unit: msg.picker });
        collected.write(ItemPickupCollected {
            pickup: msg.pickup,
            collector: msg.picker,
            item: pickup.item,
        });
        commands.entity(msg.pickup).insert(PickupCollectVfx {
            timer: Timer::from_seconds(0.18, TimerMode::Once),
        });
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

fn animate_pickup_bob(time: Res<Time>, mut pickups: Query<(&PickupBob, &mut Transform), With<ItemPickup>>) {
    for (bob, mut transform) in &mut pickups {
        transform.translation.y = bob.base_y + (time.elapsed_secs() * 1.8 + bob.phase).sin() * 0.08;
    }
}

fn face_pickup_billboards(
    camera_q: Query<&GlobalTransform, With<RtsCamera>>,
    pickup_roots: Query<&GlobalTransform, With<ItemPickup>>,
    mut billboards: Query<(&ChildOf, &mut Transform), With<PickupBillboard>>,
) {
    let Ok(camera_tf) = camera_q.single() else {
        return;
    };
    for (parent, mut transform) in &mut billboards {
        let Ok(root_tf) = pickup_roots.get(parent.parent()) else {
            continue;
        };
        let to_camera: Vec3 = (camera_tf.translation() - root_tf.translation()).into();
        let yaw = to_camera.x.atan2(to_camera.z);
        transform.rotation = Quat::from_rotation_y(yaw);
    }
}

fn animate_pickup_beams(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    beams: Query<&MeshMaterial3d<StandardMaterial>, With<PickupBeam>>,
) {
    let pulse = 0.18 + ((time.elapsed_secs() * 2.4).sin() * 0.06 + 0.06);
    for mat_handle in &beams {
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            let mut base = mat.base_color;
            base.set_alpha(pulse);
            mat.base_color = base;
        }
    }
}
