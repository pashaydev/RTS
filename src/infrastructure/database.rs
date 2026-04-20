//! SQLite persistence layer — player profiles, match history, settings, stats, ELO, presets.
//!
//! Gated behind `#[cfg(not(target_arch = "wasm32"))]` for the actual SQLite connection.
//! On WASM, every method returns a no-op / default so callers don't need conditional logic.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::infrastructure::multiplayer::NetRole;
use crate::simulation::victory::VictoryState;
use crate::types::{
    ActivePlayer, AiDifficulty, AllPlayerResources, AppState, Faction, FactionStats,
    GameSetupConfig, GameplaySettings, GraphicsSettings, MatchStartTime, SlotOccupant,
};

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct DatabasePlugin;

impl Plugin for DatabasePlugin {
    fn build(&self, app: &mut App) {
        // DB + profile are already inserted by `database::init_early()` in main().
        app.add_systems(Startup, sync_profile_to_config)
            .add_systems(OnEnter(AppState::InGame), insert_match_start_time)
            .add_systems(
                Update,
                sync_settings_to_db.run_if(
                    resource_changed::<GraphicsSettings>
                        .or(resource_changed::<crate::infrastructure::audio::AudioSettings>)
                        .or(resource_changed::<GameplaySettings>)
                        .or(resource_changed::<crate::ui::core::framework::WidgetRegistry>),
                ),
            );
    }
}

/// Call early in `main()` before building the Bevy `App`.
/// Opens the database, creates the profile, and returns resources + loaded settings
/// so they can be inserted before window creation.
pub fn init_early() -> (
    GameDatabase,
    ActiveProfile,
    GraphicsSettings,
    crate::infrastructure::audio::AudioSettings,
    GameplaySettings,
) {
    let db = GameDatabase::open();
    let profile = db.get_or_create_active_profile();
    let graphics = db
        .load_settings_blob::<GraphicsSettings>("graphics")
        .unwrap_or_default();
    let audio = db
        .load_settings_blob::<crate::infrastructure::audio::AudioSettings>("audio")
        .unwrap_or_default();
    let gameplay = db
        .load_settings_blob::<GameplaySettings>("gameplay")
        .unwrap_or_default();
    (db, profile, graphics, audio, gameplay)
}

// ── Resources ───────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct GameDatabase {
    #[cfg(not(target_arch = "wasm32"))]
    conn: Option<std::sync::Mutex<rusqlite::Connection>>,
}

#[derive(Resource, Clone, Debug)]
pub struct ActiveProfile {
    pub id: String,
    pub name: String,
}

impl Default for ActiveProfile {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: crate::types::random_commander_name(),
        }
    }
}

