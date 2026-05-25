//! Claude Code JSONL reduction into the shared coding-agent session IR.
//!
//! This module is intentionally independent from the legacy local adapter path.
//! It parses Claude rows into typed raw records, then reduces those records into
//! the same session/turn/message/tool/runtime shape expected by the agent ingest
//! contract.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::agent_session_ir::{
    default_item_projections, AgentIngestProfile, AgentItemIr, AgentItemKind, AgentItemRole,
    AgentProjection, AgentPromptSnapshotIr, AgentRuntimeEventIr, AgentRuntimeSeverity,
    AgentSessionIr, AgentSource, AgentTurnIr, AgentTurnStatus, RawEvidenceRef,
};
use crate::source_identity::canonical_source_hash;

#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeRawRecord {
    pub source_uri: String,
    pub source_record_key: String,
    pub line_number: usize,
    pub byte_offset: usize,
    pub byte_len: usize,
    pub raw_ref: RawEvidenceRef,
    pub timestamp: String,
    pub raw_kind: ClaudeRawKind,
    pub role: Option<AgentItemRole>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub project_key: Option<String>,
    pub model: Option<String>,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeRawKind {
    Message,
    ToolUse,
    ToolResult,
    RuntimeEvent,
}

#[derive(Debug)]
pub enum ClaudeAgentIngestError {
    Json {
        line_number: usize,
        source: serde_json::Error,
    },
    EmptySession {
        source_uri: String,
    },
}

impl std::fmt::Display for ClaudeAgentIngestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json {
                line_number,
                source,
            } => write!(
                formatter,
                "invalid Claude JSONL at line {line_number}: {source}"
            ),
            Self::EmptySession { source_uri } => {
                write!(
                    formatter,
                    "Claude JSONL source {source_uri:?} did not contain records"
                )
            }
        }
    }
}

impl std::error::Error for ClaudeAgentIngestError {}

pub fn parse_claude_jsonl_records(
    source_uri: impl Into<String>,
    jsonl: &str,
) -> Result<Vec<ClaudeRawRecord>, ClaudeAgentIngestError> {
    let source_uri = source_uri.into();
    let mut records = Vec::new();
    let mut byte_offset = 0;

    for (line_index, line) in jsonl.lines().enumerate() {
        let line_number = line_index + 1;
        let byte_len = line.len();
        if !line.trim().is_empty() {
            let value = serde_json::from_str::<Value>(line).map_err(|source| {
                ClaudeAgentIngestError::Json {
                    line_number,
                    source,
                }
            })?;
            parse_claude_value(
                &source_uri,
                line_number,
                byte_offset,
                byte_len,
                value,
                &mut records,
            );
        }
        byte_offset += byte_len + 1;
    }

    Ok(records)
}

