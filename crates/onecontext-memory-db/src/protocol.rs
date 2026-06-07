//! Typed Perception DB protocol DTOs.
//!
//! The process and crate still use `memory` names during V0, but the protocol
//! surface is the Perception DB API described in `docs/memory-db-api-protocol-spec.md`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PROTOCOL_SCHEMA_VERSION: u32 = 1;
pub const PROTOCOL_SURFACE: &str = "perception_db";
pub const SERVICE_NAME: &str = "onecontext-memoryd";
pub const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolRequest<T = Value> {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub method: MethodName,
    #[serde(default)]
    pub params: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolResponse<T = Value> {
    pub schema_version: u32,
    pub protocol: String,
    pub surface: String,
    pub status: ProtocolResponseStatus,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub result: Option<T>,
    #[serde(default)]
    pub error: Option<ProtocolError>,
    pub stats: ProtocolStats,
}

impl<T> ProtocolResponse<T> {
    pub fn ok(
        method: MethodName,
        request_id: Option<String>,
        result: T,
        stats: ProtocolStats,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_SCHEMA_VERSION,
            protocol: method.protocol_version().to_string(),
            surface: PROTOCOL_SURFACE.to_string(),
            status: ProtocolResponseStatus::Ok,
            request_id,
            result: Some(result),
            error: None,
            stats,
        }
    }

    pub fn error(
        method: MethodName,
        request_id: Option<String>,
        error: ProtocolError,
        stats: ProtocolStats,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_SCHEMA_VERSION,
            protocol: method.protocol_version().to_string(),
            surface: PROTOCOL_SURFACE.to_string(),
            status: ProtocolResponseStatus::Error,
            request_id,
            result: None,
            error: Some(error),
            stats,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolResponseStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default)]
    pub details: Value,
}

