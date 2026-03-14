use crate::components::*;

fn allocate_task_id(next_task_id: &mut NextTaskId) -> u64 {
    next_task_id.0 = next_task_id.0.saturating_add(1);
    next_task_id.0
}

pub fn push_queued_task(
    queue: &mut TaskQueue,
    next_task_id: &mut NextTaskId,
    task: QueuedTask,
) -> u64 {
    let id = allocate_task_id(next_task_id);
    queue.queue.push_back(TaskEntry { id, task });
    id
}

pub fn clear_task_queue(queue: &mut TaskQueue) {
    queue.clear();
}

pub fn set_current_task(
    queue: &mut TaskQueue,
    next_task_id: &mut NextTaskId,
    task: QueuedTask,
) -> u64 {
    let id = allocate_task_id(next_task_id);
    queue.current = Some(TaskEntry { id, task });
    id
}