pub fn reduce_claude_session(
    source_uri: impl Into<String>,
    records: Vec<ClaudeRawRecord>,
    profile: AgentIngestProfile,
) -> Result<AgentSessionIr, ClaudeAgentIngestError> {
    let source_uri = source_uri.into();
    if records.is_empty() {
        return Err(ClaudeAgentIngestError::EmptySession { source_uri });
    }

    let session_id = records
        .iter()
        .find_map(|record| record.session_id.clone())
        .unwrap_or_else(|| fallback_session_id(&source_uri));
    let source_id = AgentSource::Claude.source_id().to_string();
    let session_key = format!("claude:{session_id}");
    let cwd = records.iter().find_map(|record| record.cwd.clone());
    let project_key = cwd
        .as_deref()
        .and_then(project_key_from_cwd)
        .or_else(|| records.iter().find_map(|record| record.project_key.clone()))
        .or_else(|| project_key_from_source_uri(&source_uri));
    let model = records.iter().find_map(|record| record.model.clone());
    let started_at = records.first().map(|record| record.timestamp.clone());
    let ended_at = records.last().map(|record| record.timestamp.clone());

    let mut session_items = Vec::new();
    let mut runtime_events = Vec::new();
    let mut turns = Vec::<AgentTurnIr>::new();
    let mut prompt_snapshots = Vec::<AgentPromptSnapshotIr>::new();
    let mut current_turn: Option<OpenTurn> = None;
    let mut model_visible_history = Vec::<String>::new();
    let mut compact_tool_count = 0;

    for record in &records {
        match record.raw_kind {
            ClaudeRawKind::Message => {
                for block in message_blocks(record) {
                    match block {
                        ClaudeBlock::Text { text, block_index } => {
                            if text.trim().is_empty() {
                                continue;
                            }
                            let role = record.role.clone().unwrap_or(AgentItemRole::Assistant);
                            let item_source_record_key = derived_source_record_key(
                                &record.source_record_key,
                                "message",
                                block_index,
                            );
                            let item_id = item_id(&session_key, &item_source_record_key, 0);
                            if role == AgentItemRole::User {
                                finish_open_turn(&mut current_turn, &mut turns);
                                let turn_index = turns.len() as u64;
                                let turn_id = format!("{session_key}:turn:{turn_index}");
                                let prompt_snapshot_id =
                                    format!("{session_key}:prompt:{turn_index}");
                                current_turn = Some(OpenTurn {
                                    turn_id: turn_id.clone(),
                                    turn_index,
                                    event_start: record.timestamp.clone(),
                                    event_end: record.timestamp.clone(),
                                    user_goal: Some(preview_text(&text, 240)),
                                    item_ids: Vec::new(),
                                    prompt_snapshot_id: Some(prompt_snapshot_id.clone()),
                                });
                                let mut prompt_input = model_visible_history.clone();
                                prompt_input.push(item_id.clone());
                                prompt_snapshots.push(AgentPromptSnapshotIr {
                                    prompt_snapshot_id,
                                    source_record_key: prompt_source_record_key(
                                        &record.source_record_key,
                                        turn_index,
                                        block_index,
                                    ),
                                    turn_id,
                                    event_start: record.timestamp.clone(),
                                    event_end: record.timestamp.clone(),
                                    compaction_epoch: 0,
                                    input_item_count: prompt_input.len(),
                                    input_item_ids: prompt_input.clone(),
                                    tool_count: compact_tool_count,
                                    base_instructions_hash: None,
                                    dynamic_tools_hash: None,
                                    prompt_hash: Some(stable_json_hash(&json!({
                                        "source": "claude",
                                        "session_id": session_id,
                                        "turn_index": turn_index,
                                        "input_item_ids": prompt_input,
                                        "user_text": text,
                                    }))),
                                    token_estimate: Some(estimate_tokens(&text)),
                                    metadata: json!({
                                        "schema": "agent_prompt_snapshot_ir.v1",
                                        "snapshot_basis": "inferred_claude_user_turn",
                                    }),
                                });
                            }
                            let item = AgentItemIr {
                                item_id: item_id.clone(),
                                source_record_key: item_source_record_key,
                                event_start: record.timestamp.clone(),
                                event_end: record.timestamp.clone(),
                                role,
                                kind: AgentItemKind::Message,
                                projections: default_item_projections(
                                    profile,
                                    role,
                                    AgentItemKind::Message,
                                ),
                                text: Some(text.clone()),
                                compact_text: Some(preview_text(&text, 400)),
                                payload: json!({
                                    "schema": "agent_message_ir.v1",
                                    "agent_source": "claude",
                                    "source_kind": "message",
                                }),
                                raw_ref: Some(record.raw_ref.clone()),
                                metadata: claude_record_metadata(record, block_index),
                            };
                            push_item_to_turn(&mut current_turn, &item);
                            model_visible_history.push(item_id);
                            session_items.push(item);
                        }
                        ClaudeBlock::ToolUse { block, block_index } => {
                            if profile == AgentIngestProfile::HotMemory {
                                continue;
                            }
                            let item_source_record_key = derived_source_record_key(
                                &record.source_record_key,
                                "tool_call",
                                block_index,
                            );
                            let item_id = item_id(&session_key, &item_source_record_key, 0);
                            let payload = tool_payload(&block, profile);
                            let item = AgentItemIr {
                                item_id: item_id.clone(),
                                source_record_key: item_source_record_key,
                                event_start: record.timestamp.clone(),
                                event_end: record.timestamp.clone(),
                                role: AgentItemRole::Assistant,
                                kind: AgentItemKind::ToolCall,
                                projections: tool_projections(profile, AgentItemKind::ToolCall),
                                text: None,
                                compact_text: Some(compact_tool_use(&block)),
                                payload,
                                raw_ref: Some(record.raw_ref.clone()),
                                metadata: claude_record_metadata(record, block_index),
                            };
                            push_item_to_turn(&mut current_turn, &item);
                            model_visible_history.push(item_id);
                            compact_tool_count += 1;
                            session_items.push(item);
                        }
                        ClaudeBlock::ToolResult { block, block_index } => {
                            if profile == AgentIngestProfile::HotMemory {
                                continue;
                            }
                            let item_source_record_key = derived_source_record_key(
                                &record.source_record_key,
                                "tool_result",
                                block_index,
                            );
                            let item_id = item_id(&session_key, &item_source_record_key, 0);
                            let payload = tool_result_payload(&block, profile);
                            let item = AgentItemIr {
                                item_id: item_id.clone(),
                                source_record_key: item_source_record_key,
                                event_start: record.timestamp.clone(),
                                event_end: record.timestamp.clone(),
                                role: AgentItemRole::Tool,
                                kind: AgentItemKind::ToolResult,
                                projections: tool_projections(profile, AgentItemKind::ToolResult),
                                text: None,
                                compact_text: Some(compact_tool_result(&block)),
                                payload,
                                raw_ref: Some(record.raw_ref.clone()),
                                metadata: claude_record_metadata(record, block_index),
                            };
                            push_item_to_turn(&mut current_turn, &item);
                            model_visible_history.push(item_id);
                            compact_tool_count += 1;
                            session_items.push(item);
                        }
                    }
                }
            }
            ClaudeRawKind::ToolUse => {
                if profile != AgentIngestProfile::HotMemory {
                    let item_source_record_key =
                        derived_source_record_key(&record.source_record_key, "tool_call", 0);
                    let item_id = item_id(&session_key, &item_source_record_key, 0);
                    let item = AgentItemIr {
                        item_id: item_id.clone(),
                        source_record_key: item_source_record_key,
                        event_start: record.timestamp.clone(),
                        event_end: record.timestamp.clone(),
                        role: AgentItemRole::Assistant,
                        kind: AgentItemKind::ToolCall,
                        projections: tool_projections(profile, AgentItemKind::ToolCall),
                        text: None,
                        compact_text: Some(compact_tool_use(&record.payload)),
                        payload: tool_payload(&record.payload, profile),
                        raw_ref: Some(record.raw_ref.clone()),
                        metadata: claude_record_metadata(record, 0),
                    };
                    push_item_to_turn(&mut current_turn, &item);
                    model_visible_history.push(item_id);
                    compact_tool_count += 1;
                    session_items.push(item);
                }
            }
            ClaudeRawKind::ToolResult => {
                if profile != AgentIngestProfile::HotMemory {
                    let item_source_record_key =
                        derived_source_record_key(&record.source_record_key, "tool_result", 0);
                    let item_id = item_id(&session_key, &item_source_record_key, 0);
                    let item = AgentItemIr {
                        item_id: item_id.clone(),
                        source_record_key: item_source_record_key,
                        event_start: record.timestamp.clone(),
                        event_end: record.timestamp.clone(),
                        role: AgentItemRole::Tool,
                        kind: AgentItemKind::ToolResult,
                        projections: tool_projections(profile, AgentItemKind::ToolResult),
                        text: None,
                        compact_text: Some(compact_tool_result(&record.payload)),
                        payload: tool_result_payload(&record.payload, profile),
                        raw_ref: Some(record.raw_ref.clone()),
                        metadata: claude_record_metadata(record, 0),
                    };
                    push_item_to_turn(&mut current_turn, &item);
                    model_visible_history.push(item_id);
                    compact_tool_count += 1;
                    session_items.push(item);
                }
            }
            ClaudeRawKind::RuntimeEvent => {
                let event_source_record_key =
                    derived_source_record_key(&record.source_record_key, "runtime_event", 0);
                runtime_events.push(AgentRuntimeEventIr {
                    event_id: item_id(&session_key, &event_source_record_key, 0),
                    source_record_key: event_source_record_key,
                    event_start: record.timestamp.clone(),
                    event_end: record.timestamp.clone(),
                    event_kind: runtime_event_kind(&record.payload),
                    severity: runtime_severity(&record.payload),
                    compact_text: runtime_compact_text(&record.payload),
                    payload: runtime_payload(&record.payload, profile),
                    raw_ref: Some(record.raw_ref.clone()),
                });
            }
        }
    }
    finish_open_turn(&mut current_turn, &mut turns);

    Ok(AgentSessionIr {
        source: AgentSource::Claude,
        source_id,
        session_id: session_id.clone(),
        session_key,
        source_uri,
        cwd,
        project_key,
        model,
        started_at,
        ended_at,
        metadata: json!({
            "schema": "agent_session_ir.v1",
            "agent_source": "claude",
            "profile": profile,
            "record_count": records.len(),
            "tool_payload_policy": match profile {
                AgentIngestProfile::HotMemory => "skipped",
                AgentIngestProfile::CompactAudit => "compact_summary",
                AgentIngestProfile::Forensic => "raw_payload_preserved",
            },
        }),
        turns,
        session_items,
        compactions: Vec::new(),
        prompt_snapshots,
        runtime_events,
    })
}

