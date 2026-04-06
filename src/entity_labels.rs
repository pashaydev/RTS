use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::blueprints::EntityKind;
use crate::camera;
use crate::components::*;
use crate::fog::FogTweakSettings;
use crate::items::{ItemPickup, ItemPickupLabel};
use crate::selection::SelectionSet;
use crate::theme::Theme;
use crate::ui::fonts;
use std::collections::{HashMap, HashSet};

// ══════════════════════════════════════════════════════════════════════
// Components
// ══════════════════════════════════════════════════════════════════════

#[derive(Component)]
pub struct EntityLabel {
    pub target: Entity,
    pub base_color: Color,
    pub interactive: bool,
}

/// Marker for cluster-style (pill) labels — non-interactive summaries.
#[derive(Component)]
struct ClusterLabelMarker;

#[derive(Component)]
struct HoveredViaLabel;

#[derive(Component)]
struct LeaderLine;

#[derive(Component)]
struct LabelNameText;

#[derive(Component)]
struct LabelHpFill;

#[derive(Component)]
struct LabelExtraText;

// ══════════════════════════════════════════════════════════════════════
// Configuration
// ══════════════════════════════════════════════════════════════════════

#[derive(Resource)]
struct LabelConfig {
    max_focus_labels: usize,
    max_cluster_labels: usize,
    label_y_offset_unit: f32,
    label_y_offset_building: f32,
    line_width: f32,
    follow_strength: f32,
    anchor_pull: f32,
    slot_gap: f32,
    /// World-space radius for clustering same-type ambient units.
    cluster_radius: f32,
    /// Hysteresis: a cluster only splits if members exceed this distance
    /// from the centroid for this many consecutive frames.
    split_distance: f32,
    split_frames: u8,
    /// Hysteresis: clusters merge only when centroids are within this range.
    merge_distance: f32,
}

impl Default for LabelConfig {
    fn default() -> Self {
        Self {
            max_focus_labels: 15,
            max_cluster_labels: 12,
            label_y_offset_unit: 2.5,
            label_y_offset_building: 5.0,
            line_width: 1.5,
            follow_strength: 0.2,
            anchor_pull: 0.18,
            slot_gap: 6.0,
            cluster_radius: 12.0,
            split_distance: 18.0,
            split_frames: 8,
            merge_distance: 10.0,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Label intent — the representation model
// ══════════════════════════════════════════════════════════════════════

#[derive(Clone)]
enum LabelIntent {
    /// Hovered or selected entity — rich card with HP, leader line.
    Focus(FocusData),
    /// Aggregate label for nearby same-type ambient units — small pill.
    Cluster(ClusterData),
}

#[derive(Clone)]
struct FocusData {
    entity: Entity,
    #[allow(dead_code)]
    world_pos: Vec3,
    anchor: Vec2,
    label_origin: Vec2,
    distance: f32,
    priority: u8,
    color: Color,
    name: String,
    hp_ratio: Option<f32>,
    extra: Option<String>,
    interactive: bool,
}

#[derive(Clone)]
struct ClusterData {
    cluster_id: u64,
    #[allow(dead_code)]
    centroid_world: Vec3,
    anchor: Vec2,
    label_origin: Vec2,
    #[allow(dead_code)]
    distance: f32,
    color: Color,
    kind_name: String,
    count: usize,
    faction_priority: u8,
}

// ══════════════════════════════════════════════════════════════════════
// Persistent cluster state — the key to stability
// ══════════════════════════════════════════════════════════════════════

#[derive(Resource)]
struct ClusterState {
    next_id: u64,
    clusters: HashMap<u64, PersistentCluster>,
}

impl Default for ClusterState {
    fn default() -> Self {
        Self {
            next_id: 1,
            clusters: HashMap::new(),
        }
    }
}

#[derive(Clone)]
struct PersistentCluster {
    members: HashSet<Entity>,
    kind_name: String,
    faction_priority: u8,
    color: Color,
    centroid: Vec3,
    screen_pos: Vec2,
    frames_alive: u32,
    /// Per-member: how many consecutive frames the member has been far from centroid.
    far_frames: HashMap<Entity, u8>,
}

/// Key used to bucket entities before clustering: same display-name + same relationship tier.
#[derive(Hash, Eq, PartialEq, Clone)]
struct ClusterKey {
    kind_name: String,
    faction_priority: u8,
}

// ══════════════════════════════════════════════════════════════════════
// Layout cache (for smooth label positioning)
// ══════════════════════════════════════════════════════════════════════

#[derive(Resource, Default)]
struct LabelLayoutCache {
    entries: HashMap<LabelSlotKey, CachedLabelPlacement>,
}

/// Labels are keyed either by entity (focus) or cluster-id (cluster).
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
enum LabelSlotKey {
    Focus(Entity),
    Cluster(u64),
}

#[derive(Clone, Copy)]
struct CachedLabelPlacement {
    pos: Vec2,
    size: Vec2,
}

// ══════════════════════════════════════════════════════════════════════
// Plugin
// ══════════════════════════════════════════════════════════════════════

pub struct EntityLabelPlugin;

impl Plugin for EntityLabelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LabelConfig>()
            .init_resource::<LabelLayoutCache>()
            .init_resource::<ClusterState>()
            .add_systems(
                Update,
                entity_label_system
                    .in_set(GameFlowSet::Presentation)
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                handle_label_interaction
                    .in_set(GameFlowSet::Input)
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_entity_labels);
    }
}

fn cleanup_entity_labels(
    mut commands: Commands,
    labels: Query<Entity, With<EntityLabel>>,
    leader_lines: Query<Entity, With<LeaderLine>>,
    mut layout_cache: ResMut<LabelLayoutCache>,
    mut cluster_state: ResMut<ClusterState>,
) {
    for entity in &labels {
        commands.entity(entity).try_despawn();
    }

    for entity in &leader_lines {
        commands.entity(entity).try_despawn();
    }

    layout_cache.entries.clear();
    cluster_state.clusters.clear();
    cluster_state.next_id = 1;
}

// ══════════════════════════════════════════════════════════════════════
// Color helpers
// ══════════════════════════════════════════════════════════════════════

fn relationship_color(
    entity_faction: &Faction,
    active_faction: &Faction,
    teams: &TeamConfig,
) -> Color {
    if entity_faction == active_faction {
        Color::srgb(0.3, 0.6, 1.0)
    } else if teams.is_allied(active_faction, entity_faction) {
        Color::srgb(0.2, 0.8, 0.3)
    } else {
        Color::srgb(1.0, 0.3, 0.2)
    }
}

fn mob_color() -> Color {
    Color::srgb(0.6, 0.6, 0.6)
}

fn resource_color() -> Color {
    Color::srgb(0.9, 0.7, 0.2)
}

fn hp_color(ratio: f32, theme: &Theme) -> Color {
    if ratio > 0.6 {
        theme.colors.hp_high()
    } else if ratio > 0.3 {
        theme.colors.hp_mid()
    } else {
        theme.colors.hp_low()
    }
}

/// Priority number for faction relationship (lower = more important).
fn faction_priority(faction: &Faction, active: &Faction, teams: &TeamConfig) -> u8 {
    if faction == active {
        2
    } else if teams.is_allied(active, faction) {
        3
    } else {
        4
    }
}

// ══════════════════════════════════════════════════════════════════════
// Geometry helpers
// ══════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
struct LabelRect {
    min: Vec2,
    size: Vec2,
}

impl LabelRect {
    fn center(self) -> Vec2 {
        self.min + self.size * 0.5
    }

