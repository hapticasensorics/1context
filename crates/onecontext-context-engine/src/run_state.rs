//! Durable run-local scheduler state for the `context-engine/live/runs/<run-id>` envelope.

use crate::artifacts::ArtifactHandle;
use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyCursors {
    pub raw_ingest_cursor: Option<String>,
    pub wiki_memory_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobReceiptState {
    pub unit_id: String,
    pub phase_id: String,
    pub job_id: String,
    pub channel: String,
    pub authority: String,
    pub authoritative: bool,
    pub status: String,
    pub receipt_paths: Vec<String>,
    pub audit_paths: Vec<String>,
    pub external_id: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyRunState {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    #[serde(default = "default_run_status")]
    pub status: String,
    #[serde(default)]
    pub terminal_reason: Option<String>,
    pub updated_at: String,
    pub cursors: WikiCompanyCursors,
    pub completed_phase_ids: Vec<String>,
    pub artifacts: Vec<ArtifactHandle>,
    pub job_receipts: Vec<JobReceiptState>,
    pub counters: BTreeMap<String, u64>,
}

impl WikiCompanyRunState {
    pub fn new(run_id: impl Into<String>) -> Self {
        let run_id = run_id.into();
        Self {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: "onecontext.context_engine.wiki_company_run_state.v1".to_string(),
            run_id: safe_run_id(&run_id),
            status: "active".to_string(),
            terminal_reason: None,
            updated_at: now(),
            cursors: WikiCompanyCursors::default(),
            completed_phase_ids: Vec::new(),
            artifacts: Vec::new(),
            job_receipts: Vec::new(),
            counters: BTreeMap::new(),
        }
    }

    pub fn mark_phase_completed(&mut self, phase_id: impl Into<String>) {
        let phase_id = phase_id.into();
        let mut phases = self
            .completed_phase_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        phases.insert(phase_id);
        self.completed_phase_ids = phases.into_iter().collect();
        self.updated_at = now();
    }

    pub fn mark_interrupted(&mut self, reason: impl Into<String>) {
        self.status = "interrupted".to_string();
        self.terminal_reason = Some(reason.into());
        self.updated_at = now();
    }

    pub fn mark_failed(&mut self, reason: impl Into<String>) {
        self.status = "failed".to_string();
        self.terminal_reason = Some(reason.into());
        self.updated_at = now();
    }

    pub fn mark_completed(&mut self) {
        self.status = "completed".to_string();
        self.terminal_reason = None;
        self.updated_at = now();
    }

    pub fn set_raw_ingest_cursor(&mut self, cursor: impl Into<String>) {
        self.cursors.raw_ingest_cursor = Some(cursor.into());
        self.updated_at = now();
    }

    pub fn set_wiki_memory_cursor(&mut self, cursor: impl Into<String>) {
        self.cursors.wiki_memory_cursor = Some(cursor.into());
        self.updated_at = now();
    }

    pub fn update_cursors(&mut self, cursors: WikiCompanyCursors) {
        if let Some(cursor) = cursors.raw_ingest_cursor {
            self.cursors.raw_ingest_cursor = Some(cursor);
        }
        if let Some(cursor) = cursors.wiki_memory_cursor {
            self.cursors.wiki_memory_cursor = Some(cursor);
        }
        self.updated_at = now();
    }

    pub fn add_artifact(&mut self, artifact: ArtifactHandle) {
        if let Some(existing) = self
            .artifacts
            .iter_mut()
            .find(|existing| existing.artifact_id == artifact.artifact_id)
        {
            *existing = artifact;
        } else {
            self.artifacts.push(artifact);
        }
        self.updated_at = now();
    }

    pub fn record_job_receipt(&mut self, receipt: JobReceiptState) {
        if let Some(existing) = self.job_receipts.iter_mut().find(|existing| {
            existing.unit_id == receipt.unit_id
                && existing.phase_id == receipt.phase_id
                && existing.job_id == receipt.job_id
                && existing.channel == receipt.channel
        }) {
            *existing = receipt;
        } else {
            self.job_receipts.push(receipt);
        }
        self.updated_at = now();
    }

    pub fn available_artifact_kinds(&self) -> Vec<String> {
        let mut kinds = self
            .artifacts
            .iter()
            .map(|artifact| artifact.kind.clone())
            .collect::<BTreeSet<_>>();
        kinds.insert("run_state_artifacts_known".to_string());
        kinds.into_iter().collect()
    }

    pub fn available_artifacts_for_scheduler(&self) -> Vec<String> {
        artifact_availability_keys(&self.artifacts)
    }

    pub fn available_mail(&self) -> Vec<String> {
        let mut available = BTreeSet::new();
        for receipt in &self.job_receipts {
            if receipt.channel != "agent_mail"
                || !receipt.authoritative
                || receipt.status != "complete"
            {
                continue;
            }
            available.insert(format!("phase:{}", receipt.phase_id));
            available.insert(format!("job:{}", receipt.job_id));
            available.insert(format!("unit:{}", receipt.unit_id));
            for path in &receipt.receipt_paths {
                available.insert(path.clone());
            }
            if let Some(external_id) = &receipt.external_id {
                available.insert(format!("external:{external_id}"));
            }
        }
        available.into_iter().collect()
    }
}

pub fn artifact_availability_keys(artifacts: &[ArtifactHandle]) -> Vec<String> {
    let mut keys = BTreeSet::new();
    keys.insert("run_state_artifacts_known".to_string());
    for artifact in artifacts {
        keys.extend(artifact.availability_keys());
    }
    keys.into_iter().collect()
}

pub fn state_path(paths: &ContextEnginePaths, run_id: &str) -> PathBuf {
    paths.run_json_path(run_id)
}

pub fn load_or_new_state(
    paths: &ContextEnginePaths,
    run_id: &str,
) -> Result<WikiCompanyRunState, String> {
    let path = state_path(paths, run_id);
    if !path.is_file() {
        return Ok(WikiCompanyRunState::new(run_id));
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let state: WikiCompanyRunState = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    validate_state_for_run(&state, run_id, &path)?;
    Ok(state)
}

pub fn save_state(
    paths: &ContextEnginePaths,
    state: &WikiCompanyRunState,
) -> Result<PathBuf, String> {
    let path = state_path(paths, &state.run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let mut saved = state.clone();
    saved.updated_at = now();
    let text = serde_json::to_string_pretty(&saved)
        .map_err(|error| format!("failed to encode run state: {error}"))?;
    let tmp_path = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&tmp_path, text)
        .map_err(|error| format!("failed to write {}: {error}", tmp_path.display()))?;
    fs::rename(&tmp_path, &path).map_err(|error| {
        let _ = fs::remove_file(&tmp_path);
        format!(
            "failed to replace {} with {}: {error}",
            path.display(),
            tmp_path.display()
        )
    })?;
    save_current_run_pointer(paths, &saved, &path)?;
    Ok(path)
}

fn save_current_run_pointer(
    paths: &ContextEnginePaths,
    state: &WikiCompanyRunState,
    state_path: &PathBuf,
) -> Result<(), String> {
    if let Some(parent) = paths.state_current_run_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let pointer = json!({
        "schema_version": CONTEXT_ENGINE_SCHEMA_VERSION,
        "kind": "onecontext.context_engine.current_wiki_company_run.v1",
        "current_run_id": state.run_id.clone(),
        "status": state.status.clone(),
        "terminal_reason": state.terminal_reason.clone(),
        "state_path": state_path.display().to_string(),
        "updated_at": state.updated_at,
        "time_basis": "utc",
    });
    fs::write(
        &paths.state_current_run_file,
        serde_json::to_vec_pretty(&pointer)
            .map_err(|error| format!("failed to encode current run pointer: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write current run pointer {}: {error}",
            paths.state_current_run_file.display()
        )
    })
}

fn validate_state_for_run(
    state: &WikiCompanyRunState,
    requested_run_id: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let expected_run_id = safe_run_id(requested_run_id);
    if state.schema_version != CONTEXT_ENGINE_SCHEMA_VERSION {
        return Err(format!(
            "state {} has schema_version {}, expected {}",
            path.display(),
            state.schema_version,
            CONTEXT_ENGINE_SCHEMA_VERSION
        ));
    }
    if state.kind != "onecontext.context_engine.wiki_company_run_state.v1" {
        return Err(format!(
            "state {} has kind {}, expected onecontext.context_engine.wiki_company_run_state.v1",
            path.display(),
            state.kind
        ));
    }
    if state.run_id != expected_run_id {
        return Err(format!(
            "state {} belongs to run {}, expected {}",
            path.display(),
            state.run_id,
            expected_run_id
        ));
    }
    Ok(())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn default_run_status() -> String {
    "active".to_string()
}