// ── Query / result structs ──────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStats {
    pub total_matches: u32,
    pub wins: u32,
    pub losses: u32,
    pub win_rate: f32,
    pub total_play_time_secs: f64,
    pub total_units: u32,
    pub total_buildings: u32,
    pub favorite_map_size: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSummary {
    pub id: i64,
    pub started_at: String,
    pub duration_secs: f64,
    pub map_size: String,
    pub team_mode: String,
    pub num_players: u32,
    pub local_player_won: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveEntry {
    pub id: i64,
    pub label: Option<String>,
    pub created_at: String,
    pub elapsed_secs: f64,
    pub map_size: String,
    pub num_players: i32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EloRating {
    pub rating: i32,
    pub peak_rating: i32,
    pub matches_played: u32,
}

impl Default for EloRating {
    fn default() -> Self {
        Self {
            rating: 1200,
            peak_rating: 1200,
            matches_played: 0,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EloHistoryEntry {
    pub match_id: i64,
    pub rating_before: i32,
    pub rating_after: i32,
    pub recorded_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapPresetEntry {
    pub id: i64,
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEntry {
    pub id: i64,
    pub match_id: Option<i64>,
    pub file_path: String,
    pub label: Option<String>,
    pub duration_secs: Option<f64>,
    pub recorded_at: String,
}

// ── Schema ──────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS player_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    is_last_used INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_profiles_last_used ON player_profiles(is_last_used);

CREATE TABLE IF NOT EXISTS matches (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_profile_id TEXT NOT NULL REFERENCES player_profiles(id),
    started_at TEXT NOT NULL,
    duration_secs REAL NOT NULL,
    map_size TEXT NOT NULL,
    map_seed INTEGER NOT NULL,
    resource_density TEXT NOT NULL,
    team_mode TEXT NOT NULL,
    num_players INTEGER NOT NULL,
    num_ai INTEGER NOT NULL,
    is_multiplayer INTEGER NOT NULL DEFAULT 0,
    winner_faction INTEGER,
    winner_team INTEGER,
    local_player_faction INTEGER NOT NULL,
    local_player_won INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_matches_profile ON matches(player_profile_id);
CREATE INDEX IF NOT EXISTS idx_matches_started ON matches(started_at);

CREATE TABLE IF NOT EXISTS match_faction_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    faction_index INTEGER NOT NULL,
    is_local_player INTEGER NOT NULL DEFAULT 0,
    slot_type TEXT NOT NULL,
    ai_difficulty TEXT,
    team INTEGER,
    eliminated INTEGER NOT NULL DEFAULT 0,
    unit_count INTEGER NOT NULL DEFAULT 0,
    building_count INTEGER NOT NULL DEFAULT 0,
    resources_json TEXT,
    player_name TEXT
);
CREATE INDEX IF NOT EXISTS idx_mfs_match ON match_faction_stats(match_id);

CREATE TABLE IF NOT EXISTS settings (
    category TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    PRIMARY KEY (category, key)
);

CREATE TABLE IF NOT EXISTS replay_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_id INTEGER REFERENCES matches(id),
    file_path TEXT NOT NULL,
    file_size_bytes INTEGER,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
    label TEXT,
    duration_secs REAL,
    map_seed INTEGER,
    map_size TEXT
);
CREATE INDEX IF NOT EXISTS idx_replay_match ON replay_metadata(match_id);

CREATE TABLE IF NOT EXISTS elo_ratings (
    player_profile_id TEXT PRIMARY KEY REFERENCES player_profiles(id),
    rating INTEGER NOT NULL DEFAULT 1200,
    peak_rating INTEGER NOT NULL DEFAULT 1200,
    matches_played INTEGER NOT NULL DEFAULT 0,
    last_updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS elo_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_profile_id TEXT NOT NULL REFERENCES player_profiles(id),
    match_id INTEGER NOT NULL REFERENCES matches(id),
    rating_before INTEGER NOT NULL,
    rating_after INTEGER NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_elo_history_profile ON elo_history(player_profile_id);

CREATE TABLE IF NOT EXISTS map_presets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    config_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS keybind_profiles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    is_active INTEGER NOT NULL DEFAULT 0,
    bindings_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS game_saves (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_profile_id TEXT NOT NULL REFERENCES player_profiles(id),
    label TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    elapsed_secs REAL NOT NULL,
    map_size TEXT NOT NULL,
    map_seed INTEGER NOT NULL,
    num_players INTEGER NOT NULL,
    save_data BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_saves_profile ON game_saves(player_profile_id);
CREATE INDEX IF NOT EXISTS idx_saves_created ON game_saves(created_at);
";

// ── GameDatabase implementation ─────────────────────────────────────────────

#[allow(dead_code)]
impl GameDatabase {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open() -> Self {
        let _ = std::fs::create_dir_all("config");
        let conn = match rusqlite::Connection::open("config/game.db") {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to open game database: {e}");
                return Self { conn: None };
            }
        };

        // Enable WAL mode for better concurrent read performance.
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");

        // Create tables (idempotent via IF NOT EXISTS).
        if let Err(e) = Self::create_tables(&conn) {
            warn!("Database schema creation failed: {e}");
            return Self { conn: None };
        }

        Self {
            conn: Some(std::sync::Mutex::new(conn)),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn open() -> Self {
        Self {}
    }

    pub fn is_available(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.conn.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn create_tables(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(SCHEMA)
    }

    // ── Helper to lock connection ───────────────────────────────────────

    #[cfg(not(target_arch = "wasm32"))]
    fn with_conn<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<R, rusqlite::Error>,
    {
        let guard = self.conn.as_ref()?.lock().ok()?;
        match f(&guard) {
            Ok(val) => Some(val),
            Err(e) => {
                warn!("Database query error: {e}");
                None
            }
        }
    }

    // ── Profile CRUD ────────────────────────────────────────────────────

    pub fn get_or_create_active_profile(&self) -> ActiveProfile {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(profile) = self.get_last_used_profile() {
                self.update_last_seen(&profile.id);
                return profile;
            }
            let profile = ActiveProfile::default();
            self.insert_profile(&profile);
            profile
        }
        #[cfg(target_arch = "wasm32")]
        {
            ActiveProfile::default()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn get_last_used_profile(&self) -> Option<ActiveProfile> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name FROM player_profiles WHERE is_last_used = 1 LIMIT 1",
                [],
                |row| {
                    Ok(ActiveProfile {
                        id: row.get(0)?,
                        name: row.get(1)?,
                    })
                },
            )
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn insert_profile(&self, profile: &ActiveProfile) {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO player_profiles (id, name, is_last_used) VALUES (?1, ?2, 1)",
                rusqlite::params![profile.id, profile.name],
            )
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn update_last_seen(&self, profile_id: &str) {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE player_profiles SET last_seen_at = datetime('now') WHERE id = ?1",
                [profile_id],
            )
        });
    }

    pub fn update_profile_name(&self, profile_id: &str, name: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE player_profiles SET name = ?1 WHERE id = ?2",
                rusqlite::params![name, profile_id],
            )
        });
    }

    // ── Settings CRUD ───────────────────────────────────────────────────

    pub fn load_setting(&self, category: &str, key: &str) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                conn.query_row(
                    "SELECT value_json FROM settings WHERE category = ?1 AND key = ?2",
                    rusqlite::params![category, key],
                    |row| row.get(0),
                )
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (category, key);
            None
        }
    }

    pub fn save_setting(&self, category: &str, key: &str, value_json: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO settings (category, key, value_json) VALUES (?1, ?2, ?3)",
                rusqlite::params![category, key, value_json],
            )
        });
    }

    /// Load a JSON blob stored under `(category, "_blob")` and deserialize it.
    pub fn load_settings_blob<T: serde::de::DeserializeOwned>(&self, category: &str) -> Option<T> {
        let json = self.load_setting(category, "_blob")?;
        serde_json::from_str(&json).ok()
    }

    pub fn has_settings(&self, category: &str) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM settings WHERE category = ?1",
                    [category],
                    |row| row.get::<_, i64>(0),
                )
            })
            .map(|c| c > 0)
            .unwrap_or(false)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = category;
            false
        }
    }

    // ── Match recording ─────────────────────────────────────────────────

    pub fn record_match(
        &self,
        profile_id: &str,
        config: &GameSetupConfig,
        victory: &VictoryState,
        active_player: &ActivePlayer,
        net_role: &NetRole,
        faction_stats: &FactionStats,
        all_resources: &AllPlayerResources,
        duration_secs: f64,
        player_names: &std::collections::HashMap<usize, String>,
    ) -> Option<i64> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let local_faction = active_player.0;
            let local_won = victory
                .winner
                .map(|w| {
                    if victory.winner_team.is_some() {
                        // Team game: check faction not eliminated
                        victory
                            .faction_status
                            .get(&local_faction)
                            .map(|s| *s != crate::simulation::victory::FactionStatus::Eliminated)
                            .unwrap_or(false)
                    } else {
                        w == local_faction
                    }
                })
                .unwrap_or(false);

            let is_multiplayer = *net_role != NetRole::Offline;
            let num_players = config
                .slots
                .iter()
                .filter(|s| !matches!(s, SlotOccupant::Closed))
                .count() as u32;
            let num_ai = config
                .slots
                .iter()
                .filter(|s| matches!(s, SlotOccupant::Ai(_)))
                .count() as u32;

            self.with_conn(|conn| {
                let tx = conn.unchecked_transaction()?;

                tx.execute(
                    "INSERT INTO matches (
                        player_profile_id, started_at, duration_secs,
                        map_size, map_seed, resource_density, team_mode,
                        num_players, num_ai, is_multiplayer,
                        winner_faction, winner_team,
                        local_player_faction, local_player_won
                    ) VALUES (?1, datetime('now', ?2), ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    rusqlite::params![
                        profile_id,
                        format!("-{} seconds", duration_secs as i64),
                        duration_secs,
                        format!("{:?}", config.map_size),
                        config.map_seed as i64,
                        format!("{:?}", config.resource_density),
                        format!("{:?}", config.team_mode),
                        num_players,
                        num_ai,
                        is_multiplayer as i32,
                        victory.winner.map(|f| f.to_net_index() as i32),
                        victory.winner_team.map(|t| t as i32),
                        local_faction.to_net_index() as i32,
                        local_won as i32,
                    ],
                )?;

                let match_id = tx.last_insert_rowid();

                // Insert per-faction stats
                for (i, slot) in config.slots.iter().enumerate() {
                    if matches!(slot, SlotOccupant::Closed) {
                        continue;
                    }
                    let faction = Faction::PLAYERS[i];
                    let is_local = faction == local_faction;
                    let (slot_type, ai_diff) = match slot {
                        SlotOccupant::Human => ("Human", None),
                        SlotOccupant::Ai(diff) => ("Ai", Some(format!("{:?}", diff))),
                        SlotOccupant::Open => ("Human", None), // connected player
                        SlotOccupant::Closed => unreachable!(),
                    };

                    let fs = faction_stats
                        .stats
                        .get(&faction)
                        .cloned()
                        .unwrap_or_default();

                    let resources_json = all_resources
                        .resources
                        .get(&faction)
                        .map(|r| serde_json::to_string(&r.amounts).unwrap_or_default());

                    let eliminated = victory
                        .faction_status
                        .get(&faction)
                        .map(|s| *s == crate::simulation::victory::FactionStatus::Eliminated)
                        .unwrap_or(false);

                    let pname: Option<&str> = player_names.get(&i).map(|s| s.as_str());

                    tx.execute(
                        "INSERT INTO match_faction_stats (
                            match_id, faction_index, is_local_player,
                            slot_type, ai_difficulty, team, eliminated,
                            unit_count, building_count, resources_json, player_name
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        rusqlite::params![
                            match_id,
                            i as i32,
                            is_local as i32,
                            slot_type,
                            ai_diff,
                            config.player_teams[i] as i32,
                            eliminated as i32,
                            fs.unit_count,
                            fs.building_count,
                            resources_json,
                            pname,
                        ],
                    )?;
                }

                tx.commit()?;
                Ok(match_id)
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (
                profile_id,
                config,
                victory,
                active_player,
                net_role,
                faction_stats,
                all_resources,
                duration_secs,
                player_names,
            );
            None
        }
    }

    // ── Player stats (aggregated queries) ───────────────────────────────

    pub fn get_player_stats(&self, profile_id: &str) -> PlayerStats {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                let (total, wins, total_time): (u32, u32, f64) = conn.query_row(
                    "SELECT
                        COUNT(*),
                        SUM(local_player_won),
                        COALESCE(SUM(duration_secs), 0.0)
                     FROM matches WHERE player_profile_id = ?1",
                    [profile_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;

                let (total_units, total_buildings): (u32, u32) = conn
                    .query_row(
                        "SELECT
                            COALESCE(SUM(mfs.unit_count), 0),
                            COALESCE(SUM(mfs.building_count), 0)
                         FROM match_faction_stats mfs
                         JOIN matches m ON m.id = mfs.match_id
                         WHERE m.player_profile_id = ?1 AND mfs.is_local_player = 1",
                        [profile_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap_or((0, 0));

                let favorite_map: Option<String> = conn
                    .query_row(
                        "SELECT map_size FROM matches
                         WHERE player_profile_id = ?1
                         GROUP BY map_size ORDER BY COUNT(*) DESC LIMIT 1",
                        [profile_id],
                        |row| row.get(0),
                    )
                    .ok();

                let losses = total.saturating_sub(wins);
                let win_rate = if total > 0 {
                    wins as f32 / total as f32
                } else {
                    0.0
                };

                Ok(PlayerStats {
                    total_matches: total,
                    wins,
                    losses,
                    win_rate,
                    total_play_time_secs: total_time,
                    total_units,
                    total_buildings,
                    favorite_map_size: favorite_map,
                })
            })
            .unwrap_or(PlayerStats {
                total_matches: 0,
                wins: 0,
                losses: 0,
                win_rate: 0.0,
                total_play_time_secs: 0.0,
                total_units: 0,
                total_buildings: 0,
                favorite_map_size: None,
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = profile_id;
            PlayerStats {
                total_matches: 0,
                wins: 0,
                losses: 0,
                win_rate: 0.0,
                total_play_time_secs: 0.0,
                total_units: 0,
                total_buildings: 0,
                favorite_map_size: None,
            }
        }
    }

    pub fn get_recent_matches(&self, profile_id: &str, limit: u32) -> Vec<MatchSummary> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, started_at, duration_secs, map_size, team_mode, num_players, local_player_won
                     FROM matches WHERE player_profile_id = ?1
                     ORDER BY started_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![profile_id, limit], |row| {
                    Ok(MatchSummary {
                        id: row.get(0)?,
                        started_at: row.get(1)?,
                        duration_secs: row.get(2)?,
                        map_size: row.get(3)?,
                        team_mode: row.get(4)?,
                        num_players: row.get(5)?,
                        local_player_won: row.get::<_, i32>(6)? != 0,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (profile_id, limit);
            Vec::new()
        }
    }

    // ── ELO ─────────────────────────────────────────────────────────────

    pub fn get_elo(&self, profile_id: &str) -> EloRating {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                conn.query_row(
                    "SELECT rating, peak_rating, matches_played FROM elo_ratings WHERE player_profile_id = ?1",
                    [profile_id],
                    |row| {
                        Ok(EloRating {
                            rating: row.get(0)?,
                            peak_rating: row.get(1)?,
                            matches_played: row.get(2)?,
                        })
                    },
                )
            })
            .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = profile_id;
            EloRating::default()
        }
    }

    pub fn update_elo(&self, profile_id: &str, match_id: i64, won: bool, opponent_rating: i32) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| {
            let current = conn
                .query_row(
                    "SELECT rating, matches_played FROM elo_ratings WHERE player_profile_id = ?1",
                    [profile_id],
                    |row| Ok((row.get::<_, i32>(0)?, row.get::<_, u32>(1)?)),
                )
                .unwrap_or((1200, 0));

            let (old_rating, games) = current;
            let k: f64 = if games < 30 { 32.0 } else { 16.0 };
            let expected = 1.0 / (1.0 + 10.0_f64.powf((opponent_rating as f64 - old_rating as f64) / 400.0));
            let score = if won { 1.0 } else { 0.0 };
            let new_rating = (old_rating as f64 + k * (score - expected)).round() as i32;
            let new_peak = new_rating.max(
                conn.query_row(
                    "SELECT COALESCE(peak_rating, 1200) FROM elo_ratings WHERE player_profile_id = ?1",
                    [profile_id],
                    |row| row.get::<_, i32>(0),
                )
                .unwrap_or(1200),
            );
            let new_games = games + 1;

            conn.execute(
                "INSERT INTO elo_ratings (player_profile_id, rating, peak_rating, matches_played, last_updated_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(player_profile_id) DO UPDATE SET
                    rating = ?2, peak_rating = ?3, matches_played = ?4, last_updated_at = datetime('now')",
                rusqlite::params![profile_id, new_rating, new_peak, new_games],
            )?;

            conn.execute(
                "INSERT INTO elo_history (player_profile_id, match_id, rating_before, rating_after)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![profile_id, match_id, old_rating, new_rating],
            )?;

            Ok(())
        });
    }

    pub fn get_elo_history(&self, profile_id: &str, limit: u32) -> Vec<EloHistoryEntry> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT match_id, rating_before, rating_after, recorded_at
                     FROM elo_history WHERE player_profile_id = ?1
                     ORDER BY recorded_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![profile_id, limit], |row| {
                    Ok(EloHistoryEntry {
                        match_id: row.get(0)?,
                        rating_before: row.get(1)?,
                        rating_after: row.get(2)?,
                        recorded_at: row.get(3)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (profile_id, limit);
            Vec::new()
        }
    }

    /// Determine opponent rating for ELO calculation.
    /// AI: Easy=800, Medium=1200, Hard=1600. Multiplayer: use opponent's ELO.
    pub fn ai_elo_rating(difficulty: AiDifficulty) -> i32 {
        match difficulty {
            AiDifficulty::Easy => 800,
            AiDifficulty::Medium => 1200,
            AiDifficulty::Hard => 1600,
        }
    }

    // ── Map presets ─────────────────────────────────────────────────────

    pub fn save_map_preset(&self, name: &str, config: &GameSetupConfig) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let preset = SerializablePreset {
                map_size: format!("{:?}", config.map_size),
                resource_density: format!("{:?}", config.resource_density),
                team_mode: format!("{:?}", config.team_mode),
                player_teams: config.player_teams,
                day_cycle_secs: config.day_cycle_secs,
                night_spawn_intensity: config.night_spawn_intensity.index() as u8,
                starting_resources_mult: config.starting_resources_mult,
                map_seed: config.map_seed,
                slots: config
                    .slots
                    .iter()
                    .map(|s| match s {
                        SlotOccupant::Human => "Human".to_string(),
                        SlotOccupant::Ai(d) => format!("Ai:{:?}", d),
                        SlotOccupant::Closed => "Closed".to_string(),
                        SlotOccupant::Open => "Open".to_string(),
                    })
                    .collect(),
            };
            if let Ok(json) = serde_json::to_string(&preset) {
                self.with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO map_presets (name, config_json)
                         VALUES (?1, ?2)
                         ON CONFLICT(name) DO UPDATE SET config_json = ?2, updated_at = datetime('now')",
                        rusqlite::params![name, json],
                    )
                });
            }
        }
    }

    pub fn load_map_preset(&self, name: &str) -> Option<SerializablePreset> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let json: String = self.with_conn(|conn| {
                conn.query_row(
                    "SELECT config_json FROM map_presets WHERE name = ?1",
                    [name],
                    |row| row.get(0),
                )
            })?;
            serde_json::from_str(&json).ok()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = name;
            None
        }
    }

    pub fn list_map_presets(&self) -> Vec<MapPresetEntry> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                let mut stmt =
                    conn.prepare("SELECT id, name FROM map_presets ORDER BY updated_at DESC")?;
                let rows = stmt.query_map([], |row| {
                    Ok(MapPresetEntry {
                        id: row.get(0)?,
                        name: row.get(1)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Vec::new()
        }
    }

    pub fn delete_map_preset(&self, id: i64) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| conn.execute("DELETE FROM map_presets WHERE id = ?1", [id]));
    }

    // ── Replay metadata ─────────────────────────────────────────────────

    pub fn insert_replay(
        &self,
        match_id: Option<i64>,
        file_path: &str,
        file_size_bytes: Option<i64>,
        label: Option<&str>,
        duration_secs: Option<f64>,
        map_seed: Option<i64>,
        map_size: Option<&str>,
    ) -> Option<i64> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO replay_metadata (match_id, file_path, file_size_bytes, label, duration_secs, map_seed, map_size)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![match_id, file_path, file_size_bytes, label, duration_secs, map_seed, map_size],
                )?;
                Ok(conn.last_insert_rowid())
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (
                match_id,
                file_path,
                file_size_bytes,
                label,
                duration_secs,
                map_seed,
                map_size,
            );
            None
        }
    }

    pub fn list_replays(&self, limit: u32) -> Vec<ReplayEntry> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, match_id, file_path, label, duration_secs, recorded_at
                     FROM replay_metadata ORDER BY recorded_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map([limit], |row| {
                    Ok(ReplayEntry {
                        id: row.get(0)?,
                        match_id: row.get(1)?,
                        file_path: row.get(2)?,
                        label: row.get(3)?,
                        duration_secs: row.get(4)?,
                        recorded_at: row.get(5)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = limit;
            Vec::new()
        }
    }

    pub fn delete_replay(&self, id: i64) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| conn.execute("DELETE FROM replay_metadata WHERE id = ?1", [id]));
    }

    // ── Keybind profiles ────────────────────────────────────────────────

    pub fn save_keybind_profile(&self, name: &str, bindings_json: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO keybind_profiles (name, bindings_json)
                 VALUES (?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET bindings_json = ?2",
                rusqlite::params![name, bindings_json],
            )
        });
    }

    pub fn load_active_keybind_profile(&self) -> Option<(String, String)> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                conn.query_row(
                    "SELECT name, bindings_json FROM keybind_profiles WHERE is_active = 1 LIMIT 1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
        }
    }

    pub fn list_keybind_profiles(&self) -> Vec<(i64, String)> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                let mut stmt =
                    conn.prepare("SELECT id, name FROM keybind_profiles ORDER BY name")?;
                let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Vec::new()
        }
    }

    // ── Game saves CRUD ────────────────────────────────────────────────

    pub fn save_game(
        &self,
        profile_id: &str,
        label: Option<&str>,
        elapsed_secs: f64,
        map_size: &str,
        map_seed: i64,
        num_players: i32,
        data: &[u8],
    ) -> Option<i64> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO game_saves (player_profile_id, label, elapsed_secs, map_size, map_seed, num_players, save_data)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![profile_id, label, elapsed_secs, map_size, map_seed, num_players, data],
                )?;
                Ok(conn.last_insert_rowid())
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (
                profile_id,
                label,
                elapsed_secs,
                map_size,
                map_seed,
                num_players,
                data,
            );
            None
        }
    }

    pub fn list_saves(&self, profile_id: &str) -> Vec<SaveEntry> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, label, created_at, elapsed_secs, map_size, num_players
                     FROM game_saves WHERE player_profile_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map([profile_id], |row| {
                    Ok(SaveEntry {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        created_at: row.get(2)?,
                        elapsed_secs: row.get(3)?,
                        map_size: row.get(4)?,
                        num_players: row.get(5)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = profile_id;
            Vec::new()
        }
    }

    pub fn load_save(&self, save_id: i64) -> Option<Vec<u8>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.with_conn(|conn| {
                conn.query_row(
                    "SELECT save_data FROM game_saves WHERE id = ?1",
                    [save_id],
                    |row| row.get(0),
                )
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = save_id;
            None
        }
    }

    pub fn delete_save(&self, save_id: i64) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| conn.execute("DELETE FROM game_saves WHERE id = ?1", [save_id]));
    }

    pub fn rename_save(&self, save_id: i64, label: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE game_saves SET label = ?1 WHERE id = ?2",
                rusqlite::params![label, save_id],
            )
        });
    }
}

