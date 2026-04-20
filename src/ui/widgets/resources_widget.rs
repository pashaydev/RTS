//! Resource counters widget with income rates: raw resources always
//! visible, processed resources revealed once the player stocks any.

use bevy::prelude::*;

use super::core::constants::*;
use super::core::fonts::UiFonts;
use super::core::framework::HEADER_HEIGHT_PX;
use super::core::hud::MainHudRoot;
use crate::blueprints::EntityKind;
use crate::types::*;
use crate::ui::theme::{self, Theme};
use crate::world::lighting::{DayCycle, DayPhase};

pub struct ResourceHeaderBarPlugin;

impl Plugin for ResourceHeaderBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_resource_header_bar
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(
            Update,
            (
                update_resource_texts,
                update_processed_resource_visibility,
                update_daycycle_indicator,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Components ──

#[derive(Component)]
pub struct ResourceHeaderBar;

#[derive(Component)]
pub struct PopulationText;

/// Marker for processed resource rows that can be hidden.
#[derive(Component)]
pub struct ProcessedResourceRow(pub ResourceType);

#[derive(Component)]
pub struct DayCycleIcon;

#[derive(Component)]
pub struct DayCycleText;

// ── Spawn ──

fn spawn_resource_header_bar(
    mut commands: Commands,
    icons: Res<IconAssets>,
    daycycle_icons: Res<DayCycleIconAssets>,
    fonts: Res<UiFonts>,
    theme: Res<Theme>,
    root_q: Query<Entity, Added<MainHudRoot>>,
) {
    let Ok(hud_root) = root_q.single() else {
        return;
    };

    // Full-width header bar at the very top
    let bar = commands
        .spawn((
            ResourceHeaderBar,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Px(HEADER_HEIGHT_PX),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(Val::Px(8.0), Val::Px(0.0)),
                ..default()
            },
            BackgroundColor(theme::BG_PANEL.with_alpha(0.85)),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.4),
                Val::Px(0.0),
                Val::Px(2.0),
                Val::Px(0.0),
                Val::Px(6.0),
            ),
            GlobalZIndex(10),
        ))
        .id();
    commands.entity(hud_root).add_child(bar);

    // ── Left section: resources ──
    let left = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .id();
    commands.entity(bar).add_child(left);

    // Population
    let pop = commands
        .spawn((
            PopulationText,
            Text::new("0/8"),
            TextFont {
                font_size: theme::FONT_CAPTION,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
        ))
        .id();
    commands.entity(left).add_child(pop);

    spawn_separator(&mut commands, left, &theme);

    // Raw resources
    for rt in ResourceType::RAW {
        spawn_header_resource(&mut commands, left, rt, &icons, &theme, false);
    }

    spawn_separator(&mut commands, left, &theme);

    // Processed resources (hidden until unlocked)
    for rt in ResourceType::PROCESSED {
        spawn_header_resource(&mut commands, left, rt, &icons, &theme, true);
    }

    // ── Center section: day cycle indicator ──
    let center = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .id();
    commands.entity(bar).add_child(center);

    let phase_icon = commands
        .spawn((
            DayCycleIcon,
            ImageNode::new(daycycle_icons.day.clone()),
            Node {
                width: Val::Px(14.0),
                height: Val::Px(14.0),
                ..default()
            },
        ))
        .id();
    commands.entity(center).add_child(phase_icon);

    let phase_text = commands
        .spawn((
            DayCycleText,
            Text::new("Day 12:00"),
            TextFont {
                font_size: theme::FONT_CAPTION,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
        ))
        .id();
    commands.entity(center).add_child(phase_text);

    // ── Right section: widget toggles ──
    let right = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(2.0),
            ..default()
        })
        .id();
    commands.entity(bar).add_child(right);

    super::widget_toolbar::spawn_toolbar_buttons(&mut commands, right, &fonts, &theme);
}

