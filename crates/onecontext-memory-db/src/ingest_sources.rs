use std::path::PathBuf;

use chrono::{SecondsFormat, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use uuid::Uuid;

use crate::local_adapters::{
    canonical_local_source, default_context_for_source, default_home_dir,
    ingest_claude_incremental, ingest_codex_incremental, ingest_imessage_incremental,
    AdapterContext, AdapterError, IncrementalIngestBatch, IncrementalIngestOptions,
    LocalIngestCursors, SessionIngestProfile,
};
use crate::source_cursors::{
    advance_source_cursor_after_db_success, load_source_cursor, CursorAdvanceMode,
    SourceCursorError,
};
use crate::write_objects::{
    write_objects_with_client, PerceptionObjectInput, WriteObjectsError, WriteObjectsRequest,
    WriteObjectsResponse,
};

const DEFAULT_SOURCES: &[&str] = &["codex", "claude", "imessage"];
const LOCAL_INGEST_WRITE_CHUNK_SIZE: usize = 250;

#[derive(Debug)]
pub enum IngestSourcesError {
    Adapter(AdapterError),
    Cursor(SourceCursorError),
    Write(WriteObjectsError),
    Json(serde_json::Error),
    InvalidUuid { field: &'static str, value: String },
}

impl std::fmt::Display for IngestSourcesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Adapter(error) => write!(formatter, "adapter error: {error}"),
            Self::Cursor(error) => write!(formatter, "source cursor error: {error}"),
            Self::Write(error) => write!(formatter, "writeObjects error: {error}"),
            Self::Json(error) => write!(formatter, "json error: {error}"),
            Self::InvalidUuid { field, value } => {
                write!(formatter, "invalid {field} UUID {value:?}")
            }
        }
    }
}

impl std::error::Error for IngestSourcesError {}

impl From<AdapterError> for IngestSourcesError {
    fn from(value: AdapterError) -> Self {
        Self::Adapter(value)
    }
}

impl From<SourceCursorError> for IngestSourcesError {
    fn from(value: SourceCursorError) -> Self {
        Self::Cursor(value)
    }
}

impl From<WriteObjectsError> for IngestSourcesError {
    fn from(value: WriteObjectsError) -> Self {
        Self::Write(value)
    }
}