// ── Serializable preset helper ──────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializablePreset {
    pub map_size: String,
    pub resource_density: String,
    pub team_mode: String,
    pub player_teams: [u8; 4],
    pub day_cycle_secs: f32,
    #[serde(default)]
    pub night_spawn_intensity: u8,
    pub starting_resources_mult: f32,
    pub map_seed: u64,
    pub slots: Vec<String>,
}

// ── Bevy systems ────────────────────────────────────────────────────────────

fn insert_match_start_time(mut commands: Commands, time: Res<Time>) {
    commands.insert_resource(MatchStartTime(time.elapsed_secs_f64()));
}

/// On startup, copy `ActiveProfile.name` into `GameSetupConfig.player_name`
/// so the lobby / new-game UI shows the persisted player name.
fn sync_profile_to_config(profile: Res<ActiveProfile>, mut config: ResMut<GameSetupConfig>) {
    if !profile.name.is_empty() {
        config.player_name = profile.name.clone();
    }
}

fn sync_settings_to_db(
    db: Res<GameDatabase>,
    graphics: Res<GraphicsSettings>,
    audio: Res<crate::infrastructure::audio::AudioSettings>,
    gameplay: Res<GameplaySettings>,
    widget_registry: Res<crate::ui::core::framework::WidgetRegistry>,
) {
    if !db.is_available() {
        return;
    }

    if graphics.is_changed() {
        if let Ok(json) = serde_json::to_string_pretty(&*graphics) {
            db.save_setting("graphics", "_blob", &json);
        }
    }

    if audio.is_changed() {
        if let Ok(json) = serde_json::to_string_pretty(&*audio) {
            db.save_setting("audio", "_blob", &json);
        }
    }

    if gameplay.is_changed() {
        if let Ok(json) = serde_json::to_string_pretty(&*gameplay) {
            db.save_setting("gameplay", "_blob", &json);
        }
    }

    if widget_registry.is_changed() {
        if let Ok(json) = serde_json::to_string_pretty(&*widget_registry) {
            db.save_setting("widget_layout", "_blob", &json);
        }
    }
}
