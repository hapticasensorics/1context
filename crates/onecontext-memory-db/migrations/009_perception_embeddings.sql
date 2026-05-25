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
