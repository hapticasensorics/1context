//! Perception DB / memoryd client.
//!
//! Uses the existing Rust `onecontext-memoryd` protocol process for lightweight
//! source metadata during dry-runs. Storage readiness is a gate: an empty
//! packet plan is allowed only when explicitly enabled for local fallback.

use crate::packet_planner::SourceEvent;
use crate::ContextEnginePaths;
use chrono::{Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const LOCAL_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
const MEMORY_DB_URL_ENV: &str = "ONECONTEXT_MEMORY_DB_URL";
const STORAGE_BACKEND_ENV: &str = "ONECONTEXT_STORAGE_BACKEND";
const ALLOW_EXTERNAL_POSTGRES_ENV: &str = "ONECONTEXT_ALLOW_EXTERNAL_POSTGRES";
pub const ALLOW_EMPTY_MEMORY_FALLBACK_ENV: &str = "ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK";
const VIEWPORT_PAGE_LIMIT: usize = 5_000;
const MAX_VIEWPORT_OBJECTS: usize = 25_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorydDensityProbe {
    pub status: String,
    pub provider: String,
    pub executable: Option<String>,
    pub bucket_count: usize,
    pub object_count: i64,
    pub active_day_count: usize,
    pub active_hour_count: usize,
    pub error: Option<String>,
    #[serde(skip)]
    pub source_events: Vec<SourceEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorydSourceEventsProbe {
    pub status: String,
    pub provider: String,
    pub executable: Option<String>,
    pub viewport_object_count: usize,
    pub hydrated_object_count: usize,
    pub source_event_count: usize,
    pub density_bucket_count: usize,
    pub density_object_count: i64,
    pub error: Option<String>,
    #[serde(skip)]
    pub source_events: Vec<SourceEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryStorageGate {
    pub status: String,
    pub provider: String,
    pub executable: Option<String>,
    pub backend: Option<String>,
    pub storage_status: Option<String>,
    pub ready: bool,
    pub storage_ready: bool,
    pub recent_history_ready: bool,
    pub recent_backfill_status: Option<String>,
    pub schema_state: Option<String>,
    pub fallback_allowed: bool,
    pub error: Option<String>,
    pub message: Option<String>,
}

impl MemoryStorageGate {
    pub fn ready_for_source_packets(&self) -> bool {
        self.status == "ok"
            && self.ready
            && self.storage_ready
            && self.recent_history_ready
            && self.schema_state.as_deref() == Some("valid")
    }

    pub fn failure_message(&self) -> String {
        let detail = self
            .message
            .as_deref()
            .or(self.error.as_deref())
            .unwrap_or("memory storage is not ready");
        format!(
            "Perception memory storage gate failed: {detail}. Use the official managed storage path with {STORAGE_BACKEND_ENV}=managed_postgres and the packaged managed Postgres runtime. For local debug-only external storage, set {MEMORY_DB_URL_ENV} with {STORAGE_BACKEND_ENV}=external_postgres and {ALLOW_EXTERNAL_POSTGRES_ENV}=1. Set {ALLOW_EMPTY_MEMORY_FALLBACK_ENV}=1 to permit an empty dry-run packet plan."
        )
    }
}

pub fn ensure_memory_storage_gate(
    paths: &ContextEnginePaths,
    window_days: u32,
) -> MemoryStorageGate {
    let fallback_allowed = empty_memory_fallback_allowed();
    let executable = match discover_memoryd_executable() {
        Some(path) => path,
        None => {
            return MemoryStorageGate {
                status: "unavailable".to_string(),
                provider: "onecontext-memoryd".to_string(),
                executable: None,
                backend: None,
                storage_status: None,
                ready: false,
                storage_ready: false,
                recent_history_ready: false,
                recent_backfill_status: None,
                schema_state: None,
                fallback_allowed,
                error: Some("onecontext-memoryd executable not found".to_string()),
                message: None,
            }
        }
    };

    let storage_request = protocol_request(
        "memory.ensureStorageReady",
        "context-engine-storage-ready",
        json!({
            "reason": "context-engine.update_wiki",
            "repair": false,
            "context_engine_root": paths.context_engine.display().to_string()
        }),
    );
    let storage_value =
        match run_memoryd_protocol(&executable, "memory.ensureStorageReady", &storage_request) {
            Ok(value) => value,
            Err(error) => {
                return MemoryStorageGate {
                    status: "error".to_string(),
                    provider: "onecontext-memoryd".to_string(),
                    executable: Some(executable.display().to_string()),
                    backend: None,
                    storage_status: None,
                    ready: false,
                    storage_ready: false,
                    recent_history_ready: false,
                    recent_backfill_status: None,
                    schema_state: None,
                    fallback_allowed,
                    error: Some(error),
                    message: None,
                }
            }
        };
    let mut gate = storage_gate_from_protocol_value(&executable, storage_value, fallback_allowed);
    if !gate.storage_ready {
        return gate;
    }

    let backfill_request = protocol_request(
        "memory.ensureRecentBackfill",
        "context-engine-recent-backfill",
        json!({
            "reason": "context-engine.update_wiki",
            "window_hours": window_days.max(1).saturating_mul(24),
            "block_until_ready": false
        }),
    );
    match run_memoryd_protocol(
        &executable,
        "memory.ensureRecentBackfill",
        &backfill_request,
    ) {
        Ok(value) => merge_recent_backfill_value(&mut gate, value),
        Err(error) => {
            gate.status = "error".to_string();
            gate.recent_history_ready = false;
            gate.error = Some(error);
        }
    }
    gate
}

pub fn query_recent_density(paths: &ContextEnginePaths, window_days: u32) -> MemorydDensityProbe {
    let executable = match discover_memoryd_executable() {
        Some(path) => path,
        None => {
            return MemorydDensityProbe {
                status: "unavailable".to_string(),
                provider: "onecontext-memoryd".to_string(),
                executable: None,
                bucket_count: 0,
                object_count: 0,
                active_day_count: 0,
                active_hour_count: 0,
                error: Some("onecontext-memoryd executable not found".to_string()),
                source_events: Vec::new(),
            }
        }
    };

    let request = density_request(paths, window_days);
    let output = run_memoryd_protocol(&executable, "memory.queryDensity", &request);
    match output {
        Ok(value) => density_probe_from_protocol_value(executable, value),
        Err(error) => MemorydDensityProbe {
            status: "error".to_string(),
            provider: "onecontext-memoryd".to_string(),
            executable: Some(executable.display().to_string()),
            bucket_count: 0,
            object_count: 0,
            active_day_count: 0,
            active_hour_count: 0,
            error: Some(error),
            source_events: Vec::new(),
        },
    }
}

pub fn query_recent_source_events(
    paths: &ContextEnginePaths,
    window_days: u32,
) -> MemorydSourceEventsProbe {
    let density = query_recent_density(paths, window_days);
    let executable = match discover_memoryd_executable() {
        Some(path) => path,
        None => {
            return MemorydSourceEventsProbe {
                status: "unavailable".to_string(),
                provider: "onecontext-memoryd".to_string(),
                executable: None,
                viewport_object_count: 0,
                hydrated_object_count: 0,
                source_event_count: 0,
                density_bucket_count: density.bucket_count,
                density_object_count: density.object_count,
                error: Some("onecontext-memoryd executable not found".to_string()),
                source_events: Vec::new(),
            }
        }
    };

    let mut object_ids = Vec::new();
    let mut seen_object_ids = BTreeSet::new();
    let mut next_cursor = None;
    let mut incomplete_reason = None;
    loop {
        let viewport_request = viewport_request(window_days, next_cursor.as_deref());
        let viewport =
            match run_memoryd_protocol(&executable, "memory.queryViewport", &viewport_request) {
                Ok(value) => value,
                Err(error) => {
                    return MemorydSourceEventsProbe {
                        status: "error".to_string(),
                        provider: "onecontext-memoryd".to_string(),
                        executable: Some(executable.display().to_string()),
                        viewport_object_count: 0,
                        hydrated_object_count: 0,
                        source_event_count: 0,
                        density_bucket_count: density.bucket_count,
                        density_object_count: density.object_count,
                        error: Some(format!("memory.queryViewport failed: {error}")),
                        source_events: Vec::new(),
                    }
                }
            };
        for object_id in viewport_object_ids_from_protocol_value(&viewport) {
            if seen_object_ids.insert(object_id.clone()) {
                object_ids.push(object_id);
            }
        }
        next_cursor = viewport_next_cursor_from_protocol_value(&viewport);
        if next_cursor.is_none() {
            break;
        }
        if object_ids.len() >= MAX_VIEWPORT_OBJECTS {
            incomplete_reason = Some(format!(
                "memory.queryViewport returned more than {MAX_VIEWPORT_OBJECTS} objects for the source window"
            ));
            break;
        }
    }
    if let Some(error) = incomplete_reason {
        return MemorydSourceEventsProbe {
            status: "incomplete".to_string(),
            provider: "onecontext-memoryd".to_string(),
            executable: Some(executable.display().to_string()),
            viewport_object_count: object_ids.len(),
            hydrated_object_count: 0,
            source_event_count: 0,
            density_bucket_count: density.bucket_count,
            density_object_count: density.object_count,
            error: Some(error),
            source_events: Vec::new(),
        };
    }
    if object_ids.is_empty() {
        return MemorydSourceEventsProbe {
            status: if density.object_count > 0 {
                "density_only".to_string()
            } else {
                "empty".to_string()
            },
            provider: "onecontext-memoryd".to_string(),
            executable: Some(executable.display().to_string()),
            viewport_object_count: 0,
            hydrated_object_count: 0,
            source_event_count: 0,
            density_bucket_count: density.bucket_count,
            density_object_count: density.object_count,
            error: None,
            source_events: Vec::new(),
        };
    }

    let hydrate_request = hydrate_request(&object_ids);
    let hydrated =
        match run_memoryd_protocol(&executable, "memory.hydrateObjects", &hydrate_request) {
            Ok(value) => value,
            Err(error) => {
                return MemorydSourceEventsProbe {
                    status: "error".to_string(),
                    provider: "onecontext-memoryd".to_string(),
                    executable: Some(executable.display().to_string()),
                    viewport_object_count: object_ids.len(),
                    hydrated_object_count: 0,
                    source_event_count: 0,
                    density_bucket_count: density.bucket_count,
                    density_object_count: density.object_count,
                    error: Some(format!("memory.hydrateObjects failed: {error}")),
                    source_events: Vec::new(),
                }
            }
        };
    let hydrated_object_count = hydrated
        .get("result")
        .and_then(|result| result.get("objects"))
        .and_then(|objects| objects.as_array())
        .map(Vec::len)
        .unwrap_or(0);
    let source_events = source_events_from_hydrated_objects(&hydrated);
    MemorydSourceEventsProbe {
        status: if source_events.is_empty() {
            "no_raw_content".to_string()
        } else {
            "ok".to_string()
        },
        provider: "onecontext-memoryd".to_string(),
        executable: Some(executable.display().to_string()),
        viewport_object_count: object_ids.len(),
        hydrated_object_count,
        source_event_count: source_events.len(),
        density_bucket_count: density.bucket_count,
        density_object_count: density.object_count,
        error: None,
        source_events,
    }
}

fn discover_memoryd_executable() -> Option<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();
    if let Ok(path) = env::var("ONECONTEXT_MEMORYD_BIN") {
        if !path.trim().is_empty() {
            candidates.push(PathBuf::from(path));
        }
    }
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            candidates.push(dir.join("onecontext-memoryd"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("onecontext-memoryd"));
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("target/debug/onecontext-memoryd"));
        candidates.push(cwd.join("target/release/onecontext-memoryd"));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file() && is_executable(path))
}

#[cfg(unix)]
fn is_executable(path: &PathBuf) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &PathBuf) -> bool {
    path.is_file()
}