pub fn parse_reduce_claude_jsonl(
    source_uri: impl Into<String>,
    jsonl: &str,
    profile: AgentIngestProfile,
) -> Result<AgentSessionIr, ClaudeAgentIngestError> {
    let source_uri = source_uri.into();
    let records = parse_claude_jsonl_records(source_uri.clone(), jsonl)?;
    reduce_claude_session(source_uri, records, profile)
}

fn parse_claude_value(
    source_uri: &str,
    line_number: usize,
    byte_offset: usize,
    byte_len: usize,
    value: Value,
    records: &mut Vec<ClaudeRawRecord>,
) {
    let raw_ref = RawEvidenceRef {
        source_uri: source_uri.to_string(),
        byte_offset: Some(byte_offset as u64),
        byte_len: Some(byte_len as u64),
        line_number: Some(line_number as u64),
        sha256: Some(sha256_hex(value.to_string().as_bytes())),
    };
    let source_record_key = source_record_key(source_uri, line_number, &value);

    if value.get("type").and_then(Value::as_str) == Some("progress") {
        if let Some(message) = value.get("data").and_then(|data| data.get("message")) {
            if message.is_object() {
                parse_claude_value(
                    source_uri,
                    line_number,
                    byte_offset,
                    byte_len,
                    message.clone(),
                    records,
                );
                return;
            }
        }
        records.push(base_record(
            source_uri,
            source_record_key,
            line_number,
            byte_offset,
            byte_len,
            raw_ref,
            &value,
            ClaudeRawKind::RuntimeEvent,
            Some(AgentItemRole::Runtime),
            value.clone(),
        ));
        return;
    }

    let raw_type = value.get("type").and_then(Value::as_str);
    if raw_type == Some("tool_use") {
        records.push(base_record(
            source_uri,
            source_record_key,
            line_number,
            byte_offset,
            byte_len,
            raw_ref,
            &value,
            ClaudeRawKind::ToolUse,
            Some(AgentItemRole::Assistant),
            value.clone(),
        ));
        return;
    }
    if raw_type == Some("tool_result") {
        records.push(base_record(
            source_uri,
            source_record_key,
            line_number,
            byte_offset,
            byte_len,
            raw_ref,
            &value,
            ClaudeRawKind::ToolResult,
            Some(AgentItemRole::Tool),
            value.clone(),
        ));
        return;
    }

    let role = value
        .get("message")
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        .or(raw_type)
        .and_then(agent_role);
    let raw_kind = if role.is_some() {
        ClaudeRawKind::Message
    } else {
        ClaudeRawKind::RuntimeEvent
    };
    records.push(base_record(
        source_uri,
        source_record_key,
        line_number,
        byte_offset,
        byte_len,
        raw_ref,
        &value,
        raw_kind,
        role,
        value.clone(),
    ));
}