impl ProtocolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            details: Value::Object(Map::new()),
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolStats {
    pub elapsed_ms: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ProtocolStats {
    pub fn elapsed(elapsed_ms: u64) -> Self {
        Self {
            elapsed_ms,
            extra: BTreeMap::new(),
        }
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.extra.insert(
            key.into(),
            serde_json::to_value(value).unwrap_or_else(|_| Value::Null),
        );
        self
    }
}

impl Default for ProtocolStats {
    fn default() -> Self {
        Self::elapsed(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MethodName {
    Status,
    Describe,
    StorageHealth,
    EnsureStorageReady,
    EnsureRecentBackfill,
    RepairStorage,
    StopStorage,
    RestartStorage,
    StorageDiagnostics,
    WriteObjects,
    IngestSources,
    QueryViewport,
    QueryDensity,
    HydrateObjects,
    QueryEdges,
    SearchText,
    SearchSemantic,
    Explain,
    Subscribe,
}

impl MethodName {
    pub const ALL_V0: [MethodName; 19] = [
        MethodName::Status,
        MethodName::Describe,
        MethodName::StorageHealth,
        MethodName::EnsureStorageReady,
        MethodName::EnsureRecentBackfill,
        MethodName::RepairStorage,
        MethodName::StopStorage,
        MethodName::RestartStorage,
        MethodName::StorageDiagnostics,
        MethodName::WriteObjects,
        MethodName::IngestSources,
        MethodName::QueryViewport,
        MethodName::QueryDensity,
        MethodName::HydrateObjects,
        MethodName::QueryEdges,
        MethodName::SearchText,
        MethodName::SearchSemantic,
        MethodName::Explain,
        MethodName::Subscribe,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "memory.status",
            Self::Describe => "memory.describe",
            Self::StorageHealth => "memory.storageHealth",
            Self::EnsureStorageReady => "memory.ensureStorageReady",
            Self::EnsureRecentBackfill => "memory.ensureRecentBackfill",
            Self::RepairStorage => "memory.repairStorage",
            Self::StopStorage => "memory.stopStorage",
            Self::RestartStorage => "memory.restartStorage",
            Self::StorageDiagnostics => "memory.storageDiagnostics",
            Self::WriteObjects => "memory.writeObjects",
            Self::IngestSources => "memory.ingestSources",
            Self::QueryViewport => "memory.queryViewport",
            Self::QueryDensity => "memory.queryDensity",
            Self::HydrateObjects => "memory.hydrateObjects",
            Self::QueryEdges => "memory.queryEdges",
            Self::SearchText => "memory.searchText",
            Self::SearchSemantic => "memory.searchSemantic",
            Self::Explain => "memory.explain",
            Self::Subscribe => "memory.subscribe",
        }
    }

    pub fn short_name(self) -> &'static str {
        self.as_str()
            .strip_prefix("memory.")
            .expect("memory protocol method")
    }

    pub fn protocol_version(self) -> String {
        format!("{}.v1", self.as_str())
    }

    pub fn state(self) -> MethodState {
        match self {
            Self::Status | Self::Describe => MethodState::Ready,
            Self::StorageHealth | Self::EnsureStorageReady => MethodState::Ready,
            Self::WriteObjects | Self::IngestSources => MethodState::Ready,
            Self::QueryViewport
            | Self::QueryDensity
            | Self::HydrateObjects
            | Self::QueryEdges
            | Self::SearchText
            | Self::Explain => MethodState::Ready,
            Self::EnsureRecentBackfill
            | Self::RepairStorage
            | Self::StopStorage
            | Self::RestartStorage
            | Self::StorageDiagnostics => MethodState::Ready,
            Self::SearchSemantic | Self::Subscribe => MethodState::Stub,
        }
    }

    pub fn request_type(self) -> &'static str {
        match self {
            Self::Status => "StatusRequest",
            Self::Describe => "DescribeRequest",
            Self::StorageHealth => "StorageHealthRequest",
            Self::EnsureStorageReady => "EnsureStorageReadyRequest",
            Self::EnsureRecentBackfill => "EnsureRecentBackfillRequest",
            Self::RepairStorage => "RepairStorageRequest",
            Self::StopStorage => "StorageLifecycleRequest",
            Self::RestartStorage => "StorageLifecycleRequest",
            Self::StorageDiagnostics => "StorageDiagnosticsRequest",
            Self::WriteObjects => "WriteObjectsRequest",
            Self::IngestSources => "IngestSourcesRequest",
            Self::QueryViewport => "QueryViewportRequest",
            Self::QueryDensity => "QueryDensityRequest",
            Self::HydrateObjects => "HydrateObjectsRequest",
            Self::QueryEdges => "QueryEdgesRequest",
            Self::SearchText => "SearchTextRequest",
            Self::SearchSemantic => "SearchSemanticRequest",
            Self::Explain => "ExplainRequest",
            Self::Subscribe => "SubscribeRequest",
        }
    }

    pub fn response_type(self) -> &'static str {
        match self {
            Self::Status => "StatusResponse",
            Self::Describe => "DescribeResponse",
            Self::StorageHealth => "StorageHealth",
            Self::EnsureStorageReady => "StorageHealth",
            Self::EnsureRecentBackfill => "RecentBackfillHealth",
            Self::RepairStorage => "StorageHealth",
            Self::StopStorage => "StorageHealth",
            Self::RestartStorage => "StorageHealth",
            Self::StorageDiagnostics => "StorageDiagnostics",
            Self::WriteObjects => "WriteObjectsResponse",
            Self::IngestSources => "IngestSourcesResponse",
            Self::QueryViewport => "QueryViewportResponse",
            Self::QueryDensity => "QueryDensityResponse",
            Self::HydrateObjects => "HydrateObjectsResponse",
            Self::QueryEdges => "QueryEdgesResponse",
            Self::SearchText => "SearchTextResponse",
            Self::SearchSemantic => "SearchSemanticResponse",
            Self::Explain => "ExplainResponse",
            Self::Subscribe => "SubscribeResponse",
        }
    }
}

