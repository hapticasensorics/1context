//! Product-visible wiki marker for Context Engine refreshes.

use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiRefreshMarker {
    pub schema_version: u32,
    pub status: String,
    pub page: String,
    pub operation_id: String,
    pub executable: Option<String>,
    pub create_pages: Option<Value>,
    pub talk_append: Option<Value>,
    pub error: Option<String>,
}

impl WikiRefreshMarker {
    fn unavailable(error: impl Into<String>) -> Self {
        Self {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            status: "unavailable".to_string(),
            page: "for-you".to_string(),
            operation_id: String::new(),
            executable: None,
            create_pages: None,
            talk_append: None,
            error: Some(error.into()),
        }
    }
}

pub fn append_context_engine_refresh_marker(
    paths: &ContextEnginePaths,
    run_id: &str,
    trigger: &str,
    execute_agents: bool,
) -> WikiRefreshMarker {
    let Some(executable) = discover_wiki_executable() else {
        return WikiRefreshMarker::unavailable("onecontext-wiki executable not found");
    };
    let operation_id = safe_run_id(&format!("{run_id}-context-engine-refresh"));
    let create_pages = run_wiki_json(
        &executable,
        &[
            "--root",
            &paths.root.display().to_string(),
            "page-create-all",
        ],
    );
    let create_pages = match create_pages {
        Ok(value) => value,
        Err(error) => {
            return WikiRefreshMarker {
                schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
                status: "failed".to_string(),
                page: "for-you".to_string(),
                operation_id,
                executable: Some(executable.display().to_string()),
                create_pages: None,
                talk_append: None,
                error: Some(error),
            };
        }
    };

    let body = format!(
        "Context Engine refreshed the wiki through the native product path.\n\n- Run: `{}`\n- Trigger: `{}`\n- Runtime owner: `onecontext-context-engine`\n- Agent execution requested: `{}`\n",
        safe_run_id(run_id),
        trigger,
        execute_agents
    );
    let talk_append = run_wiki_json(
        &executable,
        &[
            "--root",
            &paths.root.display().to_string(),
            "talk-append",
            "--page",
            "for-you",
            "--kind",
            "status",
            "--subject",
            "Context Engine Refresh",
            "--from",
            "agent://context-engine.runner",
            "--operation-id",
            &operation_id,
            "--delivery-mode",
            "labels-only",
            "--body",
            &body,
        ],
    );
    match talk_append {
        Ok(value) => WikiRefreshMarker {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            status: "written".to_string(),
            page: "for-you".to_string(),
            operation_id,
            executable: Some(executable.display().to_string()),
            create_pages: Some(create_pages),
            talk_append: Some(value),
            error: None,
        },
        Err(error) => WikiRefreshMarker {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            status: "failed".to_string(),
            page: "for-you".to_string(),
            operation_id,
            executable: Some(executable.display().to_string()),
            create_pages: Some(create_pages),
            talk_append: None,
            error: Some(error),
        },
    }
}

fn run_wiki_json(executable: &PathBuf, args: &[&str]) -> Result<Value, String> {
    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run {}: {error}", executable.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if detail.is_empty() {
            format!("{} exited {}", executable.display(), output.status)
        } else {
            detail
        });
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("{} returned invalid JSON: {error}", executable.display()))
}

fn discover_wiki_executable() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = env::var("ONECONTEXT_WIKI_BIN") {
        if !path.trim().is_empty() {
            candidates.push(PathBuf::from(path));
        }
    }
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            candidates.push(dir.join("onecontext-wiki"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("onecontext-wiki"));
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("target/debug/onecontext-wiki"));
        candidates.push(cwd.join("target/release/onecontext-wiki"));
    }
    candidates.into_iter().find(|path| path.is_file())
}
