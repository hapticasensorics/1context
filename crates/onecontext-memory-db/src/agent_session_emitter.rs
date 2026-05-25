use std::collections::HashMap;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::agent_session_ir::{
    source_key_session, source_key_turn, AgentCompactionIr, AgentItemIr, AgentItemKind,
    AgentItemRole, AgentProjection, AgentPromptSnapshotIr, AgentRuntimeEventIr, AgentSessionIr,
    AgentSource, AgentTurnIr, RawEvidenceRef,
};
use crate::source_identity::object_id as source_object_id;
use crate::write_objects::{PerceptionEdgeInput, PerceptionObjectInput};

const AGENT_SESSION_SCHEMA: &str = "agent_session_ir.v1";
const DEFAULT_DISPLAY_TEXT_MAX_CHARS: usize = 4_096;

const AGENTS_SESSIONS_LANE_ID: &str = "20000000-0000-0000-0000-000000000101";
const AGENTS_TURNS_LANE_ID: &str = "20000000-0000-0000-0000-000000000102";
const AGENTS_MESSAGES_LANE_ID: &str = "20000000-0000-0000-0000-000000000103";
const AGENTS_TOOLS_LANE_ID: &str = "20000000-0000-0000-0000-000000000104";
const AGENTS_COMPACTIONS_LANE_ID: &str = "20000000-0000-0000-0000-000000000105";
const AGENTS_PROMPTS_LANE_ID: &str = "20000000-0000-0000-0000-000000000106";
const AGENTS_EVENTS_LANE_ID: &str = "20000000-0000-0000-0000-000000000107";

const PROMPT_INPUT_COMPLETE_EDGE_LIMIT: usize = 32;
const PROMPT_INPUT_COMPACT_EDGE_LIMIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionEmitOptions {
    pub privacy_class: String,
    pub include_tool_summaries: bool,
    pub display_text_max_chars: usize,
}

impl Default for AgentSessionEmitOptions {
    fn default() -> Self {
        Self {
            privacy_class: "normal".to_string(),
            include_tool_summaries: true,
            display_text_max_chars: DEFAULT_DISPLAY_TEXT_MAX_CHARS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionEmitterError {
    InvalidSourceId(String),
    MissingSessionTimestamp,
}

impl std::fmt::Display for AgentSessionEmitterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSourceId(value) => {
                write!(formatter, "invalid agent session source_id: {value}")
            }
            Self::MissingSessionTimestamp => write!(
                formatter,
                "agent session needs started_at/ended_at or at least one timestamped child item"
            ),
        }
    }
}

impl std::error::Error for AgentSessionEmitterError {}

pub fn emit_agent_session_objects(
    session: &AgentSessionIr,
) -> Result<Vec<PerceptionObjectInput>, AgentSessionEmitterError> {
    emit_agent_session_objects_with_options(session, &AgentSessionEmitOptions::default())
}

