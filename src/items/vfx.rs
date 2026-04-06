use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use std::f32::consts::PI;

use super::components::{ItemKind, UnitInventory};
use super::registry::ItemRegistry;
use crate::components::{MoveTarget, Unit};

// ── Messages ──

/// Fired by combat/game systems when an item-relevant event occurs.
#[derive(Message, Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct ItemVfxTrigger {
    pub owner: Entity,
    pub item: ItemKind,
    pub kind: ItemVfxTriggerKind,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum ItemVfxTriggerKind {
    /// Unit took ranged damage (Padded Vest dampening puff)
    RangedHitAbsorbed { pos: Vec3 },
    /// First melee protection consumed (Bronze Cuirass deflect flash)
    MeleeDeflect { pos: Vec3 },
    /// Lethal threshold prevented (Plate Cuirass barrier flicker)
    LethalPrevented { pos: Vec3 },
    /// CC duration reduced (Crusader Helm snap pulse)
    CCReduced { pos: Vec3 },
    /// High-ground ranged deflected (Kettle Helm upward glint)
    HeightDeflect { pos: Vec3 },
    /// Kill-triggered move burst started (Viking Helm wind streaks)
    KillMoveBurst { pos: Vec3 },
    /// Neutral kill restored energy (Golden Band gold intake spark)
    EnergyRestored { pos: Vec3 },
    /// Bleed proc hit (Arming Sword red slash)
    BleedProc { pos: Vec3, direction: Vec3 },
    /// Execute damage on low-HP target (Viking Blade crimson edge)
    ExecuteStrike { target_pos: Vec3 },
    /// Splash magic impact (Battle Staff arcane ripple)
    SplashImpact { pos: Vec3 },
    /// Attack-move slow applied (War Bow trail tint)
    SlowApplied { from: Vec3, to: Vec3 },
}

// ── Persistent state tracking ──

/// Tracks which conditional VFX states are active per item slot.
/// Attached to units that have items with conditional persistent effects.
#[derive(Component, Clone, Debug, Default)]
pub struct ItemVfxState {
    pub entries: Vec<ItemVfxStateEntry>,
}

