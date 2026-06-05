//! Codex rollout parser and reducer for the unified coding-agent session IR.
//!
//! This module deliberately stops at IR production. It does not write
//! `memory.writeObjects`, so the existing COPY/staging fast path remains owned
//! by `write_objects.rs`.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::agent_session_ir::{
    AgentCompactionIr, AgentIngestProfile, AgentItemIr, AgentItemKind, AgentItemRole,
    AgentProjection, AgentPromptSnapshotIr, AgentRuntimeEventIr, AgentRuntimeSeverity,
    AgentSessionIr, AgentSource, AgentTurnIr, AgentTurnStatus, RawEvidenceRef,
};

const DEFAULT_EVENT_TIME: &str = "1970-01-01T00:00:00.000000Z";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexRolloutRecord {
    pub ordinal: usize,
    pub timestamp: Option<String>,
    pub source_record_key: String,
    pub raw_ref: RawEvidenceRef,
    pub raw: CodexRawShape,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CodexRawShape {
    SessionMeta(CodexSessionMetaRaw),
    ResponseItem(CodexResponseItemRaw),
    Compacted(CodexCompactedRaw),
    TurnContext(CodexTurnContextRaw),
    EventMsg(CodexEventMsgRaw),
    Unknown(CodexUnknownRaw),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexSessionMetaRaw {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexResponseItemRaw {
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexCompactedRaw {
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexTurnContextRaw {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexEventMsgRaw {
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexUnknownRaw {
    pub type_name: String,
    pub payload: Value,
}

#[derive(Debug)]
pub enum CodexAgentIngestError {
    Json {
        line_number: u64,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for CodexAgentIngestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json {
                line_number,
                source,
            } => write!(
                formatter,
                "invalid Codex JSONL at line {line_number}: {source}"
            ),
        }
    }
}

impl std::error::Error for CodexAgentIngestError {}

pub fn compile_codex_rollout_jsonl(
    source_uri: &str,
    input: &str,
    profile: AgentIngestProfile,
) -> Result<AgentSessionIr, CodexAgentIngestError> {
    let records = parse_codex_rollout_jsonl(source_uri, input)?;
    Ok(reduce_codex_rollout_records(source_uri, &records, profile))
}

pub fn parse_codex_rollout_jsonl(
    source_uri: &str,
    input: &str,
) -> Result<Vec<CodexRolloutRecord>, CodexAgentIngestError> {
    let mut records = Vec::new();
    let mut byte_offset = 0_u64;

    for (index, raw_line) in input.split_inclusive('\n').enumerate() {
        let line_number = (index + 1) as u64;
        let byte_len = raw_line.len() as u64;
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            byte_offset = byte_offset.saturating_add(byte_len);
            continue;
        }

        let value: Value =
            serde_json::from_str(line).map_err(|source| CodexAgentIngestError::Json {
                line_number,
                source,
            })?;
        let raw_ref = RawEvidenceRef {
            source_uri: source_uri.to_string(),
            byte_offset: Some(byte_offset),
            byte_len: Some(byte_len),
            line_number: Some(line_number),
            sha256: Some(stable_hash(line.as_bytes())),
        };
        let source_record_key = format!(
            "codex/source/{}/raw/{}/{}",
            source_fingerprint(source_uri),
            line_number,
            raw_ref.sha256.as_deref().unwrap_or("missing-hash")
        );
        records.push(CodexRolloutRecord {
            ordinal: records.len(),
            timestamp: value
                .get("timestamp")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            source_record_key,
            raw_ref,
            raw: parse_codex_raw_shape(value),
        });
        byte_offset = byte_offset.saturating_add(byte_len);
    }

    if !input.is_empty() && !input.ends_with('\n') {
        // `split_inclusive` already yielded the final unterminated row above.
    }

    Ok(records)
}

pub fn reduce_codex_rollout_records(
    source_uri: &str,
    records: &[CodexRolloutRecord],
    profile: AgentIngestProfile,
) -> AgentSessionIr {
    let session_meta = first_session_meta(records);
    let session_id = session_meta
        .and_then(|meta| meta.id.clone())
        .unwrap_or_else(|| fallback_session_id(source_uri));
    let session_key = format!("codex:{session_id}");
    let source_id = AgentSource::Codex.source_id().to_string();
    let mut state = ReduceState {
        session_id: session_id.clone(),
        source_fingerprint: source_fingerprint(source_uri),
        profile,
        cwd: session_meta.and_then(|meta| meta.cwd.clone()),
        model: session_meta.and_then(|meta| meta.model.clone()),
        started_at: first_timestamp(records),
        ended_at: last_timestamp(records),
        turns: Vec::new(),
        session_items: Vec::new(),
        compactions: Vec::new(),
        prompt_snapshots: Vec::new(),
        runtime_events: Vec::new(),
        model_visible_item_ids: Vec::new(),
        tool_names_by_call_id: BTreeMap::new(),
        current_turn_index: None,
        compaction_epoch: 0,
    };

    for record in records {
        match &record.raw {
            CodexRawShape::SessionMeta(meta) => {
                if state.cwd.is_none() {
                    state.cwd = meta.cwd.clone();
                }
                if state.model.is_none() {
                    state.model = meta.model.clone();
                }
            }
            CodexRawShape::ResponseItem(item) => state.reduce_response_item(record, item, false),
            CodexRawShape::Compacted(compacted) => state.reduce_compaction(record, compacted),
            CodexRawShape::TurnContext(turn_context) => {
                state.reduce_turn_context(record, turn_context);
            }
            CodexRawShape::EventMsg(event) => state.reduce_event_msg(record, event),
            CodexRawShape::Unknown(unknown) => state.reduce_unknown(record, unknown),
        }
    }

    let metadata = json!({
        "schema": "agent_session_ir.v1",
        "ingest_profile": profile,
        "raw_record_count": records.len(),
        "raw_shape_counts": raw_shape_counts(records),
    });

    AgentSessionIr {
        source: AgentSource::Codex,
        source_id,
        session_id: session_id.clone(),
        session_key,
        source_uri: source_uri.to_string(),
        cwd: state.cwd,
        project_key: None,
        model: state.model,
        started_at: state.started_at,
        ended_at: state.ended_at,
        metadata,
        turns: state.turns,
        session_items: state.session_items,
        compactions: state.compactions,
        prompt_snapshots: state.prompt_snapshots,
        runtime_events: state.runtime_events,
    }
}

struct ReduceState {
    session_id: String,
    source_fingerprint: String,
    profile: AgentIngestProfile,
    cwd: Option<String>,
    model: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    turns: Vec<AgentTurnIr>,
    session_items: Vec<AgentItemIr>,
    compactions: Vec<AgentCompactionIr>,
    prompt_snapshots: Vec<AgentPromptSnapshotIr>,
    runtime_events: Vec<AgentRuntimeEventIr>,
    model_visible_item_ids: Vec<String>,
    tool_names_by_call_id: BTreeMap<String, String>,
    current_turn_index: Option<usize>,
    compaction_epoch: usize,
}

impl ReduceState {
    fn reduce_response_item(
        &mut self,
        record: &CodexRolloutRecord,
        response_item: &CodexResponseItemRaw,
        from_compaction_replacement: bool,
    ) {
        match response_item.item_type.as_str() {
            "message" => {
                let role = response_item
                    .role
                    .as_deref()
                    .and_then(agent_role_from_codex_role)
                    .unwrap_or(AgentItemRole::Assistant);
                let text = codex_message_text(&response_item.payload);
                if text.trim().is_empty() {
                    return;
                }
                let turn_index = self.ensure_turn_for_item(record, &role, Some(&text));
                let item_id = response_item
                    .id
                    .clone()
                    .unwrap_or_else(|| generated_item_id("message", record));
                let mut projections = vec![
                    AgentProjection::ModelVisible,
                    AgentProjection::UiTimeline,
                    AgentProjection::MemoryCandidate,
                ];
                if from_compaction_replacement {
                    push_projection(&mut projections, AgentProjection::PromptSnapshot);
                }
                if self.profile == AgentIngestProfile::Forensic {
                    push_projection(&mut projections, AgentProjection::Forensic);
                }

                let payload = if self.profile == AgentIngestProfile::Forensic {
                    json!({
                        "agent_source": "codex",
                        "session_id": self.session_id,
                        "turn_id": self.turns[turn_index].turn_id,
                        "raw_type": "response_item",
                        "item_type": response_item.item_type,
                        "role": response_item.role,
                        "compaction_epoch": self.compaction_epoch,
                        "raw_payload": response_item.payload,
                    })
                } else {
                    json!({
                        "agent_source": "codex",
                        "session_id": self.session_id,
                        "turn_id": self.turns[turn_index].turn_id,
                        "raw_type": "response_item",
                        "item_type": response_item.item_type,
                        "role": response_item.role,
                        "compaction_epoch": self.compaction_epoch,
                    })
                };
                let item = AgentItemIr {
                    item_id: item_id.clone(),
                    source_record_key: item_source_record_key(
                        &self.session_id,
                        &self.source_fingerprint,
                        &item_id,
                        record,
                    ),
                    event_start: event_start(record),
                    event_end: event_end(record),
                    role,
                    kind: AgentItemKind::Message,
                    projections,
                    text: Some(text.clone()),
                    compact_text: Some(compact_text(&text)),
                    payload,
                    raw_ref: Some(record.raw_ref.clone()),
                    metadata: json!({
                        "source_record_key": record.source_record_key,
                        "from_compaction_replacement": from_compaction_replacement,
                    }),
                };
                self.add_item_to_turn(turn_index, item_id.clone(), &event_start(record));
                self.model_visible_item_ids.push(item_id);
                self.session_items.push(item);
            }
            "function_call" | "custom_tool_call" | "web_search_call" => {
                self.reduce_tool_item(record, response_item, AgentItemKind::ToolCall)
            }
            "function_call_output" | "custom_tool_call_output" => {
                self.reduce_tool_item(record, response_item, AgentItemKind::ToolResult)
            }
            _ => {
                if self.profile == AgentIngestProfile::Forensic {
                    self.runtime_events.push(runtime_event_from_record(
                        &self.session_id,
                        &self.source_fingerprint,
                        record,
                        "response_item.unknown",
                        Some(response_item.item_type.clone()),
                        json!({"raw_payload": response_item.payload}),
                    ));
                }
            }
        }
    }

    fn reduce_tool_item(
        &mut self,
        record: &CodexRolloutRecord,
        response_item: &CodexResponseItemRaw,
        kind: AgentItemKind,
    ) {
        if self.profile == AgentIngestProfile::HotMemory {
            return;
        }
        let role = AgentItemRole::Tool;
        let turn_index = self.ensure_turn_for_item(record, &role, None);
        let item_id = response_item
            .call_id
            .clone()
            .or_else(|| response_item.id.clone())
            .unwrap_or_else(|| generated_item_id("tool", record));
        let mut summary = compact_tool_summary(&response_item.payload, &kind);
        if matches!(kind, AgentItemKind::ToolCall) {
            self.tool_names_by_call_id
                .insert(item_id.clone(), summary.tool_name.clone());
        } else if let Some(tool_name) = self.tool_names_by_call_id.get(&item_id) {
            summary.tool_name = tool_name.clone();
        }
        let source_record_key = match kind {
            AgentItemKind::ToolCall => {
                format!(
                    "agent/codex/{}/source/{}/tool/{item_id}/call/{}",
                    self.session_id,
                    self.source_fingerprint,
                    record_discriminator(record)
                )
            }
            AgentItemKind::ToolResult => {
                format!(
                    "agent/codex/{}/source/{}/tool/{item_id}/result/{}",
                    self.session_id,
                    self.source_fingerprint,
                    record_discriminator(record)
                )
            }
            _ => {
                item_source_record_key(&self.session_id, &self.source_fingerprint, &item_id, record)
            }
        };
        let payload = if self.profile == AgentIngestProfile::Forensic {
            json!({
                "agent_source": "codex",
                "session_id": self.session_id,
                "turn_id": self.turns[turn_index].turn_id,
                "raw_type": "response_item",
                "item_type": response_item.item_type,
                "tool_name": summary.tool_name,
                "action_preview": summary.action_preview,
                "exit_code": summary.exit_code,
                "duration_ms": summary.duration_ms,
                "stdout_bytes": summary.stdout_bytes,
                "stderr_bytes": summary.stderr_bytes,
                "content_sha256": summary.content_sha256,
                "compaction_epoch": self.compaction_epoch,
                "raw_payload": response_item.payload,
            })
        } else {
            json!({
                "agent_source": "codex",
                "session_id": self.session_id,
                "turn_id": self.turns[turn_index].turn_id,
                "raw_type": "response_item",
                "item_type": response_item.item_type,
                "tool_name": summary.tool_name,
                "action_preview": summary.action_preview,
                "exit_code": summary.exit_code,
                "duration_ms": summary.duration_ms,
                "stdout_bytes": summary.stdout_bytes,
                "stderr_bytes": summary.stderr_bytes,
                "content_sha256": summary.content_sha256,
                "compaction_epoch": self.compaction_epoch,
            })
        };
        let item = AgentItemIr {
            item_id: item_id.clone(),
            source_record_key,
            event_start: event_start(record),
            event_end: event_end(record),
            role,
            kind,
            projections: projections_for_audit_item(self.profile),
            text: None,
            compact_text: Some(summary.compact_text),
            payload,
            raw_ref: Some(record.raw_ref.clone()),
            metadata: json!({
                "source_record_key": record.source_record_key,
                "summary_policy": "compact",
            }),
        };
        self.add_item_to_turn(turn_index, item_id, &event_start(record));
        self.session_items.push(item);
    }

    fn reduce_compaction(&mut self, record: &CodexRolloutRecord, compacted: &CodexCompactedRaw) {
        self.compaction_epoch += 1;
        let replaced_item_ids = self.model_visible_item_ids.clone();
        for item in &mut self.session_items {
            if replaced_item_ids.contains(&item.item_id) {
                item.projections
                    .retain(|projection| projection != &AgentProjection::ModelVisible);
            }
        }

        let replacement_history = compaction_replacement_history(&compacted.payload);
        self.model_visible_item_ids.clear();
        let mut replacement_item_ids = Vec::new();
        for (index, replacement) in replacement_history.iter().enumerate() {
            let synthetic_record = self.synthetic_compaction_record(record, replacement, index);
            let response_item = codex_response_item_from_value(replacement);
            if let Some(response_item) = response_item {
                let before = self.session_items.len();
                self.reduce_response_item(&synthetic_record, &response_item, true);
                if self.session_items.len() > before {
                    if let Some(last_item) = self.session_items.last() {
                        replacement_item_ids.push(last_item.item_id.clone());
                    }
                }
            }
        }
        if replacement_item_ids.is_empty() {
            if let Some(summary_text) = compaction_summary_text(&compacted.payload) {
                let synthetic = self.summary_item_from_compaction(record, &summary_text);
                replacement_item_ids.push(synthetic.item_id.clone());
                self.model_visible_item_ids.push(synthetic.item_id.clone());
                self.session_items.push(synthetic);
            }
        }

        let replacement_hash = stable_json_hash(&json!({
            "epoch": self.compaction_epoch,
            "replacement_item_ids": replacement_item_ids,
            "replacement_history": replacement_history,
            "summary": compaction_summary_text(&compacted.payload),
        }));
        let compaction_id = format!("{}:compaction:{}", self.session_id, self.compaction_epoch);
        self.compactions.push(AgentCompactionIr {
            compaction_id,
            source_record_key: format!(
                "agent/codex/{}/source/{}/compaction/{}/{}",
                self.session_id, self.source_fingerprint, self.compaction_epoch, replacement_hash
            ),
            event_start: event_start(record),
            event_end: event_end(record),
            compaction_epoch: self.compaction_epoch as u64,
            replacement_item_ids,
            replaced_item_ids: Some(replaced_item_ids),
            summary_text: compaction_summary_text(&compacted.payload),
            replacement_history_hash: replacement_hash,
            raw_ref: Some(record.raw_ref.clone()),
            metadata: json!({
                "source_record_key": record.source_record_key,
                "profile": self.profile,
            }),
        });
    }

    fn reduce_turn_context(
        &mut self,
        record: &CodexRolloutRecord,
        turn_context: &CodexTurnContextRaw,
    ) {
        if self.cwd.is_none() {
            self.cwd = turn_context.cwd.clone();
        }
        if self.model.is_none() {
            self.model = turn_context.model.clone();
        }
        let turn_index = if let Some(index) = self.current_turn_index {
            if self.turns[index].prompt_snapshot_id.is_none() {
                index
            } else {
                self.create_turn(record, turn_context.turn_id.clone())
            }
        } else {
            self.create_turn(record, turn_context.turn_id.clone())
        };
        if let Some(turn_id) = &turn_context.turn_id {
            self.turns[turn_index].turn_id = turn_id.clone();
        }
        self.turns[turn_index].event_end = event_end(record);
        let prompt_snapshot =
            self.prompt_snapshot_from_turn_context(record, turn_context, turn_index);
        self.turns[turn_index].prompt_snapshot_id =
            Some(prompt_snapshot.prompt_snapshot_id.clone());
        self.prompt_snapshots.push(prompt_snapshot);
        self.current_turn_index = Some(turn_index);
    }

    fn reduce_event_msg(&mut self, record: &CodexRolloutRecord, event: &CodexEventMsgRaw) {
        if event.event_type == "user_message" {
            let text = event
                .payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if text.is_empty() {
                return;
            }
            let response = CodexResponseItemRaw {
                item_type: "message".to_string(),
                id: None,
                call_id: None,
                role: Some("user".to_string()),
                payload: json!({"type": "message", "role": "user", "content": text}),
            };
            self.reduce_response_item(record, &response, false);
            return;
        }

        if let Some(status) = turn_status_from_event(&event.event_type) {
            if let Some(index) = self.current_turn_index {
                self.turns[index].status = Some(status);
                self.turns[index].event_end = event_end(record);
            }
        }

        if self.profile != AgentIngestProfile::HotMemory
            || is_salient_runtime_event(&event.event_type)
        {
            let compact = compact_runtime_event_text(event);
            self.runtime_events.push(runtime_event_from_record(
                &self.session_id,
                &self.source_fingerprint,
                record,
                &event.event_type,
                compact,
                if self.profile == AgentIngestProfile::Forensic {
                    json!({"raw_payload": event.payload})
                } else {
                    compact_runtime_payload(event)
                },
            ));
        }
    }

    fn reduce_unknown(&mut self, record: &CodexRolloutRecord, unknown: &CodexUnknownRaw) {
        if self.profile == AgentIngestProfile::Forensic {
            self.runtime_events.push(runtime_event_from_record(
                &self.session_id,
                &self.source_fingerprint,
                record,
                &format!("unknown.{}", unknown.type_name),
                None,
                json!({"raw_payload": unknown.payload}),
            ));
        }
    }

    fn prompt_snapshot_from_turn_context(
        &self,
        record: &CodexRolloutRecord,
        turn_context: &CodexTurnContextRaw,
        turn_index: usize,
    ) -> AgentPromptSnapshotIr {
        let tool_count = tool_count_from_turn_context(&turn_context.payload);
        let base_instructions_hash = turn_context
            .payload
            .get("user_instructions")
            .or_else(|| turn_context.payload.get("base_instructions"))
            .map(stable_json_hash);
        let dynamic_tools_hash = turn_context
            .payload
            .get("tools")
            .or_else(|| turn_context.payload.get("available_tools"))
            .map(stable_json_hash);
        let prompt_hash = Some(stable_json_hash(&json!({
            "turn_id": self.turns[turn_index].turn_id,
            "compaction_epoch": self.compaction_epoch,
            "input_item_ids": self.model_visible_item_ids,
            "tool_count": tool_count,
            "base_instructions_hash": base_instructions_hash,
            "dynamic_tools_hash": dynamic_tools_hash,
        })));
        let prompt_hash_tail = prompt_hash.as_deref().unwrap_or("missing-prompt-hash");
        AgentPromptSnapshotIr {
            prompt_snapshot_id: format!(
                "{}:turn:{}:prompt:{}",
                self.session_id, turn_index, prompt_hash_tail
            ),
            source_record_key: format!(
                "agent/codex/{}/source/{}/turn/{}/prompt/{}",
                self.session_id, self.source_fingerprint, turn_index, prompt_hash_tail
            ),
            turn_id: self.turns[turn_index].turn_id.clone(),
            event_start: event_start(record),
            event_end: event_end(record),
            compaction_epoch: self.compaction_epoch as u64,
            input_item_ids: self.model_visible_item_ids.clone(),
            input_item_count: self.model_visible_item_ids.len(),
            tool_count,
            base_instructions_hash,
            dynamic_tools_hash,
            prompt_hash,
            token_estimate: turn_context
                .payload
                .get("token_estimate")
                .or_else(|| turn_context.payload.get("estimated_tokens"))
                .and_then(Value::as_u64),
            metadata: json!({
                "source_record_key": record.source_record_key,
                "model": turn_context.model,
                "projection": "prompt_snapshot",
            }),
        }
    }

    fn ensure_turn_for_item(
        &mut self,
        record: &CodexRolloutRecord,
        role: &AgentItemRole,
        user_goal: Option<&str>,
    ) -> usize {
        let should_start = match (self.current_turn_index, role) {
            (None, _) => true,
            (Some(index), AgentItemRole::User) => {
                self.turns[index].item_ids.iter().any(|item_id| {
                    self.session_items
                        .iter()
                        .find(|item| &item.item_id == item_id)
                        .is_some_and(|item| item.role == AgentItemRole::Assistant)
                })
            }
            _ => false,
        };
        let index = if should_start {
            self.create_turn(record, None)
        } else {
            self.current_turn_index.unwrap()
        };
        if self.turns[index].user_goal.is_none() {
            if let Some(goal) = user_goal {
                if matches!(role, AgentItemRole::User) {
                    self.turns[index].user_goal = Some(compact_text(goal));
                }
            }
        }
        index
    }

    fn create_turn(&mut self, record: &CodexRolloutRecord, turn_id: Option<String>) -> usize {
        let turn_index = self.turns.len();
        let turn_id = turn_id.unwrap_or_else(|| format!("{}:turn:{turn_index}", self.session_id));
        self.turns.push(AgentTurnIr {
            turn_id,
            turn_index: turn_index as u64,
            event_start: event_start(record),
            event_end: event_end(record),
            user_goal: None,
            status: Some(AgentTurnStatus::Unknown),
            item_ids: Vec::new(),
            prompt_snapshot_id: None,
            metadata: json!({
                "source_record_key": record.source_record_key,
            }),
        });
        self.current_turn_index = Some(turn_index);
        turn_index
    }

    fn add_item_to_turn(&mut self, turn_index: usize, item_id: String, event_start: &str) {
        self.turns[turn_index].item_ids.push(item_id);
        self.turns[turn_index].event_end = event_start.to_string();
    }

    fn synthetic_compaction_record(
        &self,
        record: &CodexRolloutRecord,
        replacement: &Value,
        index: usize,
    ) -> CodexRolloutRecord {
        let hash = stable_json_hash(replacement);
        let mut raw_ref = record.raw_ref.clone();
        raw_ref.sha256 = Some(hash.clone());
        CodexRolloutRecord {
            ordinal: record.ordinal,
            timestamp: record.timestamp.clone(),
            source_record_key: format!(
                "codex/source/{}/compaction/{}/replacement/{index}/{hash}",
                self.source_fingerprint, self.compaction_epoch
            ),
            raw_ref,
            raw: CodexRawShape::ResponseItem(
                codex_response_item_from_value(replacement).unwrap_or(CodexResponseItemRaw {
                    item_type: "message".to_string(),
                    id: None,
                    call_id: None,
                    role: Some("assistant".to_string()),
                    payload: json!({
                        "type": "message",
                        "role": "assistant",
                        "content": replacement.to_string(),
                    }),
                }),
            ),
        }
    }

    fn summary_item_from_compaction(
        &self,
        record: &CodexRolloutRecord,
        summary_text: &str,
    ) -> AgentItemIr {
        let hash = stable_hash(summary_text.as_bytes());
        let item_id = format!(
            "{}:compaction:{}:summary:{hash}",
            self.session_id, self.compaction_epoch
        );
        AgentItemIr {
            item_id: item_id.clone(),
            source_record_key: item_source_record_key(
                &self.session_id,
                &self.source_fingerprint,
                &item_id,
                record,
            ),
            event_start: event_start(record),
            event_end: event_end(record),
            role: AgentItemRole::Assistant,
            kind: AgentItemKind::Message,
            projections: vec![
                AgentProjection::ModelVisible,
                AgentProjection::UiTimeline,
                AgentProjection::PromptSnapshot,
                AgentProjection::MemoryCandidate,
            ],
            text: Some(summary_text.to_string()),
            compact_text: Some(compact_text(summary_text)),
            payload: json!({
                "agent_source": "codex",
                "session_id": self.session_id,
                "raw_type": "compacted",
                "item_type": "compaction_summary",
                "compaction_epoch": self.compaction_epoch,
            }),
            raw_ref: Some(record.raw_ref.clone()),
            metadata: json!({
                "source_record_key": record.source_record_key,
                "from_compaction_replacement": true,
            }),
        }
    }
}

#[derive(Debug, Clone)]
struct CompactToolSummary {
    tool_name: String,
    action_preview: Option<String>,
    exit_code: Option<i64>,
    duration_ms: Option<u64>,
    stdout_bytes: Option<u64>,
    stderr_bytes: Option<u64>,
    content_sha256: String,
    compact_text: String,
}

fn parse_codex_raw_shape(value: Value) -> CodexRawShape {
    let type_name = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let payload = value.get("payload").cloned().unwrap_or(Value::Null);
    match type_name.as_str() {
        "session_meta" | "SessionMeta" => CodexRawShape::SessionMeta(CodexSessionMetaRaw {
            id: payload
                .get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            cwd: payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            model: payload
                .get("model")
                .or_else(|| payload.get("model_slug"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            payload,
        }),
        "response_item" | "ResponseItem" => CodexRawShape::ResponseItem(
            codex_response_item_from_value(&payload).unwrap_or(CodexResponseItemRaw {
                item_type: "unknown".to_string(),
                id: None,
                call_id: None,
                role: None,
                payload,
            }),
        ),
        "compacted" | "Compacted" => CodexRawShape::Compacted(CodexCompactedRaw { payload }),
        "turn_context" | "TurnContext" => CodexRawShape::TurnContext(CodexTurnContextRaw {
            turn_id: payload
                .get("turn_id")
                .or_else(|| payload.get("turnId"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            cwd: payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            model: payload
                .get("model")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            payload,
        }),
        "event_msg" | "EventMsg" => CodexRawShape::EventMsg(CodexEventMsgRaw {
            event_type: payload
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            turn_id: payload
                .get("turn_id")
                .or_else(|| payload.get("turnId"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            payload,
        }),
        _ => CodexRawShape::Unknown(CodexUnknownRaw { type_name, payload }),
    }
}

fn codex_response_item_from_value(value: &Value) -> Option<CodexResponseItemRaw> {
    let payload = value.get("payload").unwrap_or(value);
    let item_type = payload.get("type")?.as_str()?.to_string();
    Some(CodexResponseItemRaw {
        item_type,
        id: payload
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        call_id: payload
            .get("call_id")
            .or_else(|| payload.get("callId"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        role: payload
            .get("role")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        payload: payload.clone(),
    })
}

fn first_session_meta(records: &[CodexRolloutRecord]) -> Option<&CodexSessionMetaRaw> {
    records.iter().find_map(|record| match &record.raw {
        CodexRawShape::SessionMeta(meta) => Some(meta),
        _ => None,
    })
}

fn first_timestamp(records: &[CodexRolloutRecord]) -> Option<String> {
    records.iter().find_map(|record| record.timestamp.clone())
}

fn last_timestamp(records: &[CodexRolloutRecord]) -> Option<String> {
    records
        .iter()
        .rev()
        .find_map(|record| record.timestamp.clone())
}

fn event_start(record: &CodexRolloutRecord) -> String {
    record
        .timestamp
        .clone()
        .unwrap_or_else(|| DEFAULT_EVENT_TIME.to_string())
}

fn event_end(record: &CodexRolloutRecord) -> String {
    plus_one_microsecond(&event_start(record)).unwrap_or_else(|| event_start(record))
}

fn plus_one_microsecond(timestamp: &str) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(timestamp)
        .ok()?
        .with_timezone(&Utc);
    Some((parsed + Duration::microseconds(1)).to_rfc3339_opts(SecondsFormat::Micros, true))
}

fn fallback_session_id(source_uri: &str) -> String {
    let path = Path::new(source_uri);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(source_uri);
    if stem.len() >= 36 {
        let tail = &stem[stem.len() - 36..];
        if is_uuid_like(tail) {
            return tail.to_string();
        }
    }
    format!("path-{}", &stable_hash(source_uri.as_bytes())[..16])
}

fn is_uuid_like(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(index, character)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
}

fn raw_shape_counts(records: &[CodexRolloutRecord]) -> Value {
    let mut counts = BTreeMap::<&str, usize>::new();
    for record in records {
        let label = match record.raw {
            CodexRawShape::SessionMeta(_) => "SessionMeta",
            CodexRawShape::ResponseItem(_) => "ResponseItem",
            CodexRawShape::Compacted(_) => "Compacted",
            CodexRawShape::TurnContext(_) => "TurnContext",
            CodexRawShape::EventMsg(_) => "EventMsg",
            CodexRawShape::Unknown(_) => "Unknown",
        };
        *counts.entry(label).or_default() += 1;
    }
    json!(counts)
}

fn item_source_record_key(
    session_id: &str,
    source_fingerprint: &str,
    item_id: &str,
    record: &CodexRolloutRecord,
) -> String {
    format!(
        "agent/codex/{session_id}/source/{source_fingerprint}/item/{item_id}/{}",
        record_discriminator(record)
    )
}

fn record_discriminator(record: &CodexRolloutRecord) -> String {
    let line_number = record.raw_ref.line_number.unwrap_or(record.ordinal as u64);
    let raw_hash = record.raw_ref.sha256.as_deref().unwrap_or("missing-hash");
    let record_key_hash = stable_hash(record.source_record_key.as_bytes());
    format!(
        "line-{line_number}-{}-record-{}",
        hash_prefix(raw_hash, 16),
        hash_prefix(&record_key_hash, 16)
    )
}

fn source_fingerprint(source_uri: &str) -> String {
    hash_prefix(&stable_hash(source_uri.as_bytes()), 16).to_string()
}

fn hash_prefix(hash: &str, len: usize) -> &str {
    hash.get(..len).unwrap_or(hash)
}

fn generated_item_id(prefix: &str, record: &CodexRolloutRecord) -> String {
    format!(
        "{}:{}:{}",
        prefix,
        record.raw_ref.line_number.unwrap_or(record.ordinal as u64),
        record.raw_ref.sha256.as_deref().unwrap_or("missing-hash")
    )
}

fn agent_role_from_codex_role(role: &str) -> Option<AgentItemRole> {
    match role {
        "user" => Some(AgentItemRole::User),
        "assistant" => Some(AgentItemRole::Assistant),
        "system" => Some(AgentItemRole::System),
        "tool" => Some(AgentItemRole::Tool),
        _ => None,
    }
}

fn push_projection(projections: &mut Vec<AgentProjection>, projection: AgentProjection) {
    if !projections.contains(&projection) {
        projections.push(projection);
    }
}

fn projections_for_audit_item(profile: AgentIngestProfile) -> Vec<AgentProjection> {
    let mut projections = vec![AgentProjection::Audit];
    if profile == AgentIngestProfile::Forensic {
        projections.push(AgentProjection::Forensic);
    }
    projections
}

fn codex_message_text(payload: &Value) -> String {
    match payload.get("content") {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .or_else(|| item.get("input_text"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn compact_text(text: &str) -> String {
    const MAX_CHARS: usize = 1_200;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    format!(
        "{}... [truncated; {} chars total]",
        trimmed.chars().take(MAX_CHARS).collect::<String>(),
        trimmed.chars().count()
    )
}

fn compact_tool_summary(payload: &Value, kind: &AgentItemKind) -> CompactToolSummary {
    let item_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let tool_name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(item_type)
        .to_string();
    let mut exit_code = None;
    let mut duration_ms = None;
    let mut stdout_bytes = None;
    let mut stderr_bytes = None;
    let mut action_preview = None;

    if matches!(kind, AgentItemKind::ToolCall) {
        action_preview = payload
            .get("arguments")
            .or_else(|| payload.get("input"))
            .map(|value| compact_text(&json_value_string(value)));
    } else {
        let output = payload.get("output").unwrap_or(&Value::Null);
        let output_text = json_value_string(output);
        stdout_bytes = Some(output_text.len() as u64);
        if let Ok(parsed) = serde_json::from_str::<Value>(&output_text) {
            exit_code = parsed
                .get("metadata")
                .and_then(|metadata| metadata.get("exit_code"))
                .and_then(Value::as_i64);
            duration_ms = parsed
                .get("metadata")
                .and_then(|metadata| metadata.get("duration_ms"))
                .and_then(Value::as_u64);
            let inner_output = parsed
                .get("output")
                .map(json_value_string)
                .unwrap_or(output_text);
            stdout_bytes = Some(inner_output.len() as u64);
            stderr_bytes = parsed
                .get("metadata")
                .and_then(|metadata| metadata.get("stderr"))
                .map(json_value_string)
                .map(|value| value.len() as u64);
        }
    }

    let content_sha256 = stable_json_hash(payload);
    let compact_text = match kind {
        AgentItemKind::ToolCall => format!(
            "[tool-call:{}] {}",
            tool_name,
            action_preview.clone().unwrap_or_default()
        ),
        AgentItemKind::ToolResult => format!(
            "[tool-result:{}] stdout_bytes={} exit_code={}",
            tool_name,
            stdout_bytes.unwrap_or(0),
            exit_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        _ => format!("[tool:{tool_name}]"),
    };

    CompactToolSummary {
        tool_name,
        action_preview,
        exit_code,
        duration_ms,
        stdout_bytes,
        stderr_bytes,
        content_sha256,
        compact_text,
    }
}

fn compaction_replacement_history(payload: &Value) -> Vec<Value> {
    [
        "replacement_history",
        "replacementHistory",
        "replacement_items",
        "replacementItems",
        "new_history",
        "newHistory",
        "history",
        "messages",
    ]
    .iter()
    .find_map(|key| payload.get(*key).and_then(Value::as_array))
    .cloned()
    .unwrap_or_default()
}

fn compaction_summary_text(payload: &Value) -> Option<String> {
    payload
        .get("summary_text")
        .or_else(|| payload.get("summaryText"))
        .or_else(|| payload.get("summary"))
        .or_else(|| payload.get("text"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn tool_count_from_turn_context(payload: &Value) -> usize {
    payload
        .get("tools")
        .or_else(|| payload.get("available_tools"))
        .or_else(|| payload.get("dynamic_tools"))
        .map(|value| match value {
            Value::Array(values) => values.len(),
            Value::Object(values) => values.len(),
            _ => 0,
        })
        .unwrap_or(0)
}

fn turn_status_from_event(event_type: &str) -> Option<AgentTurnStatus> {
    match event_type {
        "task_complete" | "task_completed" | "completed" => Some(AgentTurnStatus::Completed),
        "task_failed" | "error" => Some(AgentTurnStatus::Failed),
        "task_interrupted" | "interrupted" => Some(AgentTurnStatus::Interrupted),
        _ => None,
    }
}

fn is_salient_runtime_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "task_started" | "task_complete" | "task_completed" | "task_failed" | "task_interrupted"
    )
}

fn compact_runtime_event_text(event: &CodexEventMsgRaw) -> Option<String> {
    Some(match event.event_type.as_str() {
        "task_started" => format!(
            "task_started turn_id={}",
            event.turn_id.as_deref().unwrap_or("unknown")
        ),
        "task_complete" | "task_completed" => "task_completed".to_string(),
        "task_failed" => "task_failed".to_string(),
        other => other.to_string(),
    })
}

fn compact_runtime_payload(event: &CodexEventMsgRaw) -> Value {
    json!({
        "event_type": event.event_type,
        "turn_id": event.turn_id,
    })
}

fn runtime_event_from_record(
    session_id: &str,
    source_fingerprint: &str,
    record: &CodexRolloutRecord,
    event_kind: &str,
    compact_text: Option<String>,
    payload: Value,
) -> AgentRuntimeEventIr {
    let hash = record.raw_ref.sha256.as_deref().unwrap_or("missing-hash");
    AgentRuntimeEventIr {
        event_id: format!("{session_id}:runtime:{}:{hash}", record.ordinal),
        source_record_key: format!(
            "agent/codex/{session_id}/source/{source_fingerprint}/event/{}:{hash}",
            record.ordinal
        ),
        event_start: event_start(record),
        event_end: event_end(record),
        event_kind: event_kind.to_string(),
        severity: runtime_severity(event_kind),
        compact_text,
        payload,
        raw_ref: Some(record.raw_ref.clone()),
    }
}

fn runtime_severity(event_kind: &str) -> Option<AgentRuntimeSeverity> {
    match event_kind {
        "task_failed" | "error" => Some(AgentRuntimeSeverity::Error),
        "task_interrupted" => Some(AgentRuntimeSeverity::Warning),
        "task_started" | "task_complete" | "task_completed" => Some(AgentRuntimeSeverity::Info),
        _ => None,
    }
}

fn json_value_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}

fn stable_json_hash(value: &Value) -> String {
    stable_hash(&canonical_json_bytes(value))
}

fn stable_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(&canonicalize_json(value)).expect("canonical JSON serializes")
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json).collect()),
        Value::Object(map) => {
            let sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_json(value)))
                .collect::<BTreeMap<_, _>>();
            let mut ordered = Map::new();
            for (key, value) in sorted {
                ordered.insert(key, value);
            }
            Value::Object(ordered)
        }
        value => value.clone(),
    }
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

    const SOURCE_URI: &str =
        "/Users/test/.codex/sessions/2026/05/25/rollout-2026-05-25T00-00-00-019e5df1-1111-7222-8333-444455556666.jsonl";

    #[test]
    fn codex_compaction_replaces_model_visible_projection_without_erasing_raw_items() {
        let fixture = concat!(
            r#"{"timestamp":"2026-05-25T07:00:00.000Z","type":"session_meta","payload":{"id":"019e5df1-1111-7222-8333-444455556666","cwd":"/repo","model":"gpt-5"}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:01.000Z","type":"response_item","payload":{"type":"message","id":"u1","role":"user","content":[{"type":"input_text","text":"first goal"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:02.000Z","type":"turn_context","payload":{"turn_id":"turn-1","cwd":"/repo","model":"gpt-5","tools":[{"name":"shell"}],"user_instructions":"base"}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:03.000Z","type":"response_item","payload":{"type":"message","id":"a1","role":"assistant","content":[{"type":"output_text","text":"first answer"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:04.000Z","type":"response_item","payload":{"type":"message","id":"u2","role":"user","content":[{"type":"input_text","text":"second goal"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:05.000Z","type":"response_item","payload":{"type":"message","id":"a2","role":"assistant","content":[{"type":"output_text","text":"second answer"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:06.000Z","type":"compacted","payload":{"summary":"compressed first two turns","replacement_history":[{"type":"message","id":"summary-1","role":"assistant","content":[{"type":"output_text","text":"Summary of first two turns"}]},{"type":"message","id":"u2-replay","role":"user","content":[{"type":"input_text","text":"second goal"}]}]}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:07.000Z","type":"response_item","payload":{"type":"message","id":"a3","role":"assistant","content":[{"type":"output_text","text":"after compaction"}]}}"#,
            "\n",
        );

        let ir = compile_codex_rollout_jsonl(SOURCE_URI, fixture, AgentIngestProfile::HotMemory)
            .unwrap();
        ir.validate().unwrap();

        assert_eq!(ir.session_id, "019e5df1-1111-7222-8333-444455556666");
        assert_eq!(ir.compactions.len(), 1);
        assert_eq!(ir.compactions[0].compaction_epoch, 1);
        assert_eq!(
            ir.compactions[0].replacement_item_ids,
            vec!["summary-1", "u2-replay"]
        );
        assert_eq!(
            ir.compactions[0].replaced_item_ids.as_ref().unwrap(),
            &vec![
                "u1".to_string(),
                "a1".to_string(),
                "u2".to_string(),
                "a2".to_string()
            ]
        );
        assert!(ir.session_items.iter().any(|item| item.item_id == "u1"));
        assert!(!ir
            .session_items
            .iter()
            .find(|item| item.item_id == "u1")
            .unwrap()
            .projections
            .contains(&AgentProjection::ModelVisible));
        assert!(ir
            .session_items
            .iter()
            .find(|item| item.item_id == "summary-1")
            .unwrap()
            .projections
            .contains(&AgentProjection::ModelVisible));
        assert_eq!(ir.prompt_snapshots.len(), 1);
        assert_eq!(ir.prompt_snapshots[0].input_item_ids, vec!["u1"]);
        assert!(ir.session_items.iter().all(|item| item.raw_ref.is_some()));
    }

    #[test]
    fn hot_memory_skips_tool_payloads_but_parser_keeps_offsets() {
        let fixture = tool_fixture();

        let records = parse_codex_rollout_jsonl(SOURCE_URI, &fixture).unwrap();
        assert_eq!(records.len(), 6);
        assert_eq!(records[3].raw_ref.line_number, Some(4));
        assert!(records[3].raw_ref.byte_offset.unwrap() > 0);

        let ir = reduce_codex_rollout_records(SOURCE_URI, &records, AgentIngestProfile::HotMemory);
        ir.validate().unwrap();

        assert_eq!(ir.session_items.len(), 2);
        assert!(ir
            .session_items
            .iter()
            .all(|item| item.kind == AgentItemKind::Message));
        assert!(!ir
            .session_items
            .iter()
            .any(|item| item.payload.to_string().contains("very large stdout")));
        assert_eq!(ir.runtime_events.len(), 1);
        assert_eq!(ir.runtime_events[0].event_kind, "task_started");
    }

    #[test]
    fn compact_audit_emits_tool_summaries_without_full_stdout_display_text() {
        let fixture = tool_fixture();

        let ir =
            compile_codex_rollout_jsonl(SOURCE_URI, &fixture, AgentIngestProfile::CompactAudit)
                .unwrap();
        ir.validate().unwrap();

        let tool_result = ir
            .session_items
            .iter()
            .find(|item| item.kind == AgentItemKind::ToolResult)
            .unwrap();
        assert_eq!(tool_result.item_id, "call-1");
        assert_eq!(tool_result.text, None);
        assert!(tool_result
            .compact_text
            .as_deref()
            .unwrap()
            .contains("stdout_bytes="));
        assert!(!tool_result
            .compact_text
            .as_deref()
            .unwrap()
            .contains("very large stdout"));
        assert_eq!(
            tool_result.payload.get("tool_name").and_then(Value::as_str),
            Some("exec_command")
        );
        assert!(tool_result.payload.get("content_sha256").is_some());
        assert!(tool_result.projections.contains(&AgentProjection::Audit));
    }

    #[test]
    fn missing_session_meta_uses_uuid_tail_then_path_fingerprint() {
        let fixture = concat!(
            r#"{"timestamp":"2026-05-25T07:00:01.000Z","type":"response_item","payload":{"type":"message","id":"u1","role":"user","content":"hello"}}"#,
            "\n",
        );
        let ir = compile_codex_rollout_jsonl(SOURCE_URI, fixture, AgentIngestProfile::HotMemory)
            .unwrap();
        ir.validate().unwrap();
        assert_eq!(ir.session_id, "019e5df1-1111-7222-8333-444455556666");

        let fingerprinted = compile_codex_rollout_jsonl(
            "/tmp/no-uuid-rollout.jsonl",
            fixture,
            AgentIngestProfile::HotMemory,
        )
        .unwrap();
        fingerprinted.validate().unwrap();
        assert!(fingerprinted.session_id.starts_with("path-"));
    }

    fn tool_fixture() -> String {
        concat!(
            r#"{"timestamp":"2026-05-25T07:00:00.000Z","type":"session_meta","payload":{"id":"019e5df1-1111-7222-8333-444455556666","cwd":"/repo","model":"gpt-5"}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:01.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:02.000Z","type":"response_item","payload":{"type":"message","id":"u1","role":"user","content":"run tests"}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:03.000Z","type":"response_item","payload":{"type":"function_call","call_id":"call-1","name":"exec_command","arguments":"{\"cmd\":\"cargo test\"}"}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:04.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"{\"output\":\"very large stdout very large stdout very large stdout\",\"metadata\":{\"exit_code\":0,\"duration_ms\":42}}"}}"#,
            "\n",
            r#"{"timestamp":"2026-05-25T07:00:05.000Z","type":"response_item","payload":{"type":"message","id":"a1","role":"assistant","content":"tests passed"}}"#,
            "\n",
        )
        .to_string()
    }
}
