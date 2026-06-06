use onecontext_memory_db::{current_schema_sql, redact_database_url, REQUIRED_CURRENT_RELATIONS};

#[test]
fn current_schema_is_perception_only() {
    let sql = current_schema_sql();

    assert!(sql.contains("CREATE EXTENSION IF NOT EXISTS timescaledb"));
    assert!(sql.contains("create_hypertable"));
    assert!(sql.contains("CREATE SCHEMA IF NOT EXISTS perception"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.schema_migrations"));
    let migration_ledger_sql = sql
        .split("CREATE TABLE IF NOT EXISTS perception.schema_migrations")
        .nth(1)
        .and_then(|tail| tail.split("CREATE TABLE IF NOT EXISTS app.users").next())
        .expect("schema migration ledger should precede app.users");
    assert!(migration_ledger_sql.contains("version BIGINT PRIMARY KEY"));
    assert!(migration_ledger_sql.contains("name TEXT NOT NULL UNIQUE"));
    assert!(migration_ledger_sql.contains("checksum TEXT NOT NULL"));
    assert!(migration_ledger_sql.contains("app_version TEXT"));
    assert!(migration_ledger_sql.contains("destructive_bootstrap BOOLEAN NOT NULL DEFAULT false"));
    assert!(!migration_ledger_sql.contains("description TEXT NOT NULL"));
    assert!(!migration_ledger_sql.contains("metadata JSONB"));
    assert!(!migration_ledger_sql.contains("INSERT INTO perception.schema_migrations"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.sources"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.lanes"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.series"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.blobs"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.objects"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.object_edges"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.source_cursors"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.timeline_projections"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.timeline_projection_items"));
    assert!(sql.contains("CREATE MATERIALIZED VIEW IF NOT EXISTS perception.object_density_1m"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS search.object_embeddings"));
    assert!(!sql.contains("capture."));
    assert!(!sql.contains("captured_objects"));
    assert!(!sql.contains("app.schema_migrations"));
}

#[test]
fn required_relations_match_current_read_write_surface() {
    assert_eq!(
        REQUIRED_CURRENT_RELATIONS,
        &[
            "app.users",
            "perception.schema_migrations",
            "perception.sources",
            "perception.lanes",
            "perception.series",
            "perception.blobs",
            "perception.source_records",
            "perception.objects",
            "perception.object_edges",
            "perception.object_density_1m",
            "perception.source_cursors",
            "perception.timeline_projections",
            "perception.timeline_projection_items",
            "search.object_embeddings",
        ]
    );
}

#[test]
fn perception_series_is_canonical_grouping_contract() {
    let sql = current_schema_sql();

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.series"));
    assert!(sql.contains("series_id UUID PRIMARY KEY"));
    assert!(sql.contains("series_kind TEXT NOT NULL"));
    assert!(sql.contains("series_key TEXT NOT NULL"));
    assert!(sql.contains("default_lane_id UUID NOT NULL"));
    assert!(!sql.contains("  lane_id UUID NOT NULL,\n\n  series_kind TEXT NOT NULL"));
    assert!(sql.contains("parent_series_id UUID"));
    assert!(sql.contains("UNIQUE (user_id, source_id, series_key)"));
    assert!(sql.contains("series_user_default_lane_idx"));
    assert!(sql.contains("series_user_default_lane_last_seen_idx"));
    assert!(sql.contains("objects_series_live_time_idx"));
    assert!(!sql.contains("instance_kind"));
    assert!(!sql.contains("instance_id"));
}

#[test]
fn perception_source_records_own_identity_uniqueness_and_series_mapping() {
    let sql = current_schema_sql();

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.source_records"));
    assert!(sql.contains("source_record_id UUID PRIMARY KEY"));
    assert!(sql.contains("source_record_key TEXT NOT NULL"));
    assert!(sql.contains("source_record_hash TEXT NOT NULL"));
    assert!(sql.contains("series_id UUID NOT NULL"));
    assert!(sql.contains("UNIQUE (source_id, source_record_key)"));
    assert!(sql.contains("UNIQUE (object_id)"));
    assert!(sql.contains("source_records_user_time_idx"));
    assert!(sql.contains("source_records_source_seen_idx"));
    assert!(sql.contains("source_records_series_time_idx"));
}

#[test]
fn perception_objects_define_timescale_timeline() {
    let sql = current_schema_sql();

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.objects"));
    assert!(sql.contains("PRIMARY KEY (event_start, object_id)"));
    assert!(sql.contains("event_range TSTZRANGE GENERATED ALWAYS"));
    assert!(sql.contains("search_vector TSVECTOR GENERATED ALWAYS"));
    for required in [
        "series_id UUID NOT NULL",
        "modality TEXT NOT NULL DEFAULT 'mixed'",
        "body_type TEXT NOT NULL DEFAULT 'json'",
        "text_value TEXT",
        "number_value DOUBLE PRECISION",
        "bool_value BOOLEAN",
        "source_record_hash TEXT NOT NULL",
        "source_ordinal BIGINT",
    ] {
        assert!(sql.contains(required), "missing {required}");
    }
    assert!(sql.contains("by_range('event_start', INTERVAL '1 day')"));
    assert!(sql.contains("CHECK (event_end > event_start)"));
    assert!(sql.contains("CHECK (body_type <> 'text' OR text_value IS NOT NULL)"));
    assert!(sql.contains("CHECK (body_type <> 'number' OR number_value IS NOT NULL)"));
    assert!(sql.contains("CHECK (body_type <> 'boolean' OR bool_value IS NOT NULL)"));
    assert!(sql.contains("CHECK (body_type <> 'blob' OR blob_id IS NOT NULL)"));
}

#[test]
fn current_indexes_cover_viewport_overlap_json_text_density_and_source_identity() {
    let sql = current_schema_sql();

    for required in [
        "objects_user_live_time_idx",
        "objects_lane_live_time_idx",
        "objects_series_live_time_idx",
        "objects_lane_series_live_time_idx",
        "objects_source_live_time_idx",
        "objects_source_record_key_live_idx",
        "objects_event_range_live_gist_idx",
        "objects_payload_gin_idx",
        "objects_payload_browser_app_idx",
        "objects_payload_file_language_idx",
        "objects_payload_thread_idx",
        "objects_search_vector_live_idx",
        "objects_display_text_trgm_idx",
        "objects_text_value_trgm_idx",
        "source_records_user_object_hydrate_idx",
        "object_density_1m",
        "object_density_1m_lookup_idx",
    ] {
        assert!(sql.contains(required), "missing {required}");
    }
    assert!(sql.contains("WHERE valid_to IS NULL"));
    assert!(sql.contains("ON perception.objects (user_id, event_start, lane_id, object_id)"));
    assert!(sql.contains("display_text"));
    assert!(sql.contains("importance_score"));
}

#[test]
fn perception_density_uses_only_coarse_grouping_dimensions() {
    let sql = current_schema_sql();
    let density_sql = sql
        .split("CREATE MATERIALIZED VIEW IF NOT EXISTS perception.object_density_1m")
        .nth(1)
        .and_then(|tail| tail.split("WITH NO DATA").next())
        .expect("density schema should be present");
    let group_by = density_sql
        .split("GROUP BY")
        .nth(1)
        .expect("density schema should group rows");

    assert!(group_by.contains("lane_id"));
    assert!(group_by.contains("kind"));
    assert!(group_by.contains("role"));
    assert!(group_by.contains("privacy_class"));
    assert!(!group_by.contains("series_id"));
    assert!(!group_by.contains("source_id"));
}

#[test]
fn perception_projections_support_virtual_viewer_lanes() {
    let sql = current_schema_sql();

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.timeline_projections"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.timeline_projection_items"));
    assert!(sql.contains("definition_hash TEXT NOT NULL"));
    assert!(sql.contains("status TEXT NOT NULL DEFAULT 'draft'"));
    assert!(sql.contains("policy JSONB NOT NULL DEFAULT '{}'::jsonb"));
    assert!(sql.contains("source_min_event_start TIMESTAMPTZ"));
    assert!(sql.contains("source_max_event_end TIMESTAMPTZ"));
    assert!(sql.contains("built_at TIMESTAMPTZ"));
    assert!(sql.contains("invalidated_at TIMESTAMPTZ"));
    assert!(sql.contains("CHECK (status IN ('draft', 'building', 'ready', 'stale', 'failed'))"));
    assert!(sql.contains("series_id UUID NOT NULL"));
    assert!(sql.contains("base_lane_id UUID NOT NULL"));
    assert!(sql.contains("display_lane_key TEXT NOT NULL"));
    assert!(sql.contains("projection_rule_key TEXT"));
    assert!(sql.contains("PRIMARY KEY (projection_id, display_lane_key, event_start, object_id)"));
    assert!(sql.contains("timeline_projection_items_time_idx"));
    assert!(sql.contains("timeline_projection_items_range_gist_idx"));
}

#[test]
fn perception_edges_store_hypertable_object_timestamps() {
    let sql = current_schema_sql();

    assert!(sql.contains("from_object_event_start TIMESTAMPTZ NOT NULL"));
    assert!(sql.contains("to_object_event_start TIMESTAMPTZ NOT NULL"));
    assert!(sql.contains("ON perception.object_edges (user_id, from_object_id, edge_kind)"));
    assert!(sql.contains("to_object_id"));
    assert!(sql.contains("to_object_event_start"));
    assert!(sql.contains("ON perception.object_edges (user_id, to_object_id, edge_kind)"));
    assert!(sql.contains("from_object_id"));
    assert!(sql.contains("from_object_event_start"));
    assert!(sql.contains("created_at"));
    assert!(sql.contains("confidence"));
}

#[test]
fn db_helpers_keep_redaction_contract() {
    assert_eq!(
        redact_database_url("postgres://onecontext:secret@localhost/memory"),
        "postgres://***@localhost/memory"
    );
}
