use bevy::prelude::*;

use crate::components::*;
use crate::theme;

pub fn update_resource_texts(
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    mut text_q: Query<(&mut Text, &ResourceText)>,
) {
    let player_res = all_resources.get(&active_player.0);
    let carried = carried_totals.get(&active_player.0);
    for (mut text, rt_marker) in &mut text_q {
        let rt = rt_marker.0;
        let val = player_res.get(rt);
        let carried_val = carried.get(rt);
        if carried_val > 0 {
            **text = format!("{} (+{})", val, carried_val);
        } else {
            **text = format!("{}", val);
        }
    }
}

pub fn spawn_resource_content(commands: &mut Commands, parent: Entity, icons: &IconAssets) {
    let resource_types = [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ];
    for rt in resource_types {
        let row = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .id();
        commands.entity(parent).add_child(row);

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
        commands.entity(row).add_child(icon);

        let text = commands
            .spawn((
                ResourceText(rt),
                Text::new(format!("{:?}: 0", rt)),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(theme::TEXT_PRIMARY),
            ))
            .id();
        commands.entity(row).add_child(text);
    }
}
