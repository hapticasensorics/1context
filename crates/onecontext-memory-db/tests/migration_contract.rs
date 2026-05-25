use onecontext_memory_db::{
    db::{redact_database_url, MigrationState},
    migration_by_name, migration_sql_bundle,
    migrations::{migration_applied_sql, migration_mark_applied_sql, migration_schema_table_sql},
    MIGRATIONS,
};

#[test]
fn migration_manifest_is_perception_only_v2() {
    let names: Vec<_> = MIGRATIONS.iter().map(|migration| migration.name).collect();
    assert_eq!(
        names,
        vec![
            "001_extensions",
            "002_schemas",
            "003_app_users",
            "004_perception_support_tables",
            "005_perception_objects",
            "006_perception_indexes",
            "007_perception_edges",
            "008_perception_density",
            "009_perception_embeddings",
            "010_perception_source_cursors",
            "011_perception_projections",
        ]
    );
    assert!(
        names
            .iter()
            .all(|name| !name.contains("capture") && !name.contains("captured")),
        "V2 migration manifest should not expose capture-era migrations: {names:?}"
    );
}

#[test]
fn migrations_include_timescale_extension_and_search_primitives() {
    let bundle = migration_sql_bundle();
    assert!(bundle.contains("CREATE EXTENSION IF NOT EXISTS timescaledb"));
    assert!(bundle.contains("create_hypertable"));
    assert!(bundle.contains("search.object_embeddings"));
}

