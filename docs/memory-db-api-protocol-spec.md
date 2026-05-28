# 1Context Perception DB Timeseries Spec

**Version:** V0 definitive implementation spec
**Date:** 2026-05-25
**Primary implementation target:** `crates/onecontext-memory-db`
**Runtime consumers:** macOS runtime, local web viewer, future socket clients

This spec turns the current draft into one coherent contract: schema, write semantics, dedupe, cursor safety, read APIs, density queries, hydration, edges, search, viewer boundaries, and agent ownership.

The database is not “a text DB.” It is a **temporal object store**. Every captured object gets one place on the timeline. Text, files, images, video, audio, app events, Codex sessions, browser traces, UI events, transcripts, OCR, and summaries all share the same time spine.

Naming:

```text
Perception DB = raw and lightly normalized observed reality.
Memory system = processed, summarized, selected, linked, and recalled knowledge.
```

The current crate, process, route, and protocol method names still use `memory` because that is the existing implementation surface. The conceptual system name for this layer is **Perception DB**. The physical PostgreSQL schema for this layer is `perception`. A later naming cleanup may rename protocol methods, routes, crates, or process names once the contract is stable.

```text
Everything is a time-bounded object.
Everything has a lane.
Everything has source identity.
Everything can point to bytes.
Everything can point to evidence.
```

---

# 1. Design thesis

1Context Perception DB is a **Postgres + TimescaleDB time-aligned object database**.

The core table stores **timeline objects**, not just messages or logs. A timeline object may represent:

```text
screen_chunk
audio_chunk
ui_event
browser_event
agent_session
agent_turn
agent_message
agent_tool_summary
agent_compaction
agent_prompt_snapshot
imessage_message
file_version
file_diff
ocr_span
transcript_span
summary
memory
```

Every object has:

```text
source identity
event time range
lane
kind
role
privacy class
payload metadata
optional blob descriptor
optional provenance edges
```

TimescaleDB is the right V0 substrate because it is a PostgreSQL extension for high-performance time-series and event data, while hypertables keep a normal SQL surface and partition data into time-bounded chunks underneath. Timescale’s docs also require any unique index or primary key on a hypertable to include the partitioning column, which directly shapes this schema. ([GitHub][1])

---

# 2. Non-negotiable invariants

## 2.1 Write through source identity

The canonical write key is not `object_id`.

It is:

```text
(source_id, source_record_key)
```

This is the dedupe identity. If the same source record is seen twice, the database must return the same object receipt and must not insert a second object.

```text
Source identity is the write contract.
Object ID is the stable receipt.
```

## 2.2 Read through memory methods

Clients must not read JSONL files as the product protocol.

Allowed read surface:

```text
memory.queryViewport
memory.queryDensity
memory.hydrateObjects
memory.queryEdges
memory.searchText
memory.searchSemantic
memory.explain
memory.subscribe
```

Disallowed V0 product surface:

```text
/api/memory/traces
raw JSONL read provider
direct psql shelling from Swift runtime
```

## 2.3 Store in Timescale

The durable queryable timeline lives in Timescale.

Large blobs live outside the row as descriptors:

```text
video
audio
images
screenshots
raw files
large JSONL logs
binary model artifacts
```

Postgres can store binary values and has TOAST for oversized values, but the right default is still “small queryable metadata in Postgres, heavy bytes in object/blob storage.” PostgreSQL’s TOAST documentation explains that large field values are compressed or broken into multiple physical rows because tuples cannot span pages, which is exactly why dragging videos through hot SQL rows is the wrong default. ([PostgreSQL][2])

## 2.4 Show the evidence

Every derived object must be able to reveal its sources.

Examples:

```text
ocr_span       -> derived_from -> screen_chunk
transcript     -> derived_from -> audio_chunk
summary        -> derived_from -> agent_session, messages, files
memory         -> references   -> multiple non-contiguous objects
file_diff      -> derived_from -> file_version
```

## 2.5 Cursor advancement requires durability

A source adapter cursor may advance only when:

```text
DB write succeeds
```

No DB success means no cursor movement. Replay/audit ledgers may be added as
separate evidence systems later, but they are not a cursor-advancement fallback
for the active Perception DB protocol.

---

# 3. Time model

## 3.1 Canonical shared time

V0 uses `TIMESTAMPTZ` for `event_start` and `event_end`.

PostgreSQL timestamp precision is fractional seconds `0..6`, so the shared DB timeline is microsecond-precision. Native sub-microsecond facts should be preserved in nanosecond side fields, media offsets, or blob-local timing, not forced into the primary timestamp. ([PostgreSQL][3])

```text
event_start / event_end:
  shared UTC timeline used by Timescale and queries

source_start_ns / source_end_ns:
  original source or monotonic clock time

media_start_offset_ns / media_end_offset_ns:
  precise offsets inside audio/video/blob payloads

time_resolution_ns:
  smallest meaningful resolution for this object

time_uncertainty_ns:
  estimated placement uncertainty
```

## 3.2 Instants are tiny intervals

Objects must overlap viewport queries. Empty ranges are annoying little trapdoors.

For an instant event:

```text
event_start = observed time
event_end   = event_start + 1 microsecond
time_semantics = instant
```

## 3.3 Different lanes have different granularity

Precision is not value. Some data can be captured at high fidelity while only being materialized at meaningful granularity.

```text
Audio native resolution:       sample-level
Audio timeline materialization: transcript spans / chunks

Screen native resolution:      frames
Screen timeline materialization: screen chunks / OCR spans / app spans

Mouse native resolution:       high-frequency positions
Mouse timeline materialization: clicks, drags, meaningful gestures

Coding-agent native resolution: token/message/tool/runtime events
Coding-agent materialization:   agent sessions, messages, compact tool summaries, file diffs

Memory native resolution:      fuzzy semantic intervals
Memory timeline materialization: summaries and claims
```

This is the core policy:

```text
Capture truth at sufficient fidelity.
Materialize meaning, not atoms.
```

---

# 4. Schemas and extensions

Use three schemas:

```sql
CREATE SCHEMA IF NOT EXISTS app;
CREATE SCHEMA IF NOT EXISTS perception;
CREATE SCHEMA IF NOT EXISTS search;
```

Required extensions:

```sql
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
```

Optional later:

```sql
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS pg_jsonschema;
CREATE EXTENSION IF NOT EXISTS pg_cron;
```

Use app-generated deterministic UUIDs for source-derived IDs. PostgreSQL 18 has native `uuidv7()`, but V0 should not require PG18 merely for ID generation; Rust can generate deterministic UUIDs and UUIDv7s before insert. PostgreSQL’s current docs list both `gen_random_uuid()`/`uuidv4()` and `uuidv7()` as UUID generation functions. ([PostgreSQL][4])

---

# 5. Core physical model

The schema has these important tables:

```text
perception.sources
perception.lanes
perception.series
perception.blobs
perception.source_records
perception.objects              -- Timescale hypertable
perception.object_edges
perception.timeline_projections
perception.timeline_projection_items
search.object_embeddings
```

The most important table is `perception.objects`.

The most important identity table is `perception.series`.

The most important idempotency table is `perception.source_records`.

---

# 6. Tables

## 6.1 `perception.sources`

A source is a producer of records.

Examples:

```text
codex-cli
claude-code
imessage-adapter
screen-capture-agent
browser-extension
file-watcher
audio-capture
manual-note-source
derived-ocr-pipeline
derived-summary-pipeline
```

```sql
CREATE TABLE perception.sources (
  source_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  source_type TEXT NOT NULL,
  -- codex, claude, imessage, screen, audio, browser, file_watcher,
  -- ui, terminal, derived, manual, system

  source_key TEXT NOT NULL,
  -- stable local identifier, for example:
  -- "codex:local", "imessage:macbook", "screen:main-display"

  display_name TEXT,

  source_version TEXT,
  adapter_version TEXT,

  default_lane_id UUID,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, source_key)
);

CREATE INDEX sources_user_type_idx
ON perception.sources (user_id, source_type);
```

---

## 6.2 `perception.lanes`

A lane is a timeline/UI grouping.

Examples:

```text
screen.main
audio.mic
ui.events
browser.tabs
agents.sessions
agents.messages
agents.tools
imessage.threads
files.versions
ocr.spans
transcript.spans
memory.summaries
```

```sql
CREATE TABLE perception.lanes (
  lane_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  lane_key TEXT NOT NULL,
  display_name TEXT NOT NULL,

  lane_group TEXT NOT NULL,
  -- screen, audio, agents, chat, browser, files, text, memory

  sort_order INT NOT NULL DEFAULT 0,
  default_visible BOOLEAN NOT NULL DEFAULT TRUE,

  default_materialization_policy TEXT NOT NULL DEFAULT 'index_events',
  -- raw_only, index_events, index_spans, summarize, fully_materialize

  default_importance_threshold REAL NOT NULL DEFAULT 0.5,

  default_time_resolution_ns BIGINT,
  default_time_uncertainty_ns BIGINT,
  default_indexed_resolution_ns BIGINT,
  default_display_resolution_hint_ns BIGINT,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, lane_key)
);

CREATE INDEX lanes_user_group_idx
ON perception.lanes (user_id, lane_group, sort_order);
```

---

## 6.3 `perception.series`

`perception.series` stores logical stream/container identity: the Codex
session, browser window, browser tab, file path, iMessage thread, screen
display, audio input, metric, or memory topic that a temporal record belongs
to. Series are high-cardinality rows, not physical lanes.