impl std::fmt::Display for MethodName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for MethodName {
    type Err = ParseMethodNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim();
        let without_prefix = normalized.strip_prefix("memory.").unwrap_or(normalized);
        match without_prefix {
            "status" => Ok(Self::Status),
            "describe" => Ok(Self::Describe),
            "storageHealth" | "storage-health" | "storage_health" => Ok(Self::StorageHealth),
            "ensureStorageReady" | "ensure-storage-ready" | "ensure_storage_ready" => {
                Ok(Self::EnsureStorageReady)
            }
            "ensureRecentBackfill" | "ensure-recent-backfill" | "ensure_recent_backfill" => {
                Ok(Self::EnsureRecentBackfill)
            }
            "repairStorage" | "repair-storage" | "repair_storage" => Ok(Self::RepairStorage),
            "stopStorage" | "stop-storage" | "stop_storage" => Ok(Self::StopStorage),
            "restartStorage" | "restart-storage" | "restart_storage" => Ok(Self::RestartStorage),
            "storageDiagnostics" | "storage-diagnostics" | "storage_diagnostics" => {
                Ok(Self::StorageDiagnostics)
            }
            "writeObjects" | "write-objects" | "write_objects" => Ok(Self::WriteObjects),
            "ingestSources" | "ingest-sources" | "ingest_sources" => Ok(Self::IngestSources),
            "queryViewport" | "viewport" | "query-viewport" | "query_viewport" => {
                Ok(Self::QueryViewport)
            }
            "queryDensity" | "density" | "query-density" | "query_density" => {
                Ok(Self::QueryDensity)
            }
            "hydrateObjects" | "hydrate" | "hydrate-objects" | "hydrate_objects" => {
                Ok(Self::HydrateObjects)
            }
            "queryEdges" | "edges" | "query-edges" | "query_edges" => Ok(Self::QueryEdges),
            "searchText" | "search-text" | "search_text" => Ok(Self::SearchText),
            "searchSemantic" | "semantic" | "search-semantic" | "search_semantic" => {
                Ok(Self::SearchSemantic)
            }
            "explain" => Ok(Self::Explain),
            "subscribe" => Ok(Self::Subscribe),
            _ => Err(ParseMethodNameError {
                value: value.to_string(),
            }),
        }
    }
}

impl Serialize for MethodName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MethodName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        MethodName::from_str(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMethodNameError {
    pub value: String,
}

impl std::fmt::Display for ParseMethodNameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unknown memory protocol method {:?}", self.value)
    }
}

