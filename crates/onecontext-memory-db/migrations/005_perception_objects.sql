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