```sql
CREATE TABLE perception.series (
  series_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  source_id UUID NOT NULL,

  default_lane_id UUID NOT NULL,
  -- home/coarse UI lane, for example agents.sessions, browser.windows,
  -- files.versions, screen.main, system.metrics

  series_kind TEXT NOT NULL,
  -- codex_session, claude_session, browser_window, browser_tab,
  -- file_path, screen_display, audio_input, imessage_thread,
  -- terminal_session, agent_run, metric

  series_key TEXT NOT NULL,
  -- stable source/local key

  parent_series_id UUID,
  -- stable hierarchy only: browser tab inside window, file under repo,
  -- metric under source. Use object_edges for temporal relationships.

  display_name TEXT,

  modality TEXT,
  -- text, json, image, video, audio, binary, scalar, mixed

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

  UNIQUE (user_id, source_id, series_key)
);

CREATE INDEX series_user_kind_idx
ON perception.series (user_id, series_kind);

CREATE INDEX series_user_default_lane_idx
ON perception.series (user_id, default_lane_id);

CREATE INDEX series_parent_idx
ON perception.series (parent_series_id);
```

## 6.4 `perception.blobs`

`perception.blobs` stores descriptors for large bytes.

The browser-facing API must never expose raw local file paths. It may expose safe descriptors, IDs, content types, sizes, and capability URLs if explicitly minted.

```sql
CREATE TABLE perception.blobs (
  blob_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  storage_backend TEXT NOT NULL,
  -- local, s3, gcs, r2, minio, sqlite_blob, test

  uri TEXT NOT NULL,
  -- native/runtime only; never raw browser JSON

  safe_uri TEXT,
  -- optional browser-safe capability URL or redacted handle

  sha256 TEXT,
  byte_count BIGINT,
  content_type TEXT NOT NULL,

  codec TEXT,
  duration_ms BIGINT,

  width INT,
  height INT,

  blob_state TEXT NOT NULL DEFAULT 'available',
  -- available, archived, deleted, missing

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, uri)
);

CREATE INDEX blobs_user_created_idx
ON perception.blobs (user_id, created_at DESC);

CREATE INDEX blobs_sha256_idx
ON perception.blobs (sha256)
WHERE sha256 IS NOT NULL;
```

Blob examples:

```text
screen mp4
audio flac
screenshot png
file content
raw Codex session JSONL
large terminal log
raw browser event log
```

---

## 6.5 `perception.source_records`

This is the idempotency table. It is not optional.

Every object written through the protocol must have one source record.

```sql
CREATE TABLE perception.source_records (
  source_record_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  source_id UUID NOT NULL,

  source_record_key TEXT NOT NULL,
  -- stable within source

  source_record_hash TEXT NOT NULL,
  -- canonical hash of semantic source payload + blob hashes

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

CREATE INDEX source_records_user_time_idx
ON perception.source_records (user_id, object_event_start DESC);

CREATE INDEX source_records_source_seen_idx
ON perception.source_records (source_id, last_seen_at DESC);

CREATE INDEX source_records_user_series_time_idx
ON perception.source_records (user_id, series_id, object_event_start DESC);
```

### Source identity rule

Given:

```text
source_id
source_record_key
```

The writer computes:

```text
source_record_id = deterministic_uuid("source-record", source_id, source_record_key)
object_id        = deterministic_uuid("object",        source_id, source_record_key)
```

That makes receipts stable across retries.

### Hash conflict rule

If an existing `(source_id, source_record_key)` is seen again with the same `source_record_hash`:

```text
inserted = false
dedupe_reason = existing_source_record
```

If it is seen again with a different hash:

```text
default: fail the chunk with SOURCE_IDENTITY_HASH_CONFLICT
```

Do not silently overwrite. A source identity collision is a broken adapter, not a cute edge case.

---

## 6.6 `perception.objects`

This is the Timescale hypertable.

```sql
CREATE TABLE perception.objects (
  event_start TIMESTAMPTZ NOT NULL,
  event_end   TIMESTAMPTZ NOT NULL,

  object_id UUID NOT NULL,

  user_id UUID NOT NULL,

  source_id UUID NOT NULL,
  source_record_id UUID NOT NULL,
  source_record_key TEXT NOT NULL,
  source_record_hash TEXT NOT NULL,
  -- copied from perception.source_records for row-local audit/debug;
  -- perception.source_records owns conflict detection

  series_id UUID NOT NULL REFERENCES perception.series(series_id),

  lane_id UUID NOT NULL,
  -- concrete record lane. This often matches series.default_lane_id but may
  -- differ for messages, tool calls, OCR spans, file diffs, or summaries.

  kind TEXT NOT NULL,
  -- codex.session, codex.message, codex.tool_call, browser.window_state,
  -- browser.tab_event, file.version, file.diff, screen.chunk,
  -- screen.ocr_span, audio.chunk, audio.transcript_span, metric.sample,
  -- memory.summary, memory.claim

  role TEXT NOT NULL DEFAULT 'raw',
  -- raw, derived, annotation, summary, memory, synthetic

  privacy_class TEXT NOT NULL DEFAULT 'normal',
  -- public, normal, sensitive, secret, redacted

  modality TEXT NOT NULL DEFAULT 'mixed',
  -- text, json, image, video, audio, binary, scalar, mixed

  body_type TEXT NOT NULL DEFAULT 'json',
  -- none, text, json, number, boolean, blob, mixed

  text_value TEXT,
  number_value DOUBLE PRECISION,
  bool_value BOOLEAN,

  time_semantics TEXT NOT NULL DEFAULT 'interval',
  -- instant, interval, chunk, aggregate, fuzzy, nonlocal

  temporal_level TEXT NOT NULL DEFAULT 'event',
  -- sample, frame, event, span, chunk, session, episode, summary, memory

  native_resolution_ns BIGINT,
  stored_resolution_ns BIGINT,
  indexed_resolution_ns BIGINT,
  display_resolution_hint_ns BIGINT,

  time_resolution_ns BIGINT,
  time_uncertainty_ns BIGINT,
  alignment_confidence REAL,
  alignment_method TEXT,
  -- system_clock, monotonic_clock, media_timestamp, filesystem_mtime,
  -- transcript_alignment, model_inferred, user_annotated, derived_from_parent

  materialization_policy TEXT NOT NULL DEFAULT 'index_events',
  -- raw_only, index_events, index_spans, summarize, fully_materialize

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
  CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
  CHECK (alignment_confidence IS NULL OR (alignment_confidence >= 0 AND alignment_confidence <= 1)),
  CHECK (importance_score IS NULL OR (importance_score >= 0 AND importance_score <= 1)),
  CHECK (valid_to IS NULL OR valid_to >= valid_from)
);
```

Create hypertable:

```sql
SELECT create_hypertable(
  'perception.objects',
  by_range('event_start', INTERVAL '1 day'),
  if_not_exists => TRUE,
  create_default_indexes => FALSE
);
```

Start with one-day chunks. Adjust only after measuring ingest volume, index growth, and viewport query latency.

### Why `source_records` is separate

Timescale requires unique constraints on hypertables to include the partitioning column. So `perception.objects` uses:

```text
PRIMARY KEY (event_start, object_id)
```

while `perception.source_records` gives us global uniqueness for:

```text
(source_id, source_record_key)
object_id
```

This is the clean workaround. It keeps the hot timeline partitioned while preserving stable object lookup. ([TigerData][5])

---

## 6.7 `perception.object_edges`

Edges represent provenance and nonlinear relationships.

```sql
CREATE TABLE perception.object_edges (
  edge_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  from_object_id UUID NOT NULL,
  from_object_event_start TIMESTAMPTZ NOT NULL,

  to_object_id UUID NOT NULL,
  to_object_event_start TIMESTAMPTZ NOT NULL,

  edge_kind TEXT NOT NULL,
  -- derived_from, contains, references, annotates, supersedes,
  -- contradicts, same_event_as, caused_by, reply_to

  confidence REAL,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (from_object_id, to_object_id, edge_kind),

  CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1))
);

CREATE INDEX object_edges_from_idx
ON perception.object_edges (user_id, from_object_id, edge_kind)
INCLUDE (to_object_id, to_object_event_start);

CREATE INDEX object_edges_to_idx
ON perception.object_edges (user_id, to_object_id, edge_kind)
INCLUDE (from_object_id, from_object_event_start);

CREATE INDEX object_edges_user_kind_idx
ON perception.object_edges (user_id, edge_kind);
```

Examples:

```text
ocr_span       derived_from  screen_chunk
transcript     derived_from  audio_chunk
summary        derived_from  agent_session
memory         references    agent_message
new_summary    supersedes    old_summary
agent_session  contains      agent_tool_summary
```

---

## 6.8 `perception.timeline_projections`

`timeline_projections` stores saved or rebuildable viewer layouts. It is not
source of truth; it is a projection over temporal records and series.

```sql
CREATE TABLE perception.timeline_projections (
  projection_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  projection_key TEXT NOT NULL,
  display_name TEXT NOT NULL,

  projection_kind TEXT NOT NULL DEFAULT 'viewer_layout',
  -- viewer_layout, search_result, agent_investigation, project_timeline,
  -- session_timeline, debug_layout

  definition JSONB NOT NULL DEFAULT '{}'::jsonb,
  definition_hash TEXT NOT NULL,

  status TEXT NOT NULL DEFAULT 'draft',
  -- draft, building, ready, stale, failed

  policy JSONB NOT NULL DEFAULT '{}'::jsonb,
  -- max_visible_series, overflow_lane_key, ranking weights, collapse rules

  source_min_event_start TIMESTAMPTZ,
  source_max_event_end TIMESTAMPTZ,
  built_at TIMESTAMPTZ,
  invalidated_at TIMESTAMPTZ,

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, projection_key)
);
```