fn spawn_separator(commands: &mut Commands, parent: Entity, theme: &Theme) {
    let sep = commands
        .spawn((
            Node {
                width: Val::Px(1.0),
                height: Val::Px(14.0),
                ..default()
            },
            BackgroundColor(theme.colors.separator),
        ))
        .id();
    commands.entity(parent).add_child(sep);
}

fn spawn_header_resource(
    commands: &mut Commands,
    parent: Entity,
    rt: ResourceType,
    icons: &IconAssets,
    theme: &Theme,
    is_processed: bool,
) {
    let tooltip_text = format!("{}\n{}", rt.display_name(), resource_description(rt));

    let mut group_cmds = commands.spawn((
        Interaction::default(),
        ActionTooltipTrigger { text: tooltip_text },
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(2.0),
            ..default()
        },
    ));
    if is_processed {
        group_cmds.insert((ProcessedResourceRow(rt), Visibility::Hidden));
    }
    let group = group_cmds.id();
    commands.entity(parent).add_child(group);

    let icon = commands
        .spawn((
            ImageNode::new(icons.resource_icon(rt)),
            Node {
                width: Val::Px(14.0),
                height: Val::Px(14.0),
                ..default()
            },
        ))
        .id();
    commands.entity(group).add_child(icon);

    let text = commands
        .spawn((
            ResourceText(rt),
            Text::new("0"),
            TextFont {
                font_size: theme::FONT_CAPTION,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ))
        .id();
    commands.entity(group).add_child(text);
}

fn resource_description(rt: ResourceType) -> &'static str {
    match rt {
        ResourceType::Wood => "Primary construction material. Gathered from forests.",
        ResourceType::Copper => "Basic metal ore. Mined from deposits.",
        ResourceType::Iron => "Advanced metal ore. Used for steel production.",
        ResourceType::Gold => "Precious metal. Required for advanced units.",
        ResourceType::Oil => "Fuel resource. Extracted from oil deposits.",
        ResourceType::Stone => "Building material. Quarried from rock formations.",
        ResourceType::Planks => "Processed wood. Refined at Sawmills.",
        ResourceType::Charcoal => "Fuel product. Smelted from wood at Sawmills.",
        ResourceType::Bronze => "Alloy of copper. Produced at Smelters.",
        ResourceType::Steel => "Strong alloy. Produced at Smelters from iron.",
        ResourceType::Gunpowder => "Explosive compound. Made at Alchemist labs.",
    }
}

// ── Update Systems ──

pub fn update_resource_texts(
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    all_units: Query<&Faction, With<Unit>>,
    all_training_queues: Query<(&Faction, &TrainingQueue), With<Building>>,
    all_buildings_for_cap: Query<
        (&Faction, &EntityKind, &BuildingState, &BuildingLevel),
        With<Building>,
    >,
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
                "{} (+{}) / {}",
                unit_cap.used, unit_cap.queued, unit_cap.cap
            );
        } else {
            **text = format!("{} / {}", unit_cap.used, unit_cap.cap);
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
        let has_any = player_res.get(rt) > 0;
        *vis = if has_any {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn update_daycycle_indicator(
    cycle: Res<DayCycle>,
    daycycle_icons: Res<DayCycleIconAssets>,
    mut icon_q: Query<&mut ImageNode, With<DayCycleIcon>>,
    mut text_q: Query<&mut Text, With<DayCycleText>>,
) {
    let icon_handle = match cycle.phase {
        DayPhase::Dawn => &daycycle_icons.dawn,
        DayPhase::Day => &daycycle_icons.day,
        DayPhase::Dusk => &daycycle_icons.dusk,
        DayPhase::Night => &daycycle_icons.night,
    };

    for mut img in &mut icon_q {
        img.image = icon_handle.clone();
    }

    let display = format!("{} {}", cycle.phase_name(), cycle.clock_string());
    for mut text in &mut text_q {
        **text = display.clone();
    }
}