    fn anchor_point(self, anchor: Vec2) -> Vec2 {
        let x = anchor.x.clamp(self.min.x + 6.0, self.min.x + self.size.x - 6.0);
        if anchor.y <= self.center().y {
            Vec2::new(x, self.min.y)
        } else {
            Vec2::new(x, self.min.y + self.size.y)
        }
    }
}

#[derive(Clone, Copy)]
struct LeaderGeometry {
    center: Vec2,
    length: f32,
    angle: f32,
}

fn clamp_label_to_screen(pos: Vec2, size: Vec2, screen_w: f32, screen_h: f32) -> Vec2 {
    Vec2::new(
        pos.x.clamp(2.0, (screen_w - size.x - 2.0).max(2.0)),
        pos.y.clamp(2.0, (screen_h - size.y - 2.0).max(2.0)),
    )
}

fn overlap_area(a_pos: Vec2, a_size: Vec2, b_pos: Vec2, b_size: Vec2) -> f32 {
    let overlap_w = (a_pos.x + a_size.x).min(b_pos.x + b_size.x) - a_pos.x.max(b_pos.x);
    let overlap_h = (a_pos.y + a_size.y).min(b_pos.y + b_size.y) - a_pos.y.max(b_pos.y);
    overlap_w.max(0.0) * overlap_h.max(0.0)
}

fn find_best_label_slot(
    desired_pos: Vec2,
    projected_pos: Vec2,
    anchor: Vec2,
    size: Vec2,
    placed: &[LabelRect],
    screen_w: f32,
    screen_h: f32,
    gap: f32,
    anchor_pull: f32,
) -> Vec2 {
    let x_offsets = [
        0.0,
        -(size.x * 0.55 + gap),
        size.x * 0.55 + gap,
        -(size.x * 1.1 + gap * 2.0),
        size.x * 1.1 + gap * 2.0,
    ];
    let y_offsets = [
        0.0,
        -(size.y + gap),
        size.y * 0.35 + gap,
        -(size.y * 2.0 + gap * 2.0),
        size.y * 0.7 + gap * 2.0,
    ];

    let mut best_pos = clamp_label_to_screen(desired_pos, size, screen_w, screen_h);
    let mut best_score = f32::INFINITY;

    for y_offset in y_offsets {
        for x_offset in x_offsets {
            let pos = clamp_label_to_screen(
                Vec2::new(desired_pos.x + x_offset, desired_pos.y + y_offset),
                size,
                screen_w,
                screen_h,
            );
            let mut overlap_penalty = 0.0;
            let mut collision_count = 0.0;
            for placed_rect in placed {
                let area = overlap_area(pos, size, placed_rect.min, placed_rect.size);
                if area > 0.0 {
                    overlap_penalty += area;
                    collision_count += 1.0;
                }
            }

            let center = pos + size * 0.5;
            let desired_delta = pos - desired_pos;
            let projected_delta = pos - projected_pos;
            let anchor_distance = center.distance(anchor);
            let score = overlap_penalty * 25.0
                + collision_count * 5_000.0
                + desired_delta.length_squared() * 0.85
                + projected_delta.length_squared() * 0.35
                + anchor_distance * anchor_pull;

            if score < best_score {
                best_score = score;
                best_pos = pos;
            }
        }
    }

    best_pos
}

fn compute_leader_geometry(
    rect: LabelRect,
    anchor: Vec2,
    line_width: f32,
    scale: f32,
) -> Option<LeaderGeometry> {
    let start = anchor * scale;
    let target = rect.anchor_point(anchor) * scale;
    let start_world = Vec2::new(start.x, -start.y);
    let target_world = Vec2::new(target.x, -target.y);
    let delta = target_world - start_world;
    let length = delta.length();
    if length <= line_width {
        return None;
    }

    Some(LeaderGeometry {
        center: (start + target) * 0.5,
        length,
        angle: delta.y.atan2(delta.x),
    })
}

fn window_to_overlay_position(window_pos: Vec2, window_width: f32, window_height: f32) -> Vec2 {
    Vec2::new(
        window_pos.x - window_width * 0.5,
        window_height * 0.5 - window_pos.y,
    )
}

// ══════════════════════════════════════════════════════════════════════
// Phase 1 — Candidate collection
// ══════════════════════════════════════════════════════════════════════

/// Raw data collected per visible entity before intent classification.
struct RawCandidate {
    entity: Entity,
    world_pos: Vec3,
    anchor: Vec2,
    label_origin: Vec2,
    distance: f32,
    color: Color,
    name: String,
    hp_ratio: Option<f32>,
    extra: Option<String>,
    is_selected: bool,
    is_hovered: bool,
    is_unit: bool,
    is_pickup: bool,
    faction_priority: u8,
    kind_name: String,
}

fn collect_candidates(
    config: &LabelConfig,
    label_visibility: &EntityLabelVisibility,
    camera: &Camera,
    cam_gt: &GlobalTransform,
    cam_pos: Vec3,
    window: &Window,
    graphics: &GraphicsSettings,
    scale: f32,
    screen_w: f32,
    screen_h: f32,
    active_player: &ActivePlayer,
    teams: &TeamConfig,
    fog_map: &FogOfWarMap,
    fog_settings: &FogTweakSettings,
    entities_q: &Query<
        (
            Entity,
            &GlobalTransform,
            Option<&EntityKind>,
            Option<&Faction>,
            Option<&Health>,
            Option<&BuildingLevel>,
            Option<&ResourceNode>,
            Option<&ItemPickupLabel>,
        ),
        (
            Or<(With<Unit>, With<Building>, With<Mob>, With<ResourceNode>, With<ItemPickup>)>,
            Without<FrustumCulled>,
        ),
    >,
    unit_q: &Query<(), With<Unit>>,
    building_q: &Query<(), With<Building>>,
    mob_q: &Query<(), With<Mob>>,
    selected_q: &Query<(), With<Selected>>,
    hovered_q: &Query<(), With<Hovered>>,
) -> Vec<RawCandidate> {
    let mut candidates = Vec::new();

    for (entity, gt, kind_opt, faction_opt, health_opt, level_opt, resource_opt, pickup_label_opt) in entities_q {
        let world_pos = gt.translation();
        let distance = cam_pos.distance(world_pos);

        let is_selected = selected_q.contains(entity);
        let is_hovered = hovered_q.contains(entity);
        let is_unit = unit_q.contains(entity);
        let is_building = building_q.contains(entity);
        let is_mob = mob_q.contains(entity);
        let is_resource = resource_opt.is_some() && kind_opt.is_none();
        let is_pickup = pickup_label_opt.is_some();

        // Units & mobs: show ambient labels only if toggle is on
        if (is_unit || is_mob) && !label_visibility.show_unit_labels && !is_hovered && !is_selected {
            continue;
        }

        // Resources/buildings: hover/select only
        if (is_resource || is_building) && !is_hovered && !is_selected {
            continue;
        }

        // Fog of war culling
        if !fog_settings.reveal_all {
            let is_own_or_allied = faction_opt
                .map(|f| *f == active_player.0 || teams.is_allied(&active_player.0, f))
                .unwrap_or(false);
            if !is_own_or_allied {
                let vis = fog_map.get_visibility(world_pos.x, world_pos.z);
                let threshold = if is_resource {
                    fog_settings.object_threshold
                } else {
                    fog_settings.mob_threshold
                };
                if vis < threshold {
                    continue;
                }
            }
        }

        // Y offset for label projection
        let y_offset = if is_building {
            config.label_y_offset_building
        } else if is_pickup {
            0.9
        } else {
            config.label_y_offset_unit
        };

        let label_world = world_pos + Vec3::Y * y_offset;

        // Project to screen
        let Some(screen_anchor) =
            camera::world_to_window_viewport(camera, cam_gt, world_pos, window, graphics)
        else {
            continue;
        };
        let Some(screen_label_raw) =
            camera::world_to_window_viewport(camera, cam_gt, label_world, window, graphics)
        else {
            continue;
        };

        let anchor = screen_anchor / scale;
        let label_origin = Vec2::new(screen_label_raw.x / scale, screen_label_raw.y / scale);

        // Off-screen cull
        if anchor.x < -20.0
            || anchor.x > screen_w + 20.0
            || anchor.y < -20.0
            || anchor.y > screen_h + 20.0
        {
            continue;
        }

        // Color
        let color = if let Some(pickup_label) = pickup_label_opt {
            pickup_label.color
        } else if is_mob {
            mob_color()
        } else if resource_opt.is_some() {
            resource_color()
        } else if let Some(faction) = faction_opt {
            relationship_color(faction, &active_player.0, teams)
        } else {
            mob_color()
        };

        // Name
        let name = if let Some(pickup_label) = pickup_label_opt {
            pickup_label.name.clone()
        } else if let Some(kind) = kind_opt {
            let mut n = kind.display_name().to_string();
            if let Some(level) = level_opt {
                if level.0 > 1 {
                    n.push_str(&format!(" L{}", level.0));
                }
            }
            n
        } else if let Some(rn) = resource_opt {
            format!("{} ({})", rn.resource_type.display_name(), rn.amount_remaining)
        } else {
            continue;
        };

        let kind_name = if let Some(kind) = kind_opt {
            kind.display_name().to_string()
        } else {
            name.clone()
        };

        // HP ratio
        let hp_ratio = health_opt.map(|h| (h.current / h.max).clamp(0.0, 1.0));

        // Extra info
        let extra = if let Some(pickup_label) = pickup_label_opt {
            Some(pickup_label.extra.clone())
        } else if let Some(rn) = resource_opt {
            if kind_opt.is_some() {
                Some(format!("{}: {}", rn.resource_type.display_name(), rn.amount_remaining))
            } else {
                None
            }
        } else {
            None
        };

        let fp = if let Some(faction) = faction_opt {
            faction_priority(faction, &active_player.0, teams)
        } else {
            5
        };

        candidates.push(RawCandidate {
            entity,
            world_pos,
            anchor,
            label_origin,
            distance,
            color,
            name,
            hp_ratio,
            extra,
            is_selected,
            is_hovered,
            is_unit,
            is_pickup,
            faction_priority: fp,
            kind_name,
        });
    }

    candidates
}

// ══════════════════════════════════════════════════════════════════════
// Phase 2 — Focus-label extraction
// ══════════════════════════════════════════════════════════════════════

/// Split candidates into focus (hovered/selected) and ambient groupable units.
fn extract_focus_and_ambient(
    candidates: Vec<RawCandidate>,
    selection_count: usize,
) -> (Vec<FocusData>, Vec<RawCandidate>) {
    let mut focus = Vec::new();
    let mut ambient = Vec::new();

    for c in candidates {
        let is_focus = c.is_hovered
            || (c.is_selected && selection_count <= 1)
            || (!c.is_unit); // non-units that passed filtering are already hover/select

        if is_focus {
            let priority = if c.is_selected {
                0
            } else if c.is_hovered {
                1
            } else {
                c.faction_priority
            };

            focus.push(FocusData {
                entity: c.entity,
                world_pos: c.world_pos,
                anchor: c.anchor,
                label_origin: c.label_origin,
                distance: c.distance,
                priority,
                color: c.color,
                name: c.name,
                hp_ratio: c.hp_ratio,
                extra: c.extra,
                interactive: !c.is_pickup,
            });
        } else if c.is_unit && !c.is_selected && !c.is_hovered {
            // Ambient groupable unit
            ambient.push(c);
        }
        // Multi-selected units: no individual world label (HUD shows selection info)
    }

    (focus, ambient)
}

// ══════════════════════════════════════════════════════════════════════
// Phase 3 — Ambient clustering with persistent state
// ══════════════════════════════════════════════════════════════════════

fn update_clusters(
    config: &LabelConfig,
    cluster_state: &mut ClusterState,
    ambient: &[RawCandidate],
    camera: &Camera,
    cam_gt: &GlobalTransform,
    window: &Window,
    graphics: &GraphicsSettings,
    scale: f32,
) -> Vec<ClusterData> {
    // Bucket by ClusterKey (kind_name + faction_priority)
    let mut buckets: HashMap<ClusterKey, Vec<&RawCandidate>> = HashMap::new();
    for c in ambient {
        let key = ClusterKey {
            kind_name: c.kind_name.clone(),
            faction_priority: c.faction_priority,
        };
        buckets.entry(key).or_default().push(c);
    }

    // Track which existing clusters got updated this frame
    let mut updated_cluster_ids: HashSet<u64> = HashSet::new();
    let mut new_clusters: HashMap<u64, PersistentCluster> = HashMap::new();

    // For each bucket, try to match to existing clusters or create new ones
    for (_key, bucket_entities) in &buckets {
        let entity_set: HashSet<Entity> = bucket_entities.iter().map(|c| c.entity).collect();
        let entity_positions: HashMap<Entity, Vec3> =
            bucket_entities.iter().map(|c| (c.entity, c.world_pos)).collect();

        // Find existing clusters that overlap with this bucket
        let mut matched_clusters: Vec<u64> = Vec::new();
        let mut unmatched_entities: HashSet<Entity> = entity_set.clone();

        for (&cid, existing) in &cluster_state.clusters {
            if updated_cluster_ids.contains(&cid) {
                continue;
            }
            // Check if this cluster's kind matches
            if !bucket_entities.is_empty()
                && existing.kind_name == bucket_entities[0].kind_name
                && existing.faction_priority == bucket_entities[0].faction_priority
            {
                // How many of this cluster's old members are still in the bucket?
                let overlap: HashSet<Entity> =
                    existing.members.intersection(&entity_set).copied().collect();
                if !overlap.is_empty() {
                    matched_clusters.push(cid);
                    for e in &overlap {
                        unmatched_entities.remove(e);
                    }
                }
            }
        }

        // Update matched clusters: add nearby unmatched entities, remove far ones
        for &cid in &matched_clusters {
            let existing = cluster_state.clusters.get(&cid).unwrap().clone();
            let mut members: HashSet<Entity> = existing
                .members
                .iter()
                .filter(|e| entity_set.contains(e))
                .copied()
                .collect();

            // Compute centroid from current members
            let centroid = if members.is_empty() {
                existing.centroid
            } else {
                let sum: Vec3 = members
                    .iter()
                    .filter_map(|e| entity_positions.get(e))
                    .copied()
                    .sum();
                sum / members.len() as f32
            };

            // Try to absorb nearby unmatched entities (merge)
            let absorb: Vec<Entity> = unmatched_entities
                .iter()
                .filter(|e| {
                    entity_positions
                        .get(e)
                        .map(|p| p.distance(centroid) < config.merge_distance)
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            for e in &absorb {
                members.insert(*e);
                unmatched_entities.remove(e);
            }

            // Split check: remove members that have been far for too long
            let mut far_frames = existing.far_frames.clone();
            let mut to_remove = Vec::new();
            for &m in &members {
                if let Some(pos) = entity_positions.get(&m) {
                    if pos.distance(centroid) > config.split_distance {
                        let count = far_frames.entry(m).or_insert(0);
                        *count = count.saturating_add(1);
                        if *count >= config.split_frames {
                            to_remove.push(m);
                        }
                    } else {
                        far_frames.remove(&m);
                    }
                }
            }
            for e in &to_remove {
                members.remove(e);
                far_frames.remove(e);
                unmatched_entities.insert(*e); // they'll form new clusters
            }

            if members.is_empty() {
                continue;
            }

            // Recompute centroid
            let final_centroid = {
                let sum: Vec3 = members
                    .iter()
                    .filter_map(|e| entity_positions.get(e))
                    .copied()
                    .sum();
                sum / members.len() as f32
            };
            // Smooth centroid movement
            let smoothed_centroid = existing.centroid.lerp(final_centroid, 0.3);

            new_clusters.insert(
                cid,
                PersistentCluster {
                    members,
                    kind_name: existing.kind_name.clone(),
                    faction_priority: existing.faction_priority,
                    color: existing.color,
                    centroid: smoothed_centroid,
                    screen_pos: existing.screen_pos,
                    frames_alive: existing.frames_alive + 1,
                    far_frames,
                },
            );
            updated_cluster_ids.insert(cid);
        }

        // Create new clusters from remaining unmatched entities via greedy grouping
        let mut remaining: Vec<Entity> = unmatched_entities.iter().copied().collect();
        remaining.sort_by(|a, b| {
            let pa = entity_positions.get(a).map(|p| p.x).unwrap_or(0.0);
            let pb = entity_positions.get(b).map(|p| p.x).unwrap_or(0.0);
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut used: HashSet<Entity> = HashSet::new();
        for &seed in &remaining {
            if used.contains(&seed) {
                continue;
            }
            let seed_pos = match entity_positions.get(&seed) {
                Some(p) => *p,
                None => continue,
            };

            let mut cluster_members: HashSet<Entity> = HashSet::new();
            cluster_members.insert(seed);
            used.insert(seed);

            // Greedily add nearby entities
            for &other in &remaining {
                if used.contains(&other) {
                    continue;
                }
                if let Some(other_pos) = entity_positions.get(&other) {
                    if other_pos.distance(seed_pos) < config.cluster_radius {
                        cluster_members.insert(other);
                        used.insert(other);
                    }
                }
            }

            // Only create cluster label if 2+ members
            if cluster_members.len() < 2 {
                continue;
            }

            let centroid = {
                let sum: Vec3 = cluster_members
                    .iter()
                    .filter_map(|e| entity_positions.get(e))
                    .copied()
                    .sum();
                sum / cluster_members.len() as f32
            };

            let cid = cluster_state.next_id;
            cluster_state.next_id += 1;

            let rep = bucket_entities[0];

            new_clusters.insert(
                cid,
                PersistentCluster {
                    members: cluster_members,
                    kind_name: rep.kind_name.clone(),
                    faction_priority: rep.faction_priority,
                    color: rep.color,
                    centroid,
                    screen_pos: Vec2::ZERO,
                    frames_alive: 0,
                    far_frames: HashMap::new(),
                },
            );
            updated_cluster_ids.insert(cid);
        }
    }

    // Replace cluster state
    cluster_state.clusters = new_clusters;

    // Build ClusterData output
    let mut result = Vec::new();
    for (&cid, cluster) in &mut cluster_state.clusters {
        let label_world = cluster.centroid + Vec3::Y * 2.5;

        let Some(screen_anchor) =
            camera::world_to_window_viewport(camera, cam_gt, cluster.centroid, window, graphics)
        else {
            continue;
        };
        let Some(screen_label) =
            camera::world_to_window_viewport(camera, cam_gt, label_world, window, graphics)
        else {
            continue;
        };

        let anchor = screen_anchor / scale;
        let label_origin = Vec2::new(screen_label.x / scale, screen_label.y / scale);

        // Smooth screen position
        if cluster.frames_alive == 0 {
            cluster.screen_pos = label_origin;
        } else {
            cluster.screen_pos = cluster.screen_pos.lerp(label_origin, 0.15);
        }

        result.push(ClusterData {
            cluster_id: cid,
            centroid_world: cluster.centroid,
            anchor,
            label_origin: cluster.screen_pos,
            distance: 0.0, // not used for sorting clusters
            color: cluster.color,
            kind_name: cluster.kind_name.clone(),
            count: cluster.members.len(),
            faction_priority: cluster.faction_priority,
        });
    }

    result
}

// ══════════════════════════════════════════════════════════════════════
// Phase 4 — Layout
// ══════════════════════════════════════════════════════════════════════

fn layout_intents(
    intents: &mut Vec<LabelIntent>,
    config: &LabelConfig,
    layout_cache: &mut LabelLayoutCache,
    existing_labels: &Query<(Entity, &EntityLabel, Option<&ComputedNode>)>,
    screen_w: f32,
    screen_h: f32,
) -> HashMap<LabelSlotKey, LabelRect> {
    let focus_default_size = Vec2::new(80.0, 28.0);
    let cluster_default_size = Vec2::new(70.0, 20.0);
    let gap = config.slot_gap;

    // Gather actual measured sizes
    let actual_sizes: HashMap<Entity, Vec2> = existing_labels
        .iter()
        .filter_map(|(_, label, computed)| {
            let cn = computed?;
            let size = cn.size() * cn.inverse_scale_factor();
            if size.x > 1.0 && size.y > 1.0 {
                Some((label.target, size))
            } else {
                None
            }
        })
        .collect();

    let mut placed: Vec<LabelRect> = Vec::new();
    let mut final_rects: HashMap<LabelSlotKey, LabelRect> = HashMap::new();

    for intent in intents.iter_mut() {
        let (key, anchor, label_origin, default_size) = match intent {
            LabelIntent::Focus(f) => (
                LabelSlotKey::Focus(f.entity),
                f.anchor,
                f.label_origin,
                actual_sizes
                    .get(&f.entity)
                    .copied()
                    .unwrap_or(focus_default_size),
            ),
            LabelIntent::Cluster(c) => (
                LabelSlotKey::Cluster(c.cluster_id),
                c.anchor,
                c.label_origin,
                cluster_default_size,
            ),
        };

        let size = layout_cache
            .entries
            .get(&key)
            .map(|c| c.size)
            .unwrap_or(default_size);

        let projected_pos = Vec2::new(label_origin.x - size.x * 0.5, label_origin.y);
        let previous_pos = layout_cache.entries.get(&key).map(|c| c.pos).unwrap_or(projected_pos);
        let desired_pos = previous_pos.lerp(projected_pos, config.follow_strength);

        let resolved_target = find_best_label_slot(
            desired_pos,
            projected_pos,
            anchor,
            size,
            &placed,
            screen_w,
            screen_h,
            gap,
            config.anchor_pull,
        );

        let pos = previous_pos.lerp(resolved_target, config.follow_strength.max(0.05));

        // Write back smoothed origin
        match intent {
            LabelIntent::Focus(f) => f.label_origin = pos,
            LabelIntent::Cluster(c) => c.label_origin = pos,
        }

        let rect = LabelRect { min: pos, size };
        placed.push(rect);
        final_rects.insert(key, rect);
        layout_cache.entries.insert(key, CachedLabelPlacement { pos, size });
    }

    // Prune stale cache entries
    let active_keys: HashSet<LabelSlotKey> = final_rects.keys().copied().collect();
    layout_cache.entries.retain(|k, _| active_keys.contains(k));

    final_rects
}

// ══════════════════════════════════════════════════════════════════════
// Phase 5 — UI sync
// ══════════════════════════════════════════════════════════════════════

/// Main monolithic system — delegates to phase functions.
fn entity_label_system(
    mut commands: Commands,
    config: Res<LabelConfig>,
    mut layout_cache: ResMut<LabelLayoutCache>,
    mut cluster_state: ResMut<ClusterState>,
    label_visibility: Res<EntityLabelVisibility>,
    theme: Res<Theme>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
        Res<UiScale>,
    ),
    game_state: (
        Res<ActivePlayer>,
        Res<TeamConfig>,
        Res<FogOfWarMap>,
        Res<FogTweakSettings>,
    ),
    entities_q: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&EntityKind>,
            Option<&Faction>,
            Option<&Health>,
            Option<&BuildingLevel>,
            Option<&ResourceNode>,
            Option<&ItemPickupLabel>,
        ),
        (
            Or<(With<Unit>, With<Building>, With<Mob>, With<ResourceNode>, With<ItemPickup>)>,
            Without<FrustumCulled>,
        ),
    >,
    marker_queries: (
        Query<(), With<Unit>>,
        Query<(), With<Building>>,
        Query<(), With<Mob>>,
        Query<(), With<Selected>>,
        Query<(), With<Hovered>>,
    ),
    label_ui: (
        Query<(Entity, &EntityLabel, Option<&ComputedNode>)>,
        Query<Entity, With<LeaderLine>>,
    ),
    children_q: Query<&Children>,
    mut label_parts: ParamSet<(
        Query<(&mut Text, &mut TextColor), With<LabelNameText>>,
        Query<(&mut Node, &mut BackgroundColor), With<LabelHpFill>>,
        Query<&mut Text, With<LabelExtraText>>,
    )>,
    ui_fonts: Res<fonts::UiFonts>,
) {
    let (camera_q, windows, graphics, ui_scale) = viewport;
    let (active_player, teams, fog_map, fog_settings) = game_state;
    let (unit_q, building_q, mob_q, selected_q, hovered_q) = marker_queries;
    let (existing_labels, existing_lines) = label_ui;

    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let cam_pos = cam_gt.translation();
    let scale = ui_scale.0.max(0.001) as f32;
    let screen_w = window.width() / scale;
    let screen_h = window.height() / scale;

    // ── Phase 1: Collect raw candidates ──
    let candidates = collect_candidates(
        &config,
        &label_visibility,
        camera,
        cam_gt,
        cam_pos,
        window,
        &graphics,
        scale,
        screen_w,
        screen_h,
        &active_player,
        &teams,
        &fog_map,
        &fog_settings,
        &entities_q,
        &unit_q,
        &building_q,
        &mob_q,
        &selected_q,
        &hovered_q,
    );

    // ── Phase 2: Split into focus and ambient ──
    let selection_count = selected_q.iter().count();
    let (mut focus_labels, ambient) = extract_focus_and_ambient(candidates, selection_count);

    // Sort focus by priority then distance, cap
    focus_labels.sort_by(|a, b| {
        a.priority.cmp(&b.priority).then(
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    focus_labels.truncate(config.max_focus_labels);

    // ── Phase 3: Cluster ambient units ──
    let mut cluster_labels = update_clusters(
        &config,
        &mut cluster_state,
        &ambient,
        camera,
        cam_gt,
        window,
        &graphics,
        scale,
    );

    // Sort clusters by faction priority then count (larger clusters first)
    cluster_labels.sort_by(|a, b| {
        a.faction_priority
            .cmp(&b.faction_priority)
            .then(b.count.cmp(&a.count))
    });
    cluster_labels.truncate(config.max_cluster_labels);

    // ── Build intent list (focus first, then clusters) ──
    let mut intents: Vec<LabelIntent> = Vec::new();
    for f in &focus_labels {
        intents.push(LabelIntent::Focus(f.clone()));
    }
    for c in &cluster_labels {
        intents.push(LabelIntent::Cluster(c.clone()));
    }

    // ── Phase 4: Layout ──
    let final_rects = layout_intents(
        &mut intents,
        &config,
        &mut layout_cache,
        &existing_labels,
        screen_w,
        screen_h,
    );

    // ── Phase 5: Sync UI ──

    // Build set of target entities/cluster-ids that should have labels
    let mut target_entities: HashSet<Entity> = HashSet::new();
    for intent in &intents {
        if let LabelIntent::Focus(f) = intent {
            target_entities.insert(f.entity);
        }
    }

    // Collect existing label targets for reuse
    let mut existing_focus_labels: HashMap<Entity, Entity> = HashMap::new();
    let mut stale_labels: Vec<Entity> = Vec::new();

    for (label_entity, label, _) in &existing_labels {
        if target_entities.contains(&label.target) {
            existing_focus_labels.insert(label.target, label_entity);
        } else if label.target == Entity::PLACEHOLDER {
            // Cluster label — try to match by stored cluster_id
            // We can't easily recover cluster_id from EntityLabel alone,
            // so we'll despawn all cluster labels and recreate.
            // This is fine because cluster labels are cheap pills.
            stale_labels.push(label_entity);
        } else {
            stale_labels.push(label_entity);
        }
    }
    for e in stale_labels {
        commands.entity(e).despawn();
    }

    // Despawn all leader lines (recreated each frame for focus labels)
    for line_entity in &existing_lines {
        commands.entity(line_entity).despawn();
    }

    // Sync each intent
    for intent in &intents {
        match intent {
            LabelIntent::Focus(f) => {
                let key = LabelSlotKey::Focus(f.entity);
                let Some(rect) = final_rects.get(&key) else {
                    continue;
                };

                // Spawn leader line for focus labels
                if existing_focus_labels.contains_key(&f.entity) {
                    if let Some(leader) =
                        compute_leader_geometry(*rect, f.anchor, config.line_width, scale)
                    {
                        let line_thickness = (config.line_width * scale).max(1.0);
                        let line_center = window_to_overlay_position(
                            leader.center,
                            window.width(),
                            window.height(),
                        );
                        commands.spawn((
                            LeaderLine,
                            Sprite {
                                color: f.color.with_alpha(0.5),
                                custom_size: Some(Vec2::new(leader.length, line_thickness)),
                                ..default()
                            },
                            Transform {
                                translation: Vec3::new(line_center.x, line_center.y, 10.0),
                                rotation: Quat::from_rotation_z(leader.angle),
                                ..default()
                            },
                            RenderLayers::layer(camera::PRESENTATION_LAYER),
                        ));
                    }
                }

                if let Some(&label_entity) = existing_focus_labels.get(&f.entity) {
                    // Update position
                    if let Ok(mut entity_cmds) = commands.get_entity(label_entity) {
                        entity_cmds.insert(Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(rect.min.x),
                            top: Val::Px(rect.min.y),
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            min_width: Val::Px(60.0),
                            max_width: Val::Px(120.0),
                            ..default()
                        });
                        entity_cmds.insert((
                            EntityLabel {
                                target: f.entity,
                                base_color: f.color,
                                interactive: f.interactive,
                            },
                            BorderColor::all(f.color.with_alpha(0.6)),
                        ));
                    }

                    if !sync_focus_label_contents(
                        label_entity,
                        f,
                        &theme,
                        &children_q,
                        &mut label_parts,
                    ) {
                        commands.entity(label_entity).despawn();
                        spawn_focus_label(&mut commands, f, rect, &theme, &ui_fonts);
                    }
                } else {
                    // Spawn new focus label
                    spawn_focus_label(&mut commands, f, rect, &theme, &ui_fonts);
                }
            }
            LabelIntent::Cluster(c) => {
                let key = LabelSlotKey::Cluster(c.cluster_id);
                let Some(rect) = final_rects.get(&key) else {
                    continue;
                };
                // Cluster labels are always respawned (they were despawned above).
                // No leader line for cluster labels.
                spawn_cluster_label(&mut commands, c, rect, &theme, &ui_fonts);
            }
        }
    }
}

fn sync_focus_label_contents(
    label_entity: Entity,
    f: &FocusData,
    theme: &Theme,
    children_q: &Query<&Children>,
    label_parts: &mut ParamSet<(
        Query<(&mut Text, &mut TextColor), With<LabelNameText>>,
        Query<(&mut Node, &mut BackgroundColor), With<LabelHpFill>>,
        Query<&mut Text, With<LabelExtraText>>,
    )>,
) -> bool {
    let mut found_name = false;
    let mut found_hp_fill = false;
    let mut found_extra = false;
    let mut stack = vec![label_entity];

    while let Some(entity) = stack.pop() {
        if entity != label_entity {
            if let Ok((mut text, mut text_color)) = label_parts.p0().get_mut(entity) {
                **text = f.name.clone();
                text_color.0 = f.color;
                found_name = true;
            }

            if let Ok((mut node, mut bg)) = label_parts.p1().get_mut(entity) {
                if let Some(hp_ratio) = f.hp_ratio {
                    node.width = Val::Percent(hp_ratio * 100.0);
                    bg.0 = hp_color(hp_ratio, theme);
                    found_hp_fill = true;
                }
            }

            if let Ok(mut text) = label_parts.p2().get_mut(entity) {
                if let Some(extra) = &f.extra {
                    **text = extra.clone();
                    found_extra = true;
                }
            }
        }

        if let Ok(children) = children_q.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }
    }

    found_name && found_hp_fill == f.hp_ratio.is_some() && found_extra == f.extra.is_some()
}

fn spawn_focus_label(
    commands: &mut Commands,
    f: &FocusData,
    rect: &LabelRect,
    theme: &Theme,
    ui_fonts: &fonts::UiFonts,
) {
    let font = fonts::body_emphasis(ui_fonts, theme.typography.small);
    let hp_ratio = f.hp_ratio.unwrap_or(1.0);

    commands
        .spawn((
            EntityLabel {
                target: f.entity,
                base_color: f.color,
                interactive: f.interactive,
            },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.min.x),
                top: Val::Px(rect.min.y),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                min_width: Val::Px(60.0),
                max_width: Val::Px(120.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.85)),
            BorderColor::all(f.color.with_alpha(0.6)),
            GlobalZIndex(-4),
            if f.interactive {
                Interaction::default()
            } else {
                Interaction::None
            },
        ))
        .with_children(|parent| {
            // Name text
            parent.spawn((
                LabelNameText,
                Text::new(&f.name),
                font.clone(),
                TextColor(f.color),
                TextLayout::new_with_justify(Justify::Center),
                Node {
                    max_width: Val::Px(110.0),
                    ..default()
                },
            ));

            // HP bar
            if f.hp_ratio.is_some() {
                parent
                    .spawn(Node {
                        width: Val::Px(50.0),
                        height: Val::Px(3.0),
                        margin: UiRect::top(Val::Px(2.0)),
                        ..default()
                    })
                    .insert(BackgroundColor(theme.colors.hp_bar_bg))
                    .with_children(|hp_parent| {
                        hp_parent.spawn((
                            LabelHpFill,
                            Node {
                                width: Val::Percent(hp_ratio * 100.0),
                                height: Val::Px(3.0),
                                ..default()
                            },
                            BackgroundColor(hp_color(hp_ratio, theme)),
                        ));
                    });
            }

            // Extra text
            if let Some(ref extra) = f.extra {
                parent.spawn((
                    LabelExtraText,
                    Text::new(extra),
                    fonts::body_emphasis(ui_fonts, theme.typography.tiny),
                    TextColor(theme.colors.text_secondary),
                    TextLayout::new_with_justify(Justify::Center),
                ));
            }
        });
}

fn spawn_cluster_label(
    commands: &mut Commands,
    c: &ClusterData,
    rect: &LabelRect,
    theme: &Theme,
    ui_fonts: &fonts::UiFonts,
) {
    let label_text = format!("{} x{}", c.kind_name, c.count);
    let font = fonts::body_emphasis(ui_fonts, theme.typography.tiny);
    let pill_color = c.color.with_alpha(0.45);
    let text_color = c.color.with_alpha(0.8);

    commands
        .spawn((
            EntityLabel {
                target: Entity::PLACEHOLDER,
                base_color: c.color,
                interactive: false,
            },
            ClusterLabelMarker,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.min.x),
                top: Val::Px(rect.min.y),
                padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.04, 0.06, 0.6)),
            BorderColor::all(pill_color),
            GlobalZIndex(-5),
        ))
        .with_children(|parent| {
            parent.spawn((
                LabelNameText,
                Text::new(&label_text),
                font,
                TextColor(text_color),
                TextLayout::new_with_justify(Justify::Center),
            ));
        });
}

