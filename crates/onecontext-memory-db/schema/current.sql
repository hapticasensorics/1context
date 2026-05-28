CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

CREATE SCHEMA IF NOT EXISTS app;
CREATE SCHEMA IF NOT EXISTS perception;
CREATE SCHEMA IF NOT EXISTS search;

CREATE TABLE IF NOT EXISTS app.users (
  user_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email TEXT UNIQUE,
  display_name TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS perception.sources (
  source_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  source_type TEXT NOT NULL,
  source_key TEXT NOT NULL,
  display_name TEXT,

  source_version TEXT,
  adapter_version TEXT,

  default_lane_id UUID,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, source_key)
);

CREATE INDEX IF NOT EXISTS sources_user_type_idx
ON perception.sources (user_id, source_type);

CREATE TABLE IF NOT EXISTS perception.lanes (
  lane_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  lane_key TEXT NOT NULL,
  display_name TEXT NOT NULL,

  lane_group TEXT NOT NULL,

  sort_order INT NOT NULL DEFAULT 0,
  default_visible BOOLEAN NOT NULL DEFAULT TRUE,

  default_materialization_policy TEXT NOT NULL DEFAULT 'index_events',
  default_importance_threshold REAL NOT NULL DEFAULT 0.5,

  default_time_resolution_ns BIGINT,
  default_time_uncertainty_ns BIGINT,
  default_indexed_resolution_ns BIGINT,
  default_display_resolution_hint_ns BIGINT,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, lane_key)
);

CREATE INDEX IF NOT EXISTS lanes_user_group_idx
ON perception.lanes (user_id, lane_group, sort_order);

CREATE TABLE IF NOT EXISTS perception.series (
  series_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  source_id UUID NOT NULL,

  default_lane_id UUID NOT NULL,

  series_kind TEXT NOT NULL,
  series_key TEXT NOT NULL,

  parent_series_id UUID,

  display_name TEXT,

  modality TEXT,

  default_privacy_class TEXT NOT NULL DEFAULT 'normal',

  default_time_resolution_ns BIGINT,
  default_time_uncertainty_ns BIGINT,

  first_event_start TIMESTAMPTZ,
  last_event_end TIMESTAMPTZ,

  record_count BIGINT NOT NULL DEFAULT 0,

  tags JSONB NOT NULL DEFAULT '{}'::jsonb,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, source_id, series_key),
  FOREIGN KEY (parent_series_id) REFERENCES perception.series(series_id)
);

CREATE INDEX IF NOT EXISTS series_user_kind_idx
ON perception.series (user_id, series_kind);

CREATE INDEX IF NOT EXISTS series_user_default_lane_idx
ON perception.series (user_id, default_lane_id);

CREATE INDEX IF NOT EXISTS series_user_default_lane_last_seen_idx
ON perception.series (user_id, default_lane_id, last_event_end DESC NULLS LAST);

CREATE INDEX IF NOT EXISTS series_parent_idx
ON perception.series (parent_series_id);

CREATE TABLE IF NOT EXISTS perception.blobs (
  blob_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  storage_backend TEXT NOT NULL,
  uri TEXT NOT NULL,
  safe_uri TEXT,

  sha256 TEXT,
  byte_count BIGINT,
  content_type TEXT NOT NULL,

  codec TEXT,
  duration_ms BIGINT,

  width INT,
  height INT,

  blob_state TEXT NOT NULL DEFAULT 'available',

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, uri)
);

