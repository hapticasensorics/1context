use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::agent_session_emitter::{
    emit_agent_session_objects_with_options, AgentSessionEmitOptions,
};
use crate::agent_session_ir::{AgentIngestProfile, AgentSessionIr};
use crate::claude_agent_ingest::parse_reduce_claude_jsonl;
use crate::codex_agent_ingest::compile_codex_rollout_jsonl;
use crate::source_connector::{ConnectorReadPosture, SourceLocationProbe, SourceProbeReport};
use crate::write_objects::PerceptionObjectInput;

const DEFAULT_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
const CODEX_STREAM_ID: &str = "10000000-0000-0000-0000-000000000001";
const CODEX_LANE_ID: &str = "20000000-0000-0000-0000-000000000001";
const CLAUDE_STREAM_ID: &str = "10000000-0000-0000-0000-000000000002";
const CLAUDE_LANE_ID: &str = "20000000-0000-0000-0000-000000000002";
const IMESSAGE_STREAM_ID: &str = "10000000-0000-0000-0000-000000000003";
const IMESSAGE_LANE_ID: &str = "20000000-0000-0000-0000-000000000003";

#[derive(Debug)]
pub enum AdapterError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Sqlite(rusqlite::Error),
    AgentSession(String),
    InvalidTimestamp(String),
    UnknownSource(String),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "io error: {error}"),
            Self::Json(error) => write!(formatter, "json error: {error}"),
            Self::Sqlite(error) => write!(formatter, "sqlite error: {error}"),
            Self::AgentSession(error) => write!(formatter, "agent session ingest error: {error}"),
            Self::InvalidTimestamp(value) => write!(formatter, "invalid timestamp: {value}"),
            Self::UnknownSource(value) => write!(formatter, "unknown source: {value}"),
        }
    }
}

impl std::error::Error for AdapterError {}

