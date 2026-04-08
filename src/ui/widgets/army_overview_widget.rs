use bevy::prelude::*;

use super::core::constants::*;
use super::core::framework::{
    find_widget_content, Widget, WidgetContent, WidgetId, WidgetRegistry,
};
use super::core::hud::MainHudRoot;
use crate::blueprints::EntityKind;
use crate::types::*;
use crate::ui::theme::{self, Theme};

pub struct ArmyOverviewWidgetPlugin;

impl Plugin for ArmyOverviewWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArmyOverviewRenderState>()
            .add_systems(
                Update,
                spawn_army_overview_widget
                    .run_if(in_state(AppState::InGame))
                    .run_if(any_with_component::<MainHudRoot>),
            )
            .add_systems(
                Update,
                (
                    handle_army_overview_hover,
                    handle_army_overview_click,
                    update_army_overview.after(handle_army_overview_click),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

widget_spawn_system!(spawn_army_overview_widget, WidgetId::ArmyOverview);

#[derive(Component)]
pub struct ArmyOverviewContent;

#[derive(Resource, Default)]
struct ArmyOverviewRenderState {
    last_snapshot: Option<ArmyOverviewSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArmyOverviewSnapshot {
    counts: Vec<(EntityKind, u32, u32)>,
    total: u32,
    unit_cap: UnitCapStats,
}

struct UnitCount {
    kind: EntityKind,
    total: u32,
    idle_workers: u32,
}

fn count_player_units(
    units: &Query<
        (&EntityKind, &Faction, Option<&UnitState>, Option<&BuildingAssignment>),
        With<Unit>,
    >,
    faction: Faction,
) -> Vec<UnitCount> {
    let mut counts: Vec<UnitCount> = Vec::new();
    for (kind, unit_faction, unit_state, building_assignment) in units {
        if *unit_faction != faction {
            continue;
        }
        let is_idle_worker = *kind == EntityKind::Worker
            && unit_state.is_some_and(|s| *s == UnitState::Idle)
            && building_assignment.is_none();

        if let Some(entry) = counts.iter_mut().find(|c| c.kind == *kind) {
            entry.total += 1;
            if is_idle_worker {
                entry.idle_workers += 1;
            }
        } else {
            counts.push(UnitCount {
                kind: *kind,
                total: 1,
                idle_workers: u32::from(is_idle_worker),
            });
        }
    }
    counts.sort_by(|a, b| a.kind.display_name().cmp(b.kind.display_name()));
    counts
}

fn handle_army_overview_hover(
    mut commands: Commands,
    active_player: Res<ActivePlayer>,
    interactions: Query<(&Interaction, &ArmyOverviewEntry), With<Button>>,
    units: Query<(Entity, &EntityKind, &Faction), With<Unit>>,
) {
    let mut hovered_kind = None;
    for (interaction, entry) in &interactions {
        if matches!(*interaction, Interaction::Hovered | Interaction::Pressed) {
            hovered_kind = Some(entry.kind);
            break;
        }
    }

    for (entity, kind, faction) in &units {
        if *faction != active_player.0 {
            continue;
        }

        if Some(*kind) == hovered_kind {
            commands.entity(entity).insert(ArmyOverviewHighlighted);
        } else {
            commands.entity(entity).remove::<ArmyOverviewHighlighted>();
        }
    }
}

fn handle_army_overview_click(
    mut commands: Commands,
    interactions: Query<(&Interaction, &ArmyOverviewEntry), Changed<Interaction>>,
    selected: Query<Entity, With<Selected>>,
    units: Query<(Entity, &EntityKind, &Faction), With<Unit>>,
    active_player: Res<ActivePlayer>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, entry) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        ui_press.0 = true;

        for entity in &selected {
            commands.entity(entity).remove::<Selected>();
        }

        for (entity, kind, faction) in &units {
            if *faction == active_player.0 && *kind == entry.kind {
                commands.entity(entity).insert(Selected);
            }
        }
    }
}

fn update_army_overview(
    mut commands: Commands,
    active_player: Res<ActivePlayer>,
    theme: Res<Theme>,
    icons: Res<IconAssets>,
    mut render_state: ResMut<ArmyOverviewRenderState>,
    widget_q: Query<(&Widget, &Children)>,
    content_q: Query<Entity, With<WidgetContent>>,
    existing: Query<Entity, With<ArmyOverviewContent>>,
    units: Query<
        (&EntityKind, &Faction, Option<&UnitState>, Option<&BuildingAssignment>),
        With<Unit>,
    >,
    training_queues: Query<(&Faction, &TrainingQueue), With<Building>>,
    buildings: Query<(&Faction, &EntityKind, &BuildingState, &BuildingLevel), With<Building>>,
    registry: Res<WidgetRegistry>,
) {
    if !registry.is_visible(WidgetId::ArmyOverview) {
        return;
    }

    let Some(content) = find_widget_content(WidgetId::ArmyOverview, &widget_q, &content_q) else {
        return;
    };

    let counts = count_player_units(&units, active_player.0);
    let total: u32 = counts.iter().map(|c| c.total).sum();
    let unit_cap = faction_unit_cap_stats(
        active_player.0,
        units.iter().map(|(_, faction, _, _)| faction),
        training_queues.iter(),
        buildings.iter(),
    );
    let snapshot = ArmyOverviewSnapshot {
        counts: counts
            .iter()
            .map(|count| (count.kind, count.total, count.idle_workers))
            .collect(),
        total,
        unit_cap,
    };

    if render_state.last_snapshot.as_ref() == Some(&snapshot) {
        return;
    }
    render_state.last_snapshot = Some(snapshot);

    for entity in &existing {
        commands.entity(entity).try_despawn();
    }

    let container = commands
        .spawn((
            ArmyOverviewContent,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(container);

    for count in &counts {
        let entry = commands
            .spawn((
                ArmyOverviewEntry { kind: count.kind },
                Button,
                StandardButton,
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(2.0),
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                    border: BORDER_1,
                    // border_radius: RADIUS_MD,
                    ..default()
                },
                BackgroundColor(theme.colors.bg_surface),
                BorderColor::all(Color::NONE),
            ))
            .id();
        commands.entity(container).add_child(entry);

        let icon = commands
            .spawn((
                ImageNode::new(icons.entity_icon(count.kind)),
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(entry).add_child(icon);

        let count_text = commands
            .spawn((
                Text::new(format!("x{}", count.total)),
                TextFont {
                    font_size: theme.typography.caption,
                    ..default()
                },
                TextColor(theme.colors.text_primary),
            ))
            .id();
        commands.entity(entry).add_child(count_text);

        if count.idle_workers > 0 {
            let idle_badge = commands
                .spawn((
                    Text::new(format!("({})", count.idle_workers)),
                    TextFont {
                        font_size: theme.typography.tiny,
                        ..default()
                    },
                    TextColor(theme.colors.warning),
                ))
                .id();
            commands.entity(entry).add_child(idle_badge);
        }
    }

    // Total row
    let total_text = commands
        .spawn((
            ArmyOverviewContent,
            Text::new(format!("Total: {} / {}", total, unit_cap.cap)),
            TextFont {
                font_size: theme.typography.caption,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
            Node {
                margin: UiRect::top(Val::Px(2.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(content).add_child(total_text);

    // Idle workers warning
    let idle_count: u32 = counts
        .iter()
        .filter(|c| c.kind == EntityKind::Worker)
        .map(|c| c.idle_workers)
        .sum();
    if idle_count > 0 {
        let idle_row = commands
            .spawn((
                ArmyOverviewContent,
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                    margin: UiRect::top(Val::Px(2.0)),
                    // border_radius: RADIUS_XS,
                    ..default()
                },
                BackgroundColor(theme::PRESTIGE.with_alpha(0.25)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(format!(
                        "! {} idle worker{}",
                        idle_count,
                        if idle_count == 1 { "" } else { "s" }
                    )),
                    TextFont {
                        font_size: theme.typography.caption,
                        ..default()
                    },
                    TextColor(theme.colors.warning),
                ));
            })
            .id();
        commands.entity(content).add_child(idle_row);
    }
}
