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
pub const ALLOW_EMPTY_MEMORY_FALLBACK_ENV: &str = "ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK";

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
pub struct MemoryStorageGate {
    pub status: String,
    pub provider: String,
    pub executable: Option<String>,
    pub storage_status: Option<String>,
    pub storage_ready: bool,
    pub recent_history_ready: bool,
    pub recent_backfill_status: Option<String>,
    pub fallback_allowed: bool,
    pub error: Option<String>,
    pub message: Option<String>,
}

impl MemoryStorageGate {
    pub fn ready_for_source_packets(&self) -> bool {
        self.status == "ok" && self.storage_ready && self.recent_history_ready
    }

    pub fn failure_message(&self) -> String {
        let detail = self
            .message
            .as_deref()
            .or(self.error.as_deref())
            .unwrap_or("memory storage is not ready");
        format!(
            "Perception memory storage gate failed: {detail}. Set {ALLOW_EMPTY_MEMORY_FALLBACK_ENV}=1 to permit an empty dry-run packet plan."
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
                storage_status: None,
                storage_ready: false,
                recent_history_ready: false,
                recent_backfill_status: None,
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
                    storage_status: None,
                    storage_ready: false,
                    recent_history_ready: false,
                    recent_backfill_status: None,
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

fn run_memoryd_protocol(
    executable: &PathBuf,
    protocol_method: &str,
    request: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut child = Command::new(executable)
        .args(["protocol", protocol_method, "--request-json", "-"])
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
        status: if status == "ok" && storage_ready && recent_history_ready {
            "ok".to_string()
        } else if status == "ok" {
            "not_ready".to_string()
        } else {
            "error".to_string()
        },
        provider: "onecontext-memoryd".to_string(),
        executable: Some(executable.display().to_string()),
        storage_status,
        storage_ready,
        recent_history_ready,
        recent_backfill_status,
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
    gate.status = if gate.storage_ready && gate.recent_history_ready {
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
                "storage_ready": true,
                "recent_history_ready": true,
                "recent_backfill": {"status": "ready"},
                "message": "ready"
            }
        });

        let gate = storage_gate_from_protocol_value(&executable, value, false);

        assert!(gate.ready_for_source_packets());
        assert_eq!(gate.storage_status, Some("ready".to_string()));
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
                "storage_ready": false,
                "recent_history_ready": false,
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
            storage_status: Some("ready".to_string()),
            storage_ready: true,
            recent_history_ready: false,
            recent_backfill_status: None,
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
}