## 6.9 `perception.timeline_projection_items`

Projection items let the viewer split one durable lane into temporary display
lanes, such as per session or per browser tab, without mutating canonical
storage.

```sql
CREATE TABLE perception.timeline_projection_items (
  projection_id UUID NOT NULL,

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

  PRIMARY KEY (projection_id, display_lane_key, event_start, object_id)
);
```

Projection policies must cap visible series and collapse overflow into a stable
display lane. High-cardinality layout belongs here or in on-demand viewer
queries, not in the canonical lane table.

## 6.10 `search.object_embeddings`

Semantic search is a sidecar. It is not the source of truth.

```sql
CREATE TABLE search.object_embeddings (
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

CREATE INDEX object_embeddings_user_time_idx
ON search.object_embeddings (user_id, event_start DESC);

CREATE INDEX object_embeddings_hnsw_idx
ON search.object_embeddings
USING hnsw (embedding vector_cosine_ops);
```

pgvector supports exact and approximate nearest-neighbor search in Postgres, including cosine distance. Its docs state that HNSW has better query performance than IVFFlat in speed/recall tradeoff, but slower builds and more memory usage. ([GitHub][6])

V0 behavior:

```text
memory.searchSemantic exists.
If embeddings are not populated, it returns a structured not_ready result.
It must not pretend semantic search worked.
```

---

# 7. Indexes

## 7.1 Timeline viewport indexes

```sql
CREATE INDEX objects_user_time_idx
ON perception.objects (user_id, event_start DESC);

CREATE INDEX objects_lane_time_idx
ON perception.objects (user_id, lane_id, event_start DESC)
INCLUDE (object_id, series_id, kind, role, privacy_class, event_end, display_title, body_type, blob_id);

CREATE INDEX objects_series_time_idx
ON perception.objects (user_id, series_id, event_start DESC)
INCLUDE (object_id, lane_id, kind, role, privacy_class, event_end, display_title, body_type);

CREATE INDEX objects_source_time_idx
ON perception.objects (user_id, source_id, event_start DESC);

CREATE INDEX objects_kind_time_idx
ON perception.objects (user_id, kind, event_start DESC);

CREATE INDEX objects_role_time_idx
ON perception.objects (user_id, role, event_start DESC);

CREATE INDEX objects_privacy_time_idx
ON perception.objects (user_id, privacy_class, event_start DESC);
```

## 7.2 Interval overlap index

```sql
CREATE INDEX objects_event_range_gist_idx
ON perception.objects
USING GIST (user_id, event_range);
```

PostgreSQL range columns support GiST and SP-GiST indexes, which is useful for overlap queries like viewport intersection. ([PostgreSQL][7])

## 7.3 JSONB index

```sql
CREATE INDEX objects_payload_gin_idx
ON perception.objects
USING GIN (payload jsonb_path_ops);
```

`jsonb_path_ops` supports fewer operators than the default JSONB GIN operator class, but PostgreSQL’s docs say it offers better performance for the supported operators. Use expression indexes for hot fields instead of asking one giant JSONB index to be a wizard. ([PostgreSQL][8])

Example expression indexes:

```sql
CREATE INDEX objects_payload_app_idx
ON perception.objects ((payload->>'app'))
WHERE payload ? 'app';

CREATE INDEX objects_payload_thread_idx
ON perception.objects ((payload->>'thread_id'))
WHERE payload ? 'thread_id';

CREATE INDEX objects_payload_session_idx
ON perception.objects ((payload->>'session_id'))
WHERE payload ? 'session_id';
```

## 7.4 Text search indexes

```sql
CREATE INDEX objects_search_vector_idx
ON perception.objects
USING GIN (search_vector);

CREATE INDEX objects_display_text_trgm_idx
ON perception.objects
USING GIN (display_text gin_trgm_ops)
WHERE display_text IS NOT NULL;
```

`pg_trgm` supports trigram-based similarity search, and PostgreSQL exposes similarity thresholds for its operators. ([PostgreSQL][9])

---

# 8. Continuous aggregates for density

Density powers zoomed-out timeline views.

```sql
CREATE MATERIALIZED VIEW perception.object_density_1m
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
```

Refresh policy:

```sql
SELECT add_continuous_aggregate_policy(
  'perception.object_density_1m',
  start_offset => INTERVAL '7 days',
  end_offset => INTERVAL '1 minute',
  schedule_interval => INTERVAL '1 minute'
);
```

Timescale continuous aggregates are created with `CREATE MATERIALIZED VIEW ... WITH (timescaledb.continuous)`, and refresh policies are created with `add_continuous_aggregate_policy`. The docs also require `time_bucket` on the hypertable partitioning time column in continuous aggregate views. ([TigerData][10])

Add later:

```text
object_density_15m
object_density_1h
object_density_1d
```

V0 must implement only:

```text
memory.queryDensity -> perception.object_density_1m
```

Default density deliberately does not group by `source_id` or `series_id`.
Source/series-specific density can be computed on demand or materialized in a
projection when a viewer layout needs it.

---

# 9. Source cursors

## 9.1 `perception.source_cursors`

```sql
CREATE TABLE perception.source_cursors (
  source_id UUID NOT NULL,

  cursor_name TEXT NOT NULL DEFAULT 'default',

  user_id UUID NOT NULL,

  cursor_value JSONB NOT NULL,
  -- adapter-owned opaque checkpoint

  advanced_by_write_id UUID,
  advanced_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  advancement_mode TEXT NOT NULL DEFAULT 'db_success',
  -- db_success

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  PRIMARY KEY (source_id, cursor_name)
);
```

## 9.2 Cursor rule

For `memory.ingestSources`:

```text
1. Adapter reads source records.
2. Adapter calls writeObjects.
3. If DB write succeeds, cursor advances.
4. If DB write fails, cursor does not advance.
```

Audit/replay tools must remain separate from cursor advancement. A cursor moves
only after the Perception DB writer has committed the source step.

---

# 10. `memory.writeObjects`

## 10.1 Purpose

`memory.writeObjects` is the explicit protocol method for writing already-normalized timeline objects.

It replaces implicit JSONL product behavior.

## 10.2 Request

```json
{
  "method": "memory.writeObjects",
  "params": {
    "user_id": "uuid",
    "write_id": "uuid",
    "atomicity": "chunk",
    "records": [
      {
        "client_record_id": "optional-client-id",

        "source_id": "uuid",
        "source_record_key": "agent/codex/session/abc/message/42",

        "series_kind": "codex_session",
        "series_key": "codex:session:abc",
        "series_display_name": "Codex session abc",
        "series_parent_key": null,

        "lane_id": "uuid",

        "kind": "codex.message",
        "role": "raw",
        "privacy_class": "normal",
        "modality": "text",
        "body_type": "text",
        "text_value": "message text here",

        "event_start": "2026-05-25T10:00:00.000000Z",
        "event_end": "2026-05-25T10:00:01.000000Z",

        "time_semantics": "interval",
        "temporal_level": "event",

        "time_resolution_ns": 1000000,
        "time_uncertainty_ns": 5000000,
        "alignment_method": "source_event_log",
        "alignment_confidence": 0.99,

        "materialization_policy": "index_events",
        "importance_score": 0.7,

        "blob": {
          "blob_id": "optional-uuid",
          "storage_backend": "local",
          "uri": "native-only-path-or-uri",
          "content_type": "application/jsonl",
          "sha256": "hex",
          "byte_count": 1234
        },

        "payload": {
          "agent_source": "codex",
          "session_id": "abc",
          "role": "assistant"
        },

        "display_title": "Agent message",
        "display_text": "message text here",

        "edges": [
          {
            "to_object_id": "uuid",
            "edge_kind": "derived_from"
          }
        ],

        "schema_name": "agent_message",
        "schema_version": 1,

        "metadata": {}
      }
    ]
  }
}
```

## 10.3 Response

Receipts must be returned in the same order as input records.

```json
{
  "ok": true,
  "write_id": "uuid",
  "chunk_count": 5,
  "record_count": 10000,
  "inserted_count": 7342,
  "duplicate_count": 2658,
  "receipts": [
    {
      "client_record_id": "optional-client-id",
      "source_id": "uuid",
      "source_record_key": "codex-session-abc/message/42",

      "source_record_id": "stable-uuid",
      "object_id": "stable-uuid",

      "event_start": "2026-05-25T10:00:00.000000Z",

      "inserted": true,
      "dedupe_reason": null,
      "duplicate_of_ordinal": null
    },
    {
      "client_record_id": "duplicate-client-id",
      "source_id": "uuid",
      "source_record_key": "codex-session-abc/message/42",

      "source_record_id": "same-stable-uuid",
      "object_id": "same-stable-uuid",

      "event_start": "2026-05-25T10:00:00.000000Z",

      "inserted": false,
      "dedupe_reason": "same_batch",
      "duplicate_of_ordinal": 0
    }
  ]
}
```

Possible `dedupe_reason` values:

```text
same_batch
existing_source_record
```

Possible error codes:

```text
INVALID_RECORD
UNKNOWN_SOURCE
UNKNOWN_LANE
SOURCE_IDENTITY_HASH_CONFLICT
SOURCE_IDENTITY_HASH_CONFLICT_IN_BATCH
BLOB_DESCRIPTOR_INVALID
DB_WRITE_FAILED
```

