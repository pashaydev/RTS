//! `SessionLogPlugin`: tracing layer that collects JSON-formatted log events
//! plus panic hook, so each run writes a structured session log to disk.

use bevy::app::AppExit;
use bevy::ecs::message::MessageReader;
use bevy::log::tracing::{self, Event, Subscriber};
use bevy::log::tracing_subscriber::layer::{Context, Layer};
use bevy::log::tracing_subscriber::registry::LookupSpan;
use bevy::log::BoxedLayer;
use bevy::prelude::*;
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

static SESSION_LOG: OnceLock<SessionLog> = OnceLock::new();
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

#[macro_export]
macro_rules! game_info {
    ($($arg:tt)*) => {
        bevy::log::info!($($arg)*);
    };
}

#[macro_export]
macro_rules! game_warn {
    ($($arg:tt)*) => {
        bevy::log::warn!($($arg)*);
    };
}

#[macro_export]
macro_rules! game_error {
    ($($arg:tt)*) => {
        bevy::log::error!($($arg)*);
    };
}

#[derive(Resource, Clone)]
pub struct SessionLog {
    inner: Arc<Mutex<SessionLogState>>,
}

impl SessionLog {
    fn new(base_dir: PathBuf) -> Self {
        let base_dir = resolve_log_base_dir(base_dir);
        let started_at_unix_ms = now_unix_ms();
        let session_id = uuid::Uuid::new_v4().to_string();
        let output_path =
            logs_dir(&base_dir).join(format!("session-{started_at_unix_ms}-{session_id}.json"));

        Self {
            inner: Arc::new(Mutex::new(SessionLogState {
                session_id,
                started_at_unix_ms,
                finished_at_unix_ms: None,
                base_dir,
                output_path,
                entries: Vec::new(),
                flushed: false,
            })),
        }
    }

    fn record_event(&self, entry: SessionLogEntry) {
        if let Ok(mut state) = self.inner.lock() {
            state.entries.push(entry);
        }
    }

    fn snapshot(&self) {
        self.write_to_disk(false);
    }

    fn flush(&self) {
        self.write_to_disk(true);
    }

    fn write_to_disk(&self, is_final: bool) {
        let snapshot = {
            let Ok(mut state) = self.inner.lock() else {
                return;
            };

            if state.flushed {
                return;
            }

            if is_final {
                state.finished_at_unix_ms = Some(now_unix_ms());
                state.flushed = true;
            }

            SessionLogFile {
                session_id: state.session_id.clone(),
                started_at_unix_ms: state.started_at_unix_ms,
                finished_at_unix_ms: state
                    .finished_at_unix_ms
                    .unwrap_or(state.started_at_unix_ms),
                base_dir: state.base_dir.to_string_lossy().into_owned(),
                output_path: state.output_path.to_string_lossy().into_owned(),
                entries: state.entries.clone(),
            }
        };

        let output_path = PathBuf::from(&snapshot.output_path);
        if let Some(parent) = output_path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return;
            }
        }

        if let Ok(bytes) = serde_json::to_vec_pretty(&snapshot) {
            let _ = std::fs::write(&output_path, bytes);
        }
    }
}

fn resolve_log_base_dir(base_dir: PathBuf) -> PathBuf {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // In development, keep logs in the working tree instead of `target/...`.
                // In packaged desktop builds, always write next to the executable so
                // logs land inside the game folder regardless of launch CWD.
                if !exe_dir.components().any(|c| c.as_os_str() == "target") {
                    return exe_dir.to_path_buf();
                }
            }
        }
    }

    base_dir
}