fn density_request(paths: &ContextEnginePaths, window_days: u32) -> serde_json::Value {
    let end = Utc::now();
    let start = end - Duration::days(window_days.max(1) as i64);
    protocol_request(
        "memory.queryDensity",
        &format!("context-engine-density-{}", Utc::now().timestamp()),
        json!({
            "user_id": LOCAL_USER_ID,
            "time": {
                "start": start.to_rfc3339_opts(SecondsFormat::Secs, true),
                "end": end.to_rfc3339_opts(SecondsFormat::Secs, true)
            },
            "bucket": "1m",
            "filters": {},
            "explain": false,
            "context_engine_root": paths.context_engine.display().to_string()
        }),
    )
}

fn viewport_request(window_days: u32, cursor: Option<&str>) -> serde_json::Value {
    let end = Utc::now();
    let start = end - Duration::days(window_days.max(1) as i64);
    protocol_request(
        "memory.queryViewport",
        &format!("context-engine-viewport-{}", Utc::now().timestamp()),
        json!({
            "user_id": LOCAL_USER_ID,
            "time": {
                "start": start.to_rfc3339_opts(SecondsFormat::Secs, true),
                "end": end.to_rfc3339_opts(SecondsFormat::Secs, true)
            },
            "filters": {},
            "pagination": {
                "limit": VIEWPORT_PAGE_LIMIT,
                "cursor": cursor
            },
            "include": {
                "payload": false,
                "blob_descriptor": true,
                "source_record": true,
                "edges_count": false
            },
            "explain": false
        }),
    )
}