impl From<std::io::Error> for AdapterError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for AdapterError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<rusqlite::Error> for AdapterError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterContext {
    pub user_id: String,
    pub stream_id: String,
    pub lane_id: String,
    pub connector_key: String,
    pub privacy_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterSampleOptions {
    pub limit: usize,
    pub include_sensitive_text: bool,
    #[serde(default)]
    pub session_profile: SessionIngestProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncrementalIngestOptions {
    pub max_events: usize,
    pub max_lines: usize,
    pub include_sensitive_text: bool,
    #[serde(default)]
    pub session_profile: SessionIngestProfile,
}

impl Default for IncrementalIngestOptions {
    fn default() -> Self {
        Self {
            max_events: 1_000,
            max_lines: 50_000,
            include_sensitive_text: false,
            session_profile: SessionIngestProfile::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionIngestProfile {
    #[default]
    HotMemory,
    CompactAudit,
    Forensic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LocalIngestCursors {
    #[serde(default)]
    pub files: HashMap<String, FileIngestCursor>,
    #[serde(default)]
    pub sqlite: HashMap<String, SqliteIngestCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FileIngestCursor {
    pub offset: u64,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mtime_unix_ns: Option<u64>,
    #[serde(default)]
    pub parser_state: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SqliteIngestCursor {
    pub last_rowid: i64,
    #[serde(default)]
    pub last_source_date: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncrementalIngestBatch {
    pub connector_key: String,
    pub source: String,
    pub report: IncrementalIngestReport,
    #[serde(default)]
    pub perception_objects: Vec<PerceptionObjectInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncrementalIngestReport {
    pub connector_key: String,
    pub files_seen: usize,
    pub files_with_new_bytes: usize,
    pub sqlite_rows_scanned: usize,
    pub lines_scanned: usize,
    pub bytes_read: u64,
    pub objects_emitted: usize,
    pub reached_event_limit: bool,
    pub reached_line_limit: bool,
    pub partial_line_deferred: bool,
}

impl IncrementalIngestReport {
    fn new(connector_key: &str) -> Self {
        Self {
            connector_key: connector_key.to_string(),
            files_seen: 0,
            files_with_new_bytes: 0,
            sqlite_rows_scanned: 0,
            lines_scanned: 0,
            bytes_read: 0,
            objects_emitted: 0,
            reached_event_limit: false,
            reached_line_limit: false,
            partial_line_deferred: false,
        }
    }
}

pub fn default_home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn default_context_for_source(source: &str) -> Result<AdapterContext, AdapterError> {
    match source {
        "codex" => Ok(AdapterContext {
            user_id: DEFAULT_USER_ID.to_string(),
            stream_id: CODEX_STREAM_ID.to_string(),
            lane_id: CODEX_LANE_ID.to_string(),
            connector_key: "codex.local_sessions".to_string(),
            privacy_class: "normal".to_string(),
        }),
        "claude" => Ok(AdapterContext {
            user_id: DEFAULT_USER_ID.to_string(),
            stream_id: CLAUDE_STREAM_ID.to_string(),
            lane_id: CLAUDE_LANE_ID.to_string(),
            connector_key: "claude.local_sessions".to_string(),
            privacy_class: "normal".to_string(),
        }),
        "imessage" => Ok(AdapterContext {
            user_id: DEFAULT_USER_ID.to_string(),
            stream_id: IMESSAGE_STREAM_ID.to_string(),
            lane_id: IMESSAGE_LANE_ID.to_string(),
            connector_key: "imessage.chat_db".to_string(),
            privacy_class: "sensitive".to_string(),
        }),
        other => Err(AdapterError::UnknownSource(other.to_string())),
    }
}

pub fn probe_local_sources(home: &Path) -> Vec<SourceProbeReport> {
    vec![
        probe_path_source(
            "codex.local_sessions",
            ConnectorReadPosture::StableLocalRecord,
            home.join(".codex/sessions"),
            "session_root",
            "canonical",
        ),
        probe_path_source(
            "claude.local_sessions",
            ConnectorReadPosture::StableLocalRecord,
            home.join(".claude/projects"),
            "session_root",
            "canonical",
        ),
        probe_path_source(
            "imessage.chat_db",
            ConnectorReadPosture::StableLocalRecord,
            home.join("Library/Messages/chat.db"),
            "sqlite_db",
            "canonical",
        ),
    ]
}

pub fn sample_codex_objects(
    home: &Path,
    context: &AdapterContext,
    options: &AdapterSampleOptions,
) -> Result<Vec<PerceptionObjectInput>, AdapterError> {
    let mut cursors = LocalIngestCursors::default();
    Ok(ingest_codex_incremental(
        home,
        context,
        &IncrementalIngestOptions {
            max_events: options.limit,
            max_lines: usize::MAX,
            include_sensitive_text: options.include_sensitive_text,
            session_profile: options.session_profile,
        },
        &mut cursors,
    )?
    .perception_objects)
}

pub fn sample_claude_objects(
    home: &Path,
    context: &AdapterContext,
    options: &AdapterSampleOptions,
) -> Result<Vec<PerceptionObjectInput>, AdapterError> {
    let mut cursors = LocalIngestCursors::default();
    Ok(ingest_claude_incremental(
        home,
        context,
        &IncrementalIngestOptions {
            max_events: options.limit,
            max_lines: usize::MAX,
            include_sensitive_text: options.include_sensitive_text,
            session_profile: options.session_profile,
        },
        &mut cursors,
    )?
    .perception_objects)
}

pub fn ingest_codex_incremental(
    home: &Path,
    context: &AdapterContext,
    options: &IncrementalIngestOptions,
    cursors: &mut LocalIngestCursors,
) -> Result<IncrementalIngestBatch, AdapterError> {
    let files = collect_files_matching(&home.join(".codex/sessions"), |path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
    })?;
    ingest_agent_jsonl_incremental(&files, AgentJsonlSource::Codex, context, options, cursors)
}

pub fn ingest_claude_incremental(
    home: &Path,
    context: &AdapterContext,
    options: &IncrementalIngestOptions,
    cursors: &mut LocalIngestCursors,
) -> Result<IncrementalIngestBatch, AdapterError> {
    let files = collect_files_matching(&home.join(".claude/projects"), |path| {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "jsonl")
    })?;
    ingest_agent_jsonl_incremental(&files, AgentJsonlSource::Claude, context, options, cursors)
}

pub fn ingest_imessage_incremental(
    home: &Path,
    context: &AdapterContext,
    options: &IncrementalIngestOptions,
    cursors: &mut LocalIngestCursors,
) -> Result<IncrementalIngestBatch, AdapterError> {
    let db_path = home.join("Library/Messages/chat.db");
    let cursor_key = db_path.to_string_lossy().to_string();
    let sqlite_cursor = cursors.sqlite.entry(cursor_key.clone()).or_default();
    let connection = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    let mut statement = connection.prepare(
        r#"
        SELECT
          m.ROWID,
          m.guid,
          m.text,
          m.date,
          m.is_from_me,
          m.service,
          m.cache_has_attachments,
          h.id AS handle_id,
          c.guid AS chat_guid,
          c.display_name AS chat_display_name,
          c.chat_identifier AS chat_identifier
        FROM message m
        LEFT JOIN handle h ON h.ROWID = m.handle_id
        LEFT JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
        LEFT JOIN chat c ON c.ROWID = cmj.chat_id
        WHERE m.ROWID > ?1
          AND m.date IS NOT NULL
          AND (m.text IS NOT NULL OR m.cache_has_attachments = 1)
        GROUP BY m.ROWID
        ORDER BY m.ROWID ASC
        LIMIT ?2
        "#,
    )?;

    let rows = statement.query_map(
        params![sqlite_cursor.last_rowid, options.max_events as i64],
        |row| {
            Ok(ImessageRow {
                rowid: row.get(0)?,
                guid: row.get(1)?,
                text: row.get(2)?,
                date: row.get(3)?,
                is_from_me: row.get::<_, i64>(4)? != 0,
                service: row.get(5)?,
                cache_has_attachments: row.get::<_, i64>(6)? != 0,
                handle_id: row.get(7)?,
                chat_guid: row.get(8)?,
                chat_display_name: row.get(9)?,
                chat_identifier: row.get(10)?,
            })
        },
    )?;

    let mut perception_objects = Vec::new();
    let mut report = IncrementalIngestReport::new(&context.connector_key);
    for row in rows {
        let row = row?;
        report.sqlite_rows_scanned += 1;
        sqlite_cursor.last_rowid = row.rowid;
        sqlite_cursor.last_source_date = Some(row.date);
        perception_objects.push(imessage_object_from_row(
            row,
            &db_path,
            context,
            options.include_sensitive_text,
        )?);
    }
    report.objects_emitted = perception_objects.len();
    report.reached_event_limit =
        options.max_events > 0 && perception_objects.len() >= options.max_events;

    Ok(IncrementalIngestBatch {
        connector_key: context.connector_key.clone(),
        source: "imessage".to_string(),
        report,
        perception_objects,
    })
}

pub fn sample_imessage_objects(
    home: &Path,
    context: &AdapterContext,
    options: &AdapterSampleOptions,
) -> Result<Vec<PerceptionObjectInput>, AdapterError> {
    let db_path = home.join("Library/Messages/chat.db");
    let connection = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    let mut statement = connection.prepare(
        r#"
        SELECT
          m.ROWID,
          m.guid,
          m.text,
          m.date,
          m.is_from_me,
          m.service,
          m.cache_has_attachments,
          h.id AS handle_id,
          c.guid AS chat_guid,
          c.display_name AS chat_display_name,
          c.chat_identifier AS chat_identifier
        FROM message m
        LEFT JOIN handle h ON h.ROWID = m.handle_id
        LEFT JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
        LEFT JOIN chat c ON c.ROWID = cmj.chat_id
        WHERE m.date IS NOT NULL
          AND (m.text IS NOT NULL OR m.cache_has_attachments = 1)
        GROUP BY m.ROWID
        ORDER BY m.date DESC, m.ROWID DESC
        LIMIT ?1
        "#,
    )?;

    let rows = statement.query_map(params![options.limit as i64], |row| {
        Ok(ImessageRow {
            rowid: row.get(0)?,
            guid: row.get(1)?,
            text: row.get(2)?,
            date: row.get(3)?,
            is_from_me: row.get::<_, i64>(4)? != 0,
            service: row.get(5)?,
            cache_has_attachments: row.get::<_, i64>(6)? != 0,
            handle_id: row.get(7)?,
            chat_guid: row.get(8)?,
            chat_display_name: row.get(9)?,
            chat_identifier: row.get(10)?,
        })
    })?;

    let mut perception_objects = Vec::new();
    for row in rows {
        let row = row?;
        perception_objects.push(imessage_object_from_row(
            row,
            &db_path,
            context,
            options.include_sensitive_text,
        )?);
    }
    Ok(perception_objects)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentJsonlSource {
    Codex,
    Claude,
}

impl AgentJsonlSource {
    fn source_name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    fn compile_session(
        self,
        source_uri: &str,
        jsonl: &str,
        profile: AgentIngestProfile,
    ) -> Result<AgentSessionIr, AdapterError> {
        match self {
            Self::Codex => compile_codex_rollout_jsonl(source_uri, jsonl, profile)
                .map_err(|error| AdapterError::AgentSession(error.to_string())),
            Self::Claude => parse_reduce_claude_jsonl(source_uri, jsonl, profile)
                .map_err(|error| AdapterError::AgentSession(error.to_string())),
        }
    }
}

fn ingest_agent_jsonl_incremental(
    files: &[PathBuf],
    source: AgentJsonlSource,
    context: &AdapterContext,
    options: &IncrementalIngestOptions,
    cursors: &mut LocalIngestCursors,
) -> Result<IncrementalIngestBatch, AdapterError> {
    let mut report = IncrementalIngestReport::new(&context.connector_key);
    let mut perception_objects = Vec::new();
    report.files_seen = files.len();

    for path in files.iter().rev() {
        if perception_objects.len() >= options.max_events {
            report.reached_event_limit = true;
            break;
        }
        if report.lines_scanned >= options.max_lines {
            report.reached_line_limit = true;
            break;
        }

        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let cursor_key = path.to_string_lossy().to_string();
        let cursor = cursors.files.entry(cursor_key).or_default();
        let size = metadata.len();
        if size < cursor.offset {
            cursor.offset = 0;
            cursor.parser_state.clear();
        }
        cursor.size = size;
        cursor.mtime_unix_ns = mtime_unix_ns(&metadata);
        if size <= cursor.offset {
            continue;
        }

        report.files_with_new_bytes += 1;
        let jsonl = read_agent_jsonl_incremental_file(path, options, cursor, &mut report)?;
        if jsonl.trim().is_empty() {
            continue;
        }

        let profile = AgentIngestProfile::from(options.session_profile);
        let session = source.compile_session(&path.to_string_lossy(), &jsonl, profile)?;
        let emit_options = AgentSessionEmitOptions {
            privacy_class: context.privacy_class.clone(),
            include_tool_summaries: profile != AgentIngestProfile::HotMemory,
            ..AgentSessionEmitOptions::default()
        };
        let mut emitted = emit_agent_session_objects_with_options(&session, &emit_options)
            .map_err(|error| AdapterError::AgentSession(error.to_string()))?;
        perception_objects.append(&mut emitted);
    }

    report.objects_emitted = perception_objects.len();
    if perception_objects.len() >= options.max_events {
        report.reached_event_limit = true;
    }
    if report.lines_scanned >= options.max_lines {
        report.reached_line_limit = true;
    }

    Ok(IncrementalIngestBatch {
        connector_key: context.connector_key.clone(),
        source: source.source_name().to_string(),
        report,
        perception_objects,
    })
}

fn read_agent_jsonl_incremental_file(
    path: &Path,
    options: &IncrementalIngestOptions,
    cursor: &mut FileIngestCursor,
    report: &mut IncrementalIngestReport,
) -> Result<String, AdapterError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(cursor.offset))?;

    let mut jsonl = String::new();
    let mut line = Vec::new();
    loop {
        if report.lines_scanned >= options.max_lines {
            report.reached_line_limit = true;
            break;
        }

        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line)?;
        if bytes_read == 0 {
            break;
        }

        if !line.ends_with(b"\n") {
            report.partial_line_deferred = true;
            break;
        }

        cursor.offset = cursor.offset.saturating_add(bytes_read as u64);
        report.bytes_read = report.bytes_read.saturating_add(bytes_read as u64);
        report.lines_scanned += 1;

        let Ok(line_text) = std::str::from_utf8(&line) else {
            continue;
        };
        let line_text = line_text.trim_end_matches(['\r', '\n']);
        if line_text.trim().is_empty() {
            continue;
        }
        if serde_json::from_str::<Value>(line_text).is_err() {
            continue;
        }
        jsonl.push_str(line_text);
        jsonl.push('\n');
    }

    cursor.size = fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(cursor.size);
    Ok(jsonl)
}

fn imessage_object_from_row(
    row: ImessageRow,
    db_path: &Path,
    context: &AdapterContext,
    include_sensitive_text: bool,
) -> Result<PerceptionObjectInput, AdapterError> {
    let event_start = imessage_timestamp_to_rfc3339(row.date)?;
    let event_end = plus_one_microsecond(&event_start)?;
    let text = row.text.unwrap_or_default();
    let display_text = if include_sensitive_text {
        Some(clamp_text(&text))
    } else {
        None
    };
    let source_hash = stable_hash(
        &json!({
            "guid": row.guid,
            "date": row.date,
            "text": text,
        })
        .to_string(),
    );
    let chat_key = row
        .chat_guid
        .as_deref()
        .or(row.chat_identifier.as_deref())
        .or(row.handle_id.as_deref())
        .unwrap_or("unknown");
    let chat_display_name = row
        .chat_display_name
        .clone()
        .or_else(|| row.chat_identifier.clone())
        .or_else(|| row.handle_id.clone());
    let payload = json!({
        "connector_key": context.connector_key,
        "source": "imessage",
        "source_path": db_path.to_string_lossy(),
        "rowid": row.rowid,
        "guid": row.guid,
        "service": row.service,
        "direction": if row.is_from_me { "sent" } else { "received" },
        "handle_id": row.handle_id,
        "chat_guid": row.chat_guid,
        "chat_display_name": row.chat_display_name,
        "chat_identifier": row.chat_identifier,
        "cache_has_attachments": row.cache_has_attachments,
        "text_redacted": !include_sensitive_text,
        "text_char_count": text.chars().count(),
        "source_hash": source_hash,
    });
    Ok(PerceptionObjectInput {
        client_record_id: None,
        source_id: context.stream_id.clone(),
        source_record_key: row.guid,
        lane_id: context.lane_id.clone(),
        series_kind: "imessage_thread".to_string(),
        series_key: format!("imessage:thread:{chat_key}"),
        series_display_name: chat_display_name,
        series_parent_key: None,
        modality: Some("text".to_string()),
        kind: "imessage_message".to_string(),
        role: "observation".to_string(),
        privacy_class: context.privacy_class.clone(),
        event_start,
        event_end,
        time_semantics: "interval".to_string(),
        temporal_level: "event".to_string(),
        native_resolution_ns: None,
        stored_resolution_ns: None,
        indexed_resolution_ns: None,
        display_resolution_hint_ns: None,
        time_resolution_ns: None,
        time_uncertainty_ns: None,
        alignment_confidence: Some(0.95),
        alignment_method: Some("imessage.chat_db.rowid".to_string()),
        materialization_policy: "index_events".to_string(),
        importance_score: Some(0.95),
        blob: None,
        body_type: Some("text".to_string()),
        text_value: Some(clamp_text(&text)),
        number_value: None,
        bool_value: None,
        payload,
        display_title: Some(if row.is_from_me {
            "iMessage sent".to_string()
        } else {
            "iMessage received".to_string()
        }),
        display_text,
        edges: Vec::new(),
        source_start_ns: Some(row.date),
        source_end_ns: None,
        source_sequence: Some(row.rowid),
        media_start_offset_ns: None,
        media_end_offset_ns: None,
        schema_name: Some("imessage.chat_db.message".to_string()),
        schema_version: Some(1),
        confidence: Some(0.95),
        metadata: json!({
            "writer": {
                "name": "onecontext-memory-db.local_adapters.imessage",
                "schema_version": 1,
            },
            "source": {
                "connector_key": context.connector_key,
                "source_uri": db_path.to_string_lossy(),
                "adapter_source_hash": source_hash,
            },
            "lane": {
                "lane_key": "messages.imessage"
            }
        }),
    })
}

fn collect_files_matching(
    root: &Path,
    predicate: fn(&Path) -> bool,
) -> Result<Vec<PathBuf>, AdapterError> {
    let mut files = Vec::new();
    collect_files_recursive(root, predicate, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_recursive(
    root: &Path,
    predicate: fn(&Path) -> bool,
    files: &mut Vec<PathBuf>,
) -> Result<(), AdapterError> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files_recursive(&path, predicate, files)?;
        } else if file_type.is_file() && predicate(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn probe_path_source(
    connector_key: &str,
    read_posture: ConnectorReadPosture,
    path: PathBuf,
    location_kind: &str,
    canonicality: &str,
) -> SourceProbeReport {
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => json!({
            "is_file": metadata.is_file(),
            "is_dir": metadata.is_dir(),
            "bytes": metadata.len(),
        }),
        Err(error) => json!({
            "error": error.to_string(),
        }),
    };
    let readable = fs::File::open(&path).is_ok() || fs::read_dir(&path).is_ok();
    let status = if path.exists() {
        if readable {
            "found"
        } else {
            "blocked"
        }
    } else {
        "not_found"
    };
    SourceProbeReport {
        connector_key: connector_key.to_string(),
        status: status.to_string(),
        readable,
        read_posture,
        discovered_locations: vec![SourceLocationProbe {
            location_kind: location_kind.to_string(),
            uri: path.to_string_lossy().to_string(),
            canonicality: canonicality.to_string(),
            readable,
            metadata,
        }],
        diagnostics: Value::Object(Default::default()),
    }
}

fn mtime_unix_ns(metadata: &fs::Metadata) -> Option<u64> {
    let duration = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(
        duration
            .as_secs()
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::from(duration.subsec_nanos())),
    )
}

fn plus_one_microsecond(timestamp: &str) -> Result<String, AdapterError> {
    let parsed = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| AdapterError::InvalidTimestamp(timestamp.to_string()))?
        .with_timezone(&Utc);
    Ok((parsed + Duration::microseconds(1)).to_rfc3339_opts(SecondsFormat::Micros, true))
}

fn imessage_timestamp_to_rfc3339(value: i64) -> Result<String, AdapterError> {
    let apple_epoch = Utc
        .with_ymd_and_hms(2001, 1, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| AdapterError::InvalidTimestamp(value.to_string()))?;
    let timestamp = if value.abs() > 100_000_000_000 {
        apple_epoch + Duration::nanoseconds(value)
    } else {
        apple_epoch + Duration::seconds(value)
    };
    Ok(timestamp.to_rfc3339_opts(SecondsFormat::Micros, true))
}

fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    to_hex(&hasher.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn clamp_text(text: &str) -> String {
    const LIMIT: usize = 32_000;
    text.chars().take(LIMIT).collect()
}

#[derive(Debug)]
struct ImessageRow {
    rowid: i64,
    guid: String,
    text: Option<String>,
    date: i64,
    is_from_me: bool,
    service: Option<String>,
    cache_has_attachments: bool,
    handle_id: Option<String>,
    chat_guid: Option<String>,
    chat_display_name: Option<String>,
    chat_identifier: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    #[test]
    fn imessage_nanoseconds_convert_to_utc() {
        assert_eq!(
            imessage_timestamp_to_rfc3339(0).unwrap(),
            "2001-01-01T00:00:00.000000Z"
        );
        assert_eq!(
            imessage_timestamp_to_rfc3339(1).unwrap(),
            "2001-01-01T00:00:01.000000Z"
        );
        assert_eq!(
            imessage_timestamp_to_rfc3339(1_000_000_000_000).unwrap(),
            "2001-01-01T00:16:40.000000Z"
        );
    }

    #[test]
    fn probe_reports_missing_paths_without_error() {
        let reports = probe_local_sources(Path::new("/definitely/missing/home"));
        assert_eq!(reports.len(), 3);
        assert!(reports.iter().all(|report| report.status == "not_found"));
    }

    #[test]
    fn codex_incremental_ingest_emits_agent_objects_and_resumes() {
        let temp_home = std::env::temp_dir().join(format!(
            "onecontext-memory-db-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let sessions_dir = temp_home.join(".codex/sessions/2026/05/24");
        fs::create_dir_all(&sessions_dir).unwrap();
        let path = sessions_dir
            .join("rollout-2026-05-24T00-17-33-019e58d8-a3b8-7632-ba5d-b3ca2fa73c6c.jsonl");
        fs::write(
            &path,
            [
                r#"{"timestamp":"2026-05-24T07:17:38.000Z","type":"session_meta","payload":{"id":"s1","cwd":"/tmp/project"}}"#,
                r#"{"timestamp":"2026-05-24T07:17:39.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"}]}}"#,
                r#"{"timestamp":"2026-05-24T07:17:40.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"call-1","arguments":"{\"cmd\":\"cargo test\",\"workdir\":\"/tmp/project\"}"}}"#,
                "",
            ]
            .join("\n"),
        )
        .unwrap();

        let context = default_context_for_source("codex").unwrap();
        let options = IncrementalIngestOptions {
            max_events: 10,
            max_lines: 100,
            include_sensitive_text: false,
            session_profile: SessionIngestProfile::HotMemory,
        };
        let mut cursors = LocalIngestCursors::default();
        let first = ingest_codex_incremental(&temp_home, &context, &options, &mut cursors).unwrap();
        assert_eq!(
            kind_count(&first.perception_objects, "agent_tool_summary"),
            0
        );
        assert_eq!(kind_count(&first.perception_objects, "agent_message"), 1);
        assert!(first
            .perception_objects
            .iter()
            .any(|record| record.display_text.as_deref() == Some("hello")));
        assert!(first.perception_objects.iter().all(|record| {
            record.series_kind == "codex_session" && record.series_key == "codex:session:s1"
        }));
        let codex_message = first
            .perception_objects
            .iter()
            .find(|record| record.kind == "agent_message")
            .unwrap();
        assert_eq!(codex_message.body_type.as_deref(), Some("text"));
        assert_eq!(codex_message.text_value.as_deref(), Some("hello"));
        assert_eq!(first.report.lines_scanned, 3);
        assert_eq!(first.report.objects_emitted, first.perception_objects.len());

        let second =
            ingest_codex_incremental(&temp_home, &context, &options, &mut cursors).unwrap();
        assert_eq!(second.perception_objects.len(), 0);

        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(
            file,
            "{}",
            r#"{"timestamp":"2026-05-24T07:17:41.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"second"}]}}"#
        )
        .unwrap();

        let third = ingest_codex_incremental(&temp_home, &context, &options, &mut cursors).unwrap();
        assert!(third
            .perception_objects
            .iter()
            .any(|record| record.kind == "agent_message"
                && record.display_text.as_deref() == Some("second")));

        let mut full_cursors = LocalIngestCursors::default();
        let with_tools = ingest_codex_incremental(
            &temp_home,
            &context,
            &IncrementalIngestOptions {
                session_profile: SessionIngestProfile::CompactAudit,
                ..options
            },
            &mut full_cursors,
        )
        .unwrap();
        assert!(with_tools
            .perception_objects
            .iter()
            .any(|record| record.kind == "agent_tool_summary"
                && record
                    .display_text
                    .as_deref()
                    .is_some_and(|text| text.contains("[tool-call:exec_command]"))));

        let _ = fs::remove_dir_all(temp_home);
    }

    #[test]
    fn claude_incremental_ingest_maps_profiles_to_agent_objects() {
        let temp_home = std::env::temp_dir().join(format!(
            "onecontext-memory-db-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let sessions_dir = temp_home.join(".claude/projects/-tmp-project");
        fs::create_dir_all(&sessions_dir).unwrap();
        let path = sessions_dir.join("session.jsonl");
        fs::write(
            &path,
            [
                r#"{"type":"user","uuid":"u1","sessionId":"s1","cwd":"/tmp/project","timestamp":"2026-05-24T07:17:38.000Z","message":{"role":"user","content":[{"type":"text","text":"inspect"}]}}"#,
                r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"s1","cwd":"/tmp/project","timestamp":"2026-05-24T07:17:39.000Z","message":{"role":"assistant","content":[{"type":"text","text":"reading"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"/tmp/project/src/lib.rs"}}]}}"#,
                r#"{"type":"user","uuid":"u2","parentUuid":"a1","sessionId":"s1","cwd":"/tmp/project","timestamp":"2026-05-24T07:17:40.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"pub fn hello() {}"}]}}"#,
                "",
            ]
            .join("\n"),
        )
        .unwrap();

        let context = default_context_for_source("claude").unwrap();
        let options = IncrementalIngestOptions {
            max_events: 10,
            max_lines: 100,
            include_sensitive_text: false,
            session_profile: SessionIngestProfile::HotMemory,
        };
        let mut cursors = LocalIngestCursors::default();
        let hot = ingest_claude_incremental(&temp_home, &context, &options, &mut cursors).unwrap();
        assert_eq!(kind_count(&hot.perception_objects, "agent_tool_summary"), 0);
        assert_eq!(kind_count(&hot.perception_objects, "agent_message"), 2);
        assert!(hot.perception_objects.iter().all(|record| {
            record.series_kind == "claude_session" && record.series_key == "claude:session:s1"
        }));
        assert!(hot.perception_objects.iter().any(|record| {
            record.kind == "agent_message"
                && record.body_type.as_deref() == Some("text")
                && record.text_value.as_deref() == Some("inspect")
        }));

        let second =
            ingest_claude_incremental(&temp_home, &context, &options, &mut cursors).unwrap();
        assert_eq!(second.perception_objects.len(), 0);

        let mut audit_cursors = LocalIngestCursors::default();
        let audit = ingest_claude_incremental(
            &temp_home,
            &context,
            &IncrementalIngestOptions {
                session_profile: SessionIngestProfile::CompactAudit,
                ..options
            },
            &mut audit_cursors,
        )
        .unwrap();
        assert_eq!(
            kind_count(&audit.perception_objects, "agent_tool_summary"),
            2
        );
        assert!(audit
            .perception_objects
            .iter()
            .any(|record| record.kind == "agent_tool_summary"
                && record.display_text.as_deref() == Some("[Read /tmp/project/src/lib.rs]")));

        let _ = fs::remove_dir_all(temp_home);
    }

    #[test]
    fn imessage_rows_emit_thread_series_and_text_body() {
        let context = default_context_for_source("imessage").unwrap();
        let record = imessage_object_from_row(
            ImessageRow {
                rowid: 42,
                guid: "message-guid".to_string(),
                text: Some("Dinner at 7?".to_string()),
                date: 1,
                is_from_me: false,
                service: Some("iMessage".to_string()),
                cache_has_attachments: false,
                handle_id: Some("+15551234567".to_string()),
                chat_guid: Some("chat-guid".to_string()),
                chat_display_name: Some("Family".to_string()),
                chat_identifier: Some("family-chat".to_string()),
            },
            Path::new("/tmp/chat.db"),
            &context,
            false,
        )
        .unwrap();

        assert_eq!(record.series_kind, "imessage_thread");
        assert_eq!(record.series_key, "imessage:thread:chat-guid");
        assert_eq!(record.series_display_name.as_deref(), Some("Family"));
        assert_eq!(
            record.schema_name.as_deref(),
            Some("imessage.chat_db.message")
        );
        assert_eq!(record.metadata["lane"]["lane_key"], "messages.imessage");
        assert_eq!(record.modality.as_deref(), Some("text"));
        assert_eq!(record.body_type.as_deref(), Some("text"));
        assert_eq!(record.text_value.as_deref(), Some("Dinner at 7?"));
        assert_eq!(record.display_text, None);
    }

    fn kind_count(records: &[PerceptionObjectInput], kind: &str) -> usize {
        records.iter().filter(|record| record.kind == kind).count()
    }
}