## 10.4 Stable receipt rules

For each input record:

```text
source_record_id = deterministic_uuid("source-record", source_id, source_record_key)
object_id        = deterministic_uuid("object",        source_id, source_record_key)
```

This guarantees:

```text
same-batch duplicate -> same object_id, inserted=false
retry after partial success -> same object_id, inserted=false
existing DB row -> same object_id, inserted=false
```

## 10.5 Batch conflict rules

Within the same write batch:

```text
same source identity + same hash:
  first wins, later rows inserted=false, dedupe_reason=same_batch

same source identity + different hash:
  fail chunk with SOURCE_IDENTITY_HASH_CONFLICT_IN_BATCH
```

Against existing DB state:

```text
same source identity + same hash:
  inserted=false, dedupe_reason=existing_source_record

same source identity + different hash:
  fail chunk with SOURCE_IDENTITY_HASH_CONFLICT
```

## 10.6 Performance requirement

Preserve COPY/staging performance.

No row-by-row insert loops.

Implementation shape:

```text
1. Validate records.
2. Compute deterministic IDs and canonical source hashes.
3. Split into bounded chunks.
4. COPY chunk into temp staging table.
5. Deduplicate same-batch records.
6. Insert new source_records with ON CONFLICT DO NOTHING.
7. Insert corresponding objects for new source_records.
8. Insert blobs if provided.
9. Insert edges if provided.
10. Return receipts for every original ordinal.
```

Default chunk size:

```text
2,000 records
```

10k record write:

```text
5 chunks max by default
```

A call may commit earlier chunks before a later chunk fails. That is acceptable because source identity makes retry safe. Cursor advancement for source ingestion still requires the whole logical source step to succeed and commit to Perception DB.

## 10.7 Staging table

Created per chunk as `TEMP TABLE ... ON COMMIT DROP`.

```sql
CREATE TEMP TABLE write_objects_stage (
  ordinal INT NOT NULL,

  user_id UUID NOT NULL,

  source_id UUID NOT NULL,
  source_record_key TEXT NOT NULL,
  source_record_id UUID NOT NULL,
  source_record_hash TEXT NOT NULL,

  object_id UUID NOT NULL,

  series_id UUID NOT NULL,
  series_kind TEXT NOT NULL,
  series_key TEXT NOT NULL,
  series_display_name TEXT,
  series_parent_key TEXT,
  modality TEXT,

  lane_id UUID NOT NULL,

  kind TEXT NOT NULL,
  role TEXT NOT NULL,
  privacy_class TEXT NOT NULL,
  body_type TEXT NOT NULL,
  text_value TEXT,
  number_value DOUBLE PRECISION,
  bool_value BOOLEAN,

  event_start TIMESTAMPTZ NOT NULL,
  event_end TIMESTAMPTZ NOT NULL,

  time_semantics TEXT NOT NULL,
  temporal_level TEXT NOT NULL,

  native_resolution_ns BIGINT,
  stored_resolution_ns BIGINT,
  indexed_resolution_ns BIGINT,
  display_resolution_hint_ns BIGINT,

  time_resolution_ns BIGINT,
  time_uncertainty_ns BIGINT,
  alignment_confidence REAL,
  alignment_method TEXT,

  materialization_policy TEXT NOT NULL,
  importance_score REAL,

  blob_id UUID,

  payload JSONB NOT NULL,

  display_title TEXT,
  display_text TEXT,

  source_start_ns BIGINT,
  source_end_ns BIGINT,
  source_sequence BIGINT,
  source_ordinal BIGINT,

  media_start_offset_ns BIGINT,
  media_end_offset_ns BIGINT,

  schema_name TEXT,
  schema_version INT,

  confidence REAL,
  metadata JSONB NOT NULL,

  PRIMARY KEY (ordinal)
) ON COMMIT DROP;
```

---

# 11. `memory.ingestSources`

## 11.1 Purpose

`memory.ingestSources` is a higher-level source adapter operation.

It may read from:

```text
Codex local sessions
Claude local sessions
iMessage
file watcher
screen capture source
browser adapter
```

It must internally normalize records and then call the same write path as `memory.writeObjects`.

## 11.2 Cursor safety

`memory.ingestSources` owns cursor advancement.

```text
No DB write success -> no cursor advancement
```

## 11.3 Response

```json
{
  "ok": true,
  "source_results": [
    {
      "source_id": "uuid",
      "read_count": 1000,
      "written_count": 1000,
      "inserted_count": 812,
      "duplicate_count": 188,
      "cursor_advanced": true,
      "advancement_mode": "db_success"
    }
  ]
}
```

---

# 12. `memory.queryViewport`

## 12.1 Purpose

Return timeline object summaries overlapping a time range.

## 12.2 Request

```json
{
  "method": "memory.queryViewport",
  "params": {
    "user_id": "uuid",

    "time": {
      "start": "2026-05-25T09:00:00Z",
      "end": "2026-05-25T10:00:00Z"
    },

    "filters": {
      "lane_ids": ["uuid"],
      "source_ids": ["uuid"],
      "source_types": ["codex", "claude"],
      "kinds": ["codex.message", "codex.tool_call"],
      "roles": ["raw", "derived"],
      "privacy_classes": ["public", "normal", "sensitive"],
      "include_invalid": false
    },

    "pagination": {
      "limit": 5000,
      "cursor": null
    },

    "include": {
      "payload": false,
      "blob_descriptor": true,
      "source_record": true,
      "edges_count": true
    },

    "explain": false
  }
}
```

## 12.3 Response

```json
{
  "ok": true,
  "objects": [
    {
      "object_id": "uuid",
      "event_start": "2026-05-25T09:10:00Z",
      "event_end": "2026-05-25T09:10:01Z",

      "lane_id": "uuid",
      "source_id": "uuid",
      "series_id": "uuid",
      "series_kind": "codex_session",
      "series_key": "codex:session:abc",
      "series_display_name": "Codex session abc",
      "source_record_id": "uuid",

      "kind": "codex.message",
      "role": "raw",
      "privacy_class": "normal",
      "body_type": "text",
      "text_value": "message text here",

      "time_semantics": "interval",
      "temporal_level": "event",

      "time_resolution_ns": 1000000,
      "time_uncertainty_ns": 5000000,
      "alignment_confidence": 0.99,

      "importance_score": 0.7,

      "display_title": "Codex message",
      "display_text_preview": "I found the bug...",

      "blob": {
        "blob_id": "uuid",
        "content_type": "application/jsonl",
        "byte_count": 1234,
        "state": "available"
      },

      "edge_counts": {
        "derived_from": 1,
        "references": 3
      }
    }
  ],
  "next_cursor": "opaque-keyset-cursor",
  "explain": null
}
```

## 12.4 SQL shape

```sql
SELECT
  o.object_id,
  o.event_start,
  o.event_end,
  o.lane_id,
  o.source_id,
  o.series_id,
  ser.series_kind,
  ser.series_key,
  ser.display_name AS series_display_name,
  o.source_record_id,
  o.kind,
  o.role,
  o.privacy_class,
  o.body_type,
  o.text_value,
  o.number_value,
  o.bool_value,
  o.time_semantics,
  o.temporal_level,
  o.time_resolution_ns,
  o.time_uncertainty_ns,
  o.alignment_confidence,
  o.importance_score,
  o.display_title,
  left(o.display_text, 512) AS display_text_preview,
  o.blob_id
FROM perception.objects o
JOIN perception.series ser
  ON ser.series_id = o.series_id
 AND ser.user_id = o.user_id
WHERE o.user_id = $1
  AND o.event_range && tstzrange($2, $3, '[)')
  AND o.valid_to IS NULL
  AND ($4::uuid[] IS NULL OR o.lane_id = ANY($4))
  AND ($5::uuid[] IS NULL OR o.source_id = ANY($5))
  AND ($6::text[] IS NULL OR o.kind = ANY($6))
  AND ($7::text[] IS NULL OR o.role = ANY($7))
  AND ($8::text[] IS NULL OR o.privacy_class = ANY($8))
ORDER BY o.event_start ASC, o.lane_id ASC, o.object_id ASC
LIMIT $9;
```

## 12.5 Pagination

Use keyset pagination, never offset pagination.

Cursor payload before encoding:

```json
{
  "event_start": "2026-05-25T09:10:00.000000Z",
  "lane_id": "uuid",
  "object_id": "uuid"
}
```

Cursor condition:

```sql
AND (
  o.event_start,
  o.lane_id,
  o.object_id
) > (
  $cursor_event_start,
  $cursor_lane_id,
  $cursor_object_id
)
```

Acceptance:

```text
cursor pagination has no duplicate objects
```

---

# 13. `memory.queryDensity`

## 13.1 Purpose

Return bucketed counts for zoomed-out timeline views.

## 13.2 Request

```json
{
  "method": "memory.queryDensity",
  "params": {
    "user_id": "uuid",

    "time": {
      "start": "2026-05-25T09:00:00Z",
      "end": "2026-05-25T10:00:00Z"
    },

    "bucket": "1m",

    "filters": {
      "lane_ids": ["uuid"],
      "kinds": ["codex.message"],
      "roles": ["raw"],
      "privacy_classes": ["public", "normal"]
    },

    "explain": false
  }
}
```

## 13.3 Response

```json
{
  "ok": true,
  "bucket": "1m",
  "buckets": [
    {
      "bucket_start": "2026-05-25T09:00:00Z",
      "lane_id": "uuid",
      "kind": "codex.message",
      "role": "raw",
      "privacy_class": "normal",
      "object_count": 42,
      "avg_importance": 0.62
    }
  ]
}
```

