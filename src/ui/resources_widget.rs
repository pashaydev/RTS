use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::theme;

pub fn update_resource_texts(
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    all_units: Query<&Faction, With<Unit>>,
    all_training_queues: Query<(&Faction, &TrainingQueue), With<Building>>,
    all_buildings_for_cap: Query<(&Faction, &EntityKind, &BuildingState, &BuildingLevel), With<Building>>,
    mut text_sets: ParamSet<(
        Query<(&mut Text, &ResourceText)>,
        Query<&mut Text, With<PopulationText>>,
    )>,
) {
    let player_res = all_resources.get(&active_player.0);
    let carried = carried_totals.get(&active_player.0);
    for (mut text, rt_marker) in &mut text_sets.p0() {
        let rt = rt_marker.0;
        let val = player_res.get(rt);
        let carried_val = carried.get(rt);
        if carried_val > 0 {
            **text = format!("{} (+{})", val, carried_val);
        } else {
            **text = format!("{}", val);
        }
    }

    let unit_cap = faction_unit_cap_stats(
        active_player.0,
        all_units.iter(),
        all_training_queues.iter(),
        all_buildings_for_cap.iter(),
    );
    for mut text in &mut text_sets.p1() {
        if unit_cap.queued > 0 {
            **text = format!(
                "Units: {} (+{}) / {}",
                unit_cap.used, unit_cap.queued, unit_cap.cap
            );
        } else {
            **text = format!("Units: {} / {}", unit_cap.used, unit_cap.cap);
        }
    }
}

/// Show/hide processed resource rows based on whether the player has unlocked them.
pub fn update_processed_resource_visibility(
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    mut vis_q: Query<(&mut Visibility, &ProcessedResourceRow)>,
) {
    let player_res = all_resources.get(&active_player.0);
    for (mut vis, row_marker) in &mut vis_q {
        let rt = row_marker.0;
        // Show if player has any of this resource or has ever produced it
        let has_any = player_res.get(rt) > 0;
        *vis = if has_any {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Marker for processed resource rows that can be hidden.
#[derive(Component)]
pub struct ProcessedResourceRow(pub ResourceType);

#[derive(Component)]
pub struct PopulationText;

pub fn spawn_resource_content(commands: &mut Commands, parent: Entity, icons: &IconAssets) {
    let population = commands
        .spawn((
            PopulationText,
            Text::new("Units: 0 / 8"),
            TextFont {
                font_size: theme::FONT_MEDIUM,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
            Node {
                width: Val::Percent(100.0),
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(population);

    // Row 1: Raw resources (always shown)
    for rt in ResourceType::RAW {
        spawn_resource_row(commands, parent, rt, icons, false);
    }

    // Separator between raw and processed
    let sep = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                margin: UiRect::new(Val::ZERO, Val::ZERO, Val::Px(4.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::SEPARATOR),
        ))
        .id();
    commands.entity(parent).add_child(sep);

    // Row 2: Processed resources (hidden until unlocked)
    for rt in ResourceType::PROCESSED {
        spawn_resource_row(commands, parent, rt, icons, true);
    }
}

fn spawn_resource_row(
    commands: &mut Commands,
    parent: Entity,
    rt: ResourceType,
    icons: &IconAssets,
    is_processed: bool,
) {
    let mut row_cmds = commands.spawn(Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(6.0),
        ..default()
    });
    if is_processed {
        row_cmds.insert((
            ProcessedResourceRow(rt),
            Visibility::Hidden,
        ));
    }
    let row = row_cmds.id();
    commands.entity(parent).add_child(row);

    let left = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .id();
    commands.entity(row).add_child(left);

    // Color swatch for processed resources (since they reuse icons)
    let color = rt.carry_color();
    let icon = commands
        .spawn((
            ImageNode::new(icons.resource_icon(rt)),
            Node {
                width: Val::Px(18.0),
                height: Val::Px(18.0),
                ..default()
            },
        ))
        .id();
    commands.entity(left).add_child(icon);

    // For processed resources, add a small color dot
    if is_processed {
        let dot = commands
            .spawn((
                Node {
                    width: Val::Px(6.0),
                    height: Val::Px(6.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(color),
            ))
            .id();
        commands.entity(left).add_child(dot);
    }

    let text = commands
        .spawn((
            ResourceText(rt),
            Text::new(format!("{:?}: 0", rt)),
            TextFont {
                font_size: theme::FONT_MEDIUM,
                ..default()
            },
            TextColor(theme::TEXT_PRIMARY),
        ))
        .id();
    commands.entity(left).add_child(text);
}
