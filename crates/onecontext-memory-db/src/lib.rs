//! 1Context memory database contract.
//!
//! This crate is intentionally small: it owns the object envelope and the
//! current database schema bootstrap before live capture adapters are wired up.

pub mod agent_session_emitter;
pub mod agent_session_ir;
pub mod claude_agent_ingest;
pub mod codex_agent_ingest;
pub mod db;
pub mod edges;
pub mod envelope;
pub mod hydrate;
pub mod ingest_sources;
pub mod local_adapters;
pub mod local_postgres;
pub mod projections;
pub mod protocol;
pub mod query_density;
pub mod read_viewport;
pub mod schema;
pub mod search;
pub mod semantic_observation;
pub mod source_connector;
pub mod source_cursors;
pub mod source_identity;
pub mod write_objects;

pub use agent_session_emitter::{
    agent_session_source_record_key, agent_turn_source_record_key, emit_agent_session_objects,
    emit_agent_session_objects_with_options, AgentSessionEmitOptions, AgentSessionEmitterError,
};
pub use agent_session_ir::{
    compact_hash_bytes, compact_json_hash, compact_text_hash, default_item_projections,
    mark_prompt_snapshot_projection, prompt_input_hash, replacement_history_hash,
    source_key_compaction, source_key_item, source_key_line, source_key_prompt_snapshot,
    source_key_session, source_key_tool_summary, source_key_turn, AgentCompactionIr,
    AgentIngestProfile, AgentIrValidationError, AgentItemIr, AgentItemKind, AgentItemRole,
    AgentProjection, AgentPromptSnapshotIr, AgentRuntimeEventIr, AgentRuntimeSeverity,
    AgentSessionIr, AgentSource, AgentTurnIr, AgentTurnStatus, RawEvidenceRef,
};
pub use db::{
    redact_database_url, resolve_database_url, DatabaseUrl, DbError, MemoryDatabase,
    DATABASE_URL_ENV,
};
pub use envelope::{BlobEnvelope, CaptureEnvelope, EnvelopeValidationError};
pub use ingest_sources::{ingest_sources_with_client, IngestSourcesError};
pub use local_adapters::{
    default_context_for_source, default_home_dir, ingest_claude_incremental,
    ingest_codex_incremental, ingest_imessage_incremental, probe_local_sources,
    sample_claude_objects, sample_codex_objects, sample_imessage_objects, AdapterContext,
    AdapterError, AdapterSampleOptions, FileIngestCursor, IncrementalIngestBatch,
    IncrementalIngestOptions, IncrementalIngestReport, LocalIngestCursors, SessionIngestProfile,
    SqliteIngestCursor,
};
pub use local_postgres::{
    current_manifest_arch, default_app_support_dir, default_managed_pg_prefix,
    ensure_managed_postgres_ready, ensure_managed_postgres_ready_with_config,
    ensure_managed_postgres_ready_with_options, ensure_managed_postgres_ready_with_runner,
    ensure_managed_postgres_ready_with_runner_and_bootstrapper, probe_managed_postgres_read_only,
    probe_managed_postgres_read_only_with_config, required_extension_requirements,
    ManagedPgManifest, ManagedPgManifestBins, ManagedPgManifestExtensions, ManagedPgManifestFile,
    ManagedPgPathConfig, ManagedPgPaths, ManagedPostgresCommandOutput, ManagedPostgresCommandPlan,
    ManagedPostgresCommandRunner, ManagedPostgresConfigPlan, ManagedPostgresEnsureOptions,
    ManagedPostgresError, ManagedPostgresExtensionRequirement, ManagedPostgresReadOnlyHealth,
    ManagedPostgresReadOnlyState, ManagedPostgresReadyState, ManagedPostgresSchemaBootstrapSummary,
    ManagedPostgresSchemaBootstrapper, StdManagedPostgresCommandRunner,
    StdManagedPostgresSchemaBootstrapper, APP_SUPPORT_DIR_ENV, EXPECTED_POSTGRES_MAJOR,
    MANAGED_PG_PREFIX_ENV, MANAGED_POSTGRES_APP_PASSWORD, MANAGED_POSTGRES_APP_USER,
    MANAGED_POSTGRES_CONNECT_TIMEOUT_SECS, MANAGED_POSTGRES_DATABASE, MANAGED_POSTGRES_PORT,
    MANAGED_POSTGRES_SUPERUSER, MANIFEST_FILE_NAME, REQUIRED_MANAGED_POSTGRES_EXTENSIONS,
    REQUIRED_MANAGED_POSTGRES_PRELOAD_LIBRARIES,
};
pub use projections::{
    list_timeline_projections, query_projection_items, ListTimelineProjectionsRequest,
    ListTimelineProjectionsResponse, QueryProjectionItemsRequest, QueryProjectionItemsResponse,
    TimelineProjection, TimelineProjectionItem,
};
pub use protocol::{
    describe_memory_query_protocol, protocol_stub, BlobDescriptor, DensityBucket, DescribeRequest,
    DescribeResponse, EnsureRecentBackfillRequest, EnsureStorageReadyRequest, ExplainPlan,
    ExplainRequest, ExplainResponse, ExtensionHealth, HydrateObjectsRequest,
    HydrateObjectsResponse, HydratedEdges, IngestSourceResult, IngestSourcesRequest,
    IngestSourcesResponse, MemoryProtocolDescribeResponse, MemoryProtocolOperation,
    MemoryProtocolStubResponse, MethodDescriptor, MethodName, MethodState, ObjectInclude,
    Pagination, PerceptionEdge, PerceptionEdgeInput, PerceptionObjectHydration,
    PerceptionObjectInput, PerceptionObjectSummary, ProtocolError, ProtocolFilters,
    ProtocolRequest, ProtocolResponse, ProtocolResponseStatus, ProtocolStats, ProtocolTimeRange,
    QueryDensityRequest, QueryDensityResponse, QueryEdgesRequest, QueryEdgesResponse,
    QueryViewportRequest, QueryViewportResponse, RecentBackfillHealth, RepairStorageRequest,
    SearchResult, SearchSemanticRequest, SearchSemanticResponse, SearchTextRequest,
    SearchTextResponse, SourceRecordIdentity, SourceRecordReceipt, StatusRequest, StatusResponse,
    StorageBackend, StorageDiagnostics, StorageDiagnosticsRequest, StorageHealth,
    StorageHealthRequest, StorageStatus, SubscribeRequest, SubscribeResponse, WriteObjectsRequest,
    WriteObjectsResponse,
};
pub use schema::{
    bootstrap_current_schema, bootstrap_current_schema_with_client, current_schema_checksum,
    current_schema_sql, validate_current_schema_with_client, CurrentSchemaBootstrapReport,
    CurrentSchemaError, CurrentSchemaStatus, BASELINE_SCHEMA_NAME, BASELINE_SCHEMA_VERSION,
    CURRENT_SCHEMA_SQL, REQUIRED_CURRENT_RELATIONS,
};
pub use semantic_observation::{
    canonicalize_text_spans, detect_scroll_session, normalize_text_span, score_salience,
    AgentObservationPacket, CanonicalTextSpan, CaptureSourceRef, EvidenceRef,
    ObservationProvenanceEdge, ObservationRect, ObservationTimeRange, ObservedTextSpan,
    ObservedUiElement, PacketEvidence, SalienceDecision, SalienceGrade, ScrollDocument,
    ScrollSessionDecision, ScrollSessionFeatures, SemanticChangeFeatures, UserEvent,
    AGENT_OBSERVATION_PACKET_SCHEMA, SCROLL_DOCUMENT_SCHEMA,
};
pub use source_connector::{
    default_source_connectors, ConnectorAccessMode, ConnectorReadPosture, ConnectorValidationError,
    SourceConnectorSpec, SourceProbeReport,
};
pub use source_cursors::{
    advance_source_cursor_after_db_success, cursor_advance_decision, load_source_cursor,
    CursorAdvanceDecision, CursorAdvanceMode, SourceCursor, SourceCursorError,
};
pub use source_identity::{
    canonical_json_bytes, canonical_source_hash, detect_same_batch_duplicates, deterministic_uuid,
    object_id, source_record_id, SameBatchDuplicate, SourceIdentityFingerprint,
    SourceIdentityHashConflict, OBJECT_NAMESPACE_LABEL, SOURCE_RECORD_NAMESPACE_LABEL,
};