// ══════════════════════════════════════════════════════════════════════
// Phase 6 — Interaction handling
// ══════════════════════════════════════════════════════════════════════

fn handle_label_interaction(
    mut commands: Commands,
    mut labels: Query<(Entity, &EntityLabel, Option<&Interaction>)>,
    mut bg_query: Query<&mut BackgroundColor>,
    mut border_query: Query<&mut BorderColor>,
    hovered_via_label: Query<Entity, With<HoveredViaLabel>>,
    selected: Query<Entity, With<Selected>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    for (label_entity, label, interaction) in &mut labels {
        if !label.interactive {
            continue;
        }

        let Some(interaction) = interaction else {
            continue;
        };

        match *interaction {
            Interaction::Hovered => {
                if let Ok(mut bg) = bg_query.get_mut(label_entity) {
                    bg.0 = Color::srgba(0.12, 0.12, 0.18, 0.95);
                }
                if let Ok(mut border) = border_query.get_mut(label_entity) {
                    *border = BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.8));
                }
                if let Ok(mut cmds) = commands.get_entity(label.target) {
                    cmds.insert((Hovered, HoveredViaLabel));
                }
            }
            Interaction::Pressed => {
                let shift =
                    keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
                if !shift {
                    for sel_entity in &selected {
                        commands.entity(sel_entity).remove::<Selected>();
                    }
                }
                if let Ok(mut cmds) = commands.get_entity(label.target) {
                    cmds.insert(Selected);
                }
            }
            Interaction::None => {
                if let Ok(mut bg) = bg_query.get_mut(label_entity) {
                    bg.0 = Color::srgba(0.05, 0.05, 0.07, 0.85);
                }
                if let Ok(mut border) = border_query.get_mut(label_entity) {
                    *border = BorderColor::all(label.base_color.with_alpha(0.6));
                }
                if hovered_via_label.contains(label.target) {
                    if let Ok(mut cmds) = commands.get_entity(label.target) {
                        cmds.remove::<(Hovered, HoveredViaLabel)>();
                    }
                }
            }
        }
    }
}