fn hydrate_request(object_ids: &[String]) -> serde_json::Value {
    protocol_request(
        "memory.hydrateObjects",
        &format!("context-engine-hydrate-{}", Utc::now().timestamp()),
        json!({
            "user_id": LOCAL_USER_ID,
            "object_ids": object_ids,
            "include": {
                "payload": true,
                "blob_descriptor": true,
                "source_record": true,
                "edges": false
            }
        }),
    )
}

fn run_memoryd_protocol(
    executable: &PathBuf,
    protocol_method: &str,
    request: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut command = Command::new(executable);
    command.args(["protocol", protocol_method, "--request-json", "-"]);
    for (key, value) in memoryd_child_env_overrides() {
        command.env(key, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn onecontext-memoryd: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let bytes = serde_json::to_vec(request)
            .map_err(|error| format!("failed to encode memoryd request: {error}"))?;
        stdin
            .write_all(&bytes)
            .map_err(|error| format!("failed to write memoryd request: {error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for onecontext-memoryd: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if stderr.is_empty() { stdout } else { stderr });
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("memoryd returned invalid JSON: {error}"))
}

fn memoryd_child_env_overrides() -> Vec<(&'static str, String)> {
    memoryd_child_env_overrides_from(
        env::var(STORAGE_BACKEND_ENV).ok().as_deref(),
        env::var(ALLOW_EXTERNAL_POSTGRES_ENV).ok().as_deref(),
        env::var(MEMORY_DB_URL_ENV).ok().as_deref(),
    )
}

fn memoryd_child_env_overrides_from(
    storage_backend: Option<&str>,
    allow_external: Option<&str>,
    database_url: Option<&str>,
) -> Vec<(&'static str, String)> {
    let database_url_present = database_url
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let normalized_backend = storage_backend
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase().replace('-', "_"));
    let external_selected = normalized_backend
        .as_deref()
        .is_some_and(|value| matches!(value, "external" | "external_postgres"));

    let mut overrides = Vec::new();
    if database_url_present && external_selected && allow_external.map(str::trim) != Some("1") {
        overrides.push((ALLOW_EXTERNAL_POSTGRES_ENV, "1".to_string()));
    }
    overrides
}

fn protocol_request(
    method: &str,
    request_id: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request_id": request_id,
        "method": method,
        "params": params
    })
}

