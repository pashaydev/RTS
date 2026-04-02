use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::window::PrimaryWindow;

use crate::blueprints::EntityKind;
use crate::camera;
use crate::components::*;
use crate::fog::FogTweakSettings;
use crate::selection::SelectionSet;
use crate::theme::Theme;
use crate::ui::fonts;

// ── Components ──

#[derive(Component)]
pub struct EntityLabel {
    pub target: Entity,
    pub base_color: Color,
}

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

// ── Configuration ──

#[derive(Resource)]
struct LabelConfig {
    max_on_screen: usize,
    label_y_offset_unit: f32,
    label_y_offset_building: f32,
    line_width: f32,
}

impl Default for LabelConfig {
    fn default() -> Self {
        Self {
            max_on_screen: 30,
            label_y_offset_unit: 2.5,
            label_y_offset_building: 5.0,
            line_width: 1.5,
        }
    }
}

// ── Plugin ──

pub struct EntityLabelPlugin;

impl Plugin for EntityLabelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LabelConfig>()
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
            );
    }
}

// ── Faction relationship colors ──

fn relationship_color(
    entity_faction: &Faction,
    active_faction: &Faction,
    teams: &TeamConfig,
) -> Color {
    if entity_faction == active_faction {
        Color::srgb(0.3, 0.6, 1.0) // own — blue
    } else if teams.is_allied(active_faction, entity_faction) {
        Color::srgb(0.2, 0.8, 0.3) // allied — green
    } else {
        Color::srgb(1.0, 0.3, 0.2) // hostile — red
    }
}

fn mob_color() -> Color {
    Color::srgb(0.6, 0.6, 0.6) // neutral mobs — gray
}

