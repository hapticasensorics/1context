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
