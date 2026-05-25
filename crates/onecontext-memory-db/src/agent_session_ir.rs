//! Shared coding-agent session IR for Codex, Claude, and 1Context agents.
//!
//! This module is intentionally storage-agnostic. Source adapters reduce native
//! logs into these DTOs first; later emission code can map them to
//! `memory.writeObjects` inputs without re-parsing source-specific history.

use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::local_adapters::SessionIngestProfile;
use crate::source_identity::canonical_source_hash;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    Codex,
    Claude,
    OnecontextAgent,
}

impl AgentSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::OnecontextAgent => "onecontext_agent",
        }
    }

    pub const fn source_id(self) -> &'static str {
        match self {
            Self::Codex => "10000000-0000-0000-0000-000000000001",
            Self::Claude => "10000000-0000-0000-0000-000000000002",
            Self::OnecontextAgent => "10000000-0000-0000-0000-000000000004",
        }
    }
}

impl fmt::Display for AgentSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentIngestProfile {
    #[default]
    #[serde(alias = "messages_only", alias = "messages-only")]
    HotMemory,
    #[serde(
        alias = "messages_and_compact_tools",
        alias = "messages-and-compact-tools"
    )]
    CompactAudit,
    Forensic,
}

impl From<SessionIngestProfile> for AgentIngestProfile {
    fn from(value: SessionIngestProfile) -> Self {
        match value {
            SessionIngestProfile::HotMemory => Self::HotMemory,
            SessionIngestProfile::CompactAudit => Self::CompactAudit,
            SessionIngestProfile::Forensic => Self::Forensic,
        }
    }
}