impl From<serde_json::Error> for IngestSourcesError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IngestSourcesRequest {
    pub user_id: String,
    #[serde(default)]
    pub write_id: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub home: Option<PathBuf>,
    #[serde(default = "default_max_events")]
    pub max_events: usize,
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
    #[serde(default)]
    pub include_sensitive_text: bool,
    #[serde(default)]
    pub session_profile: SessionIngestProfile,
    #[serde(default)]
    pub cursor_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IngestSourcesResponse {
    pub ok: bool,
    pub write_id: String,
    pub source_results: Vec<IngestSourceResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IngestSourceResult {
    pub source: String,
    pub source_id: String,
    pub status: String,
    pub read_count: usize,
    pub written_count: usize,
    pub inserted_count: usize,
    pub duplicate_count: usize,
    pub cursor_advanced: bool,
    #[serde(default)]
    pub advancement_mode: Option<CursorAdvanceMode>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub adapter_report: Option<Value>,
}

pub fn ingest_sources_with_client(
    client: &mut Client,
    request: &IngestSourcesRequest,
) -> Result<IngestSourcesResponse, IngestSourcesError> {
    let write_id = request
        .write_id
        .clone()
        .unwrap_or_else(|| generated_write_id(&request.user_id));
    parse_uuid("user_id", &request.user_id)?;
    parse_uuid("write_id", &write_id)?;
    let sources = if request.sources.is_empty() {
        DEFAULT_SOURCES
            .iter()
            .map(|source| source.to_string())
            .collect()
    } else {
        request.sources.clone()
    };
    let cursor_name = request
        .cursor_name
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let home = request.home.clone().unwrap_or_else(default_home_dir);
    let options = IncrementalIngestOptions {
        max_events: request.max_events,
        max_lines: request.max_lines,
        include_sensitive_text: request.include_sensitive_text,
        session_profile: request.session_profile,
    };

    let mut source_results = Vec::with_capacity(sources.len());
    let mut ok = true;
    for source in sources {
        let canonical_source = canonical_request_source(&source)?;
        let context = context_for_request_source(request, canonical_source)?;
        let source_id = context.stream_id.clone();
        let cursor = load_source_cursor(client, &source_id, &cursor_name)?;
        let mut cursors = cursor
            .and_then(|cursor| serde_json::from_value(cursor.cursor_value).ok())
            .unwrap_or_default();
        let batch =
            match ingest_source_batch(&home, &context, &options, canonical_source, &mut cursors) {
                Ok(batch) => batch,
                Err(error) => {
                    ok = false;
                    source_results.push(IngestSourceResult {
                        source,
                        source_id,
                        status: "adapter_error".to_string(),
                        read_count: 0,
                        written_count: 0,
                        inserted_count: 0,
                        duplicate_count: 0,
                        cursor_advanced: false,
                        advancement_mode: None,
                        error_code: Some("ADAPTER_ERROR".to_string()),
                        error: Some(error.to_string()),
                        adapter_report: None,
                    });
                    continue;
                }
            };
        let prepared_write = prepare_batch_write_request(&request.user_id, &write_id, &batch)?;
        let read_count = prepared_write.read_count;
        let write_path = prepared_write.path;
        let adapter_report = serde_json::to_value(&batch.report).ok();
        let write = write_objects_with_client(client, &prepared_write.request);
        match write {
            Ok(write_response) => {
                advance_source_cursor_after_db_success(
                    client,
                    &source_id,
                    &cursor_name,
                    &request.user_id,
                    &serde_json::to_value(&cursors)?,
                    &write_id,
                    &json!({
                        "source": source,
                        "write_path": write_path.as_str(),
                        "read_count": read_count,
                        "written_count": write_response.record_count,
                        "inserted_count": write_response.inserted_count,
                        "duplicate_count": write_response.duplicate_count,
                    }),
                )?;
                source_results.push(success_result(
                    source,
                    source_id,
                    read_count,
                    write_response,
                    CursorAdvanceMode::DbSuccess,
                    None,
                    adapter_report,
                ));
            }
            Err(error) => {
                ok = false;
                source_results.push(IngestSourceResult {
                    source,
                    source_id,
                    status: "write_error".to_string(),
                    read_count,
                    written_count: 0,
                    inserted_count: 0,
                    duplicate_count: 0,
                    cursor_advanced: false,
                    advancement_mode: None,
                    error_code: Some(error.code().to_string()),
                    error: Some(error.to_string()),
                    adapter_report,
                });
            }
        }
    }

    Ok(IngestSourcesResponse {
        ok,
        write_id,
        source_results,
    })
}

fn context_for_request_source(
    request: &IngestSourcesRequest,
    source: &str,
) -> Result<AdapterContext, IngestSourcesError> {
    let mut context = default_context_for_source(source)?;
    context.user_id = request.user_id.clone();
    Ok(context)
}

fn canonical_request_source(source: &str) -> Result<&'static str, IngestSourcesError> {
    canonical_local_source(source)
        .ok_or_else(|| AdapterError::UnknownSource(source.to_string()).into())
}

fn ingest_source_batch(
    home: &PathBuf,
    context: &AdapterContext,
    options: &IncrementalIngestOptions,
    source: &str,
    cursors: &mut LocalIngestCursors,
) -> Result<IncrementalIngestBatch, AdapterError> {
    match source {
        "codex" => ingest_codex_incremental(home, context, options, cursors),
        "claude" => ingest_claude_incremental(home, context, options, cursors),
        "imessage" => ingest_imessage_incremental(home, context, options, cursors),
        other => Err(AdapterError::UnknownSource(other.to_string())),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IngestWritePath {
    PerceptionObjects,
}

impl IngestWritePath {
    fn as_str(self) -> &'static str {
        match self {
            Self::PerceptionObjects => "perception_objects",
        }
    }
}

struct PreparedIngestWrite {
    request: WriteObjectsRequest,
    read_count: usize,
    path: IngestWritePath,
}

fn prepare_batch_write_request(
    user_id: &str,
    write_id: &str,
    batch: &IncrementalIngestBatch,
) -> Result<PreparedIngestWrite, IngestSourcesError> {
    Ok(PreparedIngestWrite {
        request: write_request_from_perception_objects(
            user_id,
            write_id,
            &batch.perception_objects,
        ),
        read_count: batch.perception_objects.len(),
        path: IngestWritePath::PerceptionObjects,
    })
}

fn write_request_from_perception_objects(
    user_id: &str,
    write_id: &str,
    records: &[PerceptionObjectInput],
) -> WriteObjectsRequest {
    WriteObjectsRequest {
        user_id: user_id.to_string(),
        write_id: write_id.to_string(),
        atomicity: Some("chunk".to_string()),
        records: records.to_vec(),
        chunk_size: Some(LOCAL_INGEST_WRITE_CHUNK_SIZE),
    }
}

fn success_result(
    source: String,
    source_id: String,
    read_count: usize,
    write_response: WriteObjectsResponse,
    advancement_mode: CursorAdvanceMode,
    adapter_report: Option<Value>,
    report: Option<Value>,
) -> IngestSourceResult {
    IngestSourceResult {
        source,
        source_id,
        status: "ok".to_string(),
        read_count,
        written_count: write_response.record_count,
        inserted_count: write_response.inserted_count,
        duplicate_count: write_response.duplicate_count,
        cursor_advanced: true,
        advancement_mode: Some(advancement_mode),
        error_code: None,
        error: None,
        adapter_report: report.or(adapter_report),
    }
}

fn generated_write_id(user_id: &str) -> String {
    let user_uuid = Uuid::parse_str(user_id).unwrap_or_else(|_| Uuid::nil());
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
    let mut bytes = [0_u8; 16];
    let digest = sha2::Sha256::digest(
        format!("onecontext-memory-db/ingestSources/v1\0{user_uuid}\0{now}").as_bytes(),
    );
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

fn parse_uuid(field: &'static str, value: &str) -> Result<Uuid, IngestSourcesError> {
    Uuid::parse_str(value).map_err(|_| IngestSourcesError::InvalidUuid {
        field,
        value: value.to_string(),
    })
}

fn default_max_events() -> usize {
    1_000
}

fn default_max_lines() -> usize {
    50_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_adapters::IncrementalIngestReport;
    use crate::write_objects::plan_write_objects;

    #[test]
    fn generated_write_ids_are_valid_uuids() {
        let write_id = generated_write_id("00000000-0000-0000-0000-000000000001");

        Uuid::parse_str(&write_id).unwrap();
    }

    #[test]
    fn connector_key_sources_are_normalized_for_ingest_adapters() {
        assert_eq!(
            canonical_request_source("codex.local_sessions").unwrap(),
            "codex"
        );
        assert_eq!(
            canonical_request_source("claude.local_sessions").unwrap(),
            "claude"
        );
        assert_eq!(
            canonical_request_source("imessage.chat_db").unwrap(),
            "imessage"
        );
    }

    #[test]
    fn direct_perception_object_batches_use_fast_write_request() {
        let object = perception_object("agent/session/1");
        let batch = batch_with_objects(vec![object.clone()]);

        let prepared = prepare_batch_write_request(
            "00000000-0000-0000-0000-000000000001",
            "30000000-0000-0000-0000-000000000001",
            &batch,
        )
        .unwrap();

        assert_eq!(prepared.path, IngestWritePath::PerceptionObjects);
        assert_eq!(prepared.read_count, 1);
        assert_eq!(prepared.request.records, vec![object]);
        assert_eq!(prepared.request.atomicity.as_deref(), Some("chunk"));
        assert_eq!(
            prepared.request.chunk_size,
            Some(LOCAL_INGEST_WRITE_CHUNK_SIZE)
        );
    }

    #[test]
    fn direct_perception_object_batches_count_objects_only() {
        let batch = batch_with_objects(vec![
            perception_object("agent/session/direct"),
            perception_object("agent/message/direct"),
        ]);

        let prepared = prepare_batch_write_request(
            "00000000-0000-0000-0000-000000000001",
            "30000000-0000-0000-0000-000000000004",
            &batch,
        )
        .unwrap();

        assert_eq!(prepared.path, IngestWritePath::PerceptionObjects);
        assert_eq!(prepared.read_count, 2);
        assert_eq!(prepared.request.records.len(), 2);
    }

    #[test]
    fn direct_perception_object_batches_preserve_write_objects_dedupe() {
        let object = perception_object("agent/message/duplicate");
        let batch = batch_with_objects(vec![object.clone(), object]);
        let prepared = prepare_batch_write_request(
            "00000000-0000-0000-0000-000000000001",
            "30000000-0000-0000-0000-000000000002",
            &batch,
        )
        .unwrap();

        let plan = plan_write_objects(&prepared.request).unwrap();

        assert_eq!(plan.record_count, 2);
        assert_eq!(plan.leader_count, 1);
        assert_eq!(plan.same_batch_duplicate_count, 1);
        assert!(!plan.receipts[1].inserted);
        assert_eq!(
            plan.receipts[1].dedupe_reason.as_deref(),
            Some("same_batch")
        );
    }

    fn batch_with_objects(objects: Vec<PerceptionObjectInput>) -> IncrementalIngestBatch {
        let mut report = report();
        report.objects_emitted = objects.len();
        IncrementalIngestBatch {
            connector_key: "codex".to_string(),
            source: "codex".to_string(),
            report,
            perception_objects: objects,
        }
    }

    fn report() -> IncrementalIngestReport {
        IncrementalIngestReport {
            connector_key: "test".to_string(),
            files_seen: 1,
            files_with_new_bytes: 1,
            sqlite_rows_scanned: 0,
            lines_scanned: 1,
            bytes_read: 42,
            objects_emitted: 0,
            reached_event_limit: false,
            reached_line_limit: false,
            partial_line_deferred: false,
        }
    }

    fn perception_object(source_record_key: &str) -> PerceptionObjectInput {
        PerceptionObjectInput {
            client_record_id: None,
            source_id: "10000000-0000-0000-0000-000000000001".to_string(),
            source_record_key: source_record_key.to_string(),
            lane_id: "20000000-0000-0000-0000-000000000101".to_string(),
            series_kind: "codex_session".to_string(),
            series_key: "codex:session:test-session".to_string(),
            series_display_name: Some("test session".to_string()),
            series_parent_key: None,
            modality: Some("mixed".to_string()),
            kind: "agent_session".to_string(),
            role: "observation".to_string(),
            privacy_class: "normal".to_string(),
            event_start: "2026-05-25T10:00:00Z".to_string(),
            event_end: "2026-05-25T10:00:00.000001Z".to_string(),
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
            importance_score: Some(1.0),
            blob: None,
            body_type: Some("mixed".to_string()),
            text_value: None,
            number_value: None,
            bool_value: None,
            payload: json!({"session_id":"test-session"}),
            display_title: Some("Agent session".to_string()),
            display_text: Some("Agent session".to_string()),
            edges: Vec::new(),
            source_start_ns: None,
            source_end_ns: None,
            source_sequence: None,
            media_start_offset_ns: None,
            media_end_offset_ns: None,
            schema_name: Some("agent_session_ir.v1".to_string()),
            schema_version: Some(1),
            confidence: Some(1.0),
            metadata: json!({}),
        }
    }
}