#[test]
fn migrations_include_perception_product_schema() {
    let bundle = migration_sql_bundle();
    assert!(bundle.contains("CREATE SCHEMA IF NOT EXISTS perception"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.sources"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.lanes"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.series"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.blobs"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.objects"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.object_edges"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.source_cursors"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.timeline_projections"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.timeline_projection_items"));
    assert!(bundle.contains("CREATE MATERIALIZED VIEW IF NOT EXISTS perception.object_density_1m"));
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS search.object_embeddings"));
}

#[test]
fn bundled_migrations_are_perception_only_and_capture_absent() {
    let bundle = migration_sql_bundle();
    assert!(
        !bundle.contains("capture."),
        "V2 active migration bundle must not create or reference capture.*"
    );
    assert!(
        !bundle.contains("captured_objects"),
        "V2 active migration bundle must not expose captured_objects"
    );
}

#[test]
fn product_migrations_do_not_reference_capture_schema() {
    for migration in MIGRATIONS {
        assert!(
            !migration.sql.contains("capture."),
            "{} should target perception.* product tables",
            migration.name
        );
    }
}

#[test]
fn perception_series_is_canonical_grouping_contract() {
    let bundle = migration_sql_bundle();
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.series"));
    assert!(bundle.contains("series_id UUID PRIMARY KEY"));
    assert!(bundle.contains("series_kind TEXT NOT NULL"));
    assert!(bundle.contains("series_key TEXT NOT NULL"));
    assert!(bundle.contains("default_lane_id UUID NOT NULL"));
    assert!(!bundle.contains("  lane_id UUID NOT NULL,\n\n  series_kind TEXT NOT NULL"));
    assert!(bundle.contains("parent_series_id UUID"));
    assert!(bundle.contains("UNIQUE (user_id, source_id, series_key)"));
    assert!(bundle.contains("series_user_default_lane_idx"));
    assert!(bundle.contains("series_user_default_lane_last_seen_idx"));
    assert!(bundle.contains("objects_series_live_time_idx"));
}

#[test]
fn perception_source_records_own_source_identity_uniqueness_and_series_mapping() {
    let bundle = migration_sql_bundle();
    assert!(bundle.contains("CREATE TABLE IF NOT EXISTS perception.source_records"));
    assert!(bundle.contains("source_record_id UUID PRIMARY KEY"));
    assert!(bundle.contains("source_record_key TEXT NOT NULL"));
    assert!(bundle.contains("source_record_hash TEXT NOT NULL"));
    assert!(bundle.contains("series_id UUID NOT NULL"));
    assert!(bundle.contains("UNIQUE (source_id, source_record_key)"));
    assert!(bundle.contains("UNIQUE (object_id)"));
    assert!(bundle.contains("source_records_user_time_idx"));
    assert!(bundle.contains("source_records_source_seen_idx"));
    assert!(bundle.contains("source_records_series_time_idx"));
}

#[test]
fn perception_objects_migration_defines_timescale_timeline() {
    let migration = migration_by_name("005_perception_objects").unwrap();
    assert!(migration
        .sql
        .contains("CREATE TABLE IF NOT EXISTS perception.objects"));
    assert!(migration
        .sql
        .contains("PRIMARY KEY (event_start, object_id)"));
    assert!(migration
        .sql
        .contains("event_range TSTZRANGE GENERATED ALWAYS"));
    assert!(migration
        .sql
        .contains("search_vector TSVECTOR GENERATED ALWAYS"));
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
        assert!(migration.sql.contains(required), "missing {required}");
    }
    assert!(migration.sql.contains("create_hypertable"));
    assert!(migration
        .sql
        .contains("by_range('event_start', INTERVAL '1 day')"));
    assert!(migration.sql.contains("CHECK (event_end > event_start)"));
    assert!(migration
        .sql
        .contains("CHECK (body_type <> 'text' OR text_value IS NOT NULL)"));
    assert!(migration
        .sql
        .contains("CHECK (body_type <> 'number' OR number_value IS NOT NULL)"));
    assert!(migration
        .sql
        .contains("CHECK (body_type <> 'boolean' OR bool_value IS NOT NULL)"));
    assert!(migration
        .sql
        .contains("CHECK (body_type <> 'blob' OR blob_id IS NOT NULL)"));
}

#[test]
fn perception_indexes_cover_viewport_overlap_json_text_density_and_source_identity() {
    let bundle = migration_sql_bundle();
    for required in [
        "objects_user_live_time_idx",
        "objects_lane_live_time_idx",
        "objects_series_live_time_idx",
        "objects_lane_series_live_time_idx",
        "objects_source_live_time_idx",
        "objects_source_record_key_live_idx",
        "objects_event_range_live_gist_idx",
        "objects_lane_kind_live_time_idx",
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
        assert!(bundle.contains(required), "missing {required}");
    }
    assert!(bundle.contains("WHERE valid_to IS NULL"));
    assert!(bundle.contains("ON perception.objects (user_id, event_start, lane_id, object_id)"));
    assert!(bundle.contains("display_text"));
    assert!(bundle.contains("importance_score"));
}

#[test]
fn perception_density_uses_only_coarse_grouping_dimensions() {
    let migration = migration_by_name("008_perception_density").unwrap();
    let group_by = migration
        .sql
        .split("GROUP BY")
        .nth(1)
        .expect("density migration should group rows");
    assert!(group_by.contains("lane_id"));
    assert!(group_by.contains("kind"));
    assert!(group_by.contains("role"));
    assert!(group_by.contains("privacy_class"));
    assert!(!group_by.contains("series_id"));
    assert!(!group_by.contains("source_id"));
}

#[test]
fn perception_object_instances_are_not_the_active_grouping_contract() {
    let bundle = migration_sql_bundle();
    assert!(
        !bundle.contains("instance_kind"),
        "V2 uses perception.series instead of instance_* compatibility columns"
    );
    assert!(
        !bundle.contains("instance_id"),
        "V2 uses perception.series instead of instance_* compatibility columns"
    );
}

#[test]
fn perception_projections_support_virtual_viewer_lanes() {
    let migration = migration_by_name("011_perception_projections").unwrap();
    assert!(migration
        .sql
        .contains("CREATE TABLE IF NOT EXISTS perception.timeline_projections"));
    assert!(migration
        .sql
        .contains("CREATE TABLE IF NOT EXISTS perception.timeline_projection_items"));
    assert!(migration.sql.contains("definition_hash TEXT NOT NULL"));
    assert!(migration
        .sql
        .contains("status TEXT NOT NULL DEFAULT 'draft'"));
    assert!(migration
        .sql
        .contains("policy JSONB NOT NULL DEFAULT '{}'::jsonb"));
    assert!(migration.sql.contains("source_min_event_start TIMESTAMPTZ"));
    assert!(migration.sql.contains("source_max_event_end TIMESTAMPTZ"));
    assert!(migration.sql.contains("built_at TIMESTAMPTZ"));
    assert!(migration.sql.contains("invalidated_at TIMESTAMPTZ"));
    assert!(migration
        .sql
        .contains("CHECK (status IN ('draft', 'building', 'ready', 'stale', 'failed'))"));
    assert!(migration.sql.contains("series_id UUID NOT NULL"));
    assert!(migration.sql.contains("base_lane_id UUID NOT NULL"));
    assert!(migration.sql.contains("display_lane_key TEXT NOT NULL"));
    assert!(migration.sql.contains("projection_rule_key TEXT"));
    assert!(migration
        .sql
        .contains("PRIMARY KEY (projection_id, display_lane_key, event_start, object_id)"));
    assert!(migration.sql.contains("timeline_projection_items_time_idx"));
    assert!(migration
        .sql
        .contains("timeline_projection_items_range_gist_idx"));
}

#[test]
fn perception_edges_store_hypertable_object_timestamps() {
    let migration = migration_by_name("007_perception_edges").unwrap();
    assert!(migration
        .sql
        .contains("from_object_event_start TIMESTAMPTZ NOT NULL"));
    assert!(migration
        .sql
        .contains("to_object_event_start TIMESTAMPTZ NOT NULL"));
    assert!(migration
        .sql
        .contains("ON perception.object_edges (user_id, from_object_id, edge_kind)"));
    assert!(migration.sql.contains("to_object_id"));
    assert!(migration.sql.contains("to_object_event_start"));
    assert!(migration
        .sql
        .contains("ON perception.object_edges (user_id, to_object_id, edge_kind)"));
    assert!(migration.sql.contains("from_object_id"));
    assert!(migration.sql.contains("from_object_event_start"));
    assert!(migration.sql.contains("created_at"));
    assert!(migration.sql.contains("confidence"));
}

#[test]
fn runner_tracks_applied_migrations_in_app_schema() {
    let schema_sql = migration_schema_table_sql();
    assert!(schema_sql.contains("CREATE SCHEMA IF NOT EXISTS app"));
    assert!(schema_sql.contains("CREATE TABLE IF NOT EXISTS app.schema_migrations"));
    assert!(schema_sql.contains("version INT PRIMARY KEY"));
    assert!(schema_sql.contains("name TEXT NOT NULL UNIQUE"));
}

#[test]
fn runner_can_check_and_record_each_bundled_migration() {
    for migration in MIGRATIONS {
        let check_sql = migration_applied_sql(migration);
        assert!(check_sql.contains("app.schema_migrations"));
        assert!(check_sql.contains(&format!("version = {}", migration.version)));
        assert!(check_sql.contains(migration.name));

        let mark_sql = migration_mark_applied_sql(migration);
        assert!(mark_sql.contains("INSERT INTO app.schema_migrations"));
        assert!(mark_sql.contains(&format!("VALUES ({},", migration.version)));
        assert!(mark_sql.contains(migration.name));
        assert!(mark_sql.contains("ON CONFLICT (version) DO NOTHING"));
    }
}

#[test]
fn db_helpers_expose_migration_state_and_redaction_contracts() {
    let state = MigrationState {
        applied: Vec::new(),
        pending: MIGRATIONS.iter().map(|migration| migration.name).collect(),
        total: MIGRATIONS.len(),
    };

    assert_eq!(state.total, MIGRATIONS.len());
    assert!(!state.is_current());
    assert_eq!(
        redact_database_url("postgres://onecontext:secret@localhost/memory"),
        "postgres://***@localhost/memory"
    );
}
