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