fn storage_gate_from_protocol_value(
    executable: &PathBuf,
    value: serde_json::Value,
    fallback_allowed: bool,
) -> MemoryStorageGate {
    let status = value
        .get("status")
        .and_then(|status| status.as_str())
        .unwrap_or("error");
    let result = value.get("result");
    let storage_ready = result
        .and_then(|result| result.get("storage_ready"))
        .and_then(|ready| ready.as_bool())
        .unwrap_or(false);
    let recent_history_ready = result
        .and_then(|result| result.get("recent_history_ready"))
        .and_then(|ready| ready.as_bool())
        .unwrap_or(false);
    let storage_status = result
        .and_then(|result| result.get("status"))
        .and_then(|status| status.as_str())
        .map(ToString::to_string);
    let backend = result
        .and_then(|result| result.get("backend"))
        .and_then(|backend| backend.as_str())
        .map(ToString::to_string);
    let ready = result
        .and_then(|result| result.get("ready"))
        .and_then(|ready| ready.as_bool())
        .unwrap_or(storage_ready);
    let schema_state = result
        .and_then(|result| result.get("schema_state"))
        .and_then(|state| state.as_str())
        .map(ToString::to_string);
    let recent_backfill_status = result
        .and_then(|result| result.get("recent_backfill"))
        .and_then(|backfill| backfill.get("status"))
        .and_then(|status| status.as_str())
        .map(ToString::to_string);
    let message = result
        .and_then(|result| result.get("message"))
        .and_then(|message| message.as_str())
        .map(ToString::to_string);
    let error = if status == "ok" {
        None
    } else {
        Some(protocol_error_message(
            &value,
            "onecontext-memoryd ensureStorageReady returned error",
        ))
    };
    MemoryStorageGate {
        status: if status == "ok"
            && ready
            && storage_ready
            && recent_history_ready
            && schema_state.as_deref() == Some("valid")
        {
            "ok".to_string()
        } else if status == "ok" {
            "not_ready".to_string()
        } else {
            "error".to_string()
        },
        provider: "onecontext-memoryd".to_string(),
        executable: Some(executable.display().to_string()),
        backend,
        storage_status,
        ready,
        storage_ready,
        recent_history_ready,
        recent_backfill_status,
        schema_state,
        fallback_allowed,
        error,
        message,
    }
}