## 13.4 SQL shape

```sql
SELECT
  bucket_start,
  lane_id,
  kind,
  role,
  privacy_class,
  object_count,
  avg_importance
FROM perception.object_density_1m
WHERE user_id = $1
  AND bucket_start >= $2
  AND bucket_start < $3
  AND ($4::uuid[] IS NULL OR lane_id = ANY($4))
  AND ($5::text[] IS NULL OR kind = ANY($5))
  AND ($6::text[] IS NULL OR role = ANY($6))
  AND ($7::text[] IS NULL OR privacy_class = ANY($7))
ORDER BY bucket_start ASC, lane_id ASC, kind ASC;
```

`memory.queryDensity` rejects `source_ids` and `source_types` in V0. The
default continuous aggregate is intentionally lane-oriented and does not group
by source or series.

---

# 14. `memory.hydrateObjects`

## 14.1 Purpose

Return full object details for selected object IDs.

Used by:

```text
object detail viewer
show sources
debug inspector
local web object modal
```

## 14.2 Request

```json
{
  "method": "memory.hydrateObjects",
  "params": {
    "user_id": "uuid",
    "object_ids": ["uuid"],
    "include": {
      "payload": true,
      "blob_descriptor": true,
      "source_record": true,
      "edges": true
    }
  }
}
```

## 14.3 Response

```json
{
  "ok": true,
  "objects": [
    {
      "object_id": "uuid",
      "event_start": "timestamp",
      "event_end": "timestamp",
      "lane_id": "uuid",
      "source_id": "uuid",
      "series_id": "uuid",
      "series_kind": "codex_session",
      "series_key": "codex:session:abc",
      "series_display_name": "Codex session abc",
      "source_record": {
        "source_record_id": "uuid",
        "source_record_key": "key",
        "source_record_hash": "sha256",
        "first_seen_at": "timestamp",
        "last_seen_at": "timestamp",
        "seen_count": 3
      },
      "kind": "codex.message",
      "role": "raw",
      "privacy_class": "normal",
      "body_type": "text",
      "text_value": "message text here",
      "payload": {},
      "blob": {
        "blob_id": "uuid",
        "content_type": "application/jsonl",
        "byte_count": 1234,
        "blob_state": "available"
      },
      "edges": {
        "out": [],
        "in": []
      }
    }
  ],
  "missing_object_ids": []
}
```

## 14.4 Hydration SQL

Use `source_records` to map `object_id` to `(event_start, object_id)`.

```sql
SELECT o.*
FROM perception.source_records sr
JOIN perception.objects o
  ON o.object_id = sr.object_id
 AND o.event_start = sr.object_event_start
 AND o.series_id = sr.series_id
JOIN perception.series ser
  ON ser.series_id = o.series_id
 AND ser.user_id = o.user_id
WHERE sr.user_id = $1
  AND sr.object_id = ANY($2)
  AND o.valid_to IS NULL;
```

---

# 15. `memory.queryEdges`

## 15.1 Purpose

Reveal provenance and relationship graph.

## 15.2 Request

```json
{
  "method": "memory.queryEdges",
  "params": {
    "user_id": "uuid",
    "object_ids": ["uuid"],
    "direction": "both",
    "edge_kinds": ["derived_from", "references", "supersedes"],
    "limit": 1000,
    "hydrate": false
  }
}
```

## 15.3 Response

```json
{
  "ok": true,
  "edges": [
    {
      "edge_id": "uuid",
      "from_object_id": "uuid",
      "to_object_id": "uuid",
      "edge_kind": "derived_from",
      "confidence": 0.95,
      "metadata": {}
    }
  ],
  "objects": []
}
```

Acceptance:

```text
derived object can reveal source objects
```

---

# 16. `memory.searchText`

## 16.1 Purpose

Search object summaries and display text.

## 16.2 Request

```json
{
  "method": "memory.searchText",
  "params": {
    "user_id": "uuid",
    "query": "timescale database spec",
    "time": {
      "start": null,
      "end": null
    },
    "filters": {
      "lane_ids": null,
      "source_ids": null,
      "kinds": null,
      "roles": null,
      "privacy_classes": ["public", "normal", "sensitive"]
    },
    "limit": 50
  }
}
```

## 16.3 SQL shape

```sql
SELECT
  object_id,
  event_start,
  event_end,
  lane_id,
  source_id,
  kind,
  role,
  privacy_class,
  display_title,
  left(display_text, 512) AS display_text_preview,
  ts_rank(search_vector, plainto_tsquery('simple', $2)) AS rank
FROM perception.objects
WHERE user_id = $1
  AND valid_to IS NULL
  AND search_vector @@ plainto_tsquery('simple', $2)
ORDER BY rank DESC, event_start DESC
LIMIT $3;
```

Optional fuzzy fallback:

```sql
SELECT
  object_id,
  similarity(display_text, $2) AS similarity
FROM perception.objects
WHERE user_id = $1
  AND display_text % $2
ORDER BY similarity DESC
LIMIT $3;
```

Acceptance:

```text
text search returns ranked object summaries
```

---

# 17. `memory.searchSemantic`

V0 method exists but may be unpopulated.

Response when embeddings are absent:

```json
{
  "ok": false,
  "error": {
    "code": "SEMANTIC_INDEX_NOT_READY",
    "message": "Semantic search is available in the protocol but no embeddings have been populated."
  }
}
```

Once embeddings exist:

```sql
SELECT
  e.object_id,
  e.event_start,
  o.kind,
  o.display_title,
  left(o.display_text, 512) AS display_text_preview,
  e.embedding <=> $query_embedding AS distance
FROM search.object_embeddings e
JOIN perception.source_records sr
  ON sr.object_id = e.object_id
JOIN perception.objects o
  ON o.object_id = sr.object_id
 AND o.event_start = sr.object_event_start
WHERE e.user_id = $1
  AND e.embedding_model = $2
  AND o.valid_to IS NULL
ORDER BY e.embedding <=> $query_embedding
LIMIT $3;
```

pgvector queries need `ORDER BY` with a distance operator and `LIMIT` to use approximate indexes properly. ([GitHub][6])

---

# 18. `memory.explain`

## 18.1 Purpose

Return debug information for query paths without requiring psql.

Supported targets:

```text
queryViewport
queryDensity
searchText
hydrateObjects
queryEdges
```

## 18.2 Request

```json
{
  "method": "memory.explain",
  "params": {
    "target": "queryViewport",
    "params": {}
  }
}
```

## 18.3 Response

```json
{
  "ok": true,
  "target": "queryViewport",
  "sql_kind": "viewport_overlap",
  "plan": {
    "format": "json",
    "summary": {
      "estimated_rows": 5000,
      "actual_rows": 4832,
      "execution_ms": 37.2,
      "indexes_observed": [
        "objects_event_range_gist_idx",
        "objects_lane_time_idx"
      ]
    },
    "raw": {}
  }
}
```

Local web should receive sanitized explain summaries by default. Native/dev tools can request raw JSON plans.

`pg_stat_statements` must be enabled for server-side query observability; PostgreSQL documents it as tracking planning and execution statistics and notes it must be loaded through `shared_preload_libraries`. ([PostgreSQL][11])

---

# 19. Protocol surface

`memory.describe` must list:

```text
status
writeObjects
ingestSources
queryViewport
queryDensity
hydrateObjects
queryEdges
searchText
searchSemantic
explain
subscribe
```

V0 `subscribe` may be a stub, but it must be visible in describe.

Suggested `describe` response:

```json
{
  "ok": true,
  "service": "onecontext-memoryd",
  "version": "0.1.0",
  "methods": {
    "memory.status": {"state": "ready"},
    "memory.writeObjects": {"state": "ready"},
    "memory.ingestSources": {"state": "ready"},
    "memory.queryViewport": {"state": "ready"},
    "memory.queryDensity": {"state": "ready"},
    "memory.hydrateObjects": {"state": "ready"},
    "memory.queryEdges": {"state": "ready"},
    "memory.searchText": {"state": "ready"},
    "memory.searchSemantic": {"state": "stub"},
    "memory.explain": {"state": "ready"},
    "memory.subscribe": {"state": "stub"}
  }
}
```

---

# 20. Local web API

The local web API is an adapter over memory protocol methods.

## 20.1 Endpoints

```text
GET/POST /api/memory/viewport -> memory.queryViewport
GET/POST /api/memory/object   -> memory.hydrateObjects
GET/POST /api/memory/density  -> memory.queryDensity
GET/POST /api/memory/edges    -> memory.queryEdges
GET/POST /api/memory/search   -> memory.searchText
```

Hard no:

```text
/api/memory/traces -> 404
```

## 20.2 Browser redaction

Browser JSON must not include:

```text
raw local home paths
file:// URLs pointing into user directories
raw blob uri when storage_backend = local
large unsanitized terminal output by default
```

Browser-safe blob descriptor:

```json
{
  "blob_id": "uuid",
  "content_type": "video/mp4",
  "byte_count": 123456,
  "duration_ms": 30000,
  "width": 3024,
  "height": 1964,
  "blob_state": "available",
  "has_safe_uri": false
}
```

Acceptance:

```text
browser JSON contains no raw local home path
viewer can load viewport and open object details
/api/memory/traces returns 404
```

---

# 21. Future: Privacy semantics

Privacy policy is not a V0 gate.

