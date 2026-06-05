use std::collections::HashMap;
use std::io::Write;

use chrono::{DateTime, SecondsFormat, Utc};
use postgres::{Client, GenericClient, NoTls};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::envelope::EnvelopeValidationError;
use crate::source_identity::{
    canonical_source_hash as canonical_json_source_hash, object_id as source_object_id,
    source_record_id as source_identity_record_id,
};

pub const DEFAULT_WRITE_OBJECTS_CHUNK_SIZE: usize = 2_000;

#[derive(Debug)]
pub enum WriteObjectsError {
    Envelope(EnvelopeValidationError),
    Database(postgres::Error),
    Io(std::io::Error),
    InvalidRecord {
        field: &'static str,
        value: String,
        reason: String,
    },
    SourceIdentityHashConflictInBatch {
        source_id: String,
        source_record_key: String,
        first_ordinal: usize,
        conflicting_ordinal: usize,
    },
    SourceIdentityHashConflict {
        source_id: String,
        source_record_key: String,
        existing_hash: String,
        competing_hash: String,
    },
    ResultCountMismatch {
        expected: usize,
        actual: usize,
    },
}

impl WriteObjectsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Envelope(_) | Self::InvalidRecord { .. } => "INVALID_RECORD",
            Self::Database(_) | Self::Io(_) | Self::ResultCountMismatch { .. } => "DB_WRITE_FAILED",
            Self::SourceIdentityHashConflictInBatch { .. } => {
                "SOURCE_IDENTITY_HASH_CONFLICT_IN_BATCH"
            }
            Self::SourceIdentityHashConflict { .. } => "SOURCE_IDENTITY_HASH_CONFLICT",
        }
    }
}

impl std::fmt::Display for WriteObjectsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Envelope(error) => write!(formatter, "invalid capture envelope: {error}"),
            Self::Database(error) => write!(formatter, "postgres write error: {error:?}"),
            Self::Io(error) => write!(formatter, "postgres copy stream error: {error}"),
            Self::InvalidRecord {
                field,
                value,
                reason,
            } => write!(formatter, "invalid {field} value {value:?}: {reason}"),
            Self::SourceIdentityHashConflictInBatch {
                source_id,
                source_record_key,
                first_ordinal,
                conflicting_ordinal,
            } => write!(
                formatter,
                "source identity {source_id}/{source_record_key} has conflicting hashes in one batch at ordinals {first_ordinal} and {conflicting_ordinal}"
            ),
            Self::SourceIdentityHashConflict {
                source_id,
                source_record_key,
                ..
            } => write!(
                formatter,
                "source identity {source_id}/{source_record_key} already exists with a different canonical hash"
            ),
            Self::ResultCountMismatch { expected, actual } => write!(
                formatter,
                "writeObjects returned {actual} receipts for {expected} input records"
            ),
        }
    }
}

impl std::error::Error for WriteObjectsError {}

impl From<EnvelopeValidationError> for WriteObjectsError {
    fn from(value: EnvelopeValidationError) -> Self {
        Self::Envelope(value)
    }
}

impl From<postgres::Error> for WriteObjectsError {
    fn from(value: postgres::Error) -> Self {
        Self::Database(value)
    }
}

