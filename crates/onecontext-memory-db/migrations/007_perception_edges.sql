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