V0 may store `privacy_class` as object metadata because it is cheap and useful for future filtering. V0 must not claim complete privacy enforcement beyond browser-safe redaction of local paths, raw blob URIs, and oversized raw text fields.

Implement this section once the Perception DB has stable writes, stable reads, object hydration, and a real viewer surface.

Default viewport filters should include:

```text
public
normal
sensitive
```

Default viewport filters should exclude:

```text
secret
redacted
```

Native/local privileged clients may request `secret`.

Browser clients must not receive `secret` unless an explicit local capability says so.

Redacted objects may appear as timeline placeholders:

```json
{
  "object_id": "uuid",
  "kind": "redacted",
  "privacy_class": "redacted",
  "event_start": "...",
  "event_end": "...",
  "display_title": "Redacted object"
}
```

---

# 22. Correction and supersession

Never silently rewrite memory history.

Correction flow:

```text
1. Mark old object valid_to = now().
2. Insert corrected object with new source identity.
3. Add edge: new_object supersedes old_object.
```

SQL:

```sql
UPDATE perception.objects
SET valid_to = now()
WHERE user_id = $1
  AND object_id = $2
  AND event_start = $3
  AND valid_to IS NULL;
```

Then:

```sql
INSERT INTO perception.object_edges (
  edge_id,
  user_id,
  from_object_id,
  to_object_id,
  edge_kind,
  confidence
)
VALUES (
  $edge_id,
  $user_id,
  $new_object_id,
  $old_object_id,
  'supersedes',
  1.0
);
```

---

# 23. Write implementation details

## 23.1 Owned files

```text
crates/onecontext-memory-db/src/timescale_writer.rs
crates/onecontext-memory-db/src/bin/onecontext-memoryd.rs
crates/onecontext-memory-db/tests/
```

## 23.2 Writer pipeline

```text
normalize
validate
canonicalize
hash
dedupe same-batch
COPY to staging
insert blobs
insert source_records
insert objects
insert edges
return receipts
```

## 23.3 Canonical source hash

The canonical hash includes:

```text
source_id
source_record_key
kind
event_start
event_end
lane_id
role
privacy_class
payload canonical JSON
blob sha256 if present
display_title
display_text
schema_name
schema_version
```

It excludes:

```text
client_record_id
request ordinal
ingest_time
write_id
metadata fields explicitly marked non-semantic
```

Use SHA-256 or BLAKE3. The important thing is stable canonicalization.

## 23.4 Bounded chunks

Default:

```text
max records per chunk = 2,000
```

Acceptance:

```text
10k record write completes in bounded chunks
```

Response includes:

```text
chunk_count
inserted_count
duplicate_count
```

## 23.5 Partial failure semantics

If chunk 1 succeeds and chunk 2 fails:

```text
chunk 1 remains committed
method returns error
safe_to_retry = true
cursor must not advance
retry dedupes chunk 1
```

This is why source identity matters. It turns partial success from catastrophe into beige wallpaper.

---

# 24. Read implementation details

## 24.1 `queryViewport`

Must support filters:

```text
time
lane
source
kind
role
privacy
```

Must support:

```text
cursor pagination
explain
browser-safe projection
native detail projection
```

Acceptance:

```text
5k object viewport under target on local dev DB
cursor pagination has no duplicate objects
```

## 24.2 `queryDensity`

Must hit continuous aggregates, not the raw hypertable, for 1-minute density.

Acceptance:

```text
1m density returns buckets for same time window
```

## 24.3 `hydrateObjects`

Must return:

```text
full payload
blob descriptor
source record
edges if requested
```

Acceptance:

```text
object detail view can show payload, blob descriptor, source record, and edges
```

## 24.4 `queryEdges`

Must support:

```text
incoming
outgoing
both
edge kind filter
optional hydration
```

Acceptance:

```text
derived object can reveal source objects
```

## 24.5 `searchText`

Must return ranked object summaries.

Use:

```text
search_vector full-text ranking
optional pg_trgm fuzzy fallback
```

Acceptance:

```text
text search returns ranked object summaries
```

---

# 25. Feature staging

This section separates the protocol destination from the V0 build order.

| Feature | Stage | Initial implementation thought |
| --- | --- | --- |
| Perception DB naming | Now | Use Perception DB as the conceptual name immediately. Keep existing crate/process/protocol names until a dedicated naming cleanup. |
| Physical schema cutover | Now | The durable tables are `perception.*`. Reset transitional dev DBs and ship one current perception-only schema for product work. |
| Source identity model | Now | Adopt `(source_id, source_record_key)` and deterministic receipts before adding more connectors. Add an adapter from current `connector_key/source_record_id/source_hash`. |
| `memory.writeObjects` | V0 | Build this first as the single durable write protocol. Preserve COPY/staging throughput. |
| Source cursor advancement | V0 | Implement with writes so backfills and daemon polling cannot advance after a failed durable write. |
| `memory.ingestSources` | V0 | Add after `writeObjects`; source adapters should normalize and call the same writer path. |
| Codex, Claude, iMessage connectors | V0 | Keep these as the first proof set because they exercise local files, SQLite, filtered messages, and high-volume text. |
| `memory.queryViewport` | V0 | First read path. Needs time overlap, lane/source/kind/role filters, keyset pagination, and browser-safe projection. |
| `memory.queryDensity` | V0 | Needed for zoomed-out viewer performance. Implement after raw viewport is correct. |
| `memory.hydrateObjects` | V0 | Required for object details; viewport should stay light and hydrate only on demand. |
| `memory.queryEdges` | V0 small | Implement basic provenance reads early so derived artifacts can show evidence. Deeper graph traversal can wait. |
| `memory.searchText` | V0 or V0.5 | Cheap if the indexes are already present. Useful for debugging and viewer navigation. |
| Local web viewport/detail/density | V0 | Web should be an adapter over protocol methods, not its own DB reader. |
| Viewer timeline | V0 | Show lanes, objects, density, details, and source evidence. Keep it operator/debuggable before making it pretty. |
| `memory.explain` | V0 dev-only | Useful for benchmarking read paths. Return sanitized summaries to web clients. |
| Semantic embeddings | V1 | Wait until object identity, payload shapes, and text extraction are stable. Otherwise embeddings churn constantly. |
| `memory.searchSemantic` | V1 stub in V0 | Return structured `not_ready` until embeddings are populated. |
| Subscriptions | V1 | Polling is enough for V0. Add live updates after write/read protocol stability. |
| Socket JSON-RPC transport | V1 | Process transport is good enough while method shapes are moving. Add sockets when Swift/viewer traffic needs it. |
| Correction and supersession | V1 | Needed once summaries and memories are first-class. Not required for raw capture ingest. |
| Blob lifecycle and archival | V1 | Store blob descriptors now. Add retention/archive/delete policy once media ingest is real. |
| Columnstore/compression policy | V1 after load data | Add after measuring 7+ days of data and query patterns. |
| Privacy policy enforcement | Future | Store `privacy_class` now, but defer real policy, capability checks, and redacted placeholders until the viewer and local trust model are settled. |
| Multi-user/RLS | Future | Avoid for V0, especially with continuous aggregate constraints. Revisit for hosted or shared deployments. |
| Additional connectors | Future | Slack, Discord, Linear, browser, terminal, and screen/audio should plug into the same source adapter contract after the first three are solid. |

---

# 26. Implementation scaffolding and method design

This is the target Rust shape for V0 completion.

The crate and binary names may remain `onecontext-memory-db` and `onecontext-memoryd` during V0. The durable database schema must be `perception.*`.

## 26.1 Ground rules

```text
1. onecontext-memoryd is a dispatcher, not the place where SQL grows forever.
2. Protocol DTOs live in Rust types, not ad hoc serde_json construction.
3. Write paths use source identity and deterministic receipts.
4. Read paths use perception.* tables only.
5. Local web reads through protocol methods, not direct SQL and not JSONL.
6. Source adapters emit normalized perception objects; they do not own SQL.
7. Source cursors advance only after DB commits.
```

## 26.2 Proposed Rust module layout

```text
crates/onecontext-memory-db/src/
  lib.rs
  db.rs
  protocol.rs
  source_identity.rs
  write_objects.rs
  ingest_sources.rs
  source_cursors.rs
  read_viewport.rs
  query_density.rs
  hydrate.rs
  edges.rs
  search.rs
  local_adapters.rs
  source_connector.rs
  timescale_writer.rs
  bin/
    onecontext-memory-db.rs
    onecontext-memoryd.rs
```

Responsibilities:

| Module | Owns |
| --- | --- |
| `db.rs` | Connection resolution, pooled or direct Postgres client wrapper, current schema helpers, SQL error mapping. |
| `protocol.rs` | Shared request/response envelope, method enum, typed DTO exports, error shape, JSON parsing helpers. |
| `source_identity.rs` | Deterministic UUID generation, canonical source hash, same-batch duplicate detection helpers. |
| `write_objects.rs` | `memory.writeObjects`, COPY staging, chunk transactions, source record claims, object/edge insertion. |
| `ingest_sources.rs` | `memory.ingestSources`, adapter orchestration, and cursor safety. |
| `source_cursors.rs` | `perception.source_cursors` read/update helpers with durability rules. |
| `read_viewport.rs` | `memory.queryViewport`, filters, keyset pagination, browser-safe projection. |
| `query_density.rs` | `memory.queryDensity`, continuous aggregate reads, bucket validation. |
| `hydrate.rs` | `memory.hydrateObjects`, full object payload/blob/source hydration. |
| `edges.rs` | `memory.queryEdges`, provenance edge reads and lightweight graph expansion. |
| `search.rs` | `memory.searchText` and `memory.searchSemantic` stub. |
| `local_adapters.rs` | Codex, Claude, iMessage normalized perception objects. |
| `timescale_writer.rs` | Retire in favor of `write_objects.rs`, or keep only as a thin module-level wrapper with no legacy schema behavior. |

