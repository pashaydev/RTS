mod click_select;
mod picking;
mod unit_commands;

use bevy::prelude::*;

use crate::components::*;
use crate::hover_material::HoverRingMaterial;
use crate::minimap::MinimapSet;
use crate::orders;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectionSet;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<HoverRingMaterial>::default())
            .init_resource::<DragState>()
            .init_resource::<InspectedEnemy>()
            .init_resource::<UiClickedThisFrame>()
            .init_resource::<UiPressActive>()
            .init_resource::<CommandMode>()
            .init_resource::<ActiveFormation>()
            .init_resource::<EntityLabelVisibility>()
            .init_resource::<NextTaskId>()
            .init_resource::<SubgroupCycleState>()
            .init_resource::<DoubleClickDetector>()
            .init_resource::<picking::PickCycleState>()
            .add_systems(Startup, picking::setup_hover_assets)
            .add_systems(OnEnter(AppState::InGame), click_select::spawn_selection_box)
            .add_systems(
                First,
                click_select::reset_ui_clicked.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    click_select::track_drag,
                    click_select::update_selection_box_visual,
                )
                    .chain()
                    .in_set(SelectionSet)
                    .in_set(GameFlowSet::Input)
                    .after(MinimapSet)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                (
                    picking::update_hover.after(click_select::update_selection_box_visual),
                    click_select::handle_click_select.after(picking::update_hover),
                )
                    .in_set(SelectionSet)
                    .in_set(GameFlowSet::Input)
                    .after(MinimapSet)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(Update, click_select::update_entity_visuals)
            .add_systems(
                Update,
                unit_commands::handle_right_click_move
                    .in_set(GameFlowSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                unit_commands::handle_unit_command_hotkeys
                    .in_set(GameFlowSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                picking::update_hover_ring
                    .in_set(GameFlowSet::Presentation)
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    click_select::tab_subgroup_cycle_system,
                    click_select::reset_subgroup_on_selection_change,
                )
                    .chain()
                    .in_set(GameFlowSet::Input)
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                PostUpdate,
                click_select::clear_ui_press_on_release.run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Shared task-queue helpers ──

pub(crate) fn enqueue_task(
    task_queues: &mut Query<&mut TaskQueue, With<Unit>>,
    next_task_id: &mut NextTaskId,
    entity: Entity,
    task: QueuedTask,
) {
    if let Ok(mut queue) = task_queues.get_mut(entity) {
        orders::push_queued_task(&mut queue, next_task_id, task);
    }
}

pub(crate) fn clear_task_queue(
    task_queues: &mut Query<&mut TaskQueue, With<Unit>>,
    entity: Entity,
) {
    if let Ok(mut queue) = task_queues.get_mut(entity) {
        orders::clear_task_queue(&mut queue);
    }
}

pub(crate) fn set_current_task(
    task_queues: &mut Query<&mut TaskQueue, With<Unit>>,
    next_task_id: &mut NextTaskId,
    entity: Entity,
    task: QueuedTask,
) {
    if let Ok(mut queue) = task_queues.get_mut(entity) {
        queue.clear_queued();
        orders::set_current_task(&mut queue, next_task_id, task);
    }
}
