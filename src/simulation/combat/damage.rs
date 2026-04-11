use bevy::prelude::*;

use crate::types::{ArmorType, DamageType, Health, RecentCombatDamage, ReservedIncomingDamage};

/// Central damage application.
///
/// Every gameplay damage source — melee resolve, projectile hit, splash, ability direct damage,
/// status DoT, prop explosion — must route through this function so armor multipliers,
/// reservation cleanup, and `RecentCombatDamage` retaliation memory stay consistent.
///
/// Returns the actual damage dealt (after armor multiplier).
///
/// `source` may be `None` for sourceless damage (e.g. Burning DoT with no tracked caster,
/// or environmental hazards). When `None`, no `RecentCombatDamage` is written and no
/// reservation is cleared.
pub fn apply_damage(
    commands: &mut Commands,
    target: Entity,
    source: Option<Entity>,
    base_damage: f32,
    damage_type: DamageType,
    health: &mut Health,
    armor: Option<ArmorType>,
    reserved: Option<&mut ReservedIncomingDamage>,
    now_secs: f64,
    damage_memory_secs: f32,
) -> f32 {
    let multiplier = armor.map(|a| damage_type.multiplier_vs(a)).unwrap_or(1.0);
    let dealt = base_damage * multiplier;
    health.current -= dealt;

    if let Some(src) = source {
        if let Some(res) = reserved {
            res.reservations.retain(|(s, _, _)| *s != src);
        }
        commands.entity(target).insert(RecentCombatDamage {
            attacker: src,
            observed_at: now_secs,
            expires_at: now_secs + damage_memory_secs as f64,
        });
    }

    dealt
}