fn merge_recent_backfill_value(gate: &mut MemoryStorageGate, value: serde_json::Value) {
    let status = value
        .get("status")
        .and_then(|status| status.as_str())
        .unwrap_or("error");
    if status != "ok" {
        gate.status = "error".to_string();
        gate.recent_history_ready = false;
        gate.error = Some(protocol_error_message(
            &value,
            "onecontext-memoryd ensureRecentBackfill returned error",
        ));
        return;
    }

    let result = value.get("result");
    gate.recent_backfill_status = result
        .and_then(|result| result.get("status"))
        .and_then(|status| status.as_str())
        .map(ToString::to_string)
        .or_else(|| gate.recent_backfill_status.clone());
    gate.recent_history_ready = result
        .and_then(|result| result.get("density_ready"))
        .and_then(|ready| ready.as_bool())
        .unwrap_or(gate.recent_history_ready);
    if !gate.recent_history_ready {
        gate.message = result
            .and_then(|result| result.get("message"))
            .and_then(|message| message.as_str())
            .map(ToString::to_string)
            .or_else(|| gate.message.clone());
    }
    gate.status = if gate.ready
        && gate.storage_ready
        && gate.recent_history_ready
        && gate.schema_state.as_deref() == Some("valid")
    {
        "ok".to_string()
    } else {
        "not_ready".to_string()
    };
}

fn protocol_error_message(value: &serde_json::Value, fallback: &str) -> String {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
        .unwrap_or(fallback)
        .to_string()
}