impl From<std::io::Error> for WriteObjectsError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteObjectsRequest {
    pub user_id: String,
    pub write_id: String,
    #[serde(default)]
    pub atomicity: Option<String>,
    #[serde(default)]
    pub records: Vec<PerceptionObjectInput>,
    #[serde(default)]
    pub chunk_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionObjectInput {
    #[serde(default)]
    pub client_record_id: Option<String>,
    pub source_id: String,
    pub source_record_key: String,
    pub lane_id: String,
    #[serde(default)]
    pub series_kind: String,
    #[serde(default)]
    pub series_key: String,
    #[serde(default)]
    pub series_display_name: Option<String>,
    #[serde(default)]
    pub series_parent_key: Option<String>,
    #[serde(default)]
    pub modality: Option<String>,
    pub kind: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_privacy_class")]
    pub privacy_class: String,
    pub event_start: String,
    pub event_end: String,
    #[serde(default = "default_time_semantics")]
    pub time_semantics: String,
    #[serde(default = "default_temporal_level")]
    pub temporal_level: String,
    #[serde(default)]
    pub native_resolution_ns: Option<i64>,
    #[serde(default)]
    pub stored_resolution_ns: Option<i64>,
    #[serde(default)]
    pub indexed_resolution_ns: Option<i64>,
    #[serde(default)]
    pub display_resolution_hint_ns: Option<i64>,
    #[serde(default)]
    pub time_resolution_ns: Option<i64>,
    #[serde(default)]
    pub time_uncertainty_ns: Option<i64>,
    #[serde(default)]
    pub alignment_confidence: Option<f32>,
    #[serde(default)]
    pub alignment_method: Option<String>,
    #[serde(default = "default_materialization_policy")]
    pub materialization_policy: String,
    #[serde(default)]
    pub importance_score: Option<f32>,
    #[serde(default)]
    pub blob: Option<BlobDescriptor>,
    #[serde(default)]
    pub body_type: Option<String>,
    #[serde(default)]
    pub text_value: Option<String>,
    #[serde(default)]
    pub number_value: Option<f64>,
    #[serde(default)]
    pub bool_value: Option<bool>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub display_title: Option<String>,
    #[serde(default)]
    pub display_text: Option<String>,
    #[serde(default)]
    pub edges: Vec<PerceptionEdgeInput>,
    #[serde(default)]
    pub source_start_ns: Option<i64>,
    #[serde(default)]
    pub source_end_ns: Option<i64>,
    #[serde(default)]
    pub source_sequence: Option<i64>,
    #[serde(default)]
    pub media_start_offset_ns: Option<i64>,
    #[serde(default)]
    pub media_end_offset_ns: Option<i64>,
    #[serde(default)]
    pub schema_name: Option<String>,
    #[serde(default)]
    pub schema_version: Option<i32>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlobDescriptor {
    #[serde(default)]
    pub blob_id: Option<String>,
    pub storage_backend: String,
    pub uri: String,
    #[serde(default)]
    pub safe_uri: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub byte_count: Option<i64>,
    pub content_type: String,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub width: Option<i32>,
    #[serde(default)]
    pub height: Option<i32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionEdgeInput {
    pub to_object_id: String,
    pub edge_kind: String,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRecordReceipt {
    #[serde(default)]
    pub client_record_id: Option<String>,
    pub source_id: String,
    pub source_record_key: String,
    pub source_record_id: String,
    pub object_id: String,
    pub event_start: String,
    pub inserted: bool,
    #[serde(default)]
    pub dedupe_reason: Option<String>,
    #[serde(default)]
    pub duplicate_of_ordinal: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteObjectsResponse {
    pub ok: bool,
    pub write_id: String,
    pub chunk_count: usize,
    pub record_count: usize,
    pub inserted_count: usize,
    pub duplicate_count: usize,
    pub receipts: Vec<SourceRecordReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteObjectsPlanSummary {
    pub chunk_count: usize,
    pub record_count: usize,
    pub leader_count: usize,
    pub same_batch_duplicate_count: usize,
    pub receipts: Vec<SourceRecordReceipt>,
}

pub struct PerceptionWriteObjectsWriter {
    client: Client,
}

impl PerceptionWriteObjectsWriter {
    pub fn connect(database_url: &str) -> Result<Self, WriteObjectsError> {
        let client = Client::connect(database_url, NoTls)?;
        Ok(Self::new(client))
    }

    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn into_inner(self) -> Client {
        self.client
    }

    pub fn write_objects(
        &mut self,
        request: &WriteObjectsRequest,
    ) -> Result<WriteObjectsResponse, WriteObjectsError> {
        write_objects_with_client(&mut self.client, request)
    }
}

pub fn write_objects_with_client(
    client: &mut Client,
    request: &WriteObjectsRequest,
) -> Result<WriteObjectsResponse, WriteObjectsError> {
    let prepared = prepare_write_objects(request)?;
    let mut receipts = prepared.receipts;
    let chunk_count = prepared.chunk_count;
    let mut inserted_count = 0_usize;

    for chunk in prepared.leaders.chunks(prepared.chunk_size) {
        let mut transaction = client.transaction()?;
        let chunk_receipts = match write_prepared_chunk(&mut transaction, chunk) {
            Ok(chunk_receipts) => chunk_receipts,
            Err(error @ WriteObjectsError::SourceIdentityHashConflict { .. }) => {
                transaction.commit()?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        transaction.commit()?;
        inserted_count += chunk_receipts
            .iter()
            .filter(|receipt| receipt.inserted)
            .count();
        for receipt in chunk_receipts {
            let ordinal = receipt.ordinal;
            receipts[ordinal].object_id = receipt.object_id;
            receipts[ordinal].event_start = receipt.event_start;
            receipts[ordinal].inserted = receipt.inserted;
            receipts[ordinal].dedupe_reason = receipt.dedupe_reason;
        }
    }

    if receipts.len() != request.records.len() {
        return Err(WriteObjectsError::ResultCountMismatch {
            expected: request.records.len(),
            actual: receipts.len(),
        });
    }
    let duplicate_count = receipts.len().saturating_sub(inserted_count);
    Ok(WriteObjectsResponse {
        ok: true,
        write_id: request.write_id.clone(),
        chunk_count,
        record_count: request.records.len(),
        inserted_count,
        duplicate_count,
        receipts,
    })
}

pub fn plan_write_objects(
    request: &WriteObjectsRequest,
) -> Result<WriteObjectsPlanSummary, WriteObjectsError> {
    let prepared = prepare_write_objects(request)?;
    Ok(WriteObjectsPlanSummary {
        chunk_count: prepared.chunk_count,
        record_count: request.records.len(),
        leader_count: prepared.leaders.len(),
        same_batch_duplicate_count: request.records.len().saturating_sub(prepared.leaders.len()),
        receipts: prepared.receipts,
    })
}

pub fn canonical_source_hash(record: &PerceptionObjectInput) -> Result<String, WriteObjectsError> {
    let source_id = parse_uuid("source_id", &record.source_id)?;
    let lane_id = parse_uuid("lane_id", &record.lane_id)?;
    let event_start = parse_timestamp("event_start", &record.event_start)?;
    let event_end = parse_timestamp("event_end", &record.event_end)?;
    let canonical = json!({
        "source_id": source_id.to_string(),
        "source_record_key": record.source_record_key,
        "kind": record.kind,
        "series_kind": record.series_kind,
        "series_key": record.series_key,
        "series_display_name": record.series_display_name,
        "series_parent_key": record.series_parent_key,
        "modality": normalized_modality(record)?,
        "body_type": normalized_body_type(record)?,
        "text_value": record.text_value,
        "number_value": record.number_value,
        "bool_value": record.bool_value,
        "event_start": event_start.to_rfc3339_opts(SecondsFormat::Micros, true),
        "event_end": event_end.to_rfc3339_opts(SecondsFormat::Micros, true),
        "lane_id": lane_id.to_string(),
        "role": record.role,
        "privacy_class": record.privacy_class,
        "payload": record.payload,
        "blob_sha256": record.blob.as_ref().and_then(|blob| blob.sha256.as_deref()),
        "display_title": record.display_title,
        "display_text": record.display_text,
        "schema_name": record.schema_name,
        "schema_version": record.schema_version,
    });
    Ok(canonical_json_source_hash(&canonical))
}

pub fn deterministic_source_record_id(
    source_id: &str,
    source_record_key: &str,
) -> Result<String, WriteObjectsError> {
    let source_id = parse_uuid("source_id", source_id)?;
    Ok(source_identity_record_id(source_id, source_record_key).to_string())
}

pub fn deterministic_object_id(
    source_id: &str,
    source_record_key: &str,
) -> Result<String, WriteObjectsError> {
    let source_id = parse_uuid("source_id", source_id)?;
    Ok(source_object_id(source_id, source_record_key).to_string())
}

struct PreparedWriteObjects {
    receipts: Vec<SourceRecordReceipt>,
    leaders: Vec<PreparedRecord>,
    chunk_size: usize,
    chunk_count: usize,
}

#[derive(Debug, Clone)]
struct PreparedRecord {
    ordinal: usize,
    observation_count: i64,
    user_id: Uuid,
    source_id: Uuid,
    source_record_key: String,
    source_record_id: Uuid,
    source_record_hash: String,
    object_id: Uuid,
    lane_id: Uuid,
    default_lane_id: Uuid,
    series_id: Uuid,
    series_kind: String,
    series_key: String,
    series_display_name: Option<String>,
    series_parent_key: Option<String>,
    parent_series_id: Option<Uuid>,
    modality: String,
    kind: String,
    role: String,
    privacy_class: String,
    event_start: DateTime<Utc>,
    event_end: DateTime<Utc>,
    time_semantics: String,
    temporal_level: String,
    native_resolution_ns: Option<i64>,
    stored_resolution_ns: Option<i64>,
    indexed_resolution_ns: Option<i64>,
    display_resolution_hint_ns: Option<i64>,
    time_resolution_ns: Option<i64>,
    time_uncertainty_ns: Option<i64>,
    alignment_confidence: Option<f32>,
    alignment_method: Option<String>,
    materialization_policy: String,
    importance_score: Option<f32>,
    blob: Option<PreparedBlob>,
    body_type: String,
    text_value: Option<String>,
    number_value: Option<f64>,
    bool_value: Option<bool>,
    payload: Value,
    display_title: Option<String>,
    display_text: Option<String>,
    edges: Vec<PreparedEdge>,
    source_start_ns: Option<i64>,
    source_end_ns: Option<i64>,
    source_sequence: Option<i64>,
    media_start_offset_ns: Option<i64>,
    media_end_offset_ns: Option<i64>,
    schema_name: Option<String>,
    schema_version: Option<i32>,
    confidence: Option<f32>,
    metadata: Value,
}

#[derive(Debug, Clone)]
struct PreparedBlob {
    blob_id: Uuid,
    storage_backend: String,
    uri: String,
    safe_uri: Option<String>,
    sha256: Option<String>,
    byte_count: Option<i64>,
    content_type: String,
    codec: Option<String>,
    duration_ms: Option<i64>,
    width: Option<i32>,
    height: Option<i32>,
    metadata: Value,
}

#[derive(Debug, Clone)]
struct PreparedEdge {
    edge_id: Uuid,
    to_object_id: Uuid,
    edge_kind: String,
    confidence: Option<f32>,
    metadata: Value,
}

struct ChunkReceipt {
    ordinal: usize,
    object_id: String,
    event_start: String,
    inserted: bool,
    dedupe_reason: Option<String>,
}

struct ExistingSourceRecord {
    source_record_id: Uuid,
    source_id: Uuid,
    source_record_key: String,
    source_record_hash: String,
    object_id: Uuid,
    object_event_start: DateTime<Utc>,
    kind: String,
}

struct ExactDuplicateRecord {
    ordinal: usize,
    observation_count: i64,
    source_record_id: Uuid,
    object_id: Uuid,
    object_event_start: DateTime<Utc>,
}

struct SourceIdentityConflictRecord {
    source_record_id: Uuid,
    source_id: Uuid,
    source_record_key: String,
    existing_hash: String,
    competing_hash: String,
}

struct SupportRecord<'a> {
    record: &'a PreparedRecord,
    mutable_update: bool,
}

fn prepare_write_objects(
    request: &WriteObjectsRequest,
) -> Result<PreparedWriteObjects, WriteObjectsError> {
    let user_id = parse_uuid("user_id", &request.user_id)?;
    parse_uuid("write_id", &request.write_id)?;
    let chunk_size = request
        .chunk_size
        .unwrap_or(DEFAULT_WRITE_OBJECTS_CHUNK_SIZE)
        .clamp(1, DEFAULT_WRITE_OBJECTS_CHUNK_SIZE);
    let mut first_by_identity = HashMap::<(Uuid, String), (usize, usize, String)>::new();
    let mut receipts = Vec::with_capacity(request.records.len());
    let mut leaders: Vec<PreparedRecord> = Vec::new();

    for (ordinal, record) in request.records.iter().enumerate() {
        let prepared = PreparedRecord::from_input(ordinal, user_id, record)?;
        let identity = (prepared.source_id, prepared.source_record_key.clone());
        if let Some((first_ordinal, leader_index, first_hash)) = first_by_identity.get(&identity) {
            if first_hash != &prepared.source_record_hash {
                return Err(WriteObjectsError::SourceIdentityHashConflictInBatch {
                    source_id: prepared.source_id.to_string(),
                    source_record_key: prepared.source_record_key,
                    first_ordinal: *first_ordinal,
                    conflicting_ordinal: ordinal,
                });
            }
            leaders[*leader_index].observation_count += 1;
            receipts.push(SourceRecordReceipt {
                client_record_id: record.client_record_id.clone(),
                source_id: prepared.source_id.to_string(),
                source_record_key: prepared.source_record_key,
                source_record_id: prepared.source_record_id.to_string(),
                object_id: prepared.object_id.to_string(),
                event_start: prepared
                    .event_start
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
                inserted: false,
                dedupe_reason: Some("same_batch".to_string()),
                duplicate_of_ordinal: Some(*first_ordinal),
            });
            continue;
        }
        first_by_identity.insert(
            identity,
            (ordinal, leaders.len(), prepared.source_record_hash.clone()),
        );
        receipts.push(SourceRecordReceipt {
            client_record_id: record.client_record_id.clone(),
            source_id: prepared.source_id.to_string(),
            source_record_key: prepared.source_record_key.clone(),
            source_record_id: prepared.source_record_id.to_string(),
            object_id: prepared.object_id.to_string(),
            event_start: prepared
                .event_start
                .to_rfc3339_opts(SecondsFormat::Micros, true),
            inserted: true,
            dedupe_reason: None,
            duplicate_of_ordinal: None,
        });
        leaders.push(prepared);
    }

    let chunk_count = leaders.len().div_ceil(chunk_size);
    Ok(PreparedWriteObjects {
        receipts,
        leaders,
        chunk_size,
        chunk_count,
    })
}

impl PreparedRecord {
    fn from_input(
        ordinal: usize,
        user_id: Uuid,
        record: &PerceptionObjectInput,
    ) -> Result<Self, WriteObjectsError> {
        let source_id = parse_uuid("source_id", &record.source_id)?;
        let lane_id = parse_uuid("lane_id", &record.lane_id)?;
        let event_start = parse_timestamp("event_start", &record.event_start)?;
        let event_end = parse_timestamp("event_end", &record.event_end)?;
        if event_end <= event_start {
            return Err(WriteObjectsError::InvalidRecord {
                field: "event_end",
                value: record.event_end.clone(),
                reason: "event_end must be greater than event_start".to_string(),
            });
        }
        validate_required("source_record_key", &record.source_record_key)?;
        validate_required("series_kind", &record.series_kind)?;
        validate_required("series_key", &record.series_key)?;
        validate_required("kind", &record.kind)?;
        validate_required("role", &record.role)?;
        validate_required("privacy_class", &record.privacy_class)?;
        validate_required("time_semantics", &record.time_semantics)?;
        validate_required("temporal_level", &record.temporal_level)?;
        validate_required("materialization_policy", &record.materialization_policy)?;
        validate_score("alignment_confidence", record.alignment_confidence)?;
        validate_score("importance_score", record.importance_score)?;
        validate_score("confidence", record.confidence)?;
        let series_kind = record.series_kind.trim().to_string();
        let series_key = record.series_key.trim().to_string();
        let series_display_name = normalize_optional_text(record.series_display_name.as_deref());
        let series_parent_key = normalize_optional_text(record.series_parent_key.as_deref());
        let parent_series_id = series_parent_key
            .as_ref()
            .map(|parent_key| deterministic_series_id(&user_id, &source_id, parent_key));
        let modality = normalized_modality(record)?;
        let body_type = normalized_body_type(record)?;
        validate_body_fields(record, &body_type)?;

        let source_record_id = source_identity_record_id(source_id, &record.source_record_key);
        let object_id = source_object_id(source_id, &record.source_record_key);
        let series_id = deterministic_series_id(&user_id, &source_id, &series_key);
        let source_record_hash = canonical_source_hash(record)?;
        let blob = record
            .blob
            .as_ref()
            .map(|blob| PreparedBlob::from_descriptor(blob, &source_id, &record.source_record_key))
            .transpose()?;
        let edges = record
            .edges
            .iter()
            .map(|edge| PreparedEdge::from_input(edge, &object_id))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            ordinal,
            observation_count: 1,
            user_id,
            source_id,
            source_record_key: record.source_record_key.clone(),
            source_record_id,
            source_record_hash,
            object_id,
            lane_id,
            default_lane_id: lane_id,
            series_id,
            series_kind,
            series_key,
            series_display_name,
            series_parent_key,
            parent_series_id,
            modality,
            kind: record.kind.clone(),
            role: record.role.clone(),
            privacy_class: record.privacy_class.clone(),
            event_start,
            event_end,
            time_semantics: record.time_semantics.clone(),
            temporal_level: record.temporal_level.clone(),
            native_resolution_ns: record.native_resolution_ns,
            stored_resolution_ns: record.stored_resolution_ns,
            indexed_resolution_ns: record.indexed_resolution_ns,
            display_resolution_hint_ns: record.display_resolution_hint_ns,
            time_resolution_ns: record.time_resolution_ns,
            time_uncertainty_ns: record.time_uncertainty_ns,
            alignment_confidence: record.alignment_confidence,
            alignment_method: record.alignment_method.clone(),
            materialization_policy: record.materialization_policy.clone(),
            importance_score: record.importance_score,
            blob,
            body_type,
            text_value: record.text_value.clone(),
            number_value: record.number_value,
            bool_value: record.bool_value,
            payload: record.payload.clone(),
            display_title: record.display_title.clone(),
            display_text: record.display_text.clone(),
            edges,
            source_start_ns: record.source_start_ns,
            source_end_ns: record.source_end_ns,
            source_sequence: record.source_sequence,
            media_start_offset_ns: record.media_start_offset_ns,
            media_end_offset_ns: record.media_end_offset_ns,
            schema_name: record.schema_name.clone(),
            schema_version: record.schema_version,
            confidence: record.confidence,
            metadata: record.metadata.clone(),
        })
    }
}

impl PreparedBlob {
    fn from_descriptor(
        blob: &BlobDescriptor,
        source_id: &Uuid,
        source_record_key: &str,
    ) -> Result<Self, WriteObjectsError> {
        validate_required("blob.storage_backend", &blob.storage_backend)?;
        validate_required("blob.uri", &blob.uri)?;
        validate_required("blob.content_type", &blob.content_type)?;
        let blob_id = match blob.blob_id.as_deref() {
            Some(value) => parse_uuid("blob_id", value)?,
            None => deterministic_uuid(
                "blob",
                source_id,
                &format!(
                    "{}\0{}\0{}",
                    source_record_key,
                    blob.sha256.as_deref().unwrap_or(""),
                    blob.uri
                ),
            ),
        };
        Ok(Self {
            blob_id,
            storage_backend: blob.storage_backend.clone(),
            uri: blob.uri.clone(),
            safe_uri: blob.safe_uri.clone(),
            sha256: blob.sha256.clone(),
            byte_count: blob.byte_count,
            content_type: blob.content_type.clone(),
            codec: blob.codec.clone(),
            duration_ms: blob.duration_ms,
            width: blob.width,
            height: blob.height,
            metadata: blob.metadata.clone(),
        })
    }
}

impl PreparedEdge {
    fn from_input(edge: &PerceptionEdgeInput, object_id: &Uuid) -> Result<Self, WriteObjectsError> {
        validate_required("edge_kind", &edge.edge_kind)?;
        validate_score("edge.confidence", edge.confidence)?;
        let to_object_id = parse_uuid("edge.to_object_id", &edge.to_object_id)?;
        let edge_id = deterministic_uuid(
            "edge",
            object_id,
            &format!("{}\0{}", to_object_id, edge.edge_kind),
        );
        Ok(Self {
            edge_id,
            to_object_id,
            edge_kind: edge.edge_kind.clone(),
            confidence: edge.confidence,
            metadata: edge.metadata.clone(),
        })
    }
}

fn write_prepared_chunk(
    client: &mut impl GenericClient,
    records: &[PreparedRecord],
) -> Result<Vec<ChunkReceipt>, WriteObjectsError> {
    if records.is_empty() {
        return Ok(Vec::new());
    }

    let existing_by_identity = prefetch_existing_source_records(client, records)?;
    let mut exact_duplicates = Vec::new();
    let mut conflicts = Vec::new();
    let mut support_records = Vec::new();

    for record in records {
        let identity = (record.source_id, record.source_record_key.clone());
        let Some(existing) = existing_by_identity.get(&identity) else {
            support_records.push(SupportRecord {
                record,
                mutable_update: false,
            });
            continue;
        };

        if existing.source_record_hash == record.source_record_hash {
            exact_duplicates.push(ExactDuplicateRecord {
                ordinal: record.ordinal,
                observation_count: record.observation_count,
                source_record_id: existing.source_record_id,
                object_id: existing.object_id,
                object_event_start: existing.object_event_start,
            });
            continue;
        }

        if is_mutable_container_kind(&existing.kind) && is_mutable_container_kind(&record.kind) {
            support_records.push(SupportRecord {
                record,
                mutable_update: true,
            });
            continue;
        }

        conflicts.push(SourceIdentityConflictRecord {
            source_record_id: existing.source_record_id,
            source_id: existing.source_id,
            source_record_key: existing.source_record_key.clone(),
            existing_hash: existing.source_record_hash.clone(),
            competing_hash: record.source_record_hash.clone(),
        });
    }

    if let Some(error) = record_source_identity_conflicts(client, &conflicts)? {
        return Err(error);
    }

    let mut receipts = exact_duplicates
        .iter()
        .map(|record| ChunkReceipt {
            ordinal: record.ordinal,
            object_id: record.object_id.to_string(),
            event_start: record
                .object_event_start
                .to_rfc3339_opts(SecondsFormat::Micros, true),
            inserted: false,
            dedupe_reason: Some("existing_source_record".to_string()),
        })
        .collect::<Vec<_>>();

    update_exact_duplicate_seen_counts(client, &exact_duplicates)?;
    if support_records.is_empty() {
        if receipts.len() != records.len() {
            return Err(WriteObjectsError::ResultCountMismatch {
                expected: records.len(),
                actual: receipts.len(),
            });
        }
        receipts.sort_by_key(|receipt| receipt.ordinal);
        return Ok(receipts);
    }

    client.batch_execute(
        r#"
        CREATE TEMP TABLE write_objects_stage (
          ordinal INT NOT NULL,
          mutable_update BOOLEAN NOT NULL,
          observation_count BIGINT NOT NULL,
          user_id UUID NOT NULL,
          source_id UUID NOT NULL,
          source_record_key TEXT NOT NULL,
          source_record_id UUID NOT NULL,
          source_record_hash TEXT NOT NULL,
          object_id UUID NOT NULL,
          lane_id UUID NOT NULL,
          default_lane_id UUID NOT NULL,
          series_id UUID NOT NULL,
          series_kind TEXT NOT NULL,
          series_key TEXT NOT NULL,
          series_display_name TEXT,
          series_parent_key TEXT,
          parent_series_id UUID,
          modality TEXT NOT NULL,
          kind TEXT NOT NULL,
          role TEXT NOT NULL,
          privacy_class TEXT NOT NULL,
          event_start TIMESTAMPTZ NOT NULL,
          event_end TIMESTAMPTZ NOT NULL,
          time_semantics TEXT NOT NULL,
          temporal_level TEXT NOT NULL,
          native_resolution_ns BIGINT,
          stored_resolution_ns BIGINT,
          indexed_resolution_ns BIGINT,
          display_resolution_hint_ns BIGINT,
          time_resolution_ns BIGINT,
          time_uncertainty_ns BIGINT,
          alignment_confidence REAL,
          alignment_method TEXT,
          materialization_policy TEXT NOT NULL,
          importance_score REAL,
          blob_id UUID,
          blob_storage_backend TEXT,
          blob_uri TEXT,
          blob_safe_uri TEXT,
          blob_sha256 TEXT,
          blob_byte_count BIGINT,
          blob_content_type TEXT,
          blob_codec TEXT,
          blob_duration_ms BIGINT,
          blob_width INT,
          blob_height INT,
          blob_metadata JSONB,
          body_type TEXT NOT NULL,
          text_value TEXT,
          number_value DOUBLE PRECISION,
          bool_value BOOLEAN,
          payload JSONB NOT NULL,
          display_title TEXT,
          display_text TEXT,
          source_start_ns BIGINT,
          source_end_ns BIGINT,
          source_sequence BIGINT,
          media_start_offset_ns BIGINT,
          media_end_offset_ns BIGINT,
          schema_name TEXT,
          schema_version INT,
          confidence REAL,
          metadata JSONB NOT NULL,
          PRIMARY KEY (ordinal)
        ) ON COMMIT DROP;
        "#,
    )?;
    copy_prepared_records(client, &support_records)?;

    client.batch_execute(
        r#"
        CREATE TEMP TABLE write_objects_mutable_changed ON COMMIT DROP AS
        SELECT stage.*
        FROM write_objects_stage stage
        WHERE mutable_update;

        CREATE TEMP TABLE write_objects_new_rows ON COMMIT DROP AS
        SELECT stage.*
        FROM write_objects_stage stage
        WHERE NOT mutable_update;
        "#,
    )?;

    client.batch_execute(
        r#"
        INSERT INTO app.users (user_id)
        SELECT DISTINCT user_id
        FROM write_objects_stage
        ON CONFLICT (user_id) DO NOTHING;

        INSERT INTO perception.lanes (
          lane_id,
          user_id,
          lane_key,
          display_name,
          lane_group,
          metadata
        )
        SELECT DISTINCT ON (lane_id)
          lane_id,
          user_id,
          COALESCE(metadata->'source'->>'connector_key', schema_name, kind),
          COALESCE(metadata->'source'->>'connector_key', schema_name, kind),
          split_part(COALESCE(metadata->'source'->>'connector_key', schema_name, kind), '.', 1),
          jsonb_build_object(
            'created_by', 'onecontext-memory-db.write_objects',
            'source', metadata->'source'
          )
        FROM write_objects_stage
        ORDER BY lane_id, ordinal
        ON CONFLICT DO NOTHING;

        INSERT INTO perception.sources (
          source_id,
          user_id,
          source_type,
          source_key,
          display_name,
          default_lane_id,
          metadata
        )
        SELECT DISTINCT ON (source_id)
          source_id,
          user_id,
          split_part(COALESCE(metadata->'source'->>'connector_key', schema_name, kind), '.', 1),
          COALESCE(metadata->'source'->>'connector_key', source_id::text),
          COALESCE(metadata->'source'->>'connector_key', source_id::text),
          lane_id,
          jsonb_build_object(
            'created_by', 'onecontext-memory-db.write_objects',
            'source', metadata->'source'
          )
        FROM write_objects_new_rows
        ORDER BY source_id, ordinal
        ON CONFLICT (source_id) DO UPDATE
        SET updated_at = now(),
            default_lane_id = COALESCE(perception.sources.default_lane_id, EXCLUDED.default_lane_id);

        INSERT INTO perception.blobs (
          blob_id,
          user_id,
          storage_backend,
          uri,
          safe_uri,
          sha256,
          byte_count,
          content_type,
          codec,
          duration_ms,
          width,
          height,
          metadata
        )
        SELECT DISTINCT ON (blob_id)
          blob_id,
          user_id,
          blob_storage_backend,
          blob_uri,
          blob_safe_uri,
          blob_sha256,
          blob_byte_count,
          blob_content_type,
          blob_codec,
          blob_duration_ms,
          blob_width,
          blob_height,
          COALESCE(blob_metadata, '{}'::jsonb)
        FROM write_objects_stage
        WHERE blob_id IS NOT NULL
        ORDER BY blob_id, ordinal
        ON CONFLICT DO NOTHING;

        INSERT INTO perception.series (
          series_id,
          user_id,
          source_id,
          default_lane_id,
          series_kind,
          series_key,
          parent_series_id,
          display_name,
          modality,
          default_privacy_class,
          default_time_resolution_ns,
          default_time_uncertainty_ns,
          first_event_start,
          last_event_end,
          metadata
        )
        SELECT DISTINCT ON (series_id)
          series_id,
          user_id,
          source_id,
          default_lane_id,
          series_kind,
          series_key,
          NULL::uuid,
          series_display_name,
          modality,
          privacy_class,
          time_resolution_ns,
          time_uncertainty_ns,
          event_start,
          event_end,
          jsonb_build_object(
            'created_by', 'onecontext-memory-db.write_objects',
            'source', metadata->'source'
          )
        FROM write_objects_new_rows
        ORDER BY series_id, ordinal
        ON CONFLICT (user_id, source_id, series_key) DO UPDATE
        SET updated_at = now(),
            default_lane_id = COALESCE(
              perception.series.default_lane_id,
              EXCLUDED.default_lane_id
            ),
            series_kind = EXCLUDED.series_kind,
            display_name = COALESCE(EXCLUDED.display_name, perception.series.display_name),
            modality = COALESCE(EXCLUDED.modality, perception.series.modality),
            default_privacy_class = EXCLUDED.default_privacy_class,
            default_time_resolution_ns = COALESCE(
              EXCLUDED.default_time_resolution_ns,
              perception.series.default_time_resolution_ns
            ),
            default_time_uncertainty_ns = COALESCE(
              EXCLUDED.default_time_uncertainty_ns,
              perception.series.default_time_uncertainty_ns
            ),
            first_event_start = LEAST(
              COALESCE(perception.series.first_event_start, EXCLUDED.first_event_start),
              EXCLUDED.first_event_start
            ),
            last_event_end = GREATEST(
              COALESCE(perception.series.last_event_end, EXCLUDED.last_event_end),
              EXCLUDED.last_event_end
            );

        UPDATE perception.series series
        SET parent_series_id = parent.series_id,
            updated_at = now()
        FROM write_objects_new_rows stage
        JOIN perception.series parent
          ON parent.user_id = stage.user_id
         AND parent.source_id = stage.source_id
         AND parent.series_key = stage.series_parent_key
        WHERE series.series_id = stage.series_id
          AND stage.series_parent_key IS NOT NULL
          AND series.parent_series_id IS DISTINCT FROM parent.series_id;

        UPDATE perception.source_records source_records
        SET last_seen_at = now(),
            seen_count = source_records.seen_count + stage.observation_count,
            source_record_hash = stage.source_record_hash,
            series_id = stage.series_id,
            kind = stage.kind,
            metadata = COALESCE(source_records.metadata, '{}'::jsonb)
              || jsonb_build_object(
                'mutable_container_update',
                jsonb_build_object(
                  'updated_at', now(),
                  'previous_hash', source_records.source_record_hash,
                  'current_hash', stage.source_record_hash
                )
              )
        FROM write_objects_mutable_changed stage
        WHERE source_records.source_id = stage.source_id
          AND source_records.source_record_key = stage.source_record_key
          AND source_records.source_record_hash <> stage.source_record_hash
          AND source_records.kind IN (
            'agent_session',
            'agent_turn',
            'agent_message',
            'agent_tool_summary',
            'agent_runtime_event',
            'agent_prompt_snapshot',
            'agent_compaction'
          )
          AND stage.kind IN (
            'agent_session',
            'agent_turn',
            'agent_message',
            'agent_tool_summary',
            'agent_runtime_event',
            'agent_prompt_snapshot',
            'agent_compaction'
          );

        UPDATE perception.objects objects
        SET event_end = stage.event_end,
            source_record_hash = stage.source_record_hash,
            series_id = stage.series_id,
            lane_id = stage.lane_id,
            kind = stage.kind,
            role = stage.role,
            privacy_class = stage.privacy_class,
            modality = stage.modality,
            body_type = stage.body_type,
            text_value = stage.text_value,
            number_value = stage.number_value,
            bool_value = stage.bool_value,
            time_semantics = stage.time_semantics,
            temporal_level = stage.temporal_level,
            native_resolution_ns = stage.native_resolution_ns,
            stored_resolution_ns = stage.stored_resolution_ns,
            indexed_resolution_ns = stage.indexed_resolution_ns,
            display_resolution_hint_ns = stage.display_resolution_hint_ns,
            time_resolution_ns = stage.time_resolution_ns,
            time_uncertainty_ns = stage.time_uncertainty_ns,
            alignment_confidence = stage.alignment_confidence,
            alignment_method = stage.alignment_method,
            materialization_policy = stage.materialization_policy,
            importance_score = stage.importance_score,
            blob_id = stage.blob_id,
            payload = stage.payload,
            display_title = stage.display_title,
            display_text = stage.display_text,
            source_start_ns = stage.source_start_ns,
            source_end_ns = stage.source_end_ns,
            source_sequence = stage.source_sequence,
            source_ordinal = stage.source_sequence,
            media_start_offset_ns = stage.media_start_offset_ns,
            media_end_offset_ns = stage.media_end_offset_ns,
            schema_name = stage.schema_name,
            schema_version = stage.schema_version,
            confidence = stage.confidence,
            metadata = stage.metadata
        FROM write_objects_mutable_changed stage
        JOIN perception.source_records source_records
          ON source_records.source_id = stage.source_id
         AND source_records.source_record_key = stage.source_record_key
         AND source_records.object_id = stage.object_id
         AND source_records.kind IN (
            'agent_session',
            'agent_turn',
            'agent_message',
            'agent_tool_summary',
            'agent_runtime_event',
            'agent_prompt_snapshot',
            'agent_compaction'
          )
        WHERE objects.object_id = source_records.object_id
          AND objects.event_start = source_records.object_event_start
          AND objects.user_id = source_records.user_id
          AND stage.kind IN (
            'agent_session',
            'agent_turn',
            'agent_message',
            'agent_tool_summary',
            'agent_runtime_event',
            'agent_prompt_snapshot',
            'agent_compaction'
          )
          AND objects.source_record_hash IS DISTINCT FROM stage.source_record_hash;

        CREATE TEMP TABLE write_objects_claimed ON COMMIT DROP AS
        WITH inserted_source_records AS (
          INSERT INTO perception.source_records (
            source_record_id,
            user_id,
            source_id,
            source_record_key,
            source_record_hash,
            object_id,
            object_event_start,
            series_id,
            kind,
            seen_count,
            metadata
          )
          SELECT
            source_record_id,
            user_id,
            source_id,
            source_record_key,
            source_record_hash,
            object_id,
            event_start,
            series_id,
            kind,
            observation_count,
            jsonb_build_object(
              'created_by', 'onecontext-memory-db.write_objects',
              'source', metadata->'source'
          )
          FROM write_objects_new_rows
          ON CONFLICT (source_id, source_record_key) DO NOTHING
          RETURNING source_record_id
        ),
        inserted_claimed AS (
          SELECT stage.*
          FROM write_objects_stage stage
          JOIN inserted_source_records inserted
            ON inserted.source_record_id = stage.source_record_id
        ),
        mutable_claimed AS (
          SELECT stage.*
          FROM write_objects_mutable_changed stage
          JOIN perception.source_records source_records
            ON source_records.source_id = stage.source_id
           AND source_records.source_record_key = stage.source_record_key
           AND source_records.source_record_hash = stage.source_record_hash
        )
        SELECT * FROM inserted_claimed
        UNION ALL
        SELECT * FROM mutable_claimed;

        INSERT INTO perception.objects (
          event_start,
          event_end,
          object_id,
          user_id,
          source_id,
          source_record_id,
          source_record_key,
          source_record_hash,
          series_id,
          lane_id,
          kind,
          role,
          privacy_class,
          modality,
          body_type,
          text_value,
          number_value,
          bool_value,
          time_semantics,
          temporal_level,
          native_resolution_ns,
          stored_resolution_ns,
          indexed_resolution_ns,
          display_resolution_hint_ns,
          time_resolution_ns,
          time_uncertainty_ns,
          alignment_confidence,
          alignment_method,
          materialization_policy,
          importance_score,
          blob_id,
          payload,
          display_title,
          display_text,
          source_start_ns,
          source_end_ns,
          source_sequence,
          source_ordinal,
          media_start_offset_ns,
          media_end_offset_ns,
          schema_name,
          schema_version,
          confidence,
          metadata
        )
        SELECT
          event_start,
          event_end,
          object_id,
          user_id,
          source_id,
          source_record_id,
          source_record_key,
          source_record_hash,
          series_id,
          lane_id,
          kind,
          role,
          privacy_class,
          modality,
          body_type,
          text_value,
          number_value,
          bool_value,
          time_semantics,
          temporal_level,
          native_resolution_ns,
          stored_resolution_ns,
          indexed_resolution_ns,
          display_resolution_hint_ns,
          time_resolution_ns,
          time_uncertainty_ns,
          alignment_confidence,
          alignment_method,
          materialization_policy,
          importance_score,
          blob_id,
          payload,
          display_title,
          display_text,
          source_start_ns,
          source_end_ns,
          source_sequence,
          source_sequence,
          media_start_offset_ns,
          media_end_offset_ns,
          schema_name,
          schema_version,
          confidence,
          metadata
        FROM write_objects_claimed
        WHERE NOT mutable_update;

        UPDATE perception.series series
        SET record_count = series.record_count + counts.record_count,
            first_event_start = LEAST(
              COALESCE(series.first_event_start, counts.first_event_start),
              counts.first_event_start
            ),
            last_event_end = GREATEST(
              COALESCE(series.last_event_end, counts.last_event_end),
              counts.last_event_end
            ),
            updated_at = now()
        FROM (
          SELECT
            series_id,
            count(*)::bigint AS record_count,
            min(event_start) AS first_event_start,
            max(event_end) AS last_event_end
          FROM write_objects_claimed
          WHERE NOT mutable_update
          GROUP BY series_id
        ) counts
        WHERE series.series_id = counts.series_id;

        INSERT INTO perception.object_edges (
          edge_id,
          user_id,
          from_object_id,
          from_object_event_start,
          to_object_id,
          to_object_event_start,
          edge_kind,
          confidence,
          metadata
        )
        SELECT
          edges.edge_id,
          edges.user_id,
          edges.object_id,
          edges.object_event_start,
          edges.to_object_id,
          target.object_event_start,
          edges.edge_kind,
          edges.edge_confidence,
          edges.edge_metadata
        FROM write_object_edges_stage edges
        JOIN write_objects_claimed claimed
          ON claimed.object_id = edges.object_id
        JOIN perception.source_records target
          ON target.user_id = edges.user_id
         AND target.object_id = edges.to_object_id
        ON CONFLICT DO NOTHING;
        "#,
    )?;

    if let Some(row) = client.query_opt(
        r#"
        WITH conflicts AS (
          SELECT DISTINCT ON (source_records.source_record_id)
            source_records.source_record_id,
            source_records.source_id,
            source_records.source_record_key,
            source_records.source_record_hash AS existing_hash,
            stage.source_record_hash AS competing_hash
          FROM write_objects_stage stage
          LEFT JOIN write_objects_claimed claimed
            ON claimed.source_record_id = stage.source_record_id
          JOIN perception.source_records source_records
            ON source_records.source_id = stage.source_id
           AND source_records.source_record_key = stage.source_record_key
          WHERE claimed.source_record_id IS NULL
            AND source_records.source_record_hash <> stage.source_record_hash
          ORDER BY source_records.source_record_id, stage.ordinal
        ),
        updated AS (
          UPDATE perception.source_records source_records
          SET last_seen_at = now(),
              conflict_count = source_records.conflict_count + 1,
              metadata = COALESCE(source_records.metadata, '{}'::jsonb)
                || jsonb_build_object(
                  'source_identity_conflict',
                  jsonb_build_object(
                    'last_conflict_at', now(),
                    'existing_hash', conflicts.existing_hash,
                    'competing_hash', conflicts.competing_hash,
                    'source_id', conflicts.source_id::text,
                    'source_record_key', conflicts.source_record_key
                  )
                )
          FROM conflicts
          WHERE source_records.source_record_id = conflicts.source_record_id
          RETURNING
            conflicts.source_id::text AS source_id,
            conflicts.source_record_key,
            conflicts.existing_hash,
            conflicts.competing_hash
        )
        SELECT source_id, source_record_key, existing_hash, competing_hash
        FROM updated
        LIMIT 1
        "#,
        &[],
    )? {
        return Err(WriteObjectsError::SourceIdentityHashConflict {
            source_id: row.get("source_id"),
            source_record_key: row.get("source_record_key"),
            existing_hash: row.get("existing_hash"),
            competing_hash: row.get("competing_hash"),
        });
    }

    let rows = client.query(
        r#"
        SELECT
          stage.ordinal,
          source_records.object_id::text AS object_id,
          source_records.object_event_start::text AS event_start,
          (claimed.ordinal IS NOT NULL) AS inserted
        FROM write_objects_stage stage
        JOIN perception.source_records source_records
          ON source_records.source_id = stage.source_id
         AND source_records.source_record_key = stage.source_record_key
         AND source_records.source_record_hash = stage.source_record_hash
        LEFT JOIN write_objects_claimed claimed
          ON claimed.source_record_id = stage.source_record_id
        ORDER BY stage.ordinal
        "#,
        &[],
    )?;

    for row in rows {
        let inserted: bool = row.get("inserted");
        receipts.push(ChunkReceipt {
            ordinal: row.get::<_, i32>("ordinal") as usize,
            object_id: row.get("object_id"),
            event_start: row.get("event_start"),
            inserted,
            dedupe_reason: if inserted {
                None
            } else {
                Some("existing_source_record".to_string())
            },
        });
    }
    if receipts.len() != records.len() {
        return Err(WriteObjectsError::ResultCountMismatch {
            expected: records.len(),
            actual: receipts.len(),
        });
    }
    receipts.sort_by_key(|receipt| receipt.ordinal);
    Ok(receipts)
}

fn prefetch_existing_source_records(
    client: &mut impl GenericClient,
    records: &[PreparedRecord],
) -> Result<HashMap<(Uuid, String), ExistingSourceRecord>, WriteObjectsError> {
    if records.is_empty() {
        return Ok(HashMap::new());
    }

    let source_ids = records
        .iter()
        .map(|record| record.source_id)
        .collect::<Vec<_>>();
    let source_record_keys = records
        .iter()
        .map(|record| record.source_record_key.clone())
        .collect::<Vec<_>>();
    let rows = client.query(
        r#"
        WITH requested AS (
          SELECT *
          FROM unnest($1::uuid[], $2::text[]) AS request(source_id, source_record_key)
        )
        SELECT
          source_records.source_record_id,
          source_records.source_id,
          source_records.source_record_key,
          source_records.source_record_hash,
          source_records.object_id,
          source_records.object_event_start,
          source_records.kind
        FROM perception.source_records source_records
        JOIN requested
          ON requested.source_id = source_records.source_id
         AND requested.source_record_key = source_records.source_record_key
        "#,
        &[&source_ids, &source_record_keys],
    )?;

    let mut existing_by_identity = HashMap::with_capacity(rows.len());
    for row in rows {
        let source_id = row.get::<_, Uuid>("source_id");
        let source_record_key = row.get::<_, String>("source_record_key");
        existing_by_identity.insert(
            (source_id, source_record_key.clone()),
            ExistingSourceRecord {
                source_record_id: row.get("source_record_id"),
                source_id,
                source_record_key,
                source_record_hash: row.get("source_record_hash"),
                object_id: row.get("object_id"),
                object_event_start: row.get("object_event_start"),
                kind: row.get("kind"),
            },
        );
    }
    Ok(existing_by_identity)
}

fn record_source_identity_conflicts(
    client: &mut impl GenericClient,
    conflicts: &[SourceIdentityConflictRecord],
) -> Result<Option<WriteObjectsError>, WriteObjectsError> {
    let Some(first_conflict) = conflicts.first() else {
        return Ok(None);
    };

    for conflict in conflicts {
        let source_id = conflict.source_id.to_string();
        client.execute(
            r#"
            UPDATE perception.source_records source_records
            SET last_seen_at = now(),
                conflict_count = source_records.conflict_count + 1,
                metadata = COALESCE(source_records.metadata, '{}'::jsonb)
                  || jsonb_build_object(
                    'source_identity_conflict',
                    jsonb_build_object(
                      'last_conflict_at', now(),
                      'existing_hash', $2::text,
                      'competing_hash', $3::text,
                      'source_id', $4::text,
                      'source_record_key', $5::text
                    )
                  )
            WHERE source_records.source_record_id = $1
            "#,
            &[
                &conflict.source_record_id,
                &conflict.existing_hash,
                &conflict.competing_hash,
                &source_id,
                &conflict.source_record_key,
            ],
        )?;
    }

    Ok(Some(WriteObjectsError::SourceIdentityHashConflict {
        source_id: first_conflict.source_id.to_string(),
        source_record_key: first_conflict.source_record_key.clone(),
        existing_hash: first_conflict.existing_hash.clone(),
        competing_hash: first_conflict.competing_hash.clone(),
    }))
}

fn update_exact_duplicate_seen_counts(
    client: &mut impl GenericClient,
    exact_duplicates: &[ExactDuplicateRecord],
) -> Result<(), WriteObjectsError> {
    if exact_duplicates.is_empty() {
        return Ok(());
    }

    let source_record_ids = exact_duplicates
        .iter()
        .map(|record| record.source_record_id)
        .collect::<Vec<_>>();
    let observation_counts = exact_duplicates
        .iter()
        .map(|record| record.observation_count)
        .collect::<Vec<_>>();

    client.execute(
        r#"
        WITH exact AS (
          SELECT
            source_record_id,
            sum(observation_count)::bigint AS seen_count
          FROM unnest($1::uuid[], $2::bigint[]) AS updates(
            source_record_id,
            observation_count
          )
          GROUP BY source_record_id
        )
        UPDATE perception.source_records source_records
        SET last_seen_at = now(),
            seen_count = source_records.seen_count + exact.seen_count
        FROM exact
        WHERE source_records.source_record_id = exact.source_record_id;
        "#,
        &[&source_record_ids, &observation_counts],
    )?;

    Ok(())
}

fn is_mutable_container_kind(kind: &str) -> bool {
    matches!(
        kind,
        "agent_session"
            | "agent_turn"
            | "agent_message"
            | "agent_tool_summary"
            | "agent_runtime_event"
            | "agent_prompt_snapshot"
            | "agent_compaction"
    )
}

fn copy_prepared_records(
    client: &mut impl GenericClient,
    records: &[SupportRecord<'_>],
) -> Result<(), WriteObjectsError> {
    let mut writer = client.copy_in(
        r#"
        COPY write_objects_stage (
          ordinal,
          mutable_update,
          observation_count,
          user_id,
          source_id,
          source_record_key,
          source_record_id,
          source_record_hash,
          object_id,
          lane_id,
          default_lane_id,
          series_id,
          series_kind,
          series_key,
          series_display_name,
          series_parent_key,
          parent_series_id,
          modality,
          kind,
          role,
          privacy_class,
          event_start,
          event_end,
          time_semantics,
          temporal_level,
          native_resolution_ns,
          stored_resolution_ns,
          indexed_resolution_ns,
          display_resolution_hint_ns,
          time_resolution_ns,
          time_uncertainty_ns,
          alignment_confidence,
          alignment_method,
          materialization_policy,
          importance_score,
          blob_id,
          blob_storage_backend,
          blob_uri,
          blob_safe_uri,
          blob_sha256,
          blob_byte_count,
          blob_content_type,
          blob_codec,
          blob_duration_ms,
          blob_width,
          blob_height,
          blob_metadata,
          body_type,
          text_value,
          number_value,
          bool_value,
          payload,
          display_title,
          display_text,
          source_start_ns,
          source_end_ns,
          source_sequence,
          media_start_offset_ns,
          media_end_offset_ns,
          schema_name,
          schema_version,
          confidence,
          metadata
        ) FROM STDIN WITH (FORMAT text)
        "#,
    )?;

    for staged in records {
        let record = staged.record;
        copy_field(&mut writer, Some(&record.ordinal.to_string()))?;
        copy_field(&mut writer, Some(&staged.mutable_update.to_string()))?;
        copy_field(&mut writer, Some(&record.observation_count.to_string()))?;
        copy_field(&mut writer, Some(&record.user_id.to_string()))?;
        copy_field(&mut writer, Some(&record.source_id.to_string()))?;
        copy_field(&mut writer, Some(&record.source_record_key))?;
        copy_field(&mut writer, Some(&record.source_record_id.to_string()))?;
        copy_field(&mut writer, Some(&record.source_record_hash))?;
        copy_field(&mut writer, Some(&record.object_id.to_string()))?;
        copy_field(&mut writer, Some(&record.lane_id.to_string()))?;
        copy_field(&mut writer, Some(&record.default_lane_id.to_string()))?;
        copy_field(&mut writer, Some(&record.series_id.to_string()))?;
        copy_field(&mut writer, Some(&record.series_kind))?;
        copy_field(&mut writer, Some(&record.series_key))?;
        copy_field(&mut writer, record.series_display_name.as_deref())?;
        copy_field(&mut writer, record.series_parent_key.as_deref())?;
        copy_field(
            &mut writer,
            record
                .parent_series_id
                .map(|value| value.to_string())
                .as_deref(),
        )?;
        copy_field(&mut writer, Some(&record.modality))?;
        copy_field(&mut writer, Some(&record.kind))?;
        copy_field(&mut writer, Some(&record.role))?;
        copy_field(&mut writer, Some(&record.privacy_class))?;
        copy_field(
            &mut writer,
            Some(
                &record
                    .event_start
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
            ),
        )?;
        copy_field(
            &mut writer,
            Some(
                &record
                    .event_end
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
            ),
        )?;
        copy_field(&mut writer, Some(&record.time_semantics))?;
        copy_field(&mut writer, Some(&record.temporal_level))?;
        copy_optional_i64_field(&mut writer, record.native_resolution_ns)?;
        copy_optional_i64_field(&mut writer, record.stored_resolution_ns)?;
        copy_optional_i64_field(&mut writer, record.indexed_resolution_ns)?;
        copy_optional_i64_field(&mut writer, record.display_resolution_hint_ns)?;
        copy_optional_i64_field(&mut writer, record.time_resolution_ns)?;
        copy_optional_i64_field(&mut writer, record.time_uncertainty_ns)?;
        copy_optional_f32_field(&mut writer, record.alignment_confidence)?;
        copy_field(&mut writer, record.alignment_method.as_deref())?;
        copy_field(&mut writer, Some(&record.materialization_policy))?;
        copy_optional_f32_field(&mut writer, record.importance_score)?;
        if let Some(blob) = &record.blob {
            copy_field(&mut writer, Some(&blob.blob_id.to_string()))?;
            copy_field(&mut writer, Some(&blob.storage_backend))?;
            copy_field(&mut writer, Some(&blob.uri))?;
            copy_field(&mut writer, blob.safe_uri.as_deref())?;
            copy_field(&mut writer, blob.sha256.as_deref())?;
            copy_optional_i64_field(&mut writer, blob.byte_count)?;
            copy_field(&mut writer, Some(&blob.content_type))?;
            copy_field(&mut writer, blob.codec.as_deref())?;
            copy_optional_i64_field(&mut writer, blob.duration_ms)?;
            copy_optional_i32_field(&mut writer, blob.width)?;
            copy_optional_i32_field(&mut writer, blob.height)?;
            copy_field(&mut writer, Some(&blob.metadata.to_string()))?;
        } else {
            for _ in 0..12 {
                copy_field(&mut writer, None)?;
            }
        }
        copy_field(&mut writer, Some(&record.body_type))?;
        copy_field(&mut writer, record.text_value.as_deref())?;
        copy_optional_f64_field(&mut writer, record.number_value)?;
        copy_optional_bool_field(&mut writer, record.bool_value)?;
        copy_field(&mut writer, Some(&record.payload.to_string()))?;
        copy_field(&mut writer, record.display_title.as_deref())?;
        copy_field(&mut writer, record.display_text.as_deref())?;
        copy_optional_i64_field(&mut writer, record.source_start_ns)?;
        copy_optional_i64_field(&mut writer, record.source_end_ns)?;
        copy_optional_i64_field(&mut writer, record.source_sequence)?;
        copy_optional_i64_field(&mut writer, record.media_start_offset_ns)?;
        copy_optional_i64_field(&mut writer, record.media_end_offset_ns)?;
        copy_field(&mut writer, record.schema_name.as_deref())?;
        copy_optional_i32_field(&mut writer, record.schema_version)?;
        copy_optional_f32_field(&mut writer, record.confidence)?;
        copy_last_field(&mut writer, Some(&record.metadata.to_string()))?;
        writer.write_all(b"\n")?;
    }
    writer.finish()?;

    client.batch_execute(
        r#"
        CREATE TEMP TABLE write_object_edges_stage (
          edge_id UUID NOT NULL,
          user_id UUID NOT NULL,
          object_id UUID NOT NULL,
          object_event_start TIMESTAMPTZ NOT NULL,
          to_object_id UUID NOT NULL,
          edge_kind TEXT NOT NULL,
          edge_confidence REAL,
          edge_metadata JSONB NOT NULL
        ) ON COMMIT DROP;
        "#,
    )?;

    let mut edge_writer = client.copy_in(
        r#"
        COPY write_object_edges_stage (
          edge_id,
          user_id,
          object_id,
          object_event_start,
          to_object_id,
          edge_kind,
          edge_confidence,
          edge_metadata
        ) FROM STDIN WITH (FORMAT text)
        "#,
    )?;
    for record in records {
        let record = record.record;
        for edge in &record.edges {
            copy_field(&mut edge_writer, Some(&edge.edge_id.to_string()))?;
            copy_field(&mut edge_writer, Some(&record.user_id.to_string()))?;
            copy_field(&mut edge_writer, Some(&record.object_id.to_string()))?;
            copy_field(
                &mut edge_writer,
                Some(
                    &record
                        .event_start
                        .to_rfc3339_opts(SecondsFormat::Micros, true),
                ),
            )?;
            copy_field(&mut edge_writer, Some(&edge.to_object_id.to_string()))?;
            copy_field(&mut edge_writer, Some(&edge.edge_kind))?;
            copy_optional_f32_field(&mut edge_writer, edge.confidence)?;
            copy_last_field(&mut edge_writer, Some(&edge.metadata.to_string()))?;
            edge_writer.write_all(b"\n")?;
        }
    }
    edge_writer.finish()?;
    Ok(())
}

fn parse_uuid(field: &'static str, value: &str) -> Result<Uuid, WriteObjectsError> {
    Uuid::parse_str(value).map_err(|error| WriteObjectsError::InvalidRecord {
        field,
        value: value.to_string(),
        reason: error.to_string(),
    })
}

fn parse_timestamp(field: &'static str, value: &str) -> Result<DateTime<Utc>, WriteObjectsError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| WriteObjectsError::InvalidRecord {
            field,
            value: value.to_string(),
            reason: error.to_string(),
        })
}

fn validate_required(field: &'static str, value: &str) -> Result<(), WriteObjectsError> {
    if value.trim().is_empty() {
        return Err(WriteObjectsError::InvalidRecord {
            field,
            value: value.to_string(),
            reason: "must not be empty".to_string(),
        });
    }
    Ok(())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalized_body_type(record: &PerceptionObjectInput) -> Result<String, WriteObjectsError> {
    let body_type = normalize_optional_text(record.body_type.as_deref()).unwrap_or_else(|| {
        if record.text_value.is_some() {
            "text".to_string()
        } else if record.number_value.is_some() {
            "number".to_string()
        } else if record.bool_value.is_some() {
            "boolean".to_string()
        } else if record.blob.is_some() {
            "blob".to_string()
        } else {
            "json".to_string()
        }
    });
    if !matches!(
        body_type.as_str(),
        "none" | "text" | "json" | "number" | "boolean" | "blob" | "mixed"
    ) {
        return Err(WriteObjectsError::InvalidRecord {
            field: "body_type",
            value: body_type,
            reason: "must be one of none, text, json, number, boolean, blob, mixed".to_string(),
        });
    }
    Ok(body_type)
}

fn validate_body_fields(
    record: &PerceptionObjectInput,
    body_type: &str,
) -> Result<(), WriteObjectsError> {
    let missing_required_body = match body_type {
        "text" if record.text_value.is_none() => Some("text_value is required"),
        "number" if record.number_value.is_none() => Some("number_value is required"),
        "boolean" if record.bool_value.is_none() => Some("bool_value is required"),
        "blob" if record.blob.is_none() => Some("blob is required"),
        _ => None,
    };
    if let Some(reason) = missing_required_body {
        return Err(WriteObjectsError::InvalidRecord {
            field: "body_type",
            value: body_type.to_string(),
            reason: reason.to_string(),
        });
    }
    Ok(())
}

fn normalized_modality(record: &PerceptionObjectInput) -> Result<String, WriteObjectsError> {
    let modality = normalize_optional_text(record.modality.as_deref()).unwrap_or_else(|| {
        if let Some(blob) = &record.blob {
            if blob.content_type.starts_with("image/") {
                "image".to_string()
            } else if blob.content_type.starts_with("video/") {
                "video".to_string()
            } else if blob.content_type.starts_with("audio/") {
                "audio".to_string()
            } else if blob.content_type.starts_with("text/") {
                "text".to_string()
            } else {
                "binary".to_string()
            }
        } else if record.number_value.is_some() || record.bool_value.is_some() {
            "scalar".to_string()
        } else if record.text_value.is_some() {
            "text".to_string()
        } else {
            "mixed".to_string()
        }
    });
    if !matches!(
        modality.as_str(),
        "text" | "json" | "image" | "video" | "audio" | "binary" | "scalar" | "mixed"
    ) {
        return Err(WriteObjectsError::InvalidRecord {
            field: "modality",
            value: modality,
            reason: "must be one of text, json, image, video, audio, binary, scalar, mixed"
                .to_string(),
        });
    }
    Ok(modality)
}

fn validate_score(field: &'static str, value: Option<f32>) -> Result<(), WriteObjectsError> {
    if let Some(value) = value {
        if !(0.0..=1.0).contains(&value) {
            return Err(WriteObjectsError::InvalidRecord {
                field,
                value: value.to_string(),
                reason: "must be between 0 and 1".to_string(),
            });
        }
    }
    Ok(())
}

fn deterministic_uuid(namespace: &str, source_id: &Uuid, source_record_key: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"onecontext-memory-db/perception/v1\0");
    hasher.update(namespace.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_record_key.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn deterministic_series_id(user_id: &Uuid, source_id: &Uuid, series_key: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"onecontext-memory-db/perception/v2/series\0");
    hasher.update(user_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(series_key.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn copy_optional_i64_field(
    writer: &mut impl Write,
    value: Option<i64>,
) -> Result<(), WriteObjectsError> {
    copy_field(writer, value.map(|value| value.to_string()).as_deref())
}

fn copy_optional_i32_field(
    writer: &mut impl Write,
    value: Option<i32>,
) -> Result<(), WriteObjectsError> {
    copy_field(writer, value.map(|value| value.to_string()).as_deref())
}

fn copy_optional_f32_field(
    writer: &mut impl Write,
    value: Option<f32>,
) -> Result<(), WriteObjectsError> {
    copy_field(writer, value.map(|value| value.to_string()).as_deref())
}

fn copy_optional_f64_field(
    writer: &mut impl Write,
    value: Option<f64>,
) -> Result<(), WriteObjectsError> {
    copy_field(writer, value.map(|value| value.to_string()).as_deref())
}

fn copy_optional_bool_field(
    writer: &mut impl Write,
    value: Option<bool>,
) -> Result<(), WriteObjectsError> {
    copy_field(writer, value.map(|value| value.to_string()).as_deref())
}

fn copy_field(writer: &mut impl Write, value: Option<&str>) -> Result<(), WriteObjectsError> {
    write_copy_value(writer, value)?;
    writer.write_all(b"\t")?;
    Ok(())
}

fn copy_last_field(writer: &mut impl Write, value: Option<&str>) -> Result<(), WriteObjectsError> {
    write_copy_value(writer, value)
}

fn write_copy_value(writer: &mut impl Write, value: Option<&str>) -> Result<(), WriteObjectsError> {
    match value {
        Some(value) => {
            for byte in value.bytes() {
                match byte {
                    b'\\' => writer.write_all(br"\\")?,
                    b'\t' => writer.write_all(br"\t")?,
                    b'\n' => writer.write_all(br"\n")?,
                    b'\r' => writer.write_all(br"\r")?,
                    _ => writer.write_all(&[byte])?,
                }
            }
        }
        None => writer.write_all(br"\N")?,
    }
    Ok(())
}

fn default_role() -> String {
    "raw".to_string()
}

fn default_privacy_class() -> String {
    "normal".to_string()
}

fn default_time_semantics() -> String {
    "interval".to_string()
}

fn default_temporal_level() -> String {
    "event".to_string()
}

fn default_materialization_policy() -> String {
    "index_events".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(key: &str, payload: Value) -> PerceptionObjectInput {
        PerceptionObjectInput {
            client_record_id: None,
            source_id: "10000000-0000-0000-0000-000000000001".to_string(),
            source_record_key: key.to_string(),
            lane_id: "20000000-0000-0000-0000-000000000001".to_string(),
            series_kind: "codex_session".to_string(),
            series_key: "codex:session:test".to_string(),
            series_display_name: Some("test session".to_string()),
            series_parent_key: None,
            modality: None,
            kind: "codex_message".to_string(),
            role: default_role(),
            privacy_class: default_privacy_class(),
            event_start: "2026-05-25T10:00:00Z".to_string(),
            event_end: "2026-05-25T10:00:00.000001Z".to_string(),
            time_semantics: default_time_semantics(),
            temporal_level: default_temporal_level(),
            native_resolution_ns: None,
            stored_resolution_ns: None,
            indexed_resolution_ns: None,
            display_resolution_hint_ns: None,
            time_resolution_ns: None,
            time_uncertainty_ns: None,
            alignment_confidence: None,
            alignment_method: None,
            materialization_policy: default_materialization_policy(),
            importance_score: None,
            blob: None,
            body_type: None,
            text_value: None,
            number_value: None,
            bool_value: None,
            payload,
            display_title: Some("title".to_string()),
            display_text: Some("text".to_string()),
            edges: Vec::new(),
            source_start_ns: None,
            source_end_ns: None,
            source_sequence: None,
            media_start_offset_ns: None,
            media_end_offset_ns: None,
            schema_name: Some("codex_message".to_string()),
            schema_version: Some(1),
            confidence: Some(1.0),
            metadata: json!({"volatile":"ignored by source hash"}),
        }
    }

    #[test]
    fn same_batch_duplicates_are_receipted_without_leading_db_work() {
        let first = input("message/1", json!({"a":1}));
        let request = WriteObjectsRequest {
            user_id: "00000000-0000-0000-0000-000000000001".to_string(),
            write_id: "30000000-0000-0000-0000-000000000001".to_string(),
            atomicity: Some("chunk".to_string()),
            records: vec![first.clone(), first],
            chunk_size: None,
        };

        let plan = plan_write_objects(&request).unwrap();

        assert_eq!(plan.leader_count, 1);
        assert_eq!(plan.same_batch_duplicate_count, 1);
        assert!(plan.receipts[0].inserted);
        assert!(!plan.receipts[1].inserted);
        assert_eq!(
            plan.receipts[1].dedupe_reason.as_deref(),
            Some("same_batch")
        );
        assert_eq!(plan.receipts[1].duplicate_of_ordinal, Some(0));
        assert_eq!(plan.receipts[0].object_id, plan.receipts[1].object_id);
    }

    #[test]
    fn same_series_reuses_deterministic_series_id() {
        let request = WriteObjectsRequest {
            user_id: "00000000-0000-0000-0000-000000000001".to_string(),
            write_id: "30000000-0000-0000-0000-000000000001".to_string(),
            atomicity: Some("chunk".to_string()),
            records: vec![
                input("message/1", json!({"a":1})),
                input("message/2", json!({"a":2})),
            ],
            chunk_size: None,
        };

        let prepared = prepare_write_objects(&request).unwrap();

        assert_eq!(prepared.leaders.len(), 2);
        assert_eq!(prepared.leaders[0].series_id, prepared.leaders[1].series_id);
        assert_eq!(prepared.leaders[0].series_key, "codex:session:test");
        assert_eq!(
            prepared.leaders[0].default_lane_id,
            prepared.leaders[0].lane_id
        );
        assert_eq!(prepared.leaders[0].modality, "mixed");
        assert_eq!(prepared.leaders[0].body_type, "json");
    }

    #[test]
    fn same_batch_hash_conflict_fails_before_db_work() {
        let mut second = input("message/1", json!({"a":2}));
        second.display_text = Some("changed".to_string());
        let request = WriteObjectsRequest {
            user_id: "00000000-0000-0000-0000-000000000001".to_string(),
            write_id: "30000000-0000-0000-0000-000000000001".to_string(),
            atomicity: Some("chunk".to_string()),
            records: vec![input("message/1", json!({"a":1})), second],
            chunk_size: None,
        };

        let error = plan_write_objects(&request).unwrap_err();

        assert_eq!(error.code(), "SOURCE_IDENTITY_HASH_CONFLICT_IN_BATCH");
    }

    #[test]
    fn agent_derived_objects_are_mutable_for_incremental_session_reduction() {
        for kind in [
            "agent_session",
            "agent_turn",
            "agent_message",
            "agent_tool_summary",
            "agent_runtime_event",
            "agent_prompt_snapshot",
            "agent_compaction",
        ] {
            assert!(is_mutable_container_kind(kind), "{kind} should be mutable");
        }
        assert!(!is_mutable_container_kind("observation"));
    }

    #[test]
    fn canonical_hash_is_stable_for_json_key_ordering_and_ignores_metadata() {
        let mut first = input("message/1", json!({"b":2,"a":1}));
        let mut second = input("message/1", json!({"a":1,"b":2}));
        first.metadata = json!({"ingest_time":"one"});
        second.metadata = json!({"ingest_time":"two"});

        assert_eq!(
            canonical_source_hash(&first).unwrap(),
            canonical_source_hash(&second).unwrap()
        );
    }

    #[test]
    fn body_validation_fails_before_db_work_for_required_value_columns() {
        let cases = [
            ("text", "text_value is required"),
            ("number", "number_value is required"),
            ("boolean", "bool_value is required"),
            ("blob", "blob is required"),
        ];

        for (body_type, expected_reason) in cases {
            let mut record = input(&format!("body/{body_type}"), json!({"a":1}));
            record.body_type = Some(body_type.to_string());
            record.text_value = None;
            record.number_value = None;
            record.bool_value = None;
            record.blob = None;
            let request = WriteObjectsRequest {
                user_id: "00000000-0000-0000-0000-000000000001".to_string(),
                write_id: "30000000-0000-0000-0000-000000000001".to_string(),
                atomicity: Some("chunk".to_string()),
                records: vec![record],
                chunk_size: None,
            };

            let error = plan_write_objects(&request).unwrap_err();

            assert_eq!(error.code(), "INVALID_RECORD");
            assert!(
                error.to_string().contains(expected_reason),
                "unexpected error for body_type={body_type}: {error}"
            );
        }
    }

    #[test]
    fn ten_thousand_records_plan_to_five_default_chunks() {
        let records = (0..10_000)
            .map(|index| input(&format!("message/{index}"), json!({"index": index})))
            .collect::<Vec<_>>();
        let request = WriteObjectsRequest {
            user_id: "00000000-0000-0000-0000-000000000001".to_string(),
            write_id: "30000000-0000-0000-0000-000000000001".to_string(),
            atomicity: Some("chunk".to_string()),
            records,
            chunk_size: None,
        };

        let plan = plan_write_objects(&request).unwrap();

        assert_eq!(plan.chunk_count, 5);
        assert_eq!(plan.leader_count, 10_000);
    }
}