#[derive(Clone)]
struct SessionLogState {
    session_id: String,
    started_at_unix_ms: u128,
    finished_at_unix_ms: Option<u128>,
    base_dir: PathBuf,
    output_path: PathBuf,
    entries: Vec<SessionLogEntry>,
    flushed: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SessionLogFile {
    session_id: String,
    started_at_unix_ms: u128,
    finished_at_unix_ms: u128,
    base_dir: String,
    output_path: String,
    entries: Vec<SessionLogEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct SessionLogEntry {
    timestamp_unix_ms: u128,
    level: String,
    target: String,
    module_path: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    fields: Value,
    message: String,
}

pub fn configure_session_logging(base_dir: PathBuf) {
    let log = SESSION_LOG
        .get_or_init(|| SessionLog::new(base_dir))
        .clone();

    // Eagerly write the file so we fail fast if the chosen path is not
    // writable (critical for release builds on Windows/macOS where the exe
    // directory may be locked down).
    log.snapshot();

    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let panic_log = log.clone();
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let location = panic_info
                .location()
                .map(|location| format!("{}:{}", location.file(), location.line()));
            let message = if let Some(text) = panic_info.payload().downcast_ref::<&str>() {
                (*text).to_string()
            } else if let Some(text) = panic_info.payload().downcast_ref::<String>() {
                text.clone()
            } else {
                "panic payload is not a string".to_string()
            };

            panic_log.record_event(SessionLogEntry {
                timestamp_unix_ms: now_unix_ms(),
                level: "ERROR".to_string(),
                target: "panic".to_string(),
                module_path: None,
                file: location,
                line: None,
                fields: Value::Null,
                message: format!("panic: {message}"),
            });
            panic_log.flush();
            previous_hook(panic_info);
        }));
    });
}

pub fn make_tracing_layer(app: &mut App) -> Option<BoxedLayer> {
    let session_log = SESSION_LOG
        .get_or_init(|| {
            let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            SessionLog::new(base_dir)
        })
        .clone();
    app.insert_resource(session_log.clone());
    Some(Box::new(JsonCaptureLayer { session_log }))
}

pub struct SessionLogPlugin;

impl Plugin for SessionLogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionLogFlushClock>()
            .add_systems(Last, (periodic_session_log_snapshot, flush_session_log_on_exit));
    }
}

const SESSION_LOG_SNAPSHOT_INTERVAL_SECS: f32 = 3.0;

#[derive(Resource, Default)]
struct SessionLogFlushClock {
    elapsed_secs: f32,
}

fn periodic_session_log_snapshot(
    time: Res<Time>,
    mut clock: ResMut<SessionLogFlushClock>,
    session_log: Option<Res<SessionLog>>,
) {
    clock.elapsed_secs += time.delta_secs();
    if clock.elapsed_secs < SESSION_LOG_SNAPSHOT_INTERVAL_SECS {
        return;
    }
    clock.elapsed_secs = 0.0;
    if let Some(session_log) = session_log {
        session_log.snapshot();
    }
}

fn flush_session_log_on_exit(
    mut exit_events: MessageReader<AppExit>,
    session_log: Option<Res<SessionLog>>,
) {
    if exit_events.read().next().is_none() {
        return;
    }

    if let Some(session_log) = session_log {
        session_log.flush();
    }
}

struct JsonCaptureLayer {
    session_log: SessionLog,
}

impl<S> Layer<S> for JsonCaptureLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);

        let metadata = event.metadata();
        self.session_log.record_event(SessionLogEntry {
            timestamp_unix_ms: now_unix_ms(),
            level: metadata.level().as_str().to_string(),
            target: metadata.target().to_string(),
            module_path: metadata.module_path().map(str::to_string),
            file: metadata.file().map(str::to_string),
            line: metadata.line(),
            fields: Value::Object(visitor.fields),
            message: visitor
                .message
                .unwrap_or_else(|| metadata.name().to_string()),
        });
    }
}

#[derive(Default)]
struct JsonVisitor {
    fields: Map<String, Value>,
    message: Option<String>,
}

impl JsonVisitor {
    fn record_value(&mut self, field_name: &str, value: Value) {
        if field_name == "message" {
            self.message = Some(match &value {
                Value::String(text) => text.clone(),
                _ => value.to_string(),
            });
        }

        self.fields.insert(field_name.to_string(), value);
    }
}

impl tracing::field::Visit for JsonVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_value(field.name(), Value::String(value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.record_value(field.name(), Value::Bool(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.record_value(field.name(), Value::from(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_value(field.name(), Value::from(value));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.record_value(field.name(), Value::from(value));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.record_value(field.name(), Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record_value(field.name(), Value::String(format!("{value:?}")));
    }
}

fn now_unix_ms() -> u128 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u128
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }
}

#[allow(dead_code)]
pub fn session_log_path() -> Option<PathBuf> {
    let log = SESSION_LOG.get()?;
    let state = log.inner.lock().ok()?;
    Some(state.output_path.clone())
}

#[allow(dead_code)]
pub fn logs_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("logs")
}