impl From<AgentIngestProfile> for SessionIngestProfile {
    fn from(value: AgentIngestProfile) -> Self {
        match value {
            AgentIngestProfile::HotMemory => Self::HotMemory,
            AgentIngestProfile::CompactAudit => Self::CompactAudit,
            AgentIngestProfile::Forensic => Self::Forensic,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentProjection {
    ModelVisible,
    UiTimeline,
    Audit,
    PromptSnapshot,
    MemoryCandidate,
    Forensic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentItemRole {
    User,
    Assistant,
    System,
    Tool,
    Runtime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentItemKind {
    Message,
    ToolCall,
    ToolResult,
    Reasoning,
    Patch,
    FileChange,
    RuntimeEvent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnStatus {
    Completed,
    Interrupted,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeSeverity {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentSessionIr {
    pub source: AgentSource,
    pub source_id: String,
    pub session_id: String,
    pub session_key: String,
    pub source_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
    #[serde(default)]
    pub turns: Vec<AgentTurnIr>,
    #[serde(default)]
    pub session_items: Vec<AgentItemIr>,
    #[serde(default)]
    pub compactions: Vec<AgentCompactionIr>,
    #[serde(default)]
    pub prompt_snapshots: Vec<AgentPromptSnapshotIr>,
    #[serde(default)]
    pub runtime_events: Vec<AgentRuntimeEventIr>,
}

impl AgentSessionIr {
    pub fn validate(&self) -> Result<(), AgentIrValidationError> {
        validate_agent_session_ir(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurnIr {
    pub turn_id: String,
    pub turn_index: u64,
    pub event_start: String,
    pub event_end: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AgentTurnStatus>,
    #[serde(default)]
    pub item_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_snapshot_id: Option<String>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentItemIr {
    pub item_id: String,
    pub source_record_key: String,
    pub event_start: String,
    pub event_end: String,
    pub role: AgentItemRole,
    pub kind: AgentItemKind,
    #[serde(default)]
    pub projections: Vec<AgentProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_text: Option<String>,
    #[serde(default = "empty_object")]
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<RawEvidenceRef>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

impl AgentItemIr {
    pub fn validate(&self) -> Result<(), AgentIrValidationError> {
        validate_agent_item_ir(self, "item")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCompactionIr {
    pub compaction_id: String,
    pub source_record_key: String,
    pub event_start: String,
    pub event_end: String,
    pub compaction_epoch: u64,
    #[serde(default)]
    pub replacement_item_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_item_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_text: Option<String>,
    pub replacement_history_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<RawEvidenceRef>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentPromptSnapshotIr {
    pub prompt_snapshot_id: String,
    pub source_record_key: String,
    pub turn_id: String,
    pub event_start: String,
    pub event_end: String,
    pub compaction_epoch: u64,
    #[serde(default)]
    pub input_item_ids: Vec<String>,
    pub input_item_count: usize,
    pub tool_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_instructions_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_tools_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<u64>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRuntimeEventIr {
    pub event_id: String,
    pub source_record_key: String,
    pub event_start: String,
    pub event_end: String,
    pub event_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<AgentRuntimeSeverity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_text: Option<String>,
    #[serde(default = "empty_object")]
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<RawEvidenceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawEvidenceRef {
    pub source_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl RawEvidenceRef {
    pub fn validate(&self, path: impl Into<String>) -> Result<(), AgentIrValidationError> {
        validate_raw_ref(self, &path.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentIrValidationError {
    pub path: String,
    pub message: String,
}

impl AgentIrValidationError {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for AgentIrValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for AgentIrValidationError {}

pub fn default_item_projections(
    profile: AgentIngestProfile,
    role: AgentItemRole,
    kind: AgentItemKind,
) -> Vec<AgentProjection> {
    use AgentIngestProfile::{CompactAudit, Forensic, HotMemory};
    use AgentItemKind::{
        FileChange, Message, Patch, Reasoning, RuntimeEvent, ToolCall, ToolResult,
    };
    use AgentItemRole::{Assistant, System, User};
    use AgentProjection::UiTimeline;
    use AgentProjection::{Audit, Forensic as ForensicProjection, MemoryCandidate, ModelVisible};

    let mut projections = match kind {
        Message => {
            let mut values = vec![UiTimeline];
            if matches!(role, User | Assistant | System) {
                values.insert(0, ModelVisible);
            }
            if matches!(role, User | Assistant) {
                values.push(MemoryCandidate);
            }
            values
        }
        Reasoning => match profile {
            HotMemory => vec![Audit],
            CompactAudit | Forensic => vec![UiTimeline, Audit],
        },
        Patch | FileChange => vec![UiTimeline, Audit, MemoryCandidate],
        ToolCall | ToolResult => match profile {
            HotMemory => vec![Audit],
            CompactAudit | Forensic => vec![UiTimeline, Audit],
        },
        RuntimeEvent => vec![Audit],
    };

    if matches!(profile, Forensic) && !projections.contains(&ForensicProjection) {
        projections.push(ForensicProjection);
    }

    projections
}

pub fn mark_prompt_snapshot_projection(projections: &mut Vec<AgentProjection>) {
    if !projections.contains(&AgentProjection::PromptSnapshot) {
        projections.push(AgentProjection::PromptSnapshot);
    }
}

pub fn source_key_session(source: AgentSource, session_id: &str) -> String {
    format!("agent/{source}/{session_id}")
}

pub fn source_key_turn(source: AgentSource, session_id: &str, turn_index: u64) -> String {
    format!("agent/{source}/{session_id}/turn/{turn_index}")
}

pub fn source_key_item(source: AgentSource, session_id: &str, item_id: &str) -> String {
    format!("agent/{source}/{session_id}/item/{item_id}")
}

pub fn source_key_line(
    source: AgentSource,
    session_id: &str,
    line_number: u64,
    source_hash: &str,
) -> String {
    format!("agent/{source}/{session_id}/line/{line_number}/{source_hash}")
}

pub fn source_key_tool_summary(
    source: AgentSource,
    session_id: &str,
    tool_call_id: &str,
) -> String {
    format!("agent/{source}/{session_id}/tool/{tool_call_id}/summary")
}

pub fn source_key_compaction(
    source: AgentSource,
    session_id: &str,
    compaction_epoch: u64,
    hash: &str,
) -> String {
    format!("agent/{source}/{session_id}/compaction/{compaction_epoch}/{hash}")
}

pub fn source_key_prompt_snapshot(
    source: AgentSource,
    session_id: &str,
    turn_index: u64,
    hash: &str,
) -> String {
    format!("agent/{source}/{session_id}/turn/{turn_index}/prompt/{hash}")
}

pub fn compact_hash_bytes(bytes: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(bytes.as_ref());
    hex_lower(&digest)
}

pub fn compact_text_hash(text: &str) -> String {
    compact_hash_bytes(text.as_bytes())
}

pub fn compact_json_hash(value: &Value) -> String {
    canonical_source_hash(value)
}

pub fn replacement_history_hash<I, S>(item_ids: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    stable_string_sequence_hash("agent.replacement_history", item_ids)
}

pub fn prompt_input_hash<I, S>(input_item_ids: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    stable_string_sequence_hash("agent.prompt_input", input_item_ids)
}

pub fn validate_agent_session_ir(session: &AgentSessionIr) -> Result<(), AgentIrValidationError> {
    require_non_empty("session.source_id", &session.source_id)?;
    require_non_empty("session.session_id", &session.session_id)?;
    require_non_empty("session.session_key", &session.session_key)?;
    require_non_empty("session.source_uri", &session.source_uri)?;
    validate_optional_time_pair(
        "session",
        session.started_at.as_deref(),
        session.ended_at.as_deref(),
    )?;
    validate_json_object("session.metadata", &session.metadata)?;

    let item_ids = session
        .session_items
        .iter()
        .map(|item| item.item_id.as_str())
        .collect::<HashSet<_>>();
    let turn_ids = session
        .turns
        .iter()
        .map(|turn| turn.turn_id.as_str())
        .collect::<HashSet<_>>();
    let prompt_snapshot_ids = session
        .prompt_snapshots
        .iter()
        .map(|snapshot| snapshot.prompt_snapshot_id.as_str())
        .collect::<HashSet<_>>();

    for (index, turn) in session.turns.iter().enumerate() {
        let path = format!("session.turns[{index}]");
        require_non_empty(format!("{path}.turn_id"), &turn.turn_id)?;
        validate_time_pair(&path, &turn.event_start, &turn.event_end)?;
        validate_json_object(format!("{path}.metadata"), &turn.metadata)?;
        for item_id in &turn.item_ids {
            if !item_ids.contains(item_id.as_str()) {
                return Err(AgentIrValidationError::new(
                    format!("{path}.item_ids"),
                    format!("unknown item id {item_id:?}"),
                ));
            }
        }
        if let Some(prompt_snapshot_id) = &turn.prompt_snapshot_id {
            if !prompt_snapshot_ids.contains(prompt_snapshot_id.as_str()) {
                return Err(AgentIrValidationError::new(
                    format!("{path}.prompt_snapshot_id"),
                    format!("unknown prompt snapshot id {prompt_snapshot_id:?}"),
                ));
            }
        }
    }

    for (index, item) in session.session_items.iter().enumerate() {
        validate_agent_item_ir(item, &format!("session.session_items[{index}]"))?;
    }

    for (index, compaction) in session.compactions.iter().enumerate() {
        let path = format!("session.compactions[{index}]");
        require_non_empty(format!("{path}.compaction_id"), &compaction.compaction_id)?;
        require_non_empty(
            format!("{path}.source_record_key"),
            &compaction.source_record_key,
        )?;
        validate_time_pair(&path, &compaction.event_start, &compaction.event_end)?;
        require_non_empty(
            format!("{path}.replacement_history_hash"),
            &compaction.replacement_history_hash,
        )?;
        if compaction.replacement_item_ids.is_empty() {
            return Err(AgentIrValidationError::new(
                format!("{path}.replacement_item_ids"),
                "must contain at least one replacement item id",
            ));
        }
        for item_id in &compaction.replacement_item_ids {
            if !item_ids.contains(item_id.as_str()) {
                return Err(AgentIrValidationError::new(
                    format!("{path}.replacement_item_ids"),
                    format!("unknown replacement item id {item_id:?}"),
                ));
            }
        }
        if let Some(replaced_item_ids) = &compaction.replaced_item_ids {
            for item_id in replaced_item_ids {
                if !item_ids.contains(item_id.as_str()) {
                    return Err(AgentIrValidationError::new(
                        format!("{path}.replaced_item_ids"),
                        format!("unknown replaced item id {item_id:?}"),
                    ));
                }
            }
        }
        if let Some(raw_ref) = &compaction.raw_ref {
            validate_raw_ref(raw_ref, &format!("{path}.raw_ref"))?;
        }
        validate_json_object(format!("{path}.metadata"), &compaction.metadata)?;
    }

    for (index, snapshot) in session.prompt_snapshots.iter().enumerate() {
        let path = format!("session.prompt_snapshots[{index}]");
        require_non_empty(
            format!("{path}.prompt_snapshot_id"),
            &snapshot.prompt_snapshot_id,
        )?;
        require_non_empty(
            format!("{path}.source_record_key"),
            &snapshot.source_record_key,
        )?;
        require_non_empty(format!("{path}.turn_id"), &snapshot.turn_id)?;
        if !turn_ids.contains(snapshot.turn_id.as_str()) {
            return Err(AgentIrValidationError::new(
                format!("{path}.turn_id"),
                format!("unknown turn id {:?}", snapshot.turn_id),
            ));
        }
        validate_time_pair(&path, &snapshot.event_start, &snapshot.event_end)?;
        if snapshot.input_item_count != snapshot.input_item_ids.len() {
            return Err(AgentIrValidationError::new(
                format!("{path}.input_item_count"),
                "must match input_item_ids length",
            ));
        }
        for item_id in &snapshot.input_item_ids {
            if !item_ids.contains(item_id.as_str()) {
                return Err(AgentIrValidationError::new(
                    format!("{path}.input_item_ids"),
                    format!("unknown input item id {item_id:?}"),
                ));
            }
        }
        validate_optional_hash(
            format!("{path}.base_instructions_hash"),
            snapshot.base_instructions_hash.as_deref(),
        )?;
        validate_optional_hash(
            format!("{path}.dynamic_tools_hash"),
            snapshot.dynamic_tools_hash.as_deref(),
        )?;
        validate_optional_hash(
            format!("{path}.prompt_hash"),
            snapshot.prompt_hash.as_deref(),
        )?;
        validate_json_object(format!("{path}.metadata"), &snapshot.metadata)?;
    }

    for (index, event) in session.runtime_events.iter().enumerate() {
        let path = format!("session.runtime_events[{index}]");
        require_non_empty(format!("{path}.event_id"), &event.event_id)?;
        require_non_empty(
            format!("{path}.source_record_key"),
            &event.source_record_key,
        )?;
        require_non_empty(format!("{path}.event_kind"), &event.event_kind)?;
        validate_time_pair(&path, &event.event_start, &event.event_end)?;
        validate_json_object(format!("{path}.payload"), &event.payload)?;
        if let Some(raw_ref) = &event.raw_ref {
            validate_raw_ref(raw_ref, &format!("{path}.raw_ref"))?;
        }
    }

    Ok(())
}

pub fn validate_agent_item_ir(
    item: &AgentItemIr,
    path: &str,
) -> Result<(), AgentIrValidationError> {
    require_non_empty(format!("{path}.item_id"), &item.item_id)?;
    require_non_empty(format!("{path}.source_record_key"), &item.source_record_key)?;
    validate_time_pair(path, &item.event_start, &item.event_end)?;
    if item.projections.is_empty() {
        return Err(AgentIrValidationError::new(
            format!("{path}.projections"),
            "must contain at least one projection",
        ));
    }
    validate_unique_projections(path, &item.projections)?;
    validate_json_object(format!("{path}.payload"), &item.payload)?;
    validate_json_object(format!("{path}.metadata"), &item.metadata)?;
    if let Some(raw_ref) = &item.raw_ref {
        validate_raw_ref(raw_ref, &format!("{path}.raw_ref"))?;
    }
    Ok(())
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

fn validate_raw_ref(raw_ref: &RawEvidenceRef, path: &str) -> Result<(), AgentIrValidationError> {
    require_non_empty(format!("{path}.source_uri"), &raw_ref.source_uri)?;
    validate_optional_hash(format!("{path}.sha256"), raw_ref.sha256.as_deref())?;
    Ok(())
}

fn validate_json_object(
    path: impl Into<String>,
    value: &Value,
) -> Result<(), AgentIrValidationError> {
    if !value.is_object() {
        return Err(AgentIrValidationError::new(path, "must be a JSON object"));
    }
    Ok(())
}

fn validate_unique_projections(
    path: &str,
    projections: &[AgentProjection],
) -> Result<(), AgentIrValidationError> {
    let mut seen = HashSet::new();
    for projection in projections {
        if !seen.insert(*projection) {
            return Err(AgentIrValidationError::new(
                format!("{path}.projections"),
                format!("duplicate projection {projection:?}"),
            ));
        }
    }
    Ok(())
}

fn require_non_empty(path: impl Into<String>, value: &str) -> Result<(), AgentIrValidationError> {
    if value.trim().is_empty() {
        return Err(AgentIrValidationError::new(path, "must not be empty"));
    }
    Ok(())
}

fn validate_time_pair(
    path: &str,
    event_start: &str,
    event_end: &str,
) -> Result<(), AgentIrValidationError> {
    require_non_empty(format!("{path}.event_start"), event_start)?;
    require_non_empty(format!("{path}.event_end"), event_end)?;
    validate_optional_time_pair(path, Some(event_start), Some(event_end))
}

fn validate_optional_time_pair(
    path: &str,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<(), AgentIrValidationError> {
    let Some(start) = start else {
        return Ok(());
    };
    let Some(end) = end else {
        return Ok(());
    };
    let start = parse_rfc3339(path, "event_start", start)?;
    let end = parse_rfc3339(path, "event_end", end)?;
    if end < start {
        return Err(AgentIrValidationError::new(
            format!("{path}.event_end"),
            "must be greater than or equal to event_start",
        ));
    }
    Ok(())
}

fn parse_rfc3339(
    path: &str,
    field: &str,
    value: &str,
) -> Result<DateTime<Utc>, AgentIrValidationError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AgentIrValidationError::new(format!("{path}.{field}"), "must be RFC3339"))
}

fn validate_optional_hash(
    path: impl Into<String>,
    value: Option<&str>,
) -> Result<(), AgentIrValidationError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentIrValidationError::new(
            path,
            "must be a 64-character hex sha256",
        ));
    }
    Ok(())
}

fn stable_string_sequence_hash<I, S>(namespace: &str, values: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    for value in values {
        hasher.update(value.as_ref().as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    hex_lower(&digest)
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_projection_semantics_match_ingest_profiles() {
        assert_eq!(
            default_item_projections(
                AgentIngestProfile::HotMemory,
                AgentItemRole::Assistant,
                AgentItemKind::Message,
            ),
            vec![
                AgentProjection::ModelVisible,
                AgentProjection::UiTimeline,
                AgentProjection::MemoryCandidate,
            ]
        );
        assert_eq!(
            default_item_projections(
                AgentIngestProfile::HotMemory,
                AgentItemRole::Tool,
                AgentItemKind::ToolResult,
            ),
            vec![AgentProjection::Audit]
        );
        assert_eq!(
            default_item_projections(
                AgentIngestProfile::CompactAudit,
                AgentItemRole::Tool,
                AgentItemKind::ToolResult,
            ),
            vec![AgentProjection::UiTimeline, AgentProjection::Audit]
        );
        assert_eq!(
            default_item_projections(
                AgentIngestProfile::Forensic,
                AgentItemRole::Tool,
                AgentItemKind::ToolResult,
            ),
            vec![
                AgentProjection::UiTimeline,
                AgentProjection::Audit,
                AgentProjection::Forensic,
            ]
        );
    }

    #[test]
    fn source_key_helpers_are_stable_and_match_spec_shapes() {
        let source = AgentSource::Codex;
        let session_id = "019e-session";

        assert_eq!(source.source_id(), "10000000-0000-0000-0000-000000000001");
        assert_eq!(
            source_key_session(source, session_id),
            "agent/codex/019e-session"
        );
        assert_eq!(
            source_key_turn(source, session_id, 12),
            "agent/codex/019e-session/turn/12"
        );
        assert_eq!(
            source_key_item(source, session_id, "item-7"),
            "agent/codex/019e-session/item/item-7"
        );
        assert_eq!(
            source_key_compaction(source, session_id, 3, "abc123"),
            "agent/codex/019e-session/compaction/3/abc123"
        );
        assert_eq!(
            source_key_prompt_snapshot(source, session_id, 12, "def456"),
            "agent/codex/019e-session/turn/12/prompt/def456"
        );
        assert_ne!(
            source_key_session(AgentSource::Claude, session_id),
            source_key_session(AgentSource::Codex, session_id)
        );
    }

    #[test]
    fn compact_hash_helpers_are_ordered_and_canonical() {
        let first = replacement_history_hash(["item-1", "item-2"]);
        let same = replacement_history_hash(["item-1", "item-2"]);
        let reordered = replacement_history_hash(["item-2", "item-1"]);

        assert_eq!(first.len(), 64);
        assert_eq!(first, same);
        assert_ne!(first, reordered);
        assert_eq!(compact_text_hash("hello"), compact_hash_bytes(b"hello"));
        assert_eq!(
            compact_json_hash(&json!({"b": 2, "a": 1})),
            compact_json_hash(&json!({"a": 1, "b": 2}))
        );
    }

    #[test]
    fn validation_accepts_minimal_well_formed_session() {
        let item = AgentItemIr {
            item_id: "item-1".to_string(),
            source_record_key: source_key_item(AgentSource::Codex, "session-1", "item-1"),
            event_start: "2026-05-25T10:00:00Z".to_string(),
            event_end: "2026-05-25T10:00:01Z".to_string(),
            role: AgentItemRole::Assistant,
            kind: AgentItemKind::Message,
            projections: default_item_projections(
                AgentIngestProfile::HotMemory,
                AgentItemRole::Assistant,
                AgentItemKind::Message,
            ),
            text: Some("Done".to_string()),
            compact_text: None,
            payload: json!({}),
            raw_ref: Some(RawEvidenceRef {
                source_uri: "/tmp/rollout.jsonl".to_string(),
                byte_offset: Some(10),
                byte_len: Some(20),
                line_number: Some(3),
                sha256: Some(compact_text_hash("raw row")),
            }),
            metadata: json!({}),
        };
        let prompt_hash = prompt_input_hash(["item-1"]);
        let session = AgentSessionIr {
            source: AgentSource::Codex,
            source_id: "codex-local".to_string(),
            session_id: "session-1".to_string(),
            session_key: source_key_session(AgentSource::Codex, "session-1"),
            source_uri: "/tmp/rollout.jsonl".to_string(),
            cwd: Some("/repo".to_string()),
            project_key: Some("repo".to_string()),
            model: Some("gpt-5".to_string()),
            started_at: Some("2026-05-25T10:00:00Z".to_string()),
            ended_at: Some("2026-05-25T10:00:02Z".to_string()),
            metadata: json!({}),
            turns: vec![AgentTurnIr {
                turn_id: "turn-0".to_string(),
                turn_index: 0,
                event_start: "2026-05-25T10:00:00Z".to_string(),
                event_end: "2026-05-25T10:00:02Z".to_string(),
                user_goal: Some("test".to_string()),
                status: Some(AgentTurnStatus::Completed),
                item_ids: vec!["item-1".to_string()],
                prompt_snapshot_id: Some("prompt-0".to_string()),
                metadata: json!({}),
            }],
            session_items: vec![item],
            compactions: vec![],
            prompt_snapshots: vec![AgentPromptSnapshotIr {
                prompt_snapshot_id: "prompt-0".to_string(),
                source_record_key: source_key_prompt_snapshot(
                    AgentSource::Codex,
                    "session-1",
                    0,
                    &prompt_hash,
                ),
                turn_id: "turn-0".to_string(),
                event_start: "2026-05-25T10:00:00Z".to_string(),
                event_end: "2026-05-25T10:00:01Z".to_string(),
                compaction_epoch: 0,
                input_item_ids: vec!["item-1".to_string()],
                input_item_count: 1,
                tool_count: 0,
                base_instructions_hash: None,
                dynamic_tools_hash: None,
                prompt_hash: Some(prompt_hash),
                token_estimate: Some(8),
                metadata: json!({}),
            }],
            runtime_events: vec![],
        };

        session.validate().unwrap();
    }

    #[test]
    fn validation_rejects_bad_cross_reference_and_projection_duplicates() {
        let item = AgentItemIr {
            item_id: "item-1".to_string(),
            source_record_key: "agent/codex/session-1/item/item-1".to_string(),
            event_start: "2026-05-25T10:00:00Z".to_string(),
            event_end: "2026-05-25T10:00:01Z".to_string(),
            role: AgentItemRole::Assistant,
            kind: AgentItemKind::Message,
            projections: vec![AgentProjection::Audit, AgentProjection::Audit],
            text: None,
            compact_text: None,
            payload: json!({}),
            raw_ref: None,
            metadata: json!({}),
        };
        let error = item.validate().unwrap_err();
        assert_eq!(error.path, "item.projections");

        let session = AgentSessionIr {
            source: AgentSource::Codex,
            source_id: "codex-local".to_string(),
            session_id: "session-1".to_string(),
            session_key: source_key_session(AgentSource::Codex, "session-1"),
            source_uri: "/tmp/rollout.jsonl".to_string(),
            cwd: None,
            project_key: None,
            model: None,
            started_at: None,
            ended_at: None,
            metadata: json!({}),
            turns: vec![AgentTurnIr {
                turn_id: "turn-0".to_string(),
                turn_index: 0,
                event_start: "2026-05-25T10:00:00Z".to_string(),
                event_end: "2026-05-25T10:00:01Z".to_string(),
                user_goal: None,
                status: None,
                item_ids: vec!["missing".to_string()],
                prompt_snapshot_id: None,
                metadata: json!({}),
            }],
            session_items: vec![AgentItemIr {
                projections: vec![AgentProjection::Audit],
                ..item
            }],
            compactions: vec![],
            prompt_snapshots: vec![],
            runtime_events: vec![],
        };
        let error = session.validate().unwrap_err();
        assert_eq!(error.path, "session.turns[0].item_ids");
    }
}
