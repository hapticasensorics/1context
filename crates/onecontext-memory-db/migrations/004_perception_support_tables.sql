CREATE SCHEMA IF NOT EXISTS app;
CREATE SCHEMA IF NOT EXISTS perception;
CREATE SCHEMA IF NOT EXISTS search;

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