`onecontext-memoryd.rs` should shrink toward:

```text
parse CLI/process transport
read one protocol request
dispatch method enum
print exactly one JSON response to stdout
write diagnostics to stderr
return process status
```

## 26.3 Perception DB current schema

V2 is a perception-only current schema. Transitional dev databases that were
created with prototype schemas should be reset; the product contract no longer
keeps legacy tables active.

```text
reset:
  ./scripts/memory-db-dev.sh reset
  ./scripts/memory-db-dev.sh provision

current schema:
  create app, perception, and search schemas
  create perception.sources, lanes, blobs, series, source_records, objects,
    object_edges, object_density_1m, source_cursors, and search embeddings
  do not create, backfill, or query capture.* product tables

series writer:
  choose or create perception.series before claiming source_records
  store source_records.series_id and objects.series_id atomically
  keep perception.source_records as the dedupe/idempotency owner

adapter mappings:
  codex/claude session -> agent session series
  browser window/tab -> browser window/tab series
  imessage thread -> conversation series
  file path/history -> file path series
  screen/audio/metric streams -> display/input/metric series

read APIs:
  queryViewport returns object summaries with series_id
  hydrateObjects includes source_record and series provenance
  series-scoped reads return event-ordered records
  density defaults stay lane-oriented unless a series filter is explicit

benchmark gates:
  focused schema/write/read contract tests pass
  10k write in bounded chunks
  5k viewport under local target
  density and hydrate paths under local targets
```

Rules:

```text
fresh dev DB: app + perception + search schemas exist
transitional dev DB: reset before judging V2 behavior
tests: assert perception.objects is the hypertable
tests: assert current schema does not include capture.*
tests: assert writes and reads are series-aware
```

## 26.4 Protocol transport

V0 process transport:

```bash
onecontext-memoryd protocol memory.queryViewport --request-json request.json
onecontext-memoryd protocol memory.writeObjects --request-json -
```

Request shape:

```json
{
  "schema_version": 1,
  "request_id": "uuid-or-client-string",
  "method": "memory.queryViewport",
  "params": {}
}
```

Response envelope:

```json
{
  "schema_version": 1,
  "protocol": "memory.queryViewport.v1",
  "surface": "perception_db",
  "status": "ok",
  "request_id": "uuid-or-client-string",
  "result": {},
  "error": null,
  "stats": {
    "elapsed_ms": 12
  }
}
```

Error envelope:

```json
{
  "schema_version": 1,
  "protocol": "memory.writeObjects.v1",
  "surface": "perception_db",
  "status": "error",
  "request_id": "uuid-or-client-string",
  "result": null,
  "error": {
    "code": "SOURCE_IDENTITY_HASH_CONFLICT",
    "message": "Source identity was reused with a different canonical hash.",
    "retryable": false,
    "details": {}
  },
  "stats": {
    "elapsed_ms": 4
  }
}
```

Process rules:

```text
stdout: exactly one protocol JSON response
stderr: human diagnostics only
exit 0: request parsed and method returned a structured response
exit nonzero: process invocation failed before a protocol response was possible
```

## 26.5 Shared DTOs

Core Rust structs:

```text
ProtocolRequest<T>
ProtocolResponse<T>
ProtocolError
MethodName
PerceptionSource
PerceptionLane
BlobDescriptor
SourceRecordIdentity
SourceRecordReceipt
PerceptionObjectInput
PerceptionObjectSummary
PerceptionObjectHydration
PerceptionEdge
OpaqueCursor
```

Keep DTOs stable and serializable with `serde`. SQL row structs can be separate from public protocol structs.

## 26.6 Method design

| Method | Module | V0 behavior |
| --- | --- | --- |
| `memory.status` | `protocol.rs`/daemon | Return DB configured, DB reachable, schema state, and enabled sources. |
| `memory.describe` | `protocol.rs` | Return available methods, versions, and readiness state. |
| `memory.writeObjects` | `write_objects.rs` | Insert normalized records into `perception.source_records` and `perception.objects`, return stable receipts. |
| `memory.ingestSources` | `ingest_sources.rs` | Run source adapters, call `writeObjects`, update cursors only after durable write success. |
| `memory.queryViewport` | `read_viewport.rs` | Return object summaries overlapping a time range with filters and keyset pagination. |
| `memory.queryDensity` | `query_density.rs` | Read `perception.object_density_1m` for zoomed-out lanes. |
| `memory.hydrateObjects` | `hydrate.rs` | Return full payload/blob/source details for selected object IDs. |
| `memory.queryEdges` | `edges.rs` | Return provenance edges around selected object IDs. |
| `memory.searchText` | `search.rs` | Full-text search over title/text, returning summaries. |
| `memory.searchSemantic` | `search.rs` | Return structured `not_ready` until embeddings are populated. |
| `memory.explain` | read modules | Return sanitized query plan summaries for dev/benchmarking. |
| `memory.subscribe` | daemon | V0 stub; polling remains acceptable. |

### `memory.writeObjects`

Pipeline:

```text
parse request
validate records
canonicalize payloads
compute source_record_hash
compute deterministic source_record_id/object_id
dedupe same-batch source identities
split into bounded chunks
BEGIN chunk transaction
COPY chunk into temp staging table
upsert/insert blobs
claim perception.source_records
insert perception.objects for newly claimed records
insert perception.object_edges
COMMIT
return receipts in input order
```

Transaction rule:

```text
source_records and objects must commit together for a chunk
no committed source_record may point to a missing perception.objects row
retrying the same chunk returns the same receipts
```

### `memory.ingestSources`

Pipeline:

```text
load source cursor
run selected local adapters
normalize adapter output into writeObjects records
call writeObjects
advance source cursor only after durable write success
return per-source ingest report
```

### `memory.queryViewport`

Default projection:

```text
object_id
event_start/event_end
lane_id
source_id
series_id
series_kind/series_key/series_display_name
kind
role
body_type
display_title
display_text_preview
blob descriptor summary
edge counts
```

Default exclusions:

```text
full payload
raw local blob URI
large display_text
secret/redacted enforcement claims
```

Pagination:

```text
keyset cursor over (event_start, lane_id, object_id)
no OFFSET
stable ordering for repeated page fetches
```

### `memory.hydrateObjects`

Hydration is separate from viewport because the viewer should stay fast.

```text
input: object_ids[]
lookup: perception.source_records -> perception.objects by (object_id, event_start, series_id)
include: full payload, metadata, blob descriptor, source record, selected edges
redact: local filesystem paths for browser adapters
```

### `memory.queryEdges`

V0 supports:

```text
direction: incoming | outgoing | both
edge_kind filter
limit
include_object_summaries optional
```

Do not implement recursive graph traversal in V0.

### `memory.searchText`

V0 search should be practical, not magical:

```text
full-text search over search_vector
optional trigram fallback for fuzzy lookup
same browser-safe summary shape as queryViewport
```

### `memory.explain`

Explain exists to keep read performance honest:

```text
target: queryViewport | queryDensity | hydrateObjects | searchText
output: sanitized plan summary, row estimates, timing, index names
browser: no raw SQL with local paths or secrets
native/dev: may request raw JSON plan
```

## 26.7 Testing and benchmark harness

Required test layers:

```text
unit:
  deterministic UUID generation
  canonical hash stability
  same-batch dedupe
  cursor encode/decode
  protocol envelope serialization

schema:
  fresh DB has perception schema
  perception.objects is a hypertable
  required indexes exist
  density aggregate exists or is gracefully skipped when Timescale feature unavailable

integration:
  write 1k mixed Codex/Claude/iMessage objects
  rewrite same 1k and get duplicate receipts
  source identity hash conflict fails
  queryViewport returns overlap window
  hydrateObjects returns full payloads
  queryEdges returns derived_from/supersedes edges

performance:
  10k write in bounded chunks
  5k viewport under local target
  density query under local target
```

Benchmark command target:

```bash
cargo test -p onecontext-memory-db
cargo run -p onecontext-memory-db --bin onecontext-memoryd -- bench --database-url "$ONECONTEXT_MEMORY_DB_URL" --sources codex,claude,imessage
```

---

# 27. Agent workplan

Each agent should preserve the shared scaffold above and avoid broad ownership drift. If two agents need the same dispatcher file, keep dispatcher edits tiny and mechanical.

## Agent A: Schema and DB harness

Owns:

```text
crates/onecontext-memory-db/schema/current.sql
crates/onecontext-memory-db/src/db.rs
crates/onecontext-memory-db/tests/
```

Tasks:

```text
1. Replace transitional schema order with a perception-only current schema.
2. Create perception.sources, perception.lanes, perception.blobs, and
   perception.series.
3. Create perception.source_records with deterministic IDs supplied by writer
   and required series_id.
4. Create perception.objects as the Timescale hypertable with required
   series_id.
5. Add viewport, range, JSONB, text, series, and source identity indexes.
6. Add perception.object_edges.
7. Add search.object_embeddings with semantic search allowed to be empty.
8. Add perception.object_density_1m continuous aggregate.
9. Add perception.source_cursors.
10. Add db.rs helpers for connecting, resetting dev DBs, and checking current
    schema state.
11. Keep schema contract tests pointed at perception.* and assert capture.*
    is absent from the active schema.
```