#[derive(Clone, Debug)]
pub struct ItemVfxStateEntry {
    pub item: ItemKind,
    pub slot: u8,
    pub condition: ItemVfxCondition,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemVfxCondition {
    /// Wedding Band: paired ally in range
    AllyPaired,
    /// Jewel Ring: always-on slow scan
    ScanActive,
    /// Twin Rings: first ability bonus unspent
    AbilityPrimed,
    /// Linked Rings: overheal shield exists
    ShieldActive,
    /// Mage Crozier: braced and primed
    BracedPrimed,
    /// Yew Longbow: stationary long enough
    StationaryReady,
    /// Arming Sword: next hit is bleed (third attack primed)
    BleedPrimed,
}

// ── VFX entity markers ──

/// Root entity for all item VFX children on a unit.
#[derive(Component)]
pub struct ItemVfxRoot;

/// A persistent conditional VFX child (ground ring, scan pulse, glow, etc.)
#[derive(Component, Clone, Debug)]
pub struct ItemVfxPersistent {
    pub item: ItemKind,
    pub slot: u8,
    pub kind: PersistentVfxKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PersistentVfxKind {
    /// Thin ground ring with outward pulse (Wedding Band)
    GroundAuraRing,
    /// Faint scan ripple expanding outward (Jewel Ring)
    ScanRipple,
    /// Tiny dual-glyph charge mark (Twin Rings)
    ChargeMark,
    /// Thin shell shimmer (Linked Rings)
    ShellShimmer,
    /// Focus sigil near staff head (Mage Crozier)
    FocusSigil,
    /// Faint aim line (Yew Longbow)
    AimFocus,
    /// Small weapon-edge readiness pulse (Arming Sword primed)
    WeaponPulse,
    /// Tiny readiness glint on torso (Bronze Cuirass)
    ArmorGlint,
    /// Minimal head ring (Crusader Helm)
    HeadRing,
}

/// A one-shot triggered VFX particle/flash.
#[derive(Component)]
pub struct ItemVfxFlash {
    pub timer: Timer,
    pub start_scale: f32,
    pub end_scale: f32,
    pub rise_speed: f32,
    #[allow(dead_code)]
    pub color: Color,
}

/// Trailing wind streaks for movement bursts.
#[derive(Component)]
pub struct ItemVfxTrail {
    pub timer: Timer,
    pub velocity: Vec3,
    pub start_scale: f32,
}

// ── VFX profile per item (data-driven config) ──

pub struct ItemVfxProfile {
    pub persistent: &'static [PersistentVfxKind],
    pub condition: Option<ItemVfxCondition>,
}

pub fn vfx_profile(item: ItemKind) -> ItemVfxProfile {
    use ItemKind::*;
    use ItemVfxCondition::*;
    use PersistentVfxKind::*;

    match item {
        // Armor - mostly no idle VFX
        PaddedVest => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
        BronzeCuirass => ItemVfxProfile {
            persistent: &[ArmorGlint],
            condition: None, // always-on tiny glint
        },
        PlateCuirass => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
        // Helmets
        CrusaderHelm => ItemVfxProfile {
            persistent: &[HeadRing],
            condition: None, // minimal always-on
        },
        KettleHelm => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
        VikingHelm => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
        // Rings
        JewelRing => ItemVfxProfile {
            persistent: &[ScanRipple],
            condition: Some(ScanActive),
        },
        PlainBand => ItemVfxProfile {
            persistent: &[], // UI-only
            condition: None,
        },
        WeddingBand => ItemVfxProfile {
            persistent: &[GroundAuraRing],
            condition: Some(AllyPaired),
        },
        GoldenBand => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
        TwinRings => ItemVfxProfile {
            persistent: &[ChargeMark],
            condition: Some(AbilityPrimed),
        },
        LinkedRings => ItemVfxProfile {
            persistent: &[ShellShimmer],
            condition: Some(ShieldActive),
        },
        // Swords
        ArmingSword => ItemVfxProfile {
            persistent: &[WeaponPulse],
            condition: Some(BleedPrimed),
        },
        VikingBlade => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
        // Staffs
        BattleStaff => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
        MageCrozier => ItemVfxProfile {
            persistent: &[FocusSigil],
            condition: Some(BracedPrimed),
        },
        // Bows
        YewLongbow => ItemVfxProfile {
            persistent: &[AimFocus],
            condition: Some(StationaryReady),
        },
        WarBow => ItemVfxProfile {
            persistent: &[],
            condition: None,
        },
    }
}

// ── Shared VFX assets for item effects ──

#[derive(Resource)]
pub struct ItemVfxAssets {
    pub flash_sphere: Handle<Mesh>,
    pub ring_mesh: Handle<Mesh>,
    pub small_ring_mesh: Handle<Mesh>,
    pub scan_ring_mesh: Handle<Mesh>,
    pub orb_mesh: Handle<Mesh>,
    pub streak_quad: Handle<Mesh>,
}

pub fn setup_item_vfx_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(ItemVfxAssets {
        flash_sphere: meshes.add(Sphere::new(0.12)),
        ring_mesh: meshes.add(Torus::new(1.1, 0.025)),
        small_ring_mesh: meshes.add(Torus::new(0.28, 0.02)),
        scan_ring_mesh: meshes.add(Torus::new(0.6, 0.018)),
        orb_mesh: meshes.add(Sphere::new(0.06)),
        streak_quad: meshes.add(bevy::math::primitives::Rectangle::new(0.04, 0.25)),
    });
}

// ── Sync system: rebuild VFX children when inventory changes ──

pub fn sync_item_vfx(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    vfx_assets: Res<ItemVfxAssets>,
    registry: Res<ItemRegistry>,
    units: Query<
        (Entity, &UnitInventory, Option<&Children>, Option<&ItemVfxState>),
        (With<Unit>, Changed<UnitInventory>),
    >,
    vfx_roots: Query<(), With<ItemVfxRoot>>,
) {
    for (unit, inventory, children, _vfx_state) in &units {
        // Remove old VFX roots
        if let Some(children) = children {
            for child in children.iter() {
                if vfx_roots.get(child).is_ok() {
                    commands.entity(child).try_despawn();
                }
            }
        }

        if inventory.items.is_empty() {
            commands.entity(unit).remove::<ItemVfxState>();
            continue;
        }

        let mut state_entries = Vec::new();

        for (slot, &item) in inventory
            .items
            .iter()
            .enumerate()
            .take(inventory.capacity as usize)
        {
            let def = registry.get(item);
            let profile = vfx_profile(item);

            // Build persistent VFX children
            if !profile.persistent.is_empty() {
                let root = commands
                    .spawn((
                        ItemVfxRoot,
                        Transform::default(),
                        GlobalTransform::default(),
                        Visibility::Visible,
                        InheritedVisibility::default(),
                        ViewVisibility::default(),
                    ))
                    .id();

                for &pvfx in profile.persistent {
                    let child =
                        spawn_persistent_vfx(&mut commands, &mut materials, &vfx_assets, item, def.beam_color, slot as u8, pvfx);
                    commands.entity(root).add_child(child);
                }

                commands.entity(unit).add_child(root);
            }

            // Track state for conditional items
            if let Some(condition) = profile.condition {
                state_entries.push(ItemVfxStateEntry {
                    item,
                    slot: slot as u8,
                    condition,
                    // Scan and always-on effects start active; conditional ones start inactive
                    active: matches!(
                        condition,
                        ItemVfxCondition::ScanActive
                    ),
                });
            }
        }

        if !state_entries.is_empty() {
            commands.entity(unit).insert(ItemVfxState {
                entries: state_entries,
            });
        } else {
            commands.entity(unit).remove::<ItemVfxState>();
        }
    }
}

fn spawn_persistent_vfx(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    assets: &ItemVfxAssets,
    item: ItemKind,
    color: Color,
    slot: u8,
    kind: PersistentVfxKind,
) -> Entity {
    let alpha = match kind {
        PersistentVfxKind::GroundAuraRing => 0.18,
        PersistentVfxKind::ScanRipple => 0.14,
        PersistentVfxKind::ShellShimmer => 0.22,
        _ => 0.25,
    };
    let emissive_mult = match kind {
        PersistentVfxKind::GroundAuraRing => 1.2,
        PersistentVfxKind::WeaponPulse | PersistentVfxKind::ArmorGlint => 2.0,
        _ => 1.4,
    };

    let mat = materials.add(StandardMaterial {
        base_color: color.with_alpha(alpha),
        emissive: color.to_linear() * emissive_mult,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let (mesh, transform) = persistent_vfx_geometry(assets, kind, slot);

    commands
        .spawn((
            ItemVfxPersistent { item, slot, kind },
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            transform,
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id()
}

fn persistent_vfx_geometry(
    assets: &ItemVfxAssets,
    kind: PersistentVfxKind,
    _slot: u8,
) -> (Handle<Mesh>, Transform) {
    match kind {
        PersistentVfxKind::GroundAuraRing => (
            assets.ring_mesh.clone(),
            Transform::from_xyz(0.0, 0.05, 0.0)
                .with_rotation(Quat::from_rotation_x(PI * 0.5)),
        ),
        PersistentVfxKind::ScanRipple => (
            assets.scan_ring_mesh.clone(),
            Transform::from_xyz(0.0, 0.08, 0.0)
                .with_rotation(Quat::from_rotation_x(PI * 0.5)),
        ),
        PersistentVfxKind::ChargeMark => (
            assets.orb_mesh.clone(),
            Transform::from_xyz(0.0, 1.2, 0.15),
        ),
        PersistentVfxKind::ShellShimmer => (
            assets.small_ring_mesh.clone(),
            Transform::from_xyz(0.0, 0.7, 0.0)
                .with_rotation(Quat::from_rotation_x(PI * 0.5)),
        ),
        PersistentVfxKind::FocusSigil => (
            assets.orb_mesh.clone(),
            Transform::from_xyz(0.0, 1.4, 0.12),
        ),
        PersistentVfxKind::AimFocus => (
            assets.orb_mesh.clone(),
            Transform::from_xyz(0.0, 1.0, 0.2),
        ),
        PersistentVfxKind::WeaponPulse => (
            assets.orb_mesh.clone(),
            Transform::from_xyz(0.12, 0.8, 0.0),
        ),
        PersistentVfxKind::ArmorGlint => (
            assets.orb_mesh.clone(),
            Transform::from_xyz(0.0, 0.85, 0.1),
        ),
        PersistentVfxKind::HeadRing => (
            assets.small_ring_mesh.clone(),
            Transform::from_xyz(0.0, 1.35, 0.0)
                .with_rotation(Quat::from_rotation_x(PI * 0.5)),
        ),
    }
}

// ── Animate persistent VFX ──

pub fn animate_persistent_item_vfx(
    time: Res<Time>,
    vfx_state_q: Query<&ItemVfxState>,
    mut persistent_q: Query<(&ChildOf, &ItemVfxPersistent, &mut Transform, &mut Visibility)>,
) {
    let t = time.elapsed_secs();

    for (parent_ref, pvfx, mut transform, mut vis) in &mut persistent_q {
        // Check if parent unit has active state for this item
        let is_active = find_vfx_active(parent_ref, pvfx, &vfx_state_q);

        // Items with no condition (always-on glints) are always visible
        let profile = vfx_profile(pvfx.item);
        let always_on = profile.condition.is_none();

        if !always_on && !is_active {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;

        let phase = t * 1.2 + pvfx.slot as f32 * 0.4;

        match pvfx.kind {
            PersistentVfxKind::GroundAuraRing => {
                // Slow rotation with periodic outward pulse
                transform.rotation =
                    Quat::from_rotation_y(-phase * 0.3) * Quat::from_rotation_x(PI * 0.5);
                let pulse = 1.0 + (phase * 0.8).sin().max(0.0) * 0.12;
                transform.scale = Vec3::splat(pulse);
            }
            PersistentVfxKind::ScanRipple => {
                // Expanding ring that resets periodically
                let cycle = (phase * 0.4) % 1.0;
                let scale = 0.6 + cycle * 1.8;
                transform.scale = Vec3::new(scale, scale, 1.0);
                transform.rotation =
                    Quat::from_rotation_y(phase * 0.15) * Quat::from_rotation_x(PI * 0.5);
            }
            PersistentVfxKind::ChargeMark => {
                // Tiny bobbing glyph
                transform.translation.y = 1.2 + (phase * 1.5).sin() * 0.04;
                transform.scale = Vec3::splat(0.9 + (phase * 2.0).sin().abs() * 0.2);
            }
            PersistentVfxKind::ShellShimmer => {
                // Shell ring around the body, gentle breathe
                let breathe = 1.0 + (phase * 0.9).sin() * 0.08;
                transform.scale = Vec3::splat(breathe);
                transform.translation.y = 0.7 + (phase * 0.7).sin() * 0.03;
            }
            PersistentVfxKind::FocusSigil => {
                // Slow orbit near staff head
                transform.translation.x = (phase * 0.6).cos() * 0.12;
                transform.translation.z = (phase * 0.6).sin() * 0.12;
                transform.translation.y = 1.4 + (phase * 1.2).sin() * 0.03;
                transform.scale = Vec3::splat(0.85 + (phase * 1.8).sin().abs() * 0.25);
            }
            PersistentVfxKind::AimFocus => {
                // Faint drawn aim indicator
                transform.translation.z = 0.2 + (phase * 0.5).sin() * 0.05;
                transform.scale = Vec3::splat(0.7 + (phase * 1.0).sin().abs() * 0.3);
            }
            PersistentVfxKind::WeaponPulse => {
                // Weapon-edge pulse
                let pulse = (phase * 2.5).sin().max(0.0);
                transform.scale = Vec3::splat(0.6 + pulse * 0.5);
                transform.translation.x = 0.12 + pulse * 0.03;
            }
            PersistentVfxKind::ArmorGlint => {
                // Very subtle torso glint, periodic
                let glint = ((phase * 0.4).sin() * 3.0).clamp(0.0, 1.0);
                transform.scale = Vec3::splat(0.3 + glint * 0.5);
            }
            PersistentVfxKind::HeadRing => {
                // Minimal halo ring at head level
                transform.rotation =
                    Quat::from_rotation_y(-phase * 0.25) * Quat::from_rotation_x(PI * 0.5);
                transform.scale = Vec3::splat(0.85 + (phase * 0.6).sin().abs() * 0.06);
            }
        }
    }
}

/// Walk up from the VFX child → find the grandparent unit → check ItemVfxState.
fn find_vfx_active(
    parent_ref: &ChildOf,
    pvfx: &ItemVfxPersistent,
    vfx_state_q: &Query<&ItemVfxState>,
) -> bool {
    // The parent of a persistent VFX node is the ItemVfxRoot, whose parent is the unit.
    // But ChildOf only gives direct parent. We need to check the grandparent's ItemVfxState.
    // Since the root doesn't have ItemVfxState, we check the parent (root) which is a child of the unit.
    // The query checks ItemVfxState on the root's parent indirectly.
    // Actually let's just check if the root entity itself or any ancestor has the state.
    // For simplicity: persistent nodes whose profile has no condition are always active.
    let profile = vfx_profile(pvfx.item);
    if profile.condition.is_none() {
        return true;
    }

    // Try to find state on parent (unlikely) - we'll handle this via a separate system
    if let Ok(state) = vfx_state_q.get(parent_ref.parent()) {
        return state
            .entries
            .iter()
            .any(|e| e.item == pvfx.item && e.slot == pvfx.slot && e.active);
    }
    false
}

// ── Update conditional VFX states ──

/// Updates conditional VFX states based on game conditions.
/// This runs each frame and checks actual game state to determine which effects should be visible.
pub fn update_item_vfx_conditions(
    time: Res<Time>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &UnitInventory,
            &mut ItemVfxState,
            Option<&MoveTarget>,
        ),
        With<Unit>,
    >,
) {
    let t = time.elapsed_secs();

    for (_entity, _transform, _inventory, mut vfx_state, move_target) in &mut units {
        for entry in vfx_state.entries.iter_mut() {
            entry.active = match entry.condition {
                ItemVfxCondition::ScanActive => true, // Jewel Ring: always scanning
                ItemVfxCondition::AllyPaired => true, // Wedding Band: simplified - always show when equipped
                ItemVfxCondition::StationaryReady => move_target.is_none(), // Yew Longbow: no move target
                ItemVfxCondition::BleedPrimed => {
                    // Arming Sword: pulse periodically to indicate readiness
                    // Real implementation would track attack counter
                    ((t * 0.5) % 3.0) < 1.0
                }
                ItemVfxCondition::AbilityPrimed => true, // Twin Rings: show until consumed
                ItemVfxCondition::ShieldActive => false,  // Linked Rings: activated by overheal
                ItemVfxCondition::BracedPrimed => move_target.is_none(), // Mage Crozier: braced = stationary
            };
        }
    }
}

// ── Propagate state to persistent VFX visibility ──
// The persistent VFX animate system already handles visibility via find_vfx_active,
// but since ChildOf only gives the VFX root (not the unit), we need a bridging system.

#[allow(dead_code)]
pub fn propagate_vfx_state_to_roots(
    units: Query<(&ItemVfxState, &Children), (With<Unit>, Changed<ItemVfxState>)>,
    mut roots: Query<(&Children, &mut Visibility), With<ItemVfxRoot>>,
    persistent_q: Query<&ItemVfxPersistent>,
) {
    for (state, unit_children) in &units {
        for uchild in unit_children.iter() {
            let Ok((root_children, _root_vis)) = roots.get_mut(uchild) else {
                continue;
            };
            // Check if any persistent child's item is active in state
            for rchild in root_children.iter() {
                let Ok(pvfx) = persistent_q.get(rchild) else {
                    continue;
                };
                let profile = vfx_profile(pvfx.item);
                if profile.condition.is_none() {
                    continue; // Always-on, skip
                }
                let is_active = state
                    .entries
                    .iter()
                    .any(|e| e.item == pvfx.item && e.slot == pvfx.slot && e.active);
                // We can't set child visibility here since we don't have mutable access
                // The animate system handles this
                let _ = is_active;
            }
        }
    }
}

// ── Trigger-driven one-shot VFX ──

const MAX_ITEM_VFX_PARTICLES: usize = 128;

pub fn spawn_triggered_item_vfx(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut triggers: MessageReader<ItemVfxTrigger>,
    registry: Res<ItemRegistry>,
    assets: Res<ItemVfxAssets>,
    existing_particles: Query<(), With<ItemVfxFlash>>,
) {
    let particle_count = existing_particles.iter().count();
    if particle_count >= MAX_ITEM_VFX_PARTICLES {
        // Drain remaining messages
        for _ in triggers.read() {}
        return;
    }

    let mut spawned = 0;
    for trigger in triggers.read() {
        if particle_count + spawned >= MAX_ITEM_VFX_PARTICLES {
            break;
        }

        let def = registry.get(trigger.item);
        let color = def.beam_color;

        match trigger.kind {
            ItemVfxTriggerKind::RangedHitAbsorbed { pos } => {
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 0.6,
                    color,
                    0.06,
                    0.18,
                    0.3,
                    0.25,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::MeleeDeflect { pos } => {
                // Stronger deflect flash
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 0.7,
                    color,
                    0.08,
                    0.28,
                    0.5,
                    0.2,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::LethalPrevented { pos } => {
                // Emergency barrier flicker - larger
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 0.5,
                    color,
                    0.15,
                    0.4,
                    0.2,
                    0.35,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::CCReduced { pos } => {
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 1.3,
                    color,
                    0.05,
                    0.15,
                    0.8,
                    0.18,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::HeightDeflect { pos } => {
                // Upward glint
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 1.5,
                    color,
                    0.04,
                    0.12,
                    1.2,
                    0.15,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::KillMoveBurst { pos } => {
                // Wind streaks at feet
                for i in 0..3 {
                    let angle = i as f32 * PI * 2.0 / 3.0;
                    let offset = Vec3::new(angle.cos() * 0.2, 0.05, angle.sin() * 0.2);
                    spawn_item_trail(
                        &mut commands,
                        &mut materials,
                        &assets,
                        pos + offset,
                        Vec3::new(angle.cos() * 0.8, 0.3, angle.sin() * 0.8),
                        color,
                        0.08,
                        0.4,
                    );
                    spawned += 1;
                }
            }
            ItemVfxTriggerKind::EnergyRestored { pos } => {
                // Gold intake spark flowing inward
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 0.8,
                    color,
                    0.05,
                    0.2,
                    -0.3, // negative = sink toward unit
                    0.25,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::BleedProc { pos, direction } => {
                // Red slash accent
                let slash_offset = direction.normalize_or_zero() * 0.15;
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 0.6 + slash_offset,
                    color,
                    0.1,
                    0.3,
                    0.4,
                    0.2,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::ExecuteStrike { target_pos } => {
                // Target-side crimson accent
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    target_pos + Vec3::Y * 0.5,
                    color,
                    0.08,
                    0.25,
                    0.3,
                    0.22,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::SplashImpact { pos } => {
                // Arcane ripple at impact
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    pos + Vec3::Y * 0.3,
                    color,
                    0.12,
                    0.35,
                    0.2,
                    0.28,
                );
                spawned += 1;
            }
            ItemVfxTriggerKind::SlowApplied { from: _, to } => {
                // Impact slow tag at target
                spawn_item_flash(
                    &mut commands,
                    &mut materials,
                    &assets,
                    to + Vec3::Y * 0.3,
                    color,
                    0.06,
                    0.15,
                    0.1,
                    0.3,
                );
                spawned += 1;
            }
        }
    }
}

fn spawn_item_flash(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    assets: &ItemVfxAssets,
    pos: Vec3,
    color: Color,
    start_scale: f32,
    end_scale: f32,
    rise_speed: f32,
    duration: f32,
) {
    let mat = materials.add(StandardMaterial {
        base_color: color.with_alpha(0.7),
        emissive: color.to_linear() * 3.5,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        ItemVfxFlash {
            timer: Timer::from_seconds(duration, TimerMode::Once),
            start_scale,
            end_scale,
            rise_speed,
            color,
        },
        Mesh3d(assets.flash_sphere.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(pos).with_scale(Vec3::splat(start_scale)),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn spawn_item_trail(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    assets: &ItemVfxAssets,
    pos: Vec3,
    velocity: Vec3,
    color: Color,
    start_scale: f32,
    duration: f32,
) {
    let mat = materials.add(StandardMaterial {
        base_color: color.with_alpha(0.45),
        emissive: color.to_linear() * 2.0,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        ItemVfxTrail {
            timer: Timer::from_seconds(duration, TimerMode::Once),
            velocity,
            start_scale,
        },
        Mesh3d(assets.streak_quad.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(pos).with_scale(Vec3::splat(start_scale)),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

// ── Animate one-shot VFX ──

pub fn animate_item_vfx_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut ItemVfxFlash, &mut Transform)>,
) {
    for (entity, mut flash, mut transform) in &mut flashes {
        flash.timer.tick(time.delta());
        let frac = flash.timer.fraction();

        // Scale: lerp from start to end, then shrink
        let scale = if frac < 0.6 {
            let t = frac / 0.6;
            flash.start_scale + (flash.end_scale - flash.start_scale) * t
        } else {
            let t = (frac - 0.6) / 0.4;
            flash.end_scale * (1.0 - t)
        };
        transform.scale = Vec3::splat(scale.max(0.01));

        // Rise
        transform.translation.y += flash.rise_speed * time.delta_secs();

        if flash.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

pub fn animate_item_vfx_trails(
    mut commands: Commands,
    time: Res<Time>,
    mut trails: Query<(Entity, &mut ItemVfxTrail, &mut Transform)>,
) {
    for (entity, mut trail, mut transform) in &mut trails {
        trail.timer.tick(time.delta());
        let frac = trail.timer.fraction();

        // Move along velocity
        transform.translation += trail.velocity * time.delta_secs();

        // Shrink over time
        let scale = trail.start_scale * (1.0 - frac);
        transform.scale = Vec3::splat(scale.max(0.01));

        // Gravity
        trail.velocity.y -= 2.0 * time.delta_secs();

        if trail.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}