impl std::error::Error for ParseMethodNameError {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MethodState {
    Ready,
    DispatcherStub,
    DbReadWiringPending,
    Stub,
}

impl MethodState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::DispatcherStub => "dispatcher_stub",
            Self::DbReadWiringPending => "db_read_wiring_pending",
            Self::Stub => "stub",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MethodDescriptor {
    pub state: MethodState,
    pub request_type: String,
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryProtocolOperation {
    pub name: String,
    pub status: String,
    pub request_type: String,
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryProtocolDescribeResponse {
    pub ok: bool,
    pub schema_version: u32,
    pub protocol: String,
    pub surface: String,
    pub service: String,
    pub version: String,
    pub methods: BTreeMap<String, MethodDescriptor>,
    pub operations: Vec<MemoryProtocolOperation>,
}

pub type DescribeResponse = MemoryProtocolDescribeResponse;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusResponse {
    pub ok: bool,
    pub service: String,
    pub version: String,
    pub database: StorageStatus,
    pub enabled_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageStatus {
    pub configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reachable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_state: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    ManagedPostgres,
    ExternalPostgres,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StorageHealthRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnsureStorageReadyRequest {
    pub reason: String,
    #[serde(default)]
    pub repair: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnsureRecentBackfillRequest {
    pub reason: String,
    #[serde(default = "default_recent_backfill_window_hours")]
    pub window_hours: u32,
    #[serde(default)]
    pub block_until_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_density_buckets: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairStorageRequest {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageLifecycleRequest {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageDiagnosticsRequest {
    #[serde(default)]
    pub include_logs: bool,
    #[serde(default = "default_true")]
    pub redact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionHealth {
    pub name: String,
    pub required: bool,
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub preload_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preload_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentBackfillHealth {
    pub status: String,
    pub window_hours: u32,
    pub density_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_ingest_ts: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageHealth {
    pub backend: StorageBackend,
    pub status: String,
    pub ready: bool,
    pub storage_ready: bool,
    pub recent_history_ready: bool,
    pub user_action_required: bool,
    pub safe_to_retry: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_schema_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_support_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pgdata_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_postgres_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_timescale_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_build_id: Option<String>,
    #[serde(default)]
    pub required_extensions: Vec<ExtensionHealth>,
    pub recent_backfill: RecentBackfillHealth,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageDiagnostics {
    pub ok: bool,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DescribeRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteObjectsRequest {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_id: Option<String>,
    #[serde(default = "default_atomicity")]
    pub atomicity: String,
    #[serde(default)]
    pub records: Vec<PerceptionObjectInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionObjectInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_record_id: Option<String>,
    pub source_id: String,
    pub source_record_key: String,
    pub lane_id: String,
    #[serde(default)]
    pub series_kind: String,
    #[serde(default)]
    pub series_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_parent_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modality: Option<String>,
    pub kind: String,
    pub role: String,
    pub privacy_class: String,
    pub event_start: String,
    pub event_end: String,
    #[serde(default = "default_time_semantics")]
    pub time_semantics: String,
    #[serde(default = "default_temporal_level")]
    pub temporal_level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_resolution_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_uncertainty_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment_confidence: Option<f32>,
    #[serde(default = "default_materialization_policy")]
    pub materialization_policy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub importance_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,
    #[serde(default)]
    pub edges: Vec<PerceptionEdgeInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobDescriptor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_id: Option<String>,
    pub storage_backend: String,
    pub uri: String,
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionEdgeInput {
    pub to_object_id: String,
    pub edge_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteObjectsResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_id: Option<String>,
    pub chunk_count: usize,
    pub record_count: usize,
    pub inserted_count: usize,
    pub duplicate_count: usize,
    pub receipts: Vec<SourceRecordReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRecordReceipt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_record_id: Option<String>,
    pub source_id: String,
    pub source_record_key: String,
    pub source_record_id: String,
    pub object_id: String,
    pub event_start: String,
    pub inserted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_of_ordinal: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestSourcesRequest {
    pub user_id: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_events: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_sensitive_text: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestSourcesResponse {
    pub ok: bool,
    pub source_results: Vec<IngestSourceResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestSourceResult {
    pub source_id: String,
    pub read_count: usize,
    pub written_count: usize,
    pub inserted_count: usize,
    pub duplicate_count: usize,
    pub cursor_advanced: bool,
    pub advancement_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolTimeRange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProtocolFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_keys: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_classes: Option<Vec<String>>,
    #[serde(default)]
    pub include_invalid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pagination {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            cursor: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectInclude {
    #[serde(default)]
    pub payload: bool,
    #[serde(default = "default_true")]
    pub blob_descriptor: bool,
    #[serde(default = "default_true")]
    pub source_record: bool,
    #[serde(default)]
    pub edges_count: bool,
    #[serde(default)]
    pub edges: bool,
}

impl Default for ObjectInclude {
    fn default() -> Self {
        Self {
            payload: false,
            blob_descriptor: true,
            source_record: true,
            edges_count: false,
            edges: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryViewportRequest {
    pub user_id: String,
    pub time: ProtocolTimeRange,
    #[serde(default)]
    pub filters: ProtocolFilters,
    #[serde(default)]
    pub pagination: Pagination,
    #[serde(default)]
    pub include: ObjectInclude,
    #[serde(default)]
    pub explain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryViewportResponse {
    pub ok: bool,
    pub objects: Vec<PerceptionObjectSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionObjectSummary {
    pub object_id: String,
    pub event_start: String,
    pub event_end: String,
    pub lane_id: String,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_default_lane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
    pub kind: String,
    pub role: String,
    pub privacy_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    pub time_semantics: String,
    pub temporal_level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_resolution_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_uncertainty_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment_confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub importance_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobDescriptor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub edge_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryDensityRequest {
    pub user_id: String,
    pub time: ProtocolTimeRange,
    pub bucket: String,
    #[serde(default)]
    pub filters: ProtocolFilters,
    #[serde(default)]
    pub explain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryDensityResponse {
    pub ok: bool,
    pub bucket: String,
    pub buckets: Vec<DensityBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DensityBucket {
    pub bucket_start: String,
    pub lane_id: String,
    pub kind: String,
    pub role: String,
    pub privacy_class: String,
    pub object_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_importance: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HydrateObjectsRequest {
    pub user_id: String,
    pub object_ids: Vec<String>,
    #[serde(default)]
    pub include: ObjectInclude,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HydrateObjectsResponse {
    pub ok: bool,
    pub objects: Vec<PerceptionObjectHydration>,
    pub missing_object_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionObjectHydration {
    pub object_id: String,
    pub event_start: String,
    pub event_end: String,
    pub lane_id: String,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_default_lane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record: Option<SourceRecordIdentity>,
    pub kind: String,
    pub role: String,
    pub privacy_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<HydratedEdges>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRecordIdentity {
    pub source_record_id: String,
    pub source_record_key: String,
    pub source_record_hash: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub seen_count: i64,
    #[serde(default)]
    pub conflict_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HydratedEdges {
    #[serde(default)]
    pub out: Vec<PerceptionEdge>,
    #[serde(default)]
    pub r#in: Vec<PerceptionEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryEdgesRequest {
    pub user_id: String,
    pub object_ids: Vec<String>,
    #[serde(default = "default_edge_direction")]
    pub direction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_kinds: Option<Vec<String>>,
    #[serde(default = "default_edge_limit")]
    pub limit: usize,
    #[serde(default)]
    pub hydrate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryEdgesResponse {
    pub ok: bool,
    pub edges: Vec<PerceptionEdge>,
    #[serde(default)]
    pub objects: Vec<PerceptionObjectSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionEdge {
    pub edge_id: String,
    pub from_object_id: String,
    pub from_object_event_start: String,
    pub to_object_id: String,
    pub to_object_event_start: String,
    pub edge_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchTextRequest {
    pub user_id: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<ProtocolTimeRange>,
    #[serde(default)]
    pub filters: ProtocolFilters,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchTextResponse {
    pub ok: bool,
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    #[serde(flatten)]
    pub object: PerceptionObjectSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSemanticRequest {
    pub user_id: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

pub type SearchSemanticResponse = SearchTextResponse;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExplainRequest {
    pub target: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub include_raw_plan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExplainResponse {
    pub ok: bool,
    pub target: String,
    pub sql_kind: String,
    pub plan: ExplainPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExplainPlan {
    pub format: String,
    pub summary: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SubscribeRequest {
    #[serde(default)]
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeResponse {
    pub ok: bool,
    pub state: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryProtocolStubResponse {
    pub ok: bool,
    pub method: String,
    pub state: MethodState,
    pub message: String,
    #[serde(default)]
    pub request: Value,
}

pub fn describe_memory_query_protocol() -> MemoryProtocolDescribeResponse {
    let mut methods = BTreeMap::new();
    let mut operations = Vec::new();
    for method in MethodName::ALL_V0 {
        let state = method.state();
        let request_type = method.request_type().to_string();
        let response_type = method.response_type().to_string();
        methods.insert(
            method.as_str().to_string(),
            MethodDescriptor {
                state,
                request_type: request_type.clone(),
                response_type: response_type.clone(),
            },
        );
        operations.push(MemoryProtocolOperation {
            name: method.as_str().to_string(),
            status: state.as_str().to_string(),
            request_type,
            response_type,
        });
    }
    MemoryProtocolDescribeResponse {
        ok: true,
        schema_version: PROTOCOL_SCHEMA_VERSION,
        protocol: "memory.protocol.v1".to_string(),
        surface: PROTOCOL_SURFACE.to_string(),
        service: SERVICE_NAME.to_string(),
        version: SERVICE_VERSION.to_string(),
        methods,
        operations,
    }
}

pub fn protocol_stub(method: MethodName, message: impl Into<String>, request: Value) -> Value {
    serde_json::to_value(MemoryProtocolStubResponse {
        ok: false,
        method: method.as_str().to_string(),
        state: method.state(),
        message: message.into(),
        request,
    })
    .expect("stub response serializes")
}

fn default_schema_version() -> u32 {
    PROTOCOL_SCHEMA_VERSION
}

fn default_atomicity() -> String {
    "chunk".to_string()
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

fn default_limit() -> usize {
    5_000
}

fn default_true() -> bool {
    true
}

fn default_edge_direction() -> String {
    "both".to_string()
}

fn default_edge_limit() -> usize {
    1_000
}

fn default_search_limit() -> usize {
    50
}

fn default_recent_backfill_window_hours() -> u32 {
    72
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn protocol_describe_lists_all_v0_methods() {
        let describe = describe_memory_query_protocol();

        for method in MethodName::ALL_V0 {
            assert!(
                describe.methods.contains_key(method.as_str()),
                "{} missing from describe",
                method.as_str()
            );
        }
        assert_eq!(describe.methods.len(), MethodName::ALL_V0.len());
        assert_eq!(
            describe.methods["memory.searchSemantic"].state,
            MethodState::Stub
        );
        assert_eq!(
            describe.methods["memory.storageHealth"].state,
            MethodState::Ready
        );
        assert_eq!(
            describe.methods["memory.ensureStorageReady"].state,
            MethodState::Ready
        );
        assert_eq!(
            describe.methods["memory.ensureRecentBackfill"].state,
            MethodState::Ready
        );
        assert_eq!(
            describe.methods["memory.repairStorage"].state,
            MethodState::Ready
        );
        assert_eq!(
            describe.methods["memory.stopStorage"].state,
            MethodState::Ready
        );
        assert_eq!(
            describe.methods["memory.restartStorage"].state,
            MethodState::Ready
        );
        assert_eq!(
            describe.methods["memory.storageDiagnostics"].state,
            MethodState::Ready
        );
        assert_eq!(
            describe.methods["memory.subscribe"].state,
            MethodState::Stub
        );
    }

    #[test]
    fn protocol_response_envelope_has_shared_shape() {
        let response = ProtocolResponse::ok(
            MethodName::Describe,
            Some("req-1".to_string()),
            json!({"ok": true}),
            ProtocolStats::elapsed(12).with("record_count", 3),
        );
        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["protocol"], "memory.describe.v1");
        assert_eq!(value["surface"], "perception_db");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["request_id"], "req-1");
        assert_eq!(value["result"]["ok"], true);
        assert!(value["error"].is_null());
        assert_eq!(value["stats"]["elapsed_ms"], 12);
        assert_eq!(value["stats"]["record_count"], 3);
    }

    #[test]
    fn protocol_method_name_accepts_prefixed_and_short_names() {
        assert_eq!(
            MethodName::from_str("memory.queryViewport").unwrap(),
            MethodName::QueryViewport
        );
        assert_eq!(
            MethodName::from_str("query-density").unwrap(),
            MethodName::QueryDensity
        );
    }

    #[test]
    fn protocol_method_name_rejects_ambiguous_short_aliases() {
        assert!(MethodName::from_str("search").is_err());
        assert!(MethodName::from_str("objects").is_err());
    }
}