fn resource_color() -> Color {
    Color::srgb(0.9, 0.7, 0.2) // resources — gold
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

// ── Candidate scoring ──

struct LabelCandidate {
    entity: Entity,
    _world_pos: Vec3,
    anchor: Vec2,
    label_origin: Vec2,
    distance: f32,
    priority: u8,
    color: Color,
    name: String,
    hp_ratio: Option<f32>,
    extra: Option<String>,
}

#[derive(Clone, Copy)]
struct LabelRect {
    min: Vec2,
    size: Vec2,
}

impl LabelRect {
    fn center(self) -> Vec2 {
        self.min + self.size * 0.5
    }

    fn bottom_center(self) -> Vec2 {
        Vec2::new(self.min.x + self.size.x * 0.5, self.min.y + self.size.y)
    }
}

#[derive(Clone, Copy)]
struct LeaderGeometry {
    center: Vec2,
    length: f32,
    angle: f32,
}

/// Single monolithic system that manages entity labels and leader lines.
/// Combines candidate selection, projection, overlap resolution, and UI sync
/// to avoid complex inter-system data passing.
fn entity_label_system(
    mut commands: Commands,
    config: Res<LabelConfig>,
    theme: Res<Theme>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
        Res<UiScale>,
    ),
    game_state: (Res<ActivePlayer>, Res<TeamConfig>, Res<FogOfWarMap>, Res<FogTweakSettings>),
    entities_q: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&EntityKind>,
            Option<&Faction>,
            Option<&Health>,
            Option<&BuildingLevel>,
            Option<&ResourceNode>,
        ),
        (
            Or<(With<Unit>, With<Building>, With<Mob>, With<ResourceNode>)>,
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
    ui_fonts: Res<fonts::UiFonts>,
) {
    let (camera_q, windows, graphics, ui_scale) = viewport;
    let (active_player, teams, fog_map, fog_settings) = game_state;
    let (_unit_q, building_q, mob_q, selected_q, hovered_q) = marker_queries;
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

    // Phase 1: Build candidates
    let mut candidates: Vec<LabelCandidate> = Vec::new();

    for (entity, gt, kind_opt, faction_opt, health_opt, level_opt, resource_opt) in &entities_q {
        let world_pos = gt.translation();
        let distance = cam_pos.distance(world_pos);


        let is_selected = selected_q.contains(entity);
        let is_hovered = hovered_q.contains(entity);
        let is_building = building_q.contains(entity);
        let is_mob = mob_q.contains(entity);
        let is_resource = resource_opt.is_some() && kind_opt.is_none();

        // Resources and buildings only shown on hover/select
        if (is_resource || is_building) && !is_hovered && !is_selected {
            continue;
        }

        // Hide labels for entities covered by fog of war
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

        // Priority: selected=0, hovered=1, own=2, allied=3, enemy=4, neutral=5
        let priority = if is_selected {
            0
        } else if is_hovered {
            1
        } else if let Some(faction) = faction_opt {
            if *faction == active_player.0 {
                2
            } else if teams.is_allied(&active_player.0, faction) {
                3
            } else {
                4
            }
        } else {
            5
        };

        // Determine label Y offset
        let y_offset = if is_building {
            config.label_y_offset_building
        } else {
            config.label_y_offset_unit
        };

        let label_world = world_pos + Vec3::Y * y_offset;

        // Project to screen
        let Some(screen_anchor) = camera::world_to_window_viewport(
            camera,
            cam_gt,
            world_pos,
            window,
            &graphics,
        ) else {
            continue;
        };
        let Some(screen_label_raw) = camera::world_to_window_viewport(
            camera,
            cam_gt,
            label_world,
            window,
            &graphics,
        ) else {
            continue;
        };

        // Convert to UI-scale coordinates
        let anchor = screen_anchor / scale;
        let label_origin = Vec2::new(screen_label_raw.x / scale, screen_label_raw.y / scale);

        // Skip if off-screen
        if anchor.x < -20.0 || anchor.x > screen_w + 20.0
            || anchor.y < -20.0 || anchor.y > screen_h + 20.0
        {
            continue;
        }

        // Determine color
        let color = if is_mob {
            mob_color()
        } else if resource_opt.is_some() {
            resource_color()
        } else if let Some(faction) = faction_opt {
            relationship_color(faction, &active_player.0, &teams)
        } else {
            mob_color()
        };

        // Build name
        let name = if let Some(kind) = kind_opt {
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

        // HP ratio
        let hp_ratio = health_opt.map(|h| (h.current / h.max).clamp(0.0, 1.0));

        // Extra info line
        let extra = if let Some(rn) = resource_opt {
            if kind_opt.is_some() {
                Some(format!("{}: {}", rn.resource_type.display_name(), rn.amount_remaining))
            } else {
                None // already in name
            }
        } else {
            None
        };

        candidates.push(LabelCandidate {
            entity,
            _world_pos: world_pos,
            anchor,
            label_origin,
            distance,
            priority,
            color,
            name,
            hp_ratio,
            extra,
        });
    }

    // Phase 2: Sort by priority then distance, cap at max
    candidates.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal))
    });
    candidates.truncate(config.max_on_screen);

    // Re-sort by screen X position for stable, deterministic placement.
    // Priority sorting above decides WHICH labels to show; screen-position
    // sorting decides WHERE they go so hover/priority changes don't cause jumps.
    candidates.sort_by(|a, b| {
        a.anchor
            .x
            .partial_cmp(&b.anchor.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Phase 3: Resolve overlaps — push upward, stagger horizontally
    let default_size = Vec2::new(80.0, 28.0);
    let gap = 4.0_f32;

    // Use actual rendered sizes from existing labels when available
    let actual_sizes: std::collections::HashMap<Entity, Vec2> = existing_labels
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
    let mut final_rects: std::collections::HashMap<Entity, LabelRect> =
        std::collections::HashMap::with_capacity(candidates.len());
    let mut measured_targets: std::collections::HashSet<Entity> =
        std::collections::HashSet::with_capacity(actual_sizes.len());

    for candidate in &mut candidates {
        let size = actual_sizes
            .get(&candidate.entity)
            .copied()
            .unwrap_or(default_size);
        if actual_sizes.contains_key(&candidate.entity) {
            measured_targets.insert(candidate.entity);
        }
        let mut pos = candidate.label_origin;
        // Center label horizontally on its projected position
        pos.x -= size.x * 0.5;

        // Resolve overlaps: spread sideways first, then lift as needed.
        let mut iteration = 0;
        loop {
            let mut worst_overlap: Option<usize> = None;
            for (i, placed_rect) in placed.iter().enumerate() {
                if rects_overlap(pos, size, placed_rect.min, placed_rect.size) {
                    worst_overlap = Some(i);
                    break;
                }
            }
            let Some(idx) = worst_overlap else { break };
            if iteration >= 8 {
                break;
            }

            let placed_rect = placed[idx];
            let horizontal_dir = if candidate.anchor.x >= placed_rect.center().x {
                1.0
            } else {
                -1.0
            };
            let spread = size.x * (0.45 + 0.15 * iteration as f32) + gap;
            pos.x = placed_rect.min.x + horizontal_dir * spread;
            if iteration >= 3 {
                pos.y = placed_rect.min.y - size.y - gap * (iteration as f32 - 1.0);
            }
            iteration += 1;
        }

        // Clamp to screen
        pos.x = pos.x.clamp(2.0, (screen_w - size.x - 2.0).max(2.0));
        pos.y = pos.y.clamp(2.0, (screen_h - size.y - 2.0).max(2.0));

        candidate.label_origin = pos;
        let rect = LabelRect { min: pos, size };
        placed.push(rect);
        final_rects.insert(candidate.entity, rect);
    }

    // Phase 4: Sync UI nodes
    // Build a set of entities that should have labels
    let target_set: std::collections::HashSet<Entity> =
        candidates.iter().map(|c| c.entity).collect();

    // Despawn labels whose targets are no longer candidates
    for (label_entity, label, _) in &existing_labels {
        if !target_set.contains(&label.target) {
            commands.entity(label_entity).despawn();
        }
    }
    // Rebuild leaders from final layout each frame so they stay in sync with
    // resolved label positions and measured sizes.
    for line_entity in &existing_lines {
        commands.entity(line_entity).despawn();
    }

    // Find existing label targets for reuse
    let existing_targets: std::collections::HashMap<Entity, Entity> = existing_labels
        .iter()
        .map(|(label_entity, label, _)| (label.target, label_entity))
        .collect();
    for candidate in &candidates {
        let Some(rect) = final_rects.get(&candidate.entity).copied() else {
            continue;
        };
        let label_x = rect.min.x;
        let label_y = rect.min.y;
        let line_color = candidate.color.with_alpha(0.5);

        if measured_targets.contains(&candidate.entity) {
            if let Some(leader) = compute_leader_geometry(rect, candidate.anchor, config.line_width, scale)
            {
                let line_thickness = (config.line_width * scale).max(1.0);
                let line_center = window_to_overlay_position(leader.center, window.width(), window.height());
                commands.spawn((
                    LeaderLine,
                    Sprite {
                        color: line_color,
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

        if let Some(&label_entity) = existing_targets.get(&candidate.entity) {
            // Update existing label position and content
            if let Ok(mut entity_cmds) = commands.get_entity(label_entity) {
                entity_cmds.insert(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(label_x),
                    top: Val::Px(label_y),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    min_width: Val::Px(60.0),
                    max_width: Val::Px(120.0),
                    ..default()
                });
            }
        } else {
            // Spawn new label with children
            let font = fonts::body_emphasis(&ui_fonts, theme.typography.small);
            let hp_ratio = candidate.hp_ratio.unwrap_or(1.0);

            commands
                .spawn((
                    EntityLabel {
                        target: candidate.entity,
                        base_color: candidate.color,
                    },
                    Interaction::default(),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(label_x),
                        top: Val::Px(label_y),
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
                    BorderColor::all(candidate.color.with_alpha(0.6)),
                    GlobalZIndex(-4),
                ))
                .with_children(|parent| {
                    // Name text
                    parent.spawn((
                        LabelNameText,
                        Text::new(&candidate.name),
                        font.clone(),
                        TextColor(candidate.color),
                        TextLayout::new_with_justify(Justify::Center),
                        Node {
                            max_width: Val::Px(110.0),
                            ..default()
                        },
                    ));

                    // HP bar (if entity has health)
                    if candidate.hp_ratio.is_some() {
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
                                    BackgroundColor(hp_color(hp_ratio, &theme)),
                                ));
                            });
                    }

                    // Extra text (resource info, etc.)
                    if let Some(ref extra) = candidate.extra {
                        parent.spawn((
                            LabelExtraText,
                            Text::new(extra),
                            fonts::body_emphasis(&ui_fonts, theme.typography.tiny),
                            TextColor(theme.colors.text_secondary),
                            TextLayout::new_with_justify(Justify::Center),
                        ));
                    }
                });
        }
    }
}

fn rects_overlap(a_pos: Vec2, a_size: Vec2, b_pos: Vec2, b_size: Vec2) -> bool {
    a_pos.x < b_pos.x + b_size.x
        && a_pos.x + a_size.x > b_pos.x
        && a_pos.y < b_pos.y + b_size.y
        && a_pos.y + a_size.y > b_pos.y
}

fn compute_leader_geometry(
    rect: LabelRect,
    anchor: Vec2,
    line_width: f32,
    scale: f32,
) -> Option<LeaderGeometry> {
    let start = anchor * scale;
    let target = rect.center() * scale;
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

/// Handle hover and click interactions on entity labels.
/// Runs every frame (no Changed filter) so hover persists across frames.
fn handle_label_interaction(
    mut commands: Commands,
    mut labels: Query<(Entity, &EntityLabel, &Interaction)>,
    mut bg_query: Query<&mut BackgroundColor>,
    mut border_query: Query<&mut BorderColor>,
    hovered_via_label: Query<Entity, With<HoveredViaLabel>>,
    selected: Query<Entity, With<Selected>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    for (label_entity, label, interaction) in &mut labels {
        match *interaction {
            Interaction::Hovered => {
                // Highlight the label itself
                if let Ok(mut bg) = bg_query.get_mut(label_entity) {
                    bg.0 = Color::srgba(0.12, 0.12, 0.18, 0.95);
                }
                if let Ok(mut border) = border_query.get_mut(label_entity) {
                    *border = BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.8));
                }
                // Mark the target entity as hovered
                if let Ok(mut cmds) = commands.get_entity(label.target) {
                    cmds.insert((Hovered, HoveredViaLabel));
                }
            }
            Interaction::Pressed => {
                let shift = keyboard.pressed(KeyCode::ShiftLeft)
                    || keyboard.pressed(KeyCode::ShiftRight);
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
                // Restore default label visuals
                if let Ok(mut bg) = bg_query.get_mut(label_entity) {
                    bg.0 = Color::srgba(0.05, 0.05, 0.07, 0.85);
                }
                if let Ok(mut border) = border_query.get_mut(label_entity) {
                    *border = BorderColor::all(label.base_color.with_alpha(0.6));
                }
                // Remove hover from target only if it was set by label hover
                if hovered_via_label.contains(label.target) {
                    if let Ok(mut cmds) = commands.get_entity(label.target) {
                        cmds.remove::<(Hovered, HoveredViaLabel)>();
                    }
                }
            }
        }
    }
}
