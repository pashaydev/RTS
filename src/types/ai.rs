//! AI system types: personality, difficulty, notifications, patrol.

use bevy::prelude::*;
use std::collections::HashMap;

use super::app::Faction;

/// AI personality — governs build order & army composition.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiPersonality {
    #[default]
    Balanced,
    Aggressive,
    Defensive,
    #[allow(dead_code)]
    Economic,
    Supportive,
}

/// Relation of an AI faction to the human player.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiRelation {
    Friendly,
    #[default]
    Enemy,
}

/// A single ally notification event.
#[derive(Clone, Debug)]
pub struct AllyNotification {
    pub message: String,
    pub world_pos: Option<Vec3>,
    pub timestamp: f32,
    pub kind: AllyNotifyKind,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum AllyNotifyKind {
    UnderAttack,
    Attacking,
    ReadyToAttack,
    EnemySpotted,
    ItemPickupFail,
}

impl AllyNotifyKind {
    pub fn color(&self) -> Color {
        match self {
            Self::UnderAttack => Color::srgb(1.0, 0.6, 0.2),
            Self::Attacking | Self::ReadyToAttack => Color::srgb(0.3, 0.8, 1.0),
            Self::EnemySpotted => Color::srgb(0.9, 0.9, 0.3),
            Self::ItemPickupFail => Color::srgb(0.9, 0.55, 0.55),
        }
    }
}

/// Active ally notifications (displayed as toasts).
#[derive(Resource, Default)]
pub struct AllyNotifications {
    pub active: Vec<AllyNotification>,
    pub last_per_kind: HashMap<AllyNotifyKind, f32>,
}

impl AllyNotifications {
    pub fn push(
        &mut self,
        kind: AllyNotifyKind,
        message: String,
        world_pos: Option<Vec3>,
        game_time: f32,
    ) {
        // Throttle: max 1 per kind per N seconds (shorter for item feedback)
        let throttle_secs = match kind {
            AllyNotifyKind::ItemPickupFail => 2.0,
            _ => 10.0,
        };
        if let Some(&last) = self.last_per_kind.get(&kind) {
            if game_time - last < throttle_secs {
                return;
            }
        }
        self.last_per_kind.insert(kind, game_time);
        self.active.push(AllyNotification {
            message,
            world_pos,
            timestamp: game_time,
            kind,
        });
        // Keep max 5
        while self.active.len() > 5 {
            self.active.remove(0);
        }
    }
}

/// Per-faction AI settings (public interface for debug panel).
#[derive(Resource, Default)]
pub struct AiFactionSettings {
    pub settings: HashMap<Faction, AiFactionConfig>,
}

#[derive(Clone, Debug)]
pub struct AiFactionConfig {
    pub difficulty: super::app::AiDifficulty,
    pub personality: AiPersonality,
    pub relation: AiRelation,
    pub phase_name: String,
    pub posture_name: String,
    pub attack_squad_size: usize,
    pub defense_squad_size: usize,
    pub relative_strength: f32,
    pub worker_count: u8,
    pub military_count: u8,
}

impl Default for AiFactionConfig {
    fn default() -> Self {
        Self {
            difficulty: super::app::AiDifficulty::Medium,
            personality: AiPersonality::Balanced,
            relation: AiRelation::Enemy,
            phase_name: "Founding".to_string(),
            posture_name: "Normal".to_string(),
            attack_squad_size: 0,
            defense_squad_size: 0,
            relative_strength: 0.0,
            worker_count: 0,
            military_count: 0,
        }
    }
}