#[allow(clippy::too_many_arguments)]
fn base_record(
    source_uri: &str,
    source_record_key: String,
    line_number: usize,
    byte_offset: usize,
    byte_len: usize,
    raw_ref: RawEvidenceRef,
    value: &Value,
    raw_kind: ClaudeRawKind,
    role: Option<AgentItemRole>,
    payload: Value,
) -> ClaudeRawRecord {
    ClaudeRawRecord {
        source_uri: source_uri.to_string(),
        source_record_key,
        line_number,
        byte_offset,
        byte_len,
        raw_ref,
        timestamp: timestamp_from_value(value, line_number),
        raw_kind,
        role,
        session_id: value
            .get("sessionId")
            .or_else(|| value.get("session_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        cwd: value
            .get("cwd")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .or_else(|| cwd_from_claude_path(Path::new(source_uri))),
        project_key: value
            .get("project")
            .or_else(|| value.get("projectName"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .or_else(|| project_key_from_source_uri(source_uri)),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        uuid: value
            .get("uuid")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        parent_uuid: value
            .get("parentUuid")
            .or_else(|| value.get("parent_uuid"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        payload,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ClaudeBlock {
    Text { text: String, block_index: usize },
    ToolUse { block: Value, block_index: usize },
    ToolResult { block: Value, block_index: usize },
}

fn message_blocks(record: &ClaudeRawRecord) -> Vec<ClaudeBlock> {
    let content = record
        .payload
        .get("message")
        .and_then(|message| message.get("content"))
        .or_else(|| record.payload.get("content"));

    match content {
        Some(Value::String(text)) => vec![ClaudeBlock::Text {
            text: text.trim().to_string(),
            block_index: 0,
        }],
        Some(Value::Array(blocks)) => {
            blocks
                .iter()
                .enumerate()
                .filter_map(
                    |(index, block)| match block.get("type").and_then(Value::as_str) {
                        Some("text") => block.get("text").and_then(Value::as_str).map(|text| {
                            ClaudeBlock::Text {
                                text: text.trim().to_string(),
                                block_index: index,
                            }
                        }),
                        Some("tool_use") => Some(ClaudeBlock::ToolUse {
                            block: block.clone(),
                            block_index: index,
                        }),
                        Some("tool_result") => Some(ClaudeBlock::ToolResult {
                            block: block.clone(),
                            block_index: index,
                        }),
                        _ => None,
                    },
                )
                .collect()
        }
        Some(value) => vec![ClaudeBlock::Text {
            text: value.to_string(),
            block_index: 0,
        }],
        None => Vec::new(),
    }
}

#[derive(Debug, Clone)]
struct OpenTurn {
    turn_id: String,
    turn_index: u64,
    event_start: String,
    event_end: String,
    user_goal: Option<String>,
    item_ids: Vec<String>,
    prompt_snapshot_id: Option<String>,
}

fn push_item_to_turn(current_turn: &mut Option<OpenTurn>, item: &AgentItemIr) {
    if let Some(turn) = current_turn {
        turn.event_end = item.event_end.clone();
        turn.item_ids.push(item.item_id.clone());
    }
}

fn finish_open_turn(current_turn: &mut Option<OpenTurn>, turns: &mut Vec<AgentTurnIr>) {
    if let Some(turn) = current_turn.take() {
        turns.push(AgentTurnIr {
            turn_id: turn.turn_id,
            turn_index: turn.turn_index,
            event_start: turn.event_start,
            event_end: turn.event_end,
            user_goal: turn.user_goal,
            status: Some(AgentTurnStatus::Completed),
            item_ids: turn.item_ids,
            prompt_snapshot_id: turn.prompt_snapshot_id,
            metadata: json!({
                "schema": "agent_turn_ir.v1",
                "turn_basis": "claude_user_message",
            }),
        });
    }
}

fn tool_payload(block: &Value, profile: AgentIngestProfile) -> Value {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let input = block.get("input").cloned().unwrap_or(Value::Null);
    let mut payload = json!({
        "schema": "agent_tool_summary_ir.v1",
        "agent_source": "claude",
        "source_kind": "tool_use",
        "tool_id": block.get("id").cloned().unwrap_or(Value::Null),
        "tool_name": name,
        "input_preview": preview_text(&json_value_string(&input), 500),
        "input_sha256": canonical_source_hash(&input),
    });
    if profile == AgentIngestProfile::Forensic {
        payload["raw_block"] = block.clone();
    }
    payload
}

fn tool_result_payload(block: &Value, profile: AgentIngestProfile) -> Value {
    let content = block.get("content").cloned().unwrap_or(Value::Null);
    let mut payload = json!({
        "schema": "agent_tool_summary_ir.v1",
        "agent_source": "claude",
        "source_kind": "tool_result",
        "tool_use_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
        "is_error": block.get("is_error").cloned().unwrap_or(Value::Bool(false)),
        "content_preview": preview_text(&tool_result_content_text(block), 800),
        "content_bytes": json_value_string(&content).len(),
        "content_sha256": canonical_source_hash(&content),
    });
    if profile == AgentIngestProfile::Forensic {
        payload["raw_block"] = block.clone();
    }
    payload
}

fn runtime_payload(payload: &Value, profile: AgentIngestProfile) -> Value {
    let mut compact = json!({
        "schema": "agent_runtime_event_ir.v1",
        "agent_source": "claude",
        "source_kind": payload.get("type").cloned().unwrap_or(Value::Null),
    });
    if let Some(message) = payload.get("message") {
        compact["message"] = message.clone();
    }
    if let Some(data) = payload.get("data") {
        compact["data_hash"] = Value::String(canonical_source_hash(data));
    }
    if profile == AgentIngestProfile::Forensic {
        compact["raw"] = payload.clone();
    }
    compact
}

fn tool_projections(profile: AgentIngestProfile, kind: AgentItemKind) -> Vec<AgentProjection> {
    default_item_projections(profile, AgentItemRole::Tool, kind)
}

fn compact_tool_use(block: &Value) -> String {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let input = block.get("input").unwrap_or(&Value::Null);
    match name {
        "Bash" => {
            let command = input
                .get("command")
                .map(json_value_string)
                .unwrap_or_default();
            let description = input
                .get("description")
                .map(json_value_string)
                .unwrap_or_default();
            if description.trim().is_empty() {
                format!("[Bash] {}", compact_cli_text(&command))
            } else {
                format!(
                    "[Bash] {}\n  -> {}",
                    compact_cli_text(&command),
                    description.trim()
                )
            }
        }
        "Read" => format!(
            "[Read {}]",
            input
                .get("file_path")
                .map(json_value_string)
                .unwrap_or_default()
        ),
        "Edit" => format!(
            "[Edit {}]",
            input
                .get("file_path")
                .map(json_value_string)
                .unwrap_or_default()
        ),
        "Write" => format!(
            "[Write {}]",
            input
                .get("file_path")
                .map(json_value_string)
                .unwrap_or_default()
        ),
        "Grep" => format!(
            "[Grep] {}",
            input
                .get("pattern")
                .map(json_value_string)
                .unwrap_or_default()
        ),
        "Glob" => format!(
            "[Glob] {}",
            input
                .get("pattern")
                .map(json_value_string)
                .unwrap_or_default()
        ),
        _ => format!(
            "[tool:{name}] {}",
            preview_text(&json_value_string(input), 200)
        ),
    }
}

fn compact_tool_result(block: &Value) -> String {
    let mut prefix = "[tool-result]";
    if block.get("is_error").and_then(Value::as_bool) == Some(true) {
        prefix = "[tool-result:error]";
    }
    format!(
        "{prefix} {}",
        compact_cli_text(&tool_result_content_text(block))
    )
}

fn tool_result_content_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(value) => json_value_string(value),
        None => String::new(),
    }
}

fn runtime_event_kind(payload: &Value) -> String {
    payload
        .get("event")
        .or_else(|| payload.get("kind"))
        .or_else(|| payload.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("runtime_event")
        .to_string()
}

fn runtime_severity(payload: &Value) -> Option<AgentRuntimeSeverity> {
    match payload
        .get("severity")
        .or_else(|| payload.get("level"))
        .and_then(Value::as_str)
        .unwrap_or("info")
    {
        "debug" => Some(AgentRuntimeSeverity::Debug),
        "warn" | "warning" => Some(AgentRuntimeSeverity::Warning),
        "error" | "fatal" => Some(AgentRuntimeSeverity::Error),
        "info" | "progress" => Some(AgentRuntimeSeverity::Info),
        _ => None,
    }
}

fn runtime_compact_text(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .or_else(|| payload.get("data").and_then(|data| data.get("message")))
        .and_then(Value::as_str)
        .map(|message| preview_text(message, 500))
}

fn claude_record_metadata(record: &ClaudeRawRecord, block_index: usize) -> Value {
    json!({
        "claude": {
            "uuid": record.uuid,
            "parent_uuid": record.parent_uuid,
            "line_number": record.line_number,
            "block_index": block_index,
            "raw_kind": format!("{:?}", record.raw_kind),
        }
    })
}

fn source_record_key(source_uri: &str, line_number: usize, value: &Value) -> String {
    if let Some(uuid) = value.get("uuid").and_then(Value::as_str) {
        return format!("claude:{source_uri}:line:{line_number}:uuid:{uuid}");
    }
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        return format!("claude:{source_uri}:line:{line_number}:id:{id}");
    }
    format!(
        "claude:{source_uri}:line:{line_number}:{}",
        &canonical_source_hash(value)[..16]
    )
}

fn derived_source_record_key(
    raw_source_record_key: &str,
    record_kind: &str,
    block_index: usize,
) -> String {
    format!("{raw_source_record_key}/{record_kind}/{block_index}")
}

fn prompt_source_record_key(
    raw_source_record_key: &str,
    turn_index: u64,
    block_index: usize,
) -> String {
    format!("{raw_source_record_key}/prompt/{turn_index}/{block_index}")
}

fn item_id(session_key: &str, source_record_key: &str, block_index: usize) -> String {
    format!(
        "{session_key}:item:{}:{block_index}",
        &sha256_hex(source_record_key.as_bytes())[..16]
    )
}

fn fallback_session_id(source_uri: &str) -> String {
    format!("path-{}", &sha256_hex(source_uri.as_bytes())[..20])
}

fn timestamp_from_value(value: &Value, line_number: usize) -> String {
    value
        .get("timestamp")
        .or_else(|| value.get("created_at"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("1970-01-01T00:00:00.{:06}Z", line_number % 1_000_000))
}

fn agent_role(value: &str) -> Option<AgentItemRole> {
    match value {
        "user" => Some(AgentItemRole::User),
        "assistant" => Some(AgentItemRole::Assistant),
        "system" => Some(AgentItemRole::System),
        "tool" => Some(AgentItemRole::Tool),
        _ => None,
    }
}

fn cwd_from_claude_path(path: &Path) -> Option<String> {
    let parent = path.parent()?.file_name()?.to_string_lossy();
    if parent.is_empty() {
        return None;
    }
    let decoded = parent.replace('-', "/");
    if decoded.starts_with('/') {
        Some(decoded)
    } else {
        None
    }
}

fn project_key_from_source_uri(source_uri: &str) -> Option<String> {
    PathBuf::from(source_uri)
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
}

fn project_key_from_cwd(cwd: &str) -> Option<String> {
    Path::new(cwd)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        compact
    } else {
        let mut output = compact.chars().take(max_chars).collect::<String>();
        output.push_str("...");
        output
    }
}

fn compact_cli_text(text: &str) -> String {
    preview_text(text, 1_000)
}

fn json_value_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        value => value.to_string(),
    }
}

fn stable_json_hash(value: &Value) -> String {
    canonical_source_hash(value)
}

fn estimate_tokens(text: &str) -> u64 {
    text.split_whitespace().count().max(1) as u64
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const CLAUDE_JSONL: &str = r#"{"type":"user","uuid":"u1","sessionId":"sess-123","cwd":"/Users/paulhan/dev/demo","timestamp":"2026-05-25T10:00:00Z","message":{"role":"user","content":[{"type":"text","text":"Please inspect the file."}]}}
{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-123","cwd":"/Users/paulhan/dev/demo","timestamp":"2026-05-25T10:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"I will read it."},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"/Users/paulhan/dev/demo/src/lib.rs"}}]}}
{"type":"user","uuid":"u2","parentUuid":"a1","sessionId":"sess-123","cwd":"/Users/paulhan/dev/demo","timestamp":"2026-05-25T10:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"pub fn hello() {}"}]}}
{"type":"progress","timestamp":"2026-05-25T10:00:03Z","message":"tokens processed","data":{"phase":"runtime","message":"tokens processed"}}"#;

    #[test]
    fn compact_audit_normalizes_messages_tools_and_runtime_rows() {
        let ir = parse_reduce_claude_jsonl(
            "/Users/paulhan/.claude/projects/-Users-paulhan-dev-demo/session.jsonl",
            CLAUDE_JSONL,
            AgentIngestProfile::CompactAudit,
        )
        .unwrap();
        ir.validate().unwrap();

        assert_eq!(ir.source, AgentSource::Claude);
        assert_eq!(ir.session_id, "sess-123");
        assert_eq!(ir.session_key, "claude:sess-123");
        assert_eq!(ir.cwd.as_deref(), Some("/Users/paulhan/dev/demo"));
        assert_eq!(ir.project_key.as_deref(), Some("demo"));
        assert_eq!(ir.turns.len(), 1);
        assert_eq!(ir.prompt_snapshots.len(), 1);
        assert_eq!(ir.runtime_events.len(), 1);

        let kinds = ir
            .session_items
            .iter()
            .map(|item| item.kind.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                AgentItemKind::Message,
                AgentItemKind::Message,
                AgentItemKind::ToolCall,
                AgentItemKind::ToolResult,
            ]
        );

        let tool_call = ir
            .session_items
            .iter()
            .find(|item| item.kind == AgentItemKind::ToolCall)
            .unwrap();
        assert_eq!(tool_call.role, AgentItemRole::Assistant);
        assert_eq!(
            tool_call.compact_text.as_deref(),
            Some("[Read /Users/paulhan/dev/demo/src/lib.rs]")
        );
        assert_eq!(tool_call.payload["schema"], "agent_tool_summary_ir.v1");
        assert_eq!(tool_call.payload["tool_name"], "Read");
        assert!(tool_call.payload.get("raw_block").is_none());
        assert!(tool_call.projections.contains(&AgentProjection::Audit));

        let tool_result = ir
            .session_items
            .iter()
            .find(|item| item.kind == AgentItemKind::ToolResult)
            .unwrap();
        assert_eq!(tool_result.role, AgentItemRole::Tool);
        assert_eq!(tool_result.payload["tool_use_id"], "toolu_1");
        assert!(tool_result
            .compact_text
            .as_deref()
            .unwrap()
            .contains("pub fn hello"));

        let first_turn = &ir.turns[0];
        assert_eq!(first_turn.item_ids.len(), 4);
        assert_eq!(
            first_turn.user_goal.as_deref(),
            Some("Please inspect the file.")
        );
        assert_eq!(ir.prompt_snapshots[0].input_item_ids.len(), 1);

        let source_keys = ir
            .session_items
            .iter()
            .map(|item| item.source_record_key.as_str())
            .chain(
                ir.prompt_snapshots
                    .iter()
                    .map(|prompt| prompt.source_record_key.as_str()),
            )
            .chain(
                ir.runtime_events
                    .iter()
                    .map(|event| event.source_record_key.as_str()),
            )
            .collect::<BTreeSet<_>>();
        assert_eq!(
            source_keys.len(),
            ir.session_items.len() + ir.prompt_snapshots.len() + ir.runtime_events.len()
        );
    }

    #[test]
    fn repeated_claude_uuid_lines_get_distinct_source_keys() {
        let jsonl = r#"{"type":"user","uuid":"same","sessionId":"sess-repeat","timestamp":"2026-05-25T10:00:00Z","message":{"role":"user","content":[{"type":"text","text":"first"}]}}
{"type":"user","uuid":"same","sessionId":"sess-repeat","timestamp":"2026-05-25T10:00:01Z","message":{"role":"user","content":[{"type":"text","text":"second"}]}}"#;

        let records = parse_claude_jsonl_records("/tmp/repeated-uuid.jsonl", jsonl).unwrap();
        assert_eq!(records.len(), 2);
        assert_ne!(records[0].source_record_key, records[1].source_record_key);

        let ir = reduce_claude_session(
            "/tmp/repeated-uuid.jsonl",
            records,
            AgentIngestProfile::HotMemory,
        )
        .unwrap();
        let source_keys = ir
            .session_items
            .iter()
            .map(|item| item.source_record_key.as_str())
            .chain(
                ir.prompt_snapshots
                    .iter()
                    .map(|prompt| prompt.source_record_key.as_str()),
            )
            .collect::<BTreeSet<_>>();
        assert_eq!(
            source_keys.len(),
            ir.session_items.len() + ir.prompt_snapshots.len()
        );
    }

    #[test]
    fn hot_memory_skips_tool_payload_items_but_keeps_messages_and_turns() {
        let ir = parse_reduce_claude_jsonl(
            "/Users/paulhan/.claude/projects/-Users-paulhan-dev-demo/session.jsonl",
            CLAUDE_JSONL,
            AgentIngestProfile::HotMemory,
        )
        .unwrap();
        ir.validate().unwrap();

        assert_eq!(ir.session_items.len(), 2);
        assert!(ir
            .session_items
            .iter()
            .all(|item| item.kind == AgentItemKind::Message));
        assert!(ir
            .session_items
            .iter()
            .all(|item| item.payload.get("raw_block").is_none()));
        assert_eq!(ir.runtime_events.len(), 1);
        assert_eq!(ir.metadata["tool_payload_policy"], "skipped");
    }

    #[test]
    fn forensic_preserves_raw_tool_blocks_and_uses_path_fingerprint_without_session_id() {
        let jsonl = r#"{"type":"assistant","uuid":"a-no-session","timestamp":"2026-05-25T11:00:00Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_2","name":"Bash","input":{"command":"cargo test -p onecontext-memory-db"}}]}}"#;

        let ir = parse_reduce_claude_jsonl(
            "/tmp/no-session/claude.jsonl",
            jsonl,
            AgentIngestProfile::Forensic,
        )
        .unwrap();
        ir.validate().unwrap();

        assert!(ir.session_id.starts_with("path-"));
        let tool = ir.session_items.first().unwrap();
        assert_eq!(tool.kind, AgentItemKind::ToolCall);
        assert!(tool.projections.contains(&AgentProjection::Forensic));
        assert_eq!(tool.payload["raw_block"]["name"], "Bash");
        assert!(tool.raw_ref.as_ref().unwrap().sha256.is_some());
    }
}