pub fn emit_agent_session_objects_with_options(
    session: &AgentSessionIr,
    options: &AgentSessionEmitOptions,
) -> Result<Vec<PerceptionObjectInput>, AgentSessionEmitterError> {
    let source_uuid = Uuid::parse_str(&session.source_id)
        .map_err(|_| AgentSessionEmitterError::InvalidSourceId(session.source_id.clone()))?;
    let (session_start, session_end) = session_time_range(session)?;
    let turn_by_item_id = turn_by_item_id(&session.turns);
    let item_key_by_id = session
        .session_items
        .iter()
        .map(|item| (item.item_id.as_str(), item.source_record_key.as_str()))
        .collect::<HashMap<_, _>>();
    let prompt_key_by_id = session
        .prompt_snapshots
        .iter()
        .map(|prompt| {
            (
                prompt.prompt_snapshot_id.as_str(),
                prompt.source_record_key.as_str(),
            )
        })
        .collect::<HashMap<_, _>>();
    let compaction_key_by_epoch = session
        .compactions
        .iter()
        .map(|compaction| {
            (
                compaction.compaction_epoch,
                compaction.source_record_key.as_str(),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut records = Vec::new();

    records.push(agent_session_object(
        session,
        &session_start,
        &session_end,
        options,
    ));

    for turn in &session.turns {
        records.push(agent_turn_object(
            session,
            turn,
            &source_uuid,
            &prompt_key_by_id,
            options,
        ));
    }

    for (ordinal, item) in session.session_items.iter().enumerate() {
        if item_is_tool_summary(item) && !options.include_tool_summaries {
            continue;
        }
        records.push(agent_item_object(
            session,
            item,
            turn_by_item_id.get(item.item_id.as_str()).copied(),
            ordinal,
            options,
        ));
    }

    for compaction in &session.compactions {
        records.push(agent_compaction_object(
            session,
            compaction,
            &source_uuid,
            &item_key_by_id,
            options,
        ));
    }

    for prompt in &session.prompt_snapshots {
        records.push(agent_prompt_snapshot_object(
            session,
            prompt,
            &source_uuid,
            &item_key_by_id,
            &compaction_key_by_epoch,
            options,
        ));
    }

    for (ordinal, event) in session.runtime_events.iter().enumerate() {
        records.push(agent_runtime_event_object(session, event, ordinal, options));
    }

    Ok(records)
}

pub fn agent_session_source_record_key(source: AgentSource, session_id: &str) -> String {
    source_key_session(source, session_id)
}

pub fn agent_turn_source_record_key(
    source: AgentSource,
    session_id: &str,
    turn_index: u64,
) -> String {
    source_key_turn(source, session_id, turn_index)
}

fn agent_session_container_source_record_key(session: &AgentSessionIr) -> String {
    if agent_session_uses_source_uri_fingerprint(session.source) {
        format!(
            "{}/source/{}",
            agent_session_source_record_key(session.source, &session.session_id),
            short_hash(&session.source_uri)
        )
    } else {
        agent_session_source_record_key(session.source, &session.session_id)
    }
}

fn agent_turn_container_source_record_key(session: &AgentSessionIr, turn_index: u64) -> String {
    if agent_session_uses_source_uri_fingerprint(session.source) {
        format!(
            "{}/source/{}",
            agent_turn_source_record_key(session.source, &session.session_id, turn_index),
            short_hash(&session.source_uri)
        )
    } else {
        agent_turn_source_record_key(session.source, &session.session_id, turn_index)
    }
}

fn agent_session_uses_source_uri_fingerprint(source: AgentSource) -> bool {
    matches!(source, AgentSource::Claude)
}

fn agent_session_object(
    session: &AgentSessionIr,
    session_start: &str,
    session_end: &str,
    options: &AgentSessionEmitOptions,
) -> PerceptionObjectInput {
    let projections = vec![
        "ui_timeline".to_string(),
        "audit".to_string(),
        "memory_candidate".to_string(),
    ];
    let payload = common_payload(
        session,
        None,
        projections,
        None,
        json!({
            "session_key": session.session_key,
            "cwd": session.cwd,
            "project_key": session.project_key,
            "model": session.model,
            "source_uri": session.source_uri,
            "schema": AGENT_SESSION_SCHEMA,
            "metadata": session.metadata
        }),
    );
    base_object(
        session,
        agent_session_container_source_record_key(session),
        "agent_session",
        "system",
        "agents.sessions",
        AGENTS_SESSIONS_LANE_ID,
        session_start,
        session_end,
        payload,
        Some(format!(
            "{} session {}",
            session.source.as_str(),
            session.session_id
        )),
        None,
        Vec::new(),
        None,
        options,
    )
}

fn agent_turn_object(
    session: &AgentSessionIr,
    turn: &AgentTurnIr,
    source_uuid: &Uuid,
    prompt_key_by_id: &HashMap<&str, &str>,
    options: &AgentSessionEmitOptions,
) -> PerceptionObjectInput {
    let mut edges = Vec::new();
    if let Some(prompt_id) = turn.prompt_snapshot_id.as_deref() {
        if let Some(prompt_key) = prompt_key_by_id.get(prompt_id) {
            edges.push(edge_to(
                source_uuid,
                prompt_key,
                "references",
                json!({
                    "agent_edge": "turn_prompt_snapshot",
                    "turn_id": turn.turn_id,
                    "prompt_snapshot_id": prompt_id
                }),
            ));
        }
    }
    let payload = common_payload(
        session,
        Some(&turn.turn_id),
        vec!["ui_timeline".to_string(), "audit".to_string()],
        None,
        json!({
            "turn_index": turn.turn_index,
            "status": turn.status,
            "item_ids": turn.item_ids,
            "item_count": turn.item_ids.len(),
            "prompt_snapshot_id": turn.prompt_snapshot_id,
            "user_goal": turn.user_goal,
            "metadata": turn.metadata
        }),
    );
    base_object(
        session,
        agent_turn_container_source_record_key(session, turn.turn_index),
        "agent_turn",
        "system",
        "agents.turns",
        AGENTS_TURNS_LANE_ID,
        &turn.event_start,
        &turn.event_end,
        payload,
        Some(format!("Turn {}", turn.turn_index)),
        turn.user_goal.clone(),
        edges,
        Some(i64::try_from(turn.turn_index).unwrap_or(i64::MAX)),
        options,
    )
}

fn agent_item_object(
    session: &AgentSessionIr,
    item: &AgentItemIr,
    turn_id: Option<&str>,
    ordinal: usize,
    options: &AgentSessionEmitOptions,
) -> PerceptionObjectInput {
    let output_kind = agent_item_output_kind(item.kind);
    let lane_key = lane_key_for_kind(output_kind);
    let lane_id = lane_id_for_kind(output_kind);
    let projections = projections_or_default(&item.projections, output_kind);
    let display_text = item
        .compact_text
        .as_deref()
        .or(item.text.as_deref())
        .map(|text| truncate_display_text(text, options.display_text_max_chars));
    let payload = common_payload(
        session,
        turn_id,
        projections,
        item.raw_ref.as_ref(),
        merge_payload(
            &item.payload,
            json!({
                "item_id": item.item_id,
                "source_item_kind": item_kind_str(item.kind),
                "role": item_role_str(item.role),
                "metadata": item.metadata
            }),
        ),
    );
    base_object(
        session,
        item.source_record_key.clone(),
        output_kind,
        item_role_str(item.role),
        lane_key,
        lane_id,
        &item.event_start,
        &item.event_end,
        payload,
        Some(item_display_title(item)),
        display_text,
        Vec::new(),
        Some(i64::try_from(ordinal).unwrap_or(i64::MAX)),
        options,
    )
}

fn agent_compaction_object(
    session: &AgentSessionIr,
    compaction: &AgentCompactionIr,
    source_uuid: &Uuid,
    item_key_by_id: &HashMap<&str, &str>,
    options: &AgentSessionEmitOptions,
) -> PerceptionObjectInput {
    let mut edges = Vec::new();
    for item_id in compaction.replaced_item_ids.iter().flatten() {
        if let Some(item_key) = item_key_by_id.get(item_id.as_str()) {
            edges.push(edge_to(
                source_uuid,
                item_key,
                "derived_from",
                json!({
                    "agent_edge": "compaction_replaced_item",
                    "compaction_id": compaction.compaction_id,
                    "compaction_epoch": compaction.compaction_epoch,
                    "item_id": item_id
                }),
            ));
        }
    }
    for item_id in &compaction.replacement_item_ids {
        if let Some(item_key) = item_key_by_id.get(item_id.as_str()) {
            edges.push(edge_to(
                source_uuid,
                item_key,
                "contains",
                json!({
                    "agent_edge": "compaction_replacement_item",
                    "compaction_id": compaction.compaction_id,
                    "compaction_epoch": compaction.compaction_epoch,
                    "item_id": item_id
                }),
            ));
        }
    }
    let payload = common_payload(
        session,
        None,
        vec!["audit".to_string(), "prompt_snapshot".to_string()],
        compaction.raw_ref.as_ref(),
        json!({
            "compaction_id": compaction.compaction_id,
            "compaction_epoch": compaction.compaction_epoch,
            "replacement_item_ids": compaction.replacement_item_ids,
            "replacement_item_count": compaction.replacement_item_ids.len(),
            "replaced_item_ids": compaction.replaced_item_ids,
            "replaced_item_count": compaction.replaced_item_ids.as_ref().map(Vec::len),
            "replacement_history_hash": compaction.replacement_history_hash,
            "summary_text": compaction.summary_text,
            "metadata": compaction.metadata
        }),
    );
    base_object(
        session,
        compaction.source_record_key.clone(),
        "agent_compaction",
        "system",
        "agents.compactions",
        AGENTS_COMPACTIONS_LANE_ID,
        &compaction.event_start,
        &compaction.event_end,
        payload,
        Some(format!("Compaction {}", compaction.compaction_epoch)),
        compaction
            .summary_text
            .as_deref()
            .map(|text| truncate_display_text(text, options.display_text_max_chars)),
        edges,
        Some(i64::try_from(compaction.compaction_epoch).unwrap_or(i64::MAX)),
        options,
    )
}

fn agent_prompt_snapshot_object(
    session: &AgentSessionIr,
    prompt: &AgentPromptSnapshotIr,
    source_uuid: &Uuid,
    item_key_by_id: &HashMap<&str, &str>,
    compaction_key_by_epoch: &HashMap<u64, &str>,
    options: &AgentSessionEmitOptions,
) -> PerceptionObjectInput {
    let input_refs = prompt_input_refs(prompt, source_uuid, item_key_by_id);
    let input_object_ids = input_refs
        .iter()
        .map(|input_ref| input_ref.object_id.clone())
        .collect::<Vec<_>>();
    let edge_indexes = prompt_input_edge_indexes(&input_refs);
    let mut edges = Vec::new();
    for input_ref in &input_refs {
        if edge_indexes.should_materialize(input_ref.input_index) {
            edges.push(PerceptionEdgeInput {
                to_object_id: input_ref.object_id.clone(),
                edge_kind: "references".to_string(),
                confidence: Some(1.0),
                metadata: json!({
                    "agent_edge": "prompt_input",
                    "turn_id": prompt.turn_id,
                    "prompt_snapshot_id": prompt.prompt_snapshot_id,
                    "item_id": input_ref.item_id,
                    "input_index": input_ref.input_index,
                    "projection": "model_visible",
                    "materialization": edge_indexes.policy_name()
                }),
            });
        }
    }
    if let Some(compaction_key) = compaction_key_by_epoch.get(&prompt.compaction_epoch) {
        edges.push(edge_to(
            source_uuid,
            compaction_key,
            "references",
            json!({
                "agent_edge": "prompt_compaction_epoch",
                "turn_id": prompt.turn_id,
                "prompt_snapshot_id": prompt.prompt_snapshot_id,
                "compaction_epoch": prompt.compaction_epoch
            }),
        ));
    }
    let payload = common_payload(
        session,
        Some(&prompt.turn_id),
        vec!["prompt_snapshot".to_string(), "audit".to_string()],
        None,
        json!({
            "prompt_snapshot_id": prompt.prompt_snapshot_id,
            "compaction_epoch": prompt.compaction_epoch,
            "input_item_ids": prompt.input_item_ids,
            "input_item_count": prompt.input_item_count,
            "input_object_ids": input_object_ids,
            "input_ref_materialization": edge_indexes.payload_metadata(&input_refs, prompt),
            "tool_count": prompt.tool_count,
            "base_instructions_hash": prompt.base_instructions_hash,
            "dynamic_tools_hash": prompt.dynamic_tools_hash,
            "prompt_hash": prompt.prompt_hash,
            "token_estimate": prompt.token_estimate,
            "metadata": prompt.metadata
        }),
    );
    base_object(
        session,
        prompt.source_record_key.clone(),
        "agent_prompt_snapshot",
        "system",
        "agents.prompts",
        AGENTS_PROMPTS_LANE_ID,
        &prompt.event_start,
        &prompt.event_end,
        payload,
        Some(format!("Prompt snapshot {}", prompt.turn_id)),
        None,
        edges,
        Some(i64::try_from(prompt.compaction_epoch).unwrap_or(i64::MAX)),
        options,
    )
}

fn agent_runtime_event_object(
    session: &AgentSessionIr,
    event: &AgentRuntimeEventIr,
    ordinal: usize,
    options: &AgentSessionEmitOptions,
) -> PerceptionObjectInput {
    let payload = common_payload(
        session,
        None,
        vec!["audit".to_string()],
        event.raw_ref.as_ref(),
        merge_payload(
            &event.payload,
            json!({
                "event_id": event.event_id,
                "event_kind": event.event_kind,
                "severity": event.severity
            }),
        ),
    );
    base_object(
        session,
        event.source_record_key.clone(),
        "agent_runtime_event",
        "runtime",
        "agents.events",
        AGENTS_EVENTS_LANE_ID,
        &event.event_start,
        &event.event_end,
        payload,
        Some(event.event_kind.clone()),
        event
            .compact_text
            .as_deref()
            .map(|text| truncate_display_text(text, options.display_text_max_chars)),
        Vec::new(),
        Some(i64::try_from(ordinal).unwrap_or(i64::MAX)),
        options,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptInputRef {
    item_id: String,
    object_id: String,
    input_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptInputEdgeIndexes {
    policy: PromptInputEdgePolicy,
    indexes: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptInputEdgePolicy {
    Complete,
    CompactPrefixSuffix,
}

impl PromptInputEdgeIndexes {
    fn should_materialize(&self, input_index: usize) -> bool {
        self.indexes.binary_search(&input_index).is_ok()
    }

    fn policy_name(&self) -> &'static str {
        match self.policy {
            PromptInputEdgePolicy::Complete => "complete_edges",
            PromptInputEdgePolicy::CompactPrefixSuffix => "compact_prefix_suffix_edges",
        }
    }

    fn payload_metadata(
        &self,
        input_refs: &[PromptInputRef],
        prompt: &AgentPromptSnapshotIr,
    ) -> Value {
        json!({
            "policy": self.policy_name(),
            "full_reference_projection": "payload.input_item_ids + payload.input_object_ids",
            "input_item_count": prompt.input_item_count,
            "known_input_object_count": input_refs.len(),
            "explicit_prompt_input_edge_count": self.indexes.len(),
            "omitted_prompt_input_edge_count": input_refs.len().saturating_sub(self.indexes.len()),
            "complete_edge_limit": PROMPT_INPUT_COMPLETE_EDGE_LIMIT,
            "compact_edge_limit": PROMPT_INPUT_COMPACT_EDGE_LIMIT,
            "input_refs_hash": prompt_input_refs_hash(input_refs),
        })
    }
}

fn prompt_input_refs(
    prompt: &AgentPromptSnapshotIr,
    source_uuid: &Uuid,
    item_key_by_id: &HashMap<&str, &str>,
) -> Vec<PromptInputRef> {
    prompt
        .input_item_ids
        .iter()
        .enumerate()
        .filter_map(|(input_index, item_id)| {
            item_key_by_id
                .get(item_id.as_str())
                .map(|item_key| PromptInputRef {
                    item_id: item_id.clone(),
                    object_id: object_id_for_key(source_uuid, item_key),
                    input_index,
                })
        })
        .collect()
}

fn prompt_input_edge_indexes(input_refs: &[PromptInputRef]) -> PromptInputEdgeIndexes {
    if input_refs.len() <= PROMPT_INPUT_COMPLETE_EDGE_LIMIT {
        return PromptInputEdgeIndexes {
            policy: PromptInputEdgePolicy::Complete,
            indexes: input_refs
                .iter()
                .map(|input_ref| input_ref.input_index)
                .collect(),
        };
    }

    let prefix_count = PROMPT_INPUT_COMPACT_EDGE_LIMIT / 2;
    let suffix_count = PROMPT_INPUT_COMPACT_EDGE_LIMIT.saturating_sub(prefix_count);
    let suffix_start = input_refs.len().saturating_sub(suffix_count);
    let mut indexes = input_refs
        .iter()
        .enumerate()
        .filter_map(|(known_index, input_ref)| {
            (known_index < prefix_count || known_index >= suffix_start)
                .then_some(input_ref.input_index)
        })
        .collect::<Vec<_>>();
    indexes.sort_unstable();
    indexes.dedup();

    PromptInputEdgeIndexes {
        policy: PromptInputEdgePolicy::CompactPrefixSuffix,
        indexes,
    }
}

fn prompt_input_refs_hash(input_refs: &[PromptInputRef]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agent.prompt_input_refs.v1\0");
    for input_ref in input_refs {
        hasher.update(input_ref.input_index.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(input_ref.item_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(input_ref.object_id.as_bytes());
        hasher.update(b"\0");
    }
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn base_object(
    session: &AgentSessionIr,
    source_record_key: String,
    kind: &str,
    role: &str,
    lane_key: &str,
    lane_id: &str,
    event_start: &str,
    event_end: &str,
    payload: Value,
    display_title: Option<String>,
    display_text: Option<String>,
    edges: Vec<PerceptionEdgeInput>,
    source_sequence: Option<i64>,
    options: &AgentSessionEmitOptions,
) -> PerceptionObjectInput {
    let (modality, body_type, text_value) = body_fields_for_record(kind, display_text.as_deref());
    PerceptionObjectInput {
        client_record_id: None,
        source_id: session.source_id.clone(),
        source_record_key,
        lane_id: lane_id.to_string(),
        series_kind: agent_series_kind(session.source).to_string(),
        series_key: agent_series_key(session),
        series_display_name: Some(agent_series_display_name(session)),
        series_parent_key: None,
        modality: Some(modality.to_string()),
        kind: kind.to_string(),
        role: role.to_string(),
        privacy_class: options.privacy_class.clone(),
        event_start: event_start.to_string(),
        event_end: ensure_event_end_after_start(event_start, event_end),
        time_semantics: "interval".to_string(),
        temporal_level: "event".to_string(),
        native_resolution_ns: None,
        stored_resolution_ns: None,
        indexed_resolution_ns: None,
        display_resolution_hint_ns: None,
        time_resolution_ns: None,
        time_uncertainty_ns: None,
        alignment_confidence: Some(1.0),
        alignment_method: Some("agent_session_ir".to_string()),
        materialization_policy: "index_events".to_string(),
        importance_score: Some(0.5),
        blob: None,
        body_type: Some(body_type.to_string()),
        text_value,
        number_value: None,
        bool_value: None,
        payload,
        display_title,
        display_text,
        edges,
        source_start_ns: None,
        source_end_ns: None,
        source_sequence,
        media_start_offset_ns: None,
        media_end_offset_ns: None,
        schema_name: Some(lane_key.to_string()),
        schema_version: Some(1),
        confidence: Some(1.0),
        metadata: json!({
            "writer": {
                "name": "onecontext-memory-db.agent_session_emitter",
                "schema_version": 1
            },
            "source": {
                "agent_source": session.source.as_str(),
                "agent_connector_key": connector_key_for_agent_source(session.source),
                "source_uri": session.source_uri,
                "session_id": session.session_id
            },
            "lane": {
                "lane_key": lane_key
            }
        }),
    }
}

fn agent_series_kind(source: AgentSource) -> &'static str {
    match source {
        AgentSource::Codex => "codex_session",
        AgentSource::Claude => "claude_session",
        AgentSource::OnecontextAgent => "onecontext_agent_session",
    }
}

fn agent_series_key(session: &AgentSessionIr) -> String {
    format!("{}:session:{}", session.source.as_str(), session.session_id)
}

fn agent_series_display_name(session: &AgentSessionIr) -> String {
    let location = session
        .project_key
        .as_deref()
        .or_else(|| session.cwd.as_deref().and_then(last_path_component))
        .or(session.cwd.as_deref())
        .unwrap_or(&session.session_id);
    format!(
        "{} session in {location}",
        agent_source_display_name(session.source)
    )
}

fn last_path_component(path: &str) -> Option<&str> {
    path.rsplit('/').find(|component| !component.is_empty())
}

fn agent_source_display_name(source: AgentSource) -> &'static str {
    match source {
        AgentSource::Codex => "Codex",
        AgentSource::Claude => "Claude",
        AgentSource::OnecontextAgent => "1Context agent",
    }
}

fn body_fields_for_record(
    kind: &str,
    display_text: Option<&str>,
) -> (&'static str, &'static str, Option<String>) {
    match (kind, display_text) {
        ("agent_message", Some(text)) => ("text", "text", Some(text.to_string())),
        ("agent_tool_summary", Some(text)) => ("mixed", "text", Some(text.to_string())),
        ("agent_session", _) => ("mixed", "mixed", None),
        ("agent_compaction", Some(text)) | ("agent_runtime_event", Some(text)) => {
            ("mixed", "mixed", Some(text.to_string()))
        }
        _ => ("mixed", "json", None),
    }
}

fn common_payload(
    session: &AgentSessionIr,
    turn_id: Option<&str>,
    projections: Vec<String>,
    raw_ref: Option<&RawEvidenceRef>,
    details: Value,
) -> Value {
    let mut payload = match details {
        Value::Object(map) => map,
        value => {
            let mut map = Map::new();
            map.insert("details".to_string(), value);
            map
        }
    };
    payload.insert("agent_source".to_string(), json!(session.source.as_str()));
    payload.insert("source_id".to_string(), json!(session.source_id));
    payload.insert("session_id".to_string(), json!(session.session_id));
    payload.insert("turn_id".to_string(), json!(turn_id));
    payload.insert("projections".to_string(), json!(projections));
    payload.insert("raw_ref".to_string(), json!(raw_ref));
    Value::Object(payload)
}

fn merge_payload(source_payload: &Value, details: Value) -> Value {
    let mut merged = match details {
        Value::Object(map) => map,
        value => {
            let mut map = Map::new();
            map.insert("details".to_string(), value);
            map
        }
    };
    match source_payload {
        Value::Object(map) => {
            for (key, value) in map {
                merged.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        Value::Null => {}
        value => {
            merged.insert("source_payload".to_string(), value.clone());
        }
    }
    Value::Object(merged)
}

fn session_time_range(
    session: &AgentSessionIr,
) -> Result<(String, String), AgentSessionEmitterError> {
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    if let Some(started_at) = &session.started_at {
        starts.push(started_at.as_str());
    }
    if let Some(ended_at) = &session.ended_at {
        ends.push(ended_at.as_str());
    }
    for turn in &session.turns {
        starts.push(&turn.event_start);
        ends.push(&turn.event_end);
    }
    for item in &session.session_items {
        starts.push(&item.event_start);
        ends.push(&item.event_end);
    }
    for compaction in &session.compactions {
        starts.push(&compaction.event_start);
        ends.push(&compaction.event_end);
    }
    for prompt in &session.prompt_snapshots {
        starts.push(&prompt.event_start);
        ends.push(&prompt.event_end);
    }
    for event in &session.runtime_events {
        starts.push(&event.event_start);
        ends.push(&event.event_end);
    }

    let start = min_timestamp(&starts).ok_or(AgentSessionEmitterError::MissingSessionTimestamp)?;
    let end = max_timestamp(&ends).ok_or(AgentSessionEmitterError::MissingSessionTimestamp)?;
    Ok((start.clone(), ensure_event_end_after_start(&start, &end)))
}

fn min_timestamp(values: &[&str]) -> Option<String> {
    values
        .iter()
        .filter_map(|value| parse_rfc3339_utc(value).map(|ts| (ts, *value)))
        .min_by_key(|(ts, _)| *ts)
        .map(|(_, value)| value.to_string())
}

fn max_timestamp(values: &[&str]) -> Option<String> {
    values
        .iter()
        .filter_map(|value| parse_rfc3339_utc(value).map(|ts| (ts, *value)))
        .max_by_key(|(ts, _)| *ts)
        .map(|(_, value)| value.to_string())
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .ok()
}

fn ensure_event_end_after_start(event_start: &str, event_end: &str) -> String {
    match (parse_rfc3339_utc(event_start), parse_rfc3339_utc(event_end)) {
        (Some(start), Some(end)) if end <= start => {
            (start + Duration::microseconds(1)).to_rfc3339_opts(SecondsFormat::Micros, true)
        }
        _ => event_end.to_string(),
    }
}

fn item_is_tool_summary(item: &AgentItemIr) -> bool {
    matches!(
        item.kind,
        AgentItemKind::ToolCall | AgentItemKind::ToolResult
    )
}

fn agent_item_output_kind(item_kind: AgentItemKind) -> &'static str {
    match item_kind {
        AgentItemKind::ToolCall | AgentItemKind::ToolResult => "agent_tool_summary",
        AgentItemKind::RuntimeEvent => "agent_runtime_event",
        _ => "agent_message",
    }
}

fn lane_key_for_kind(kind: &str) -> &'static str {
    match kind {
        "agent_session" => "agents.sessions",
        "agent_turn" => "agents.turns",
        "agent_tool_summary" => "agents.tools",
        "agent_compaction" => "agents.compactions",
        "agent_prompt_snapshot" => "agents.prompts",
        "agent_runtime_event" => "agents.events",
        _ => "agents.messages",
    }
}

fn lane_id_for_kind(kind: &str) -> &'static str {
    match kind {
        "agent_session" => AGENTS_SESSIONS_LANE_ID,
        "agent_turn" => AGENTS_TURNS_LANE_ID,
        "agent_tool_summary" => AGENTS_TOOLS_LANE_ID,
        "agent_compaction" => AGENTS_COMPACTIONS_LANE_ID,
        "agent_prompt_snapshot" => AGENTS_PROMPTS_LANE_ID,
        "agent_runtime_event" => AGENTS_EVENTS_LANE_ID,
        _ => AGENTS_MESSAGES_LANE_ID,
    }
}

fn projections_or_default(projections: &[AgentProjection], kind: &str) -> Vec<String> {
    if !projections.is_empty() {
        return projections
            .iter()
            .copied()
            .map(projection_str)
            .map(str::to_string)
            .collect();
    }
    match kind {
        "agent_tool_summary" | "agent_runtime_event" => vec!["audit".to_string()],
        _ => vec![
            "ui_timeline".to_string(),
            "model_visible".to_string(),
            "memory_candidate".to_string(),
        ],
    }
}

fn connector_key_for_agent_source(source: AgentSource) -> &'static str {
    match source {
        AgentSource::Codex => "codex.local_sessions",
        AgentSource::Claude => "claude.local_sessions",
        AgentSource::OnecontextAgent => "onecontext.agent_sessions",
    }
}

fn turn_by_item_id(turns: &[AgentTurnIr]) -> HashMap<&str, &str> {
    let mut map = HashMap::new();
    for turn in turns {
        for item_id in &turn.item_ids {
            map.insert(item_id.as_str(), turn.turn_id.as_str());
        }
    }
    map
}

fn item_display_title(item: &AgentItemIr) -> String {
    match agent_item_output_kind(item.kind) {
        "agent_tool_summary" => item
            .payload
            .get("tool_name")
            .and_then(Value::as_str)
            .map(|tool_name| format!("Tool: {tool_name}"))
            .unwrap_or_else(|| "Tool summary".to_string()),
        _ => match item.role {
            AgentItemRole::Assistant => "Assistant message".to_string(),
            AgentItemRole::User => "User message".to_string(),
            AgentItemRole::System => "System message".to_string(),
            AgentItemRole::Tool => "tool message".to_string(),
            AgentItemRole::Runtime => "runtime message".to_string(),
        },
    }
}

fn projection_str(projection: AgentProjection) -> &'static str {
    match projection {
        AgentProjection::ModelVisible => "model_visible",
        AgentProjection::UiTimeline => "ui_timeline",
        AgentProjection::Audit => "audit",
        AgentProjection::PromptSnapshot => "prompt_snapshot",
        AgentProjection::MemoryCandidate => "memory_candidate",
        AgentProjection::Forensic => "forensic",
    }
}

fn item_role_str(role: AgentItemRole) -> &'static str {
    match role {
        AgentItemRole::User => "user",
        AgentItemRole::Assistant => "assistant",
        AgentItemRole::System => "system",
        AgentItemRole::Tool => "tool",
        AgentItemRole::Runtime => "runtime",
    }
}

fn item_kind_str(kind: AgentItemKind) -> &'static str {
    match kind {
        AgentItemKind::Message => "message",
        AgentItemKind::ToolCall => "tool_call",
        AgentItemKind::ToolResult => "tool_result",
        AgentItemKind::Reasoning => "reasoning",
        AgentItemKind::Patch => "patch",
        AgentItemKind::FileChange => "file_change",
        AgentItemKind::RuntimeEvent => "runtime_event",
    }
}

fn edge_to(
    source_uuid: &Uuid,
    source_record_key: &str,
    edge_kind: &str,
    metadata: Value,
) -> PerceptionEdgeInput {
    PerceptionEdgeInput {
        to_object_id: object_id_for_key(source_uuid, source_record_key),
        edge_kind: edge_kind.to_string(),
        confidence: Some(1.0),
        metadata,
    }
}

fn object_id_for_key(source_uuid: &Uuid, source_record_key: &str) -> String {
    source_object_id(*source_uuid, source_record_key).to_string()
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut output = String::with_capacity(16);
    for byte in &digest[..8] {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn truncate_display_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    const SUFFIX: &str = "\n[truncated]";
    let suffix_len = SUFFIX.chars().count();
    if max_chars <= suffix_len {
        return text.chars().take(max_chars).collect();
    }
    let mut output = text
        .chars()
        .take(max_chars - suffix_len)
        .collect::<String>();
    output.push_str(SUFFIX);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_session_ir::{AgentRuntimeSeverity, AgentTurnStatus};

    #[test]
    fn emits_expected_generic_object_counts() {
        let records = emit_agent_session_objects(&sample_session()).unwrap();

        assert_eq!(records.len(), 12);
        assert_eq!(kind_count(&records, "agent_session"), 1);
        assert_eq!(kind_count(&records, "agent_turn"), 2);
        assert_eq!(kind_count(&records, "agent_message"), 4);
        assert_eq!(kind_count(&records, "agent_tool_summary"), 1);
        assert_eq!(kind_count(&records, "agent_compaction"), 1);
        assert_eq!(kind_count(&records, "agent_prompt_snapshot"), 2);
        assert_eq!(kind_count(&records, "agent_runtime_event"), 1);
    }

    #[test]
    fn emits_stable_source_record_key_shapes_and_preserves_item_keys() {
        let records = emit_agent_session_objects(&sample_session()).unwrap();

        assert!(records.iter().any(|record| {
            record.kind == "agent_session"
                && record.source_record_key == "agent/codex/session-1"
                && record.source_id == "10000000-0000-0000-0000-000000000001"
        }));
        assert!(records.iter().any(|record| {
            record.kind == "agent_turn"
                && record.source_record_key == "agent/codex/session-1/turn/0"
        }));
        assert!(records.iter().any(|record| {
            record.kind == "agent_message"
                && record.source_record_key == "agent/codex/session-1/item/assistant-2"
        }));
        assert!(records.iter().any(|record| {
            record.kind == "agent_compaction"
                && record.source_record_key == "agent/codex/session-1/compaction/1/replacement-hash"
        }));
        assert!(records.iter().any(|record| {
            record.kind == "agent_prompt_snapshot"
                && record.source_record_key == "agent/codex/session-1/turn/1/prompt/prompt-hash-2"
        }));
    }

    #[test]
    fn claude_container_source_keys_include_source_uri_fingerprint() {
        let mut first = sample_session();
        first.source = AgentSource::Claude;
        first.source_id = AgentSource::Claude.source_id().to_string();
        first.session_key = "claude:session-1".to_string();
        first.source_uri = "/tmp/claude/a.jsonl".to_string();

        let mut second = first.clone();
        second.source_uri = "/tmp/claude/b.jsonl".to_string();

        let first_records = emit_agent_session_objects(&first).unwrap();
        let second_records = emit_agent_session_objects(&second).unwrap();
        let first_session = first_records
            .iter()
            .find(|record| record.kind == "agent_session")
            .unwrap();
        let second_session = second_records
            .iter()
            .find(|record| record.kind == "agent_session")
            .unwrap();
        let first_turn = first_records
            .iter()
            .find(|record| record.kind == "agent_turn" && record.payload["turn_index"] == 0)
            .unwrap();
        let second_turn = second_records
            .iter()
            .find(|record| record.kind == "agent_turn" && record.payload["turn_index"] == 0)
            .unwrap();

        assert_ne!(
            first_session.source_record_key,
            second_session.source_record_key
        );
        assert_ne!(first_turn.source_record_key, second_turn.source_record_key);
        assert!(first_session
            .source_record_key
            .starts_with("agent/claude/session-1/source/"));
        assert!(first_turn
            .source_record_key
            .starts_with("agent/claude/session-1/turn/0/source/"));
    }

    #[test]
    fn emits_series_identity_and_body_fields() {
        let records = emit_agent_session_objects(&sample_session()).unwrap();
        let session = records
            .iter()
            .find(|record| record.kind == "agent_session")
            .unwrap();
        let message = records
            .iter()
            .find(|record| {
                record.kind == "agent_message"
                    && record.source_record_key == "agent/codex/session-1/item/assistant-1"
            })
            .unwrap();
        let tool = records
            .iter()
            .find(|record| record.kind == "agent_tool_summary")
            .unwrap();

        assert!(records.iter().all(|record| {
            record.series_kind == "codex_session"
                && record.series_key == "codex:session:session-1"
                && record.series_parent_key.is_none()
        }));
        assert_eq!(
            session.series_display_name.as_deref(),
            Some("Codex session in repo")
        );
        assert_eq!(session.modality.as_deref(), Some("mixed"));
        assert_eq!(session.body_type.as_deref(), Some("mixed"));
        assert_eq!(message.modality.as_deref(), Some("text"));
        assert_eq!(message.body_type.as_deref(), Some("text"));
        assert_eq!(message.text_value.as_deref(), Some("tests passed"));
        assert_eq!(tool.modality.as_deref(), Some("mixed"));
        assert_eq!(tool.body_type.as_deref(), Some("text"));
        assert_eq!(
            tool.text_value.as_deref(),
            Some("cargo test -p onecontext-memory-db")
        );
    }

    #[test]
    fn compaction_and_prompt_edges_include_agent_metadata() {
        let records = emit_agent_session_objects(&sample_session()).unwrap();
        let compaction = records
            .iter()
            .find(|record| record.kind == "agent_compaction")
            .unwrap();
        let prompt = records
            .iter()
            .find(|record| {
                record.kind == "agent_prompt_snapshot"
                    && record.payload["prompt_snapshot_id"] == "prompt-2"
            })
            .unwrap();

        assert!(compaction.edges.iter().any(|edge| {
            edge.edge_kind == "derived_from"
                && edge.metadata["agent_edge"] == "compaction_replaced_item"
                && edge.metadata["compaction_epoch"] == 1
                && edge.metadata["item_id"] == "assistant-1"
        }));
        assert!(compaction.edges.iter().any(|edge| {
            edge.edge_kind == "contains"
                && edge.metadata["agent_edge"] == "compaction_replacement_item"
                && edge.metadata["item_id"] == "assistant-2"
        }));
        assert!(prompt.edges.iter().any(|edge| {
            edge.edge_kind == "references"
                && edge.metadata["agent_edge"] == "prompt_input"
                && edge.metadata["projection"] == "model_visible"
                && edge.metadata["turn_id"] == "turn-2"
        }));
        assert!(prompt.edges.iter().any(|edge| {
            edge.edge_kind == "references"
                && edge.metadata["agent_edge"] == "prompt_compaction_epoch"
                && edge.metadata["compaction_epoch"] == 1
        }));
    }

    #[test]
    fn large_prompt_inputs_keep_full_payload_refs_with_compact_edge_anchors() {
        let mut session = sample_session();
        let input_item_ids = (0..40)
            .map(|index| format!("large-input-{index}"))
            .collect::<Vec<_>>();
        session.session_items = input_item_ids
            .iter()
            .enumerate()
            .map(|(index, item_id)| AgentItemIr {
                item_id: item_id.clone(),
                source_record_key: format!("agent/codex/session-1/item/{item_id}"),
                event_start: ts(index as i64 + 1),
                event_end: ts(index as i64 + 2),
                role: AgentItemRole::Assistant,
                kind: AgentItemKind::Message,
                projections: vec![AgentProjection::ModelVisible, AgentProjection::UiTimeline],
                text: Some(format!("large prompt input {index}")),
                compact_text: None,
                payload: json!({}),
                raw_ref: Some(raw_ref(100 + index as u64)),
                metadata: json!({}),
            })
            .collect();
        session.turns = vec![AgentTurnIr {
            turn_id: "turn-large".to_string(),
            turn_index: 0,
            event_start: ts(0),
            event_end: ts(50),
            user_goal: Some("large prompt".to_string()),
            status: Some(AgentTurnStatus::Completed),
            item_ids: input_item_ids.clone(),
            prompt_snapshot_id: Some("prompt-large".to_string()),
            metadata: json!({}),
        }];
        session.compactions.clear();
        session.runtime_events.clear();
        session.prompt_snapshots = vec![AgentPromptSnapshotIr {
            prompt_snapshot_id: "prompt-large".to_string(),
            source_record_key: "agent/codex/session-1/turn/0/prompt/prompt-large".to_string(),
            turn_id: "turn-large".to_string(),
            event_start: ts(0),
            event_end: ts(1),
            compaction_epoch: 0,
            input_item_ids: input_item_ids.clone(),
            input_item_count: input_item_ids.len(),
            tool_count: 0,
            base_instructions_hash: Some("base-hash".to_string()),
            dynamic_tools_hash: None,
            prompt_hash: Some("prompt-large".to_string()),
            token_estimate: Some(4_000),
            metadata: json!({}),
        }];

        let records = emit_agent_session_objects(&session).unwrap();
        let prompt = records
            .iter()
            .find(|record| record.kind == "agent_prompt_snapshot")
            .unwrap();
        let prompt_input_edges = prompt
            .edges
            .iter()
            .filter(|edge| edge.metadata["agent_edge"] == "prompt_input")
            .collect::<Vec<_>>();

        assert_eq!(
            prompt.payload["input_item_ids"].as_array().unwrap().len(),
            40
        );
        assert_eq!(
            prompt.payload["input_object_ids"].as_array().unwrap().len(),
            40
        );
        assert_eq!(
            prompt.payload["input_ref_materialization"]["policy"],
            "compact_prefix_suffix_edges"
        );
        assert_eq!(
            prompt.payload["input_ref_materialization"]["explicit_prompt_input_edge_count"],
            PROMPT_INPUT_COMPACT_EDGE_LIMIT
        );
        assert_eq!(
            prompt.payload["input_ref_materialization"]["omitted_prompt_input_edge_count"],
            24
        );
        assert_eq!(prompt_input_edges.len(), PROMPT_INPUT_COMPACT_EDGE_LIMIT);
        assert!(prompt_input_edges
            .iter()
            .any(|edge| edge.metadata["input_index"] == 0));
        assert!(prompt_input_edges
            .iter()
            .any(|edge| edge.metadata["input_index"] == 7));
        assert!(!prompt_input_edges
            .iter()
            .any(|edge| edge.metadata["input_index"] == 8));
        assert!(prompt_input_edges
            .iter()
            .any(|edge| edge.metadata["input_index"] == 32));
        assert!(prompt_input_edges
            .iter()
            .any(|edge| edge.metadata["input_index"] == 39));
    }

    #[test]
    fn display_text_is_truncated_for_large_messages() {
        let mut session = sample_session();
        session.session_items[0].text = Some("a".repeat(80));
        session.session_items[0].compact_text = None;
        let options = AgentSessionEmitOptions {
            display_text_max_chars: 32,
            ..AgentSessionEmitOptions::default()
        };

        let records = emit_agent_session_objects_with_options(&session, &options).unwrap();
        let message = records
            .iter()
            .find(|record| record.source_record_key == "agent/codex/session-1/item/user-1")
            .unwrap();
        let display_text = message.display_text.as_ref().unwrap();

        assert_eq!(display_text.chars().count(), 32);
        assert!(display_text.ends_with("[truncated]"));
    }

    fn kind_count(records: &[PerceptionObjectInput], kind: &str) -> usize {
        records.iter().filter(|record| record.kind == kind).count()
    }

    fn sample_session() -> AgentSessionIr {
        AgentSessionIr {
            source: AgentSource::Codex,
            source_id: "10000000-0000-0000-0000-000000000001".to_string(),
            session_id: "session-1".to_string(),
            session_key: "codex:session-1".to_string(),
            source_uri: "/Users/example/.codex/sessions/rollout-session-1.jsonl".to_string(),
            cwd: Some("/repo".to_string()),
            project_key: Some("repo".to_string()),
            model: Some("gpt-5".to_string()),
            started_at: Some(ts(0)),
            ended_at: Some(ts(90)),
            metadata: json!({"fixture": true}),
            turns: vec![
                AgentTurnIr {
                    turn_id: "turn-1".to_string(),
                    turn_index: 0,
                    event_start: ts(0),
                    event_end: ts(20),
                    user_goal: Some("first goal".to_string()),
                    status: Some(AgentTurnStatus::Completed),
                    item_ids: vec![
                        "user-1".to_string(),
                        "tool-1".to_string(),
                        "assistant-1".to_string(),
                    ],
                    prompt_snapshot_id: Some("prompt-1".to_string()),
                    metadata: json!({}),
                },
                AgentTurnIr {
                    turn_id: "turn-2".to_string(),
                    turn_index: 1,
                    event_start: ts(30),
                    event_end: ts(80),
                    user_goal: Some("second goal".to_string()),
                    status: Some(AgentTurnStatus::Completed),
                    item_ids: vec!["user-2".to_string(), "assistant-2".to_string()],
                    prompt_snapshot_id: Some("prompt-2".to_string()),
                    metadata: json!({}),
                },
            ],
            session_items: vec![
                AgentItemIr {
                    item_id: "user-1".to_string(),
                    source_record_key: "agent/codex/session-1/item/user-1".to_string(),
                    event_start: ts(1),
                    event_end: ts(2),
                    role: AgentItemRole::User,
                    kind: AgentItemKind::Message,
                    projections: vec![AgentProjection::ModelVisible, AgentProjection::UiTimeline],
                    text: Some("please run tests".to_string()),
                    compact_text: None,
                    payload: json!({}),
                    raw_ref: Some(raw_ref(10)),
                    metadata: json!({}),
                },
                AgentItemIr {
                    item_id: "tool-1".to_string(),
                    source_record_key: "agent/codex/session-1/item/tool-1".to_string(),
                    event_start: ts(3),
                    event_end: ts(4),
                    role: AgentItemRole::Tool,
                    kind: AgentItemKind::ToolCall,
                    projections: vec![AgentProjection::Audit],
                    text: None,
                    compact_text: Some("cargo test -p onecontext-memory-db".to_string()),
                    payload: json!({"tool_name": "shell", "exit_code": 0}),
                    raw_ref: Some(raw_ref(20)),
                    metadata: json!({}),
                },
                AgentItemIr {
                    item_id: "assistant-1".to_string(),
                    source_record_key: "agent/codex/session-1/item/assistant-1".to_string(),
                    event_start: ts(5),
                    event_end: ts(6),
                    role: AgentItemRole::Assistant,
                    kind: AgentItemKind::Message,
                    projections: vec![
                        AgentProjection::ModelVisible,
                        AgentProjection::UiTimeline,
                        AgentProjection::MemoryCandidate,
                    ],
                    text: Some("tests passed".to_string()),
                    compact_text: None,
                    payload: json!({}),
                    raw_ref: Some(raw_ref(30)),
                    metadata: json!({}),
                },
                AgentItemIr {
                    item_id: "user-2".to_string(),
                    source_record_key: "agent/codex/session-1/item/user-2".to_string(),
                    event_start: ts(31),
                    event_end: ts(32),
                    role: AgentItemRole::User,
                    kind: AgentItemKind::Message,
                    projections: vec![AgentProjection::ModelVisible, AgentProjection::UiTimeline],
                    text: Some("continue".to_string()),
                    compact_text: None,
                    payload: json!({}),
                    raw_ref: Some(raw_ref(40)),
                    metadata: json!({}),
                },
                AgentItemIr {
                    item_id: "assistant-2".to_string(),
                    source_record_key: "agent/codex/session-1/item/assistant-2".to_string(),
                    event_start: ts(33),
                    event_end: ts(34),
                    role: AgentItemRole::Assistant,
                    kind: AgentItemKind::Message,
                    projections: vec![
                        AgentProjection::ModelVisible,
                        AgentProjection::UiTimeline,
                        AgentProjection::MemoryCandidate,
                    ],
                    text: Some("continued".to_string()),
                    compact_text: None,
                    payload: json!({}),
                    raw_ref: Some(raw_ref(50)),
                    metadata: json!({}),
                },
            ],
            compactions: vec![AgentCompactionIr {
                compaction_id: "compact-1".to_string(),
                source_record_key: "agent/codex/session-1/compaction/1/replacement-hash"
                    .to_string(),
                event_start: ts(25),
                event_end: ts(26),
                compaction_epoch: 1,
                replacement_item_ids: vec!["assistant-2".to_string()],
                replaced_item_ids: Some(vec!["assistant-1".to_string()]),
                summary_text: Some("replaced the first assistant message".to_string()),
                replacement_history_hash: "replacement-hash".to_string(),
                raw_ref: Some(raw_ref(60)),
                metadata: json!({}),
            }],
            prompt_snapshots: vec![
                AgentPromptSnapshotIr {
                    prompt_snapshot_id: "prompt-1".to_string(),
                    source_record_key: "agent/codex/session-1/turn/0/prompt/prompt-hash-1"
                        .to_string(),
                    turn_id: "turn-1".to_string(),
                    event_start: ts(0),
                    event_end: ts(1),
                    compaction_epoch: 0,
                    input_item_ids: vec!["user-1".to_string()],
                    input_item_count: 1,
                    tool_count: 1,
                    base_instructions_hash: Some("base-hash".to_string()),
                    dynamic_tools_hash: Some("tools-hash".to_string()),
                    prompt_hash: Some("prompt-hash-1".to_string()),
                    token_estimate: Some(100),
                    metadata: json!({}),
                },
                AgentPromptSnapshotIr {
                    prompt_snapshot_id: "prompt-2".to_string(),
                    source_record_key: "agent/codex/session-1/turn/1/prompt/prompt-hash-2"
                        .to_string(),
                    turn_id: "turn-2".to_string(),
                    event_start: ts(30),
                    event_end: ts(31),
                    compaction_epoch: 1,
                    input_item_ids: vec!["user-2".to_string(), "assistant-2".to_string()],
                    input_item_count: 2,
                    tool_count: 1,
                    base_instructions_hash: Some("base-hash".to_string()),
                    dynamic_tools_hash: Some("tools-hash".to_string()),
                    prompt_hash: Some("prompt-hash-2".to_string()),
                    token_estimate: Some(80),
                    metadata: json!({}),
                },
            ],
            runtime_events: vec![AgentRuntimeEventIr {
                event_id: "event-1".to_string(),
                source_record_key: "agent/codex/session-1/runtime/event-1".to_string(),
                event_start: ts(70),
                event_end: ts(71),
                event_kind: "event_msg".to_string(),
                severity: Some(AgentRuntimeSeverity::Info),
                compact_text: Some("worker update".to_string()),
                payload: json!({"native_kind": "EventMsg"}),
                raw_ref: Some(raw_ref(70)),
            }],
        }
    }

    fn raw_ref(line_number: u64) -> RawEvidenceRef {
        RawEvidenceRef {
            source_uri: "/Users/example/.codex/sessions/rollout-session-1.jsonl".to_string(),
            byte_offset: Some(line_number * 100),
            byte_len: Some(64),
            line_number: Some(line_number),
            sha256: Some(format!("hash-{line_number}")),
        }
    }

    fn ts(second: i64) -> String {
        let timestamp = DateTime::parse_from_rfc3339("2026-05-25T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            + Duration::seconds(second);
        timestamp.to_rfc3339_opts(SecondsFormat::Micros, true)
    }
}