pub fn empty_memory_fallback_allowed() -> bool {
    env::var(ALLOW_EMPTY_MEMORY_FALLBACK_ENV)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn density_probe_from_protocol_value(
    executable: PathBuf,
    value: serde_json::Value,
) -> MemorydDensityProbe {
    let status = value
        .get("status")
        .and_then(|status| status.as_str())
        .unwrap_or("error");
    if status != "ok" {
        return MemorydDensityProbe {
            status: "error".to_string(),
            provider: "onecontext-memoryd".to_string(),
            executable: Some(executable.display().to_string()),
            bucket_count: 0,
            object_count: 0,
            active_day_count: 0,
            active_hour_count: 0,
            error: Some(protocol_error_message(
                &value,
                "onecontext-memoryd queryDensity returned error",
            )),
            source_events: Vec::new(),
        };
    }

    let buckets = value
        .get("result")
        .and_then(|result| result.get("buckets"))
        .and_then(|buckets| buckets.as_array())
        .cloned()
        .unwrap_or_default();
    let mut object_count = 0_i64;
    let mut active_days = BTreeSet::<String>::new();
    let mut active_hours = BTreeSet::<String>::new();
    let mut source_events = Vec::new();
    for bucket in buckets {
        let bucket_start = bucket
            .get("bucket_start")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        if bucket_start.is_empty() {
            continue;
        }
        let count = bucket
            .get("object_count")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        object_count += count;
        if bucket_start.len() >= 13 {
            active_days.insert(bucket_start[..10].to_string());
            active_hours.insert(bucket_start[..13].to_string());
        }
        source_events.push(SourceEvent {
            ts: bucket_start,
            session_id: format!(
                "{}:{}:{}",
                string_field(&bucket, "lane_id"),
                string_field(&bucket, "kind"),
                string_field(&bucket, "role")
            ),
            kind: string_field(&bucket, "kind"),
            text: format!("{count} Perception object(s) in density bucket"),
            source: Some("onecontext-memoryd.queryDensity".to_string()),
            cwd: None,
            project_key: None,
            char_count: Some((count.max(1) as u32).saturating_mul(512)),
            privacy_class: Some(string_field(&bucket, "privacy_class")),
            role: Some(string_field(&bucket, "role")),
            ..SourceEvent::default()
        });
    }

    MemorydDensityProbe {
        status: "ok".to_string(),
        provider: "onecontext-memoryd".to_string(),
        executable: Some(executable.display().to_string()),
        bucket_count: source_events.len(),
        object_count,
        active_day_count: active_days.len(),
        active_hour_count: active_hours.len(),
        error: None,
        source_events,
    }
}

fn string_field(value: &serde_json::Value, field: &str) -> String {
    value
        .get(field)
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string()
}

fn viewport_object_ids_from_protocol_value(value: &serde_json::Value) -> Vec<String> {
    if value.get("status").and_then(|status| status.as_str()) != Some("ok") {
        return Vec::new();
    }
    value
        .get("result")
        .and_then(|result| result.get("objects"))
        .and_then(|objects| objects.as_array())
        .into_iter()
        .flatten()
        .filter_map(|object| object.get("object_id").and_then(|id| id.as_str()))
        .filter(|id| !id.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

fn viewport_next_cursor_from_protocol_value(value: &serde_json::Value) -> Option<String> {
    if value.get("status").and_then(|status| status.as_str()) != Some("ok") {
        return None;
    }
    value
        .get("result")
        .and_then(|result| result.get("next_cursor"))
        .and_then(|cursor| cursor.as_str())
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty())
        .map(ToString::to_string)
}

fn source_events_from_hydrated_objects(value: &serde_json::Value) -> Vec<SourceEvent> {
    if value.get("status").and_then(|status| status.as_str()) != Some("ok") {
        return Vec::new();
    }
    let mut events = value
        .get("result")
        .and_then(|result| result.get("objects"))
        .and_then(|objects| objects.as_array())
        .into_iter()
        .flatten()
        .filter_map(source_event_from_hydrated_object)
        .collect::<Vec<_>>();
    events.sort_by(|left, right| left.ts.cmp(&right.ts));
    events
}

fn source_event_from_hydrated_object(object: &serde_json::Value) -> Option<SourceEvent> {
    let ts = object
        .get("event_start")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    let text = hydrated_object_text(object)?;
    let session_id = object
        .get("series_key")
        .and_then(|value| value.as_str())
        .or_else(|| {
            object
                .get("source_record")
                .and_then(|record| record.get("source_record_key"))
                .and_then(|value| value.as_str())
        })
        .or_else(|| object.get("series_id").and_then(|value| value.as_str()))
        .or_else(|| object.get("source_id").and_then(|value| value.as_str()))
        .unwrap_or("unknown")
        .to_string();
    let source_record = object.get("source_record");
    let source_record_key = source_record
        .and_then(|record| record.get("source_record_key"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let source = source_record_key
        .as_deref()
        .or_else(|| object.get("source_key").and_then(|value| value.as_str()))
        .or_else(|| object.get("source_id").and_then(|value| value.as_str()))
        .map(ToString::to_string);
    let source_type = object
        .get("source_type")
        .and_then(|value| value.as_str())
        .or_else(|| object.get("series_kind").and_then(|value| value.as_str()))
        .map(ToString::to_string);
    let source_key = object
        .get("source_key")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let source_display_name = object
        .get("source_display_name")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    Some(SourceEvent {
        object_id: object
            .get("object_id")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        ts,
        session_id,
        kind: object
            .get("kind")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
        char_count: Some(text.len() as u32),
        text,
        source_id: object
            .get("source_id")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        source_type,
        source_key,
        source_display_name,
        source,
        source_record_id: source_record
            .and_then(|record| record.get("source_record_id"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        source_record_key,
        source_record_hash: source_record
            .and_then(|record| record.get("source_record_hash"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        series_id: object
            .get("series_id")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        series_kind: object
            .get("series_kind")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        series_key: object
            .get("series_key")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        series_display_name: object
            .get("series_display_name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        role: object
            .get("role")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        privacy_class: object
            .get("privacy_class")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        body_type: object
            .get("body_type")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        cwd: None,
        project_key: object
            .get("series_display_name")
            .and_then(|value| value.as_str())
            .or_else(|| object.get("series_kind").and_then(|value| value.as_str()))
            .map(ToString::to_string),
    })
}

fn hydrated_object_text(object: &serde_json::Value) -> Option<String> {
    if let Some(text) = object
        .get("text_value")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(text.to_string());
    }
    let payload = object.get("payload")?;
    for field in ["text", "body", "content", "message", "value"] {
        if let Some(text) = payload
            .get(field)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(text.to_string());
        }
    }
    if payload.is_null() {
        None
    } else {
        Some(payload.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_gate_reads_ready_storage_and_backfill() {
        let executable = PathBuf::from("/tmp/onecontext-memoryd");
        let value = json!({
            "status": "ok",
            "result": {
                "status": "ready",
                "backend": "external_postgres",
                "ready": true,
                "storage_ready": true,
                "recent_history_ready": true,
                "schema_state": "valid",
                "recent_backfill": {"status": "ready"},
                "message": "ready"
            }
        });

        let gate = storage_gate_from_protocol_value(&executable, value, false);

        assert!(gate.ready_for_source_packets());
        assert_eq!(gate.backend, Some("external_postgres".to_string()));
        assert_eq!(gate.storage_status, Some("ready".to_string()));
        assert_eq!(gate.schema_state, Some("valid".to_string()));
        assert_eq!(gate.recent_backfill_status, Some("ready".to_string()));
        assert!(!gate.fallback_allowed);
    }

    #[test]
    fn storage_gate_marks_scaffold_storage_not_ready() {
        let executable = PathBuf::from("/tmp/onecontext-memoryd");
        let value = json!({
            "status": "ok",
            "result": {
                "status": "managed_postgres_not_implemented",
                "backend": "managed_postgres",
                "ready": false,
                "storage_ready": false,
                "recent_history_ready": false,
                "schema_state": "missing_or_incompatible",
                "recent_backfill": {"status": "not_checked"},
                "message": "Managed Postgres storage is declared, but ensureStorageReady is not implemented."
            }
        });

        let gate = storage_gate_from_protocol_value(&executable, value, true);

        assert_eq!(gate.status, "not_ready");
        assert!(!gate.ready_for_source_packets());
        assert!(gate.fallback_allowed);
        assert!(gate
            .failure_message()
            .contains(ALLOW_EMPTY_MEMORY_FALLBACK_ENV));
    }

    #[test]
    fn recent_backfill_error_closes_gate() {
        let mut gate = MemoryStorageGate {
            status: "not_ready".to_string(),
            provider: "onecontext-memoryd".to_string(),
            executable: Some("/tmp/onecontext-memoryd".to_string()),
            backend: Some("external_postgres".to_string()),
            storage_status: Some("ready".to_string()),
            ready: true,
            storage_ready: true,
            recent_history_ready: false,
            recent_backfill_status: None,
            schema_state: Some("valid".to_string()),
            fallback_allowed: false,
            error: None,
            message: None,
        };

        merge_recent_backfill_value(
            &mut gate,
            json!({
                "status": "error",
                "error": {"message": "memory.ensureRecentBackfill is declared but not implemented"}
            }),
        );

        assert_eq!(gate.status, "error");
        assert!(!gate.ready_for_source_packets());
        assert!(gate
            .error
            .as_deref()
            .is_some_and(|error| error.contains("ensureRecentBackfill")));
    }

    #[test]
    fn memoryd_child_env_does_not_select_external_backend_from_db_url_alone() {
        let overrides = memoryd_child_env_overrides_from(
            None,
            None,
            Some("postgres://onecontext:secret@127.0.0.1:5432/onecontext"),
        );

        assert!(overrides.is_empty());
    }

    #[test]
    fn memoryd_child_env_allows_explicit_external_backend() {
        let overrides = memoryd_child_env_overrides_from(
            Some("external_postgres"),
            None,
            Some("postgres://onecontext:secret@127.0.0.1:5432/onecontext"),
        );

        assert_eq!(
            overrides,
            vec![(ALLOW_EXTERNAL_POSTGRES_ENV, "1".to_string())]
        );
    }

    #[test]
    fn memoryd_child_env_respects_explicit_managed_backend() {
        let overrides = memoryd_child_env_overrides_from(
            Some("managed_postgres"),
            None,
            Some("postgres://onecontext:secret@127.0.0.1:5432/onecontext"),
        );

        assert!(overrides.is_empty());
    }

    #[test]
    fn storage_gate_requires_valid_schema() {
        let executable = PathBuf::from("/tmp/onecontext-memoryd");
        let value = json!({
            "status": "ok",
            "result": {
                "status": "external_postgres_ready",
                "backend": "external_postgres",
                "ready": true,
                "storage_ready": true,
                "recent_history_ready": true,
                "schema_state": "missing_or_incompatible",
                "recent_backfill": {"status": "ready"},
                "message": "External Postgres is reachable."
            }
        });

        let gate = storage_gate_from_protocol_value(&executable, value, false);

        assert_eq!(gate.status, "not_ready");
        assert!(!gate.ready_for_source_packets());
    }

    #[test]
    fn hydrated_objects_become_real_source_events() {
        let value = json!({
            "status": "ok",
            "result": {
                "objects": [{
                    "object_id": "11111111-1111-1111-1111-111111111111",
                    "event_start": "2026-06-08T12:00:00Z",
                    "source_id": "22222222-2222-2222-2222-222222222222",
                    "series_id": "33333333-3333-3333-3333-333333333333",
                    "series_kind": "imessage",
                    "series_key": "chat-a",
                    "series_display_name": "iMessage Chat A",
                    "kind": "imessage_message",
                    "role": "user",
                    "privacy_class": "private",
                    "body_type": "text",
                    "text_value": "The actual message body.",
                    "source_record": {
                        "source_record_key": "imessage:row:42"
                    }
                }]
            }
        });

        let events = source_events_from_hydrated_objects(&value);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text, "The actual message body.");
        assert_eq!(events[0].session_id, "chat-a");
        assert_eq!(events[0].kind, "imessage_message");
        assert_eq!(events[0].source.as_deref(), Some("imessage:row:42"));
        assert_eq!(
            events[0].object_id.as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(
            events[0].source_id.as_deref(),
            Some("22222222-2222-2222-2222-222222222222")
        );
        assert_eq!(events[0].source_type.as_deref(), Some("imessage"));
        assert_eq!(
            events[0].source_record_key.as_deref(),
            Some("imessage:row:42")
        );
        assert_eq!(events[0].privacy_class.as_deref(), Some("private"));
        assert_eq!(events[0].role.as_deref(), Some("user"));
    }

    #[test]
    fn hydrated_objects_without_body_are_not_raw_source_events() {
        let value = json!({
            "status": "ok",
            "result": {
                "objects": [{
                    "object_id": "11111111-1111-1111-1111-111111111111",
                    "event_start": "2026-06-08T12:00:00Z",
                    "source_id": "22222222-2222-2222-2222-222222222222",
                    "series_id": "33333333-3333-3333-3333-333333333333",
                    "series_key": "chat-a",
                    "kind": "imessage_message",
                    "payload": null
                }]
            }
        });

        assert!(source_events_from_hydrated_objects(&value).is_empty());
    }
}