CREATE INDEX IF NOT EXISTS blobs_user_created_idx
ON perception.blobs (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS blobs_sha256_idx
ON perception.blobs (sha256)
WHERE sha256 IS NOT NULL;

CREATE TABLE IF NOT EXISTS perception.source_records (
  source_record_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  source_id UUID NOT NULL,

  source_record_key TEXT NOT NULL,
  source_record_hash TEXT NOT NULL,

  object_id UUID NOT NULL,
  object_event_start TIMESTAMPTZ NOT NULL,

  series_id UUID NOT NULL,

  kind TEXT NOT NULL,

  first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  seen_count BIGINT NOT NULL DEFAULT 1,
  conflict_count BIGINT NOT NULL DEFAULT 0,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  UNIQUE (source_id, source_record_key),
  UNIQUE (object_id)
);

CREATE INDEX IF NOT EXISTS source_records_user_time_idx
ON perception.source_records (user_id, object_event_start DESC);

CREATE INDEX IF NOT EXISTS source_records_source_seen_idx
ON perception.source_records (source_id, last_seen_at DESC);

CREATE INDEX IF NOT EXISTS source_records_series_time_idx
ON perception.source_records (user_id, series_id, object_event_start DESC);


CREATE TABLE IF NOT EXISTS perception.objects (
  event_start TIMESTAMPTZ NOT NULL,
  event_end   TIMESTAMPTZ NOT NULL,

  object_id UUID NOT NULL,

  user_id UUID NOT NULL,

  source_id UUID NOT NULL,
  source_record_id UUID NOT NULL,
  source_record_key TEXT NOT NULL,
  source_record_hash TEXT NOT NULL,

  series_id UUID NOT NULL REFERENCES perception.series(series_id),

  lane_id UUID NOT NULL,

  kind TEXT NOT NULL,

  role TEXT NOT NULL DEFAULT 'raw',
  privacy_class TEXT NOT NULL DEFAULT 'normal',
  modality TEXT NOT NULL DEFAULT 'mixed',
  body_type TEXT NOT NULL DEFAULT 'json',

  text_value TEXT,
  number_value DOUBLE PRECISION,
  bool_value BOOLEAN,

  time_semantics TEXT NOT NULL DEFAULT 'interval',
  temporal_level TEXT NOT NULL DEFAULT 'event',

  native_resolution_ns BIGINT,
  stored_resolution_ns BIGINT,
  indexed_resolution_ns BIGINT,
  display_resolution_hint_ns BIGINT,

  time_resolution_ns BIGINT,
  time_uncertainty_ns BIGINT,
  alignment_confidence REAL,
  alignment_method TEXT,

  materialization_policy TEXT NOT NULL DEFAULT 'index_events',

  importance_score REAL,

  blob_id UUID,

  payload JSONB NOT NULL DEFAULT '{}'::jsonb,

  display_title TEXT,
  display_text TEXT,

  search_vector TSVECTOR GENERATED ALWAYS AS (
    to_tsvector(
      'simple',
      coalesce(display_title, '') || ' ' ||
      coalesce(display_text, '') || ' ' ||
      coalesce(text_value, '')
    )
  ) STORED,

  event_range TSTZRANGE GENERATED ALWAYS AS (
    tstzrange(event_start, event_end, '[)')
  ) STORED,

  source_start_ns BIGINT,
  source_end_ns BIGINT,
  source_sequence BIGINT,
  source_ordinal BIGINT,

  media_start_offset_ns BIGINT,
  media_end_offset_ns BIGINT,

  ingest_time TIMESTAMPTZ NOT NULL DEFAULT now(),

  valid_from TIMESTAMPTZ NOT NULL DEFAULT now(),
  valid_to TIMESTAMPTZ,

  schema_name TEXT,
  schema_version INT,

  confidence REAL,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  PRIMARY KEY (event_start, object_id),

  CHECK (event_end > event_start),
  CHECK (body_type IN ('none', 'text', 'json', 'number', 'boolean', 'blob', 'mixed')),
  CHECK (body_type <> 'text' OR text_value IS NOT NULL),
  CHECK (body_type <> 'number' OR number_value IS NOT NULL),
  CHECK (body_type <> 'boolean' OR bool_value IS NOT NULL),
  CHECK (body_type <> 'blob' OR blob_id IS NOT NULL),
  CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
  CHECK (alignment_confidence IS NULL OR (alignment_confidence >= 0 AND alignment_confidence <= 1)),
  CHECK (importance_score IS NULL OR (importance_score >= 0 AND importance_score <= 1)),
  CHECK (valid_to IS NULL OR valid_to >= valid_from)
);

DO $$
BEGIN
  BEGIN
    PERFORM create_hypertable(
      'perception.objects',
      by_range('event_start', INTERVAL '1 day'),
      if_not_exists => TRUE,
      create_default_indexes => FALSE
    );
  EXCEPTION
    WHEN undefined_function THEN
      BEGIN
        PERFORM create_hypertable(
          'perception.objects',
          'event_start',
          chunk_time_interval => INTERVAL '1 day',
          if_not_exists => TRUE,
          create_default_indexes => FALSE
        );
      EXCEPTION
        WHEN duplicate_table OR duplicate_object THEN
          NULL;
      END;
    WHEN duplicate_table OR duplicate_object THEN
      NULL;
  END;
END $$;


CREATE INDEX IF NOT EXISTS objects_user_live_time_idx
ON perception.objects (user_id, event_start, lane_id, object_id)
INCLUDE (
  event_end,
  source_id,
  source_record_id,
  series_id,
  kind,
  role,
  privacy_class,
  body_type,
  blob_id,
  importance_score
)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_lane_live_time_idx
ON perception.objects (user_id, lane_id, event_start, object_id)
INCLUDE (
  event_end,
  source_id,
  source_record_id,
  series_id,
  kind,
  role,
  privacy_class,
  display_title,
  display_text,
  body_type,
  blob_id,
  importance_score
)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_lane_kind_live_time_idx
ON perception.objects (user_id, lane_id, kind, event_start DESC)
INCLUDE (object_id, series_id, role, privacy_class, event_end, display_title, body_type, blob_id)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_series_live_time_idx
ON perception.objects (user_id, series_id, event_start DESC)
INCLUDE (object_id, kind, role, privacy_class, event_end, display_title, body_type)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_lane_series_live_time_idx
ON perception.objects (user_id, lane_id, series_id, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_source_live_time_idx
ON perception.objects (user_id, source_id, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_source_record_key_live_idx
ON perception.objects (source_id, source_record_key, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_kind_live_time_idx
ON perception.objects (user_id, kind, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_role_live_time_idx
ON perception.objects (user_id, role, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_privacy_live_time_idx
ON perception.objects (user_id, privacy_class, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_event_range_live_gist_idx
ON perception.objects
USING GIST (user_id, event_range)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_gin_idx
ON perception.objects
USING GIN (payload jsonb_path_ops);

CREATE INDEX IF NOT EXISTS objects_payload_app_idx
ON perception.objects ((payload->>'app'))
WHERE payload ? 'app' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_thread_idx
ON perception.objects ((payload->>'thread_id'))
WHERE payload ? 'thread_id' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_session_idx
ON perception.objects ((payload->>'session_id'))
WHERE payload ? 'session_id' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_browser_app_idx
ON perception.objects ((payload->'browser'->>'app'))
WHERE payload ? 'browser' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_payload_file_language_idx
ON perception.objects ((payload->'file'->>'language'))
WHERE payload ? 'file' AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_search_vector_live_idx
ON perception.objects
USING GIN (search_vector)
WHERE valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_display_text_trgm_idx
ON perception.objects
USING GIN (display_text gin_trgm_ops)
WHERE display_text IS NOT NULL AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS objects_text_value_trgm_idx
ON perception.objects
USING GIN (text_value gin_trgm_ops)
WHERE text_value IS NOT NULL AND valid_to IS NULL;

CREATE INDEX IF NOT EXISTS source_records_user_object_hydrate_idx
ON perception.source_records (user_id, object_id)
INCLUDE (
  object_event_start,
  series_id,
  source_record_id,
  source_record_key,
  source_record_hash,
  first_seen_at,
  last_seen_at,
  seen_count
);


CREATE TABLE IF NOT EXISTS perception.object_edges (
  edge_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  from_object_id UUID NOT NULL,
  from_object_event_start TIMESTAMPTZ NOT NULL,

  to_object_id UUID NOT NULL,
  to_object_event_start TIMESTAMPTZ NOT NULL,

  edge_kind TEXT NOT NULL,

  confidence REAL,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (from_object_id, to_object_id, edge_kind),

  CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1))
);

CREATE INDEX IF NOT EXISTS object_edges_from_idx
ON perception.object_edges (user_id, from_object_id, edge_kind)
INCLUDE (
  edge_id,
  to_object_id,
  to_object_event_start,
  created_at,
  confidence
);

CREATE INDEX IF NOT EXISTS object_edges_to_idx
ON perception.object_edges (user_id, to_object_id, edge_kind)
INCLUDE (
  edge_id,
  from_object_id,
  from_object_event_start,
  created_at,
  confidence
);


CREATE MATERIALIZED VIEW IF NOT EXISTS perception.object_density_1m
WITH (timescaledb.continuous) AS
SELECT
  user_id,
  lane_id,
  kind,
  role,
  privacy_class,
  time_bucket(INTERVAL '1 minute', event_start) AS bucket_start,
  count(*) AS object_count,
  min(event_start) AS first_event_start,
  max(event_end) AS last_event_end,
  avg(importance_score) AS avg_importance
FROM perception.objects
WHERE valid_to IS NULL
GROUP BY
  user_id,
  lane_id,
  kind,
  role,
  privacy_class,
  time_bucket(INTERVAL '1 minute', event_start)
WITH NO DATA;

CREATE INDEX IF NOT EXISTS object_density_1m_lookup_idx
ON perception.object_density_1m
  (user_id, lane_id, bucket_start DESC, kind, role, privacy_class);

DO $$
BEGIN
  BEGIN
    PERFORM add_continuous_aggregate_policy(
      'perception.object_density_1m',
      start_offset => INTERVAL '7 days',
      end_offset => INTERVAL '1 minute',
      schedule_interval => INTERVAL '1 minute',
      if_not_exists => TRUE
    );
  EXCEPTION
    WHEN undefined_function THEN
      BEGIN
        PERFORM add_continuous_aggregate_policy(
          'perception.object_density_1m',
          start_offset => INTERVAL '7 days',
          end_offset => INTERVAL '1 minute',
          schedule_interval => INTERVAL '1 minute'
        );
      EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
          NULL;
      END;
    WHEN duplicate_object OR unique_violation THEN
      NULL;
  END;
END $$;


CREATE TABLE IF NOT EXISTS search.object_embeddings (
  embedding_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  object_id UUID NOT NULL,

  event_start TIMESTAMPTZ NOT NULL,

  embedding_model TEXT NOT NULL,
  embedding vector(1536) NOT NULL,

  content_text TEXT,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (object_id, embedding_model)
);

CREATE INDEX IF NOT EXISTS object_embeddings_user_time_idx
ON search.object_embeddings (user_id, event_start DESC);

CREATE INDEX IF NOT EXISTS object_embeddings_hnsw_idx
ON search.object_embeddings
USING hnsw (embedding vector_cosine_ops);


CREATE TABLE IF NOT EXISTS perception.source_cursors (
  source_id UUID NOT NULL,

  cursor_name TEXT NOT NULL DEFAULT 'default',

  user_id UUID NOT NULL,

  cursor_value JSONB NOT NULL,

  advanced_by_write_id UUID,
  advanced_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  advancement_mode TEXT NOT NULL DEFAULT 'db_success',

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  PRIMARY KEY (source_id, cursor_name)
);

CREATE INDEX IF NOT EXISTS source_cursors_user_advanced_idx
ON perception.source_cursors (user_id, advanced_at DESC);


CREATE TABLE IF NOT EXISTS perception.timeline_projections (
  projection_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  projection_key TEXT NOT NULL,
  display_name TEXT NOT NULL,

  projection_kind TEXT NOT NULL DEFAULT 'viewer_layout',

  definition JSONB NOT NULL DEFAULT '{}'::jsonb,
  definition_hash TEXT NOT NULL,

  status TEXT NOT NULL DEFAULT 'draft',

  policy JSONB NOT NULL DEFAULT '{}'::jsonb,

  source_min_event_start TIMESTAMPTZ,
  source_max_event_end TIMESTAMPTZ,
  built_at TIMESTAMPTZ,
  invalidated_at TIMESTAMPTZ,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, projection_key),

  CHECK (status IN ('draft', 'building', 'ready', 'stale', 'failed'))
);

CREATE TABLE IF NOT EXISTS perception.timeline_projection_items (
  projection_id UUID NOT NULL REFERENCES perception.timeline_projections(projection_id) ON DELETE CASCADE,

  user_id UUID NOT NULL,

  object_id UUID NOT NULL,
  object_event_start TIMESTAMPTZ NOT NULL,

  series_id UUID NOT NULL,
  base_lane_id UUID NOT NULL,

  display_lane_key TEXT NOT NULL,
  display_group_key TEXT,
  projection_rule_key TEXT,

  event_start TIMESTAMPTZ NOT NULL,
  event_end TIMESTAMPTZ NOT NULL,

  event_range TSTZRANGE GENERATED ALWAYS AS (
    tstzrange(event_start, event_end, '[)')
  ) STORED,

  rank REAL,
  collapsed BOOLEAN NOT NULL DEFAULT FALSE,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  PRIMARY KEY (projection_id, display_lane_key, event_start, object_id),

  CHECK (event_end > event_start),
  CHECK (rank IS NULL OR (rank >= 0 AND rank <= 1))
);

CREATE INDEX IF NOT EXISTS timeline_projection_items_time_idx
ON perception.timeline_projection_items
  (user_id, projection_id, event_start, display_lane_key);

CREATE INDEX IF NOT EXISTS timeline_projection_items_range_gist_idx
ON perception.timeline_projection_items
USING GIST (user_id, projection_id, event_range);

CREATE INDEX IF NOT EXISTS timeline_projections_user_kind_idx
ON perception.timeline_projections (user_id, projection_kind);
