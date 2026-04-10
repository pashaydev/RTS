use bevy::prelude::*;

use crate::types::*;

const DEFAULT_COMMAND_BUFFER_TTL_SECS: f64 = 0.35;
const DEFAULT_MANUAL_TARGET_LOCK_SECS: f64 = 1.5;
const DEFAULT_AUTO_TARGET_LOCK_SECS: f64 = 1.1;

fn make_engagement(
    target: Option<Entity>,
    source: IntentSource,
    mode: EngageMode,
    anchor: Vec3,
    issue_time: f64,
    persistence_secs: f64,
) -> Engagement {
    Engagement {
        target,
        source,
        mode,
        anchor,
        acquired_at: issue_time,
        last_confirmed_at: issue_time,
        persistence_until: issue_time + persistence_secs,
        status: EngageStatus::Acquiring,
    }
}

pub struct CombatIntentsPlugin;

impl Plugin for CombatIntentsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatTuning>()
            .init_resource::<CombatBudget>()
            .init_resource::<CombatStatsDebug>()
            .init_resource::<MeleeSlotCache>()
            .add_systems(
                FixedUpdate,
                cleanup_expired_combat_state
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

pub fn apply_manual_move_intent(
    commands: &mut Commands,
    entity: Entity,
    destination: Vec3,
    issue_time: f64,
) {
    let mut ec = commands.entity(entity);
    ec.insert(TaskSource::Manual)
        .insert(CombatIntent::Move(destination))
        .insert(BufferedCommand {
            kind: BufferedCommandKind::Move(destination),
            issue_time,
            expires_at: Some(issue_time + DEFAULT_COMMAND_BUFFER_TTL_SECS),
        })
        .remove::<CombatTargetLock>()
        .remove::<Engagement>()
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn apply_manual_attack_intent(
    commands: &mut Commands,
    entity: Entity,
    target: Entity,
    issue_time: f64,
) {
    let mut ec = commands.entity(entity);
    ec.insert(TaskSource::Manual)
        .insert(CombatIntent::Attack(target, IntentSource::Manual))
        .insert(BufferedCommand {
            kind: BufferedCommandKind::Attack(target),
            issue_time,
            expires_at: Some(issue_time + DEFAULT_COMMAND_BUFFER_TTL_SECS),
        })
        .insert(CombatTargetLock {
            target,
            locked_until: issue_time + DEFAULT_MANUAL_TARGET_LOCK_SECS,
            source: IntentSource::Manual,
        })
        .insert(make_engagement(
            Some(target),
            IntentSource::Manual,
            EngageMode::Direct,
            Vec3::ZERO,
            issue_time,
            DEFAULT_MANUAL_TARGET_LOCK_SECS,
        ))
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn apply_manual_attack_move_intent(
    commands: &mut Commands,
    entity: Entity,
    destination: Vec3,
    issue_time: f64,
) {
    let mut ec = commands.entity(entity);
    ec.insert(TaskSource::Manual)
        .insert(CombatIntent::AttackMove(destination, IntentSource::Manual))
        .insert(BufferedCommand {
            kind: BufferedCommandKind::AttackMove(destination),
            issue_time,
            expires_at: Some(issue_time + DEFAULT_COMMAND_BUFFER_TTL_SECS),
        })
        .insert(make_engagement(
            None,
            IntentSource::Manual,
            EngageMode::AttackMove,
            destination,
            issue_time,
            DEFAULT_MANUAL_TARGET_LOCK_SECS,
        ))
        .remove::<CombatTargetLock>()
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn apply_manual_hold_intent(commands: &mut Commands, entity: Entity, issue_time: f64) {
    let mut ec = commands.entity(entity);
    ec.insert(TaskSource::Manual)
        .insert(CombatIntent::Hold)
        .insert(BufferedCommand {
            kind: BufferedCommandKind::Hold,
            issue_time,
            expires_at: Some(issue_time + DEFAULT_COMMAND_BUFFER_TTL_SECS),
        })
        .insert(make_engagement(
            None,
            IntentSource::Manual,
            EngageMode::Hold,
            Vec3::ZERO,
            issue_time,
            DEFAULT_MANUAL_TARGET_LOCK_SECS,
        ))
        .remove::<CombatTargetLock>()
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn clear_combat_intent(commands: &mut Commands, entity: Entity, issue_time: f64) {
    let mut ec = commands.entity(entity);
    ec.insert(CombatIntent::None)
        .insert(BufferedCommand {
            kind: BufferedCommandKind::Stop,
            issue_time,
            expires_at: Some(issue_time + DEFAULT_COMMAND_BUFFER_TTL_SECS),
        })
        .remove::<CombatTargetLock>()
        .remove::<Engagement>()
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn reset_combat_state(commands: &mut Commands, entity: Entity) {
    commands
        .entity(entity)
        .insert(CombatIntent::None)
        .remove::<BufferedCommand>()
        .remove::<CombatTargetLock>()
        .remove::<Engagement>()
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn apply_auto_move_intent(commands: &mut Commands, entity: Entity, destination: Vec3) {
    commands
        .entity(entity)
        .insert(TaskSource::Auto)
        .insert(CombatIntent::Move(destination))
        .remove::<BufferedCommand>()
        .remove::<CombatTargetLock>()
        .remove::<Engagement>()
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn apply_auto_attack_intent(
    commands: &mut Commands,
    entity: Entity,
    target: Entity,
    anchor: Vec3,
    issue_time: f64,
) {
    commands
        .entity(entity)
        .insert(TaskSource::Auto)
        .insert(CombatIntent::Attack(target, IntentSource::Auto))
        .insert(CombatTargetLock {
            target,
            locked_until: issue_time + DEFAULT_AUTO_TARGET_LOCK_SECS,
            source: IntentSource::Auto,
        })
        .insert(make_engagement(
            Some(target),
            IntentSource::Auto,
            EngageMode::Direct,
            anchor,
            issue_time,
            DEFAULT_AUTO_TARGET_LOCK_SECS,
        ))
        .insert(LeashOrigin(anchor))
        .remove::<BufferedCommand>()
        .remove::<AttackCommit>()
        .remove::<PathBlockedTimer>()
        .remove::<SlotClaim>();
}

pub fn set_intent_target_lock(
    commands: &mut Commands,
    entity: Entity,
    target: Entity,
    source: IntentSource,
    issue_time: f64,
    mode: EngageMode,
    anchor: Vec3,
) {
    let lock_secs = match source {
        IntentSource::Manual => DEFAULT_MANUAL_TARGET_LOCK_SECS,
        IntentSource::Auto => DEFAULT_AUTO_TARGET_LOCK_SECS,
    };
    commands.entity(entity).insert(CombatTargetLock {
        target,
        locked_until: issue_time + lock_secs,
        source,
    });
    commands.entity(entity).insert(Engagement {
        target: Some(target),
        source,
        mode,
        anchor,
        acquired_at: issue_time,
        last_confirmed_at: issue_time,
        persistence_until: issue_time + lock_secs,
        status: EngageStatus::Acquiring,
    });
}

fn cleanup_expired_combat_state(
    mut commands: Commands,
    time: Res<Time>,
    mut stats: ResMut<CombatStatsDebug>,
    buffered_commands: Query<(Entity, &BufferedCommand)>,
    target_locks: Query<(Entity, &CombatTargetLock)>,
    slot_claims: Query<Entity, With<SlotClaim>>,
) {
    let now = time.elapsed_secs_f64();
    let mut expired_buffers = 0_u64;
    let mut expired_locks = 0_u64;

    for (entity, buffered) in &buffered_commands {
        if buffered
            .expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            commands.entity(entity).remove::<BufferedCommand>();
            expired_buffers += 1;
        }
    }

    for (entity, lock) in &target_locks {
        if now >= lock.locked_until {
            commands.entity(entity).remove::<CombatTargetLock>();
            expired_locks += 1;
        }
    }

    stats.buffered_commands_active = buffered_commands.iter().count();
    stats.target_locks_active = target_locks.iter().count();
    stats.slot_claims_active = slot_claims.iter().count();
    stats.expired_buffered_commands += expired_buffers;
    stats.expired_target_locks += expired_locks;
}