Acceptance:

```text
fresh DB bootstraps cleanly
perception.objects hypertable exists
perception.source_records uniqueness works
perception.object_density_1m exists or degrades explicitly when unavailable
required indexes exist
tests prove product schema targets perception.* for new work
```

---

## Agent B: Protocol DTOs, source identity, and dispatcher

Owns:

```text
crates/onecontext-memory-db/src/protocol.rs
crates/onecontext-memory-db/src/source_identity.rs
crates/onecontext-memory-db/src/bin/onecontext-memoryd.rs
crates/onecontext-memory-db/tests/
```

Tasks:

```text
1. Define ProtocolRequest<T>, ProtocolResponse<T>, ProtocolError.
2. Define MethodName and typed request/response DTOs for V0 methods.
3. Standardize response envelope: schema_version, protocol, surface, status, request_id, result, error, stats.
4. Implement deterministic UUID helpers for source_record_id and object_id.
5. Implement canonical source hash helpers.
6. Add same-batch duplicate detection helpers.
7. Refactor onecontext-memoryd protocol command into a thin typed dispatcher.
8. Keep existing protocol method names working where practical.
```

Acceptance:

```text
describe lists all V0 methods and stub states
all protocol methods return the shared envelope shape
deterministic UUIDs are stable across runs
canonical hash tests cover JSON key ordering
daemon prints exactly one JSON response on stdout for protocol requests
```

---

## Agent C: Write path, ingestSources, and cursors

Owns:

```text
crates/onecontext-memory-db/src/write_objects.rs
crates/onecontext-memory-db/src/ingest_sources.rs
crates/onecontext-memory-db/src/source_cursors.rs
crates/onecontext-memory-db/src/timescale_writer.rs
crates/onecontext-memory-db/src/bin/onecontext-memoryd.rs
crates/onecontext-memory-db/tests/
```

Tasks:

```text
1. Implement memory.writeObjects against perception.*.
2. Preserve COPY/staging and bounded chunking.
3. Claim perception.source_records before inserting perception.objects.
4. Return stable receipts in input order.
5. Detect same-batch duplicate source identities.
6. Detect hash conflicts against existing source records.
7. Implement memory.ingestSources by normalizing local adapters and calling writeObjects.
8. Advance perception.source_cursors only after durable write success.
9. Keep the hot ingest path DB-first and cursor-safe.
```

Acceptance:

```text
10k record write completes in bounded chunks
same-batch duplicates return inserted=false
existing source_records rows return inserted=false
hash conflict fails safely
partial committed chunk is safe to retry
cursor does not advance after failed DB write
Codex/Claude/iMessage ingestSources can populate perception.objects
```

---

## Agent D: Read methods, hydration, edges, search, and explain

Owns:

```text
crates/onecontext-memory-db/src/bin/onecontext-memoryd.rs
crates/onecontext-memory-db/src/read_viewport.rs
crates/onecontext-memory-db/src/query_density.rs
crates/onecontext-memory-db/src/hydrate.rs
crates/onecontext-memory-db/src/edges.rs
crates/onecontext-memory-db/src/search.rs
crates/onecontext-memory-db/tests/
```

Tasks:

```text
1. Implement memory.queryViewport over perception.objects.
2. Support time, lane, source, kind, and role filters.
3. Add keyset cursor pagination.
4. Implement memory.queryDensity over perception.object_density_1m.
5. Implement memory.hydrateObjects.
6. Implement memory.queryEdges.
7. Implement memory.searchText.
8. Stub memory.searchSemantic cleanly until embeddings are populated.
9. Add memory.explain for viewport/density/search with sanitized summaries.
```

Acceptance:

```text
5k object viewport under target on local dev DB
1m density returns buckets for same time window
cursor pagination has no duplicate objects
hydrateObjects returns payload, blob descriptor, source record, and edges
queryEdges can reveal derived_from source objects
text search returns ranked object summaries
semantic search returns structured not_ready when unpopulated
explain returns sanitized plan summary
```

---

## Agent E: Swift runtime, local web adapter, viewer, and end-to-end harness

Owns:

```text
macos/Sources/OneContextMemoryRuntime/
macos/Sources/OneContextLocalWeb/WikiLocalAPI.swift
macos/Tests/OneContextLocalWebTests/
local web viewer files
scripts/test-installed-app-live-permission-capabilities.sh
```

Tasks:

```text
1. Keep Swift runtime talking to onecontext-memoryd protocol transport.
2. Ensure Swift does not shell out to psql or read JSONL as product state.
3. Keep /api/memory/viewport mapped to memory.queryViewport.
4. Add /api/memory/object mapped to memory.hydrateObjects.
5. Add /api/memory/density mapped to memory.queryDensity.
6. Add /api/memory/edges mapped to memory.queryEdges.
7. Add /api/memory/search mapped to memory.searchText.
8. Keep browser redaction tests.
9. Build visible loading/error/empty states in the viewer.
10. Ensure /api/memory/traces remains unavailable.
11. Add an end-to-end dev harness that writes sample data and opens the viewer.
```

Acceptance:

```text
/api/memory/traces returns 404
browser JSON contains no raw local home path
viewer can load viewport and open object details
viewer can show sources for a derived object
viewer can display density buckets
Swift runtime tests prove no psql provider is used
Swift runtime tests prove no JSONL read provider is used
```

---

# 28. V0 acceptance tests

The protocol is ready for V0 when these pass:

```text
1. describe lists status, writeObjects, ingestSources, queryViewport,
   queryDensity, hydrateObjects, queryEdges, searchText, searchSemantic,
   explain, subscribe.

2. writeObjects inserts 1,000 mixed Codex/Claude/iMessage objects.

3. Rewriting the same 1,000 source records deduplicates all rows.

4. Same-batch duplicate source identities return inserted=false.

5. Existing source_records rows return inserted=false.

6. A duplicate source identity with a different hash fails safely.

7. 10k record write completes in bounded chunks.

8. queryViewport returns objects overlapping a 1-hour window.

9. queryViewport supports lane, source, kind, and role filters.

10. queryViewport cursor pagination has no duplicate objects.

11. queryDensity returns 1-minute buckets for the same hour.

12. hydrateObjects returns full payloads for selected object IDs.

13. queryEdges returns OCR/transcript/summary provenance edges.

14. searchText returns ranked object summaries.

15. searchSemantic returns structured not_ready if embeddings are absent.

16. Local web /api/memory/viewport returns browser-safe JSON.

17. Local web /api/memory/object opens object detail.

18. Local web /api/memory/density returns density buckets.

19. Local web /api/memory/traces remains unavailable.

20. Browser JSON contains no raw local home path.

21. Swift runtime tests prove no psql provider is used.

22. Swift runtime tests prove no JSONL read provider is used.

23. Cursor advancement requires DB success.
```

---

# 29. Current gap list

Already in good shape:

```text
Timescale write path
source_records idempotency
Codex/Claude/iMessage local source adapters
memory.queryViewport process transport
local web /api/memory/viewport adapter
Swift protocol process client
```

Still to build:

```text
memory.writeObjects as explicit protocol method
memory.ingestSources as explicit protocol method
queryViewport filters and cursor pagination
queryDensity implementation
hydrateObjects implementation
queryEdges implementation
searchText implementation
socket JSON-RPC transport
subscription notifications
viewer object detail flow
viewer show-sources flow
```

---

# 30. Final implementation slogan

```text
Write through source identity.
Read through memory methods.
Store in Timescale.
Show the evidence.
Never make JSONL the product protocol again.
```

The shape is simple but not simplistic: one time spine, heterogeneous objects, stable receipts, strict source identity, blob-backed media, density views for zoom, and evidence edges for trust.

[1]: https://github.com/timescale/timescaledb?utm_source=chatgpt.com "timescale/timescaledb: A time-series database for high- ..."
[2]: https://www.postgresql.org/docs/current/storage-toast.html?utm_source=chatgpt.com "Documentation: 18: 66.2. TOAST"
[3]: https://www.postgresql.org/docs/current/datatype-datetime.html?utm_source=chatgpt.com "Documentation: 18: 8.5. Date/Time Types"
[4]: https://www.postgresql.org/docs/current/functions-uuid.html?utm_source=chatgpt.com "Documentation: 18: 9.14. UUID Functions"
[5]: https://www.tigerdata.com/docs/learn/data-model/primary-keys-time-and-uniqueness?utm_source=chatgpt.com "Primary keys, time columns, and uniqueness for hypertables"
[6]: https://github.com/pgvector/pgvector "GitHub - pgvector/pgvector: Open-source vector similarity search for Postgres · GitHub"
[7]: https://www.postgresql.org/docs/current/rangetypes.html?utm_source=chatgpt.com "Documentation: 18: 8.17. Range Types"
[8]: https://www.postgresql.org/docs/current/gin.html?utm_source=chatgpt.com "Documentation: 18: 65.4. GIN Indexes"
[9]: https://www.postgresql.org/docs/current/pgtrgm.html?utm_source=chatgpt.com "F.35. pg_trgm — support for similarity of text using trigram ..."
[10]: https://www.tigerdata.com/docs/reference/timescaledb/continuous-aggregates/create_materialized_view?utm_source=chatgpt.com "CREATE MATERIALIZED VIEW (continuous aggregate)"
[11]: https://www.postgresql.org/docs/current/pgstatstatements.html?utm_source=chatgpt.com "F.32. pg_stat_statements — track statistics of SQL planning ..."
