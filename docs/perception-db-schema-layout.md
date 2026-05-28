---
title: 1Context Perception DB Schema Layout
slug: perception-db-schema-layout
section: architecture
access: private
summary: "Canonical Perception DB schema layout: lanes are presentation, series are identity, objects are temporal records, blobs are bytes, and edges are meaning."
status: draft
last_updated: 2026-05-25
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Perception DB Schema Layout

## 0. Purpose

This document is the schema-shape companion to:

- [Memory DB Design Spec](memory-db-design-spec.md)
- [Memory DB API And Protocol Spec](memory-db-api-protocol-spec.md)
- [Memory DB Infra And Viewer Spec](memory-db-infra-viewer-spec.md)
- [Coding Agent Ingest Spec](coding-agent-ingest-spec.md)

Those docs define the product thesis, protocol, and implementation path. This
spec defines the row model and table layout that keep Perception DB from
becoming a column, table, or lane explosion as Codex sessions, browser windows,
tabs, files, app windows, terminal sessions, iMessage threads, metrics,
screenshots, summaries, and memories multiply.

The row rule:

```text
Every timeline row means exactly one thing:
  a time-correlated record in a logical series.
```

The grouping rule:

```text
Columns are ontology.
Lanes are presentation.
Series are identity.
Rows are time.
Blobs are bytes.
Edges are meaning.
```

The growth rule:

```text
Rows should grow with time.
Columns should grow only with the universal record model.
Lanes should grow slowly.
Series can grow fast.
Views can explode freely.
```

Concrete consequence:

```text
Codex session      = series
Browser window     = series
Browser tab        = series, often child of a window series
App window         = series
iMessage thread    = series
File path/history  = series
Screen display     = series
Audio input        = series
Scalar metric      = series
Viewer lane split  = projection or virtual lane
```

Not:

```text
codex_session_abc as a SQL column
browser_window_123 as a SQL column
one durable physical lane per tab/thread/session/file
one source-specific table per connector
```

## 1. Core Ontology

Use these concepts consistently.

### Lane

A lane is a coarse, durable product track. It answers where records appear in
the default viewer.

Good durable lanes:

```text
screen.main
audio.mic
ui.events
browser.windows
browser.tabs
browser.events
agents.sessions
agents.messages
agents.tools
terminal.sessions
terminal.output
files.versions
messages.imessage
system.metrics
memory.summaries
system.health
```

Bad durable lanes:

```text
codex.session.abc123
chrome.window.927.3
imessage.thread.family
file./repo/src/app.ts
metric.codex.latency_ms
```

Those are series, not lanes.

### Series

A series is a logical thing with a timeline.

Examples:

```text
codex_session
claude_session
browser_window
browser_tab
screen_display
audio_input
imessage_thread
file_path
repo
terminal_session
agent_run
metric
memory_topic
```

Series can grow quickly because they are rows in `perception.series`, not SQL
columns.

Example series keys:

```text
codex:session:abc123
claude:session:sess-123
browser:chrome:window:927:3
browser:chrome:tab:128
file:path_hash:sha256...
screen:main
audio:mic:default
imessage:thread:chat-abc
metric:codex:latency_ms
```

Each series has a default or home lane, but individual records in that series
may appear in more specific lanes. For example, a Codex session series can have
`agents.sessions` as its default lane while its message records use
`agents.messages` and its tool-call records use `agents.tools`. The series is
identity; the record lane is presentation.

`parent_series_id` is only for stable stream hierarchy, such as browser tab
inside browser window, file inside repo, metric under source, or conversation
inside account. It is not a substitute for `object_edges`. Use edges for
temporal containment, derivation, references, corrections, and provenance.

### Temporal Record

A temporal record is one time-correlated fact, event, artifact, interval,
summary, claim, sample, or media chunk inside a series.

The existing table name is:

```text
perception.objects
```

This spec treats each `perception.objects` row as a temporal record. A future
renaming to `perception.records` is allowed, but not required for the row model.

The row always says:

```text
For user U, series S produced or contains record R of kind K
over time range [event_start, event_end),
with body B, optional blob bytes, and source identity I.
```

No separate row meanings:

```text
click at T         -> [T, T + 1 microsecond)
Codex session      -> [first rollout timestamp, last rollout timestamp)
screen video chunk -> [chunk start, chunk end)
file version       -> [mtime, mtime + 1 microsecond)
audio transcript   -> [phrase start, phrase end)
memory             -> [best known relevant interval]
metric sample      -> [T, T + 1 microsecond)
```

### Blob

A blob stores heavy bytes. The record owns time. The blob owns bytes.

Blob examples:

```text
video chunk
audio chunk
screenshot
file contents
raw Codex rollout JSONL
large terminal log
raw browser event log
```

### Source Record

A source record is the idempotency and dedupe claim:

```text
(source_id, source_record_key) -> object_id
```

It answers whether the same source fact has already been written.

### Edge

Edges preserve containment, provenance, references, derivations, corrections,
and relationships without changing the row shape.

Examples:

```text
codex.session contains codex.message
codex.session contains codex.tool_call
codex.tool_call references file.diff
ocr_span derived_from screen.chunk
summary derived_from codex.session
memory references summary
new_summary supersedes old_summary
```

### Projection

A projection is a viewer layout or materialized view over canonical records. It
may split one coarse lane into many visible sublanes without changing storage.

Projection examples:

```text
one display lane per Codex session
one display lane per browser window
one display lane per active project
one display lane per file touched in a selected session
one display lane per search result cluster
```

Projection explosion is allowed because projections are view state or rebuildable
cache, not source-of-truth ontology.

## 2. Truth Hierarchy

Use this hierarchy when deciding where data belongs:

```text
Canonical timeline rows:
  perception.objects

Logical stream/container identity:
  perception.series

Durable product presentation:
  perception.lanes

Source identity and dedupe:
  perception.sources
  perception.source_records

Large bytes:
  perception.blobs

Semantic grouping and provenance:
  perception.object_edges

Source-specific detail:
  perception.objects.payload JSONB
  promoted hot columns or expression indexes

Viewer explosion:
  perception.timeline_projections
  perception.timeline_projection_items
```

## 3. Core Tables

### 3.1 `perception.series`

`perception.series` stores the logical streams and containers that records
belong to. This is the canonical answer to "which session/window/thread/file is
this record part of?"

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
  -- optional: browser tab inside browser window, message stream inside
  -- session, metric under adapter, file under repo, etc.

  display_name TEXT,

  modality TEXT,
  -- text, json, image, video, audio, binary, scalar, mixed

  default_privacy_class TEXT NOT NULL DEFAULT 'normal',

  default_time_resolution_ns BIGINT,
  default_time_uncertainty_ns BIGINT,

  first_event_start TIMESTAMPTZ,
  last_event_end TIMESTAMPTZ,

  record_count BIGINT NOT NULL DEFAULT 0,
  -- maintained summary/cache; perception.objects remains the source of truth

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

CREATE INDEX series_user_default_lane_last_seen_idx
ON perception.series (user_id, default_lane_id, last_event_end DESC NULLS LAST);

CREATE INDEX series_parent_idx
ON perception.series (parent_series_id);
```

Series are allowed to be high-cardinality. A machine with many browser windows,
files, terminal sessions, and agent sessions should create many series rows.
That is the point. It is still not creating columns, tables, or durable physical
lanes.

### 3.2 `perception.objects`

`perception.objects` is the Timescale hypertable. In this spec, each object row
is a temporal record in a series.

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
  -- concrete display/product lane for this record. It often matches
  -- series.default_lane_id, but can differ for records like messages,
  -- tool calls, OCR spans, file diffs, and derived summaries.

  kind TEXT NOT NULL,
  -- codex.session, codex.rollout_event, codex.message,
  -- codex.tool_call, browser.window_state, browser.tab_event,
  -- file.version, file.diff, screen.chunk, screen.ocr_span,
  -- audio.chunk, audio.transcript_span, metric.sample,
  -- memory.summary, memory.claim

  role TEXT NOT NULL DEFAULT 'raw',
  -- raw, derived, summary, memory, annotation, synthetic

  privacy_class TEXT NOT NULL DEFAULT 'normal',
  -- public, normal, sensitive, secret, redacted

  modality TEXT NOT NULL DEFAULT 'mixed',
  -- text, json, image, video, audio, binary, scalar, mixed

  body_type TEXT NOT NULL DEFAULT 'json',
  -- none, text, json, number, boolean, blob, mixed

  text_value TEXT,
  number_value DOUBLE PRECISION,
  bool_value BOOLEAN,

  payload JSONB NOT NULL DEFAULT '{}'::jsonb,

  blob_id UUID,

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

  time_resolution_ns BIGINT,
  time_uncertainty_ns BIGINT,
  alignment_confidence REAL,
  alignment_method TEXT,

  importance_score REAL,

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
```

Create the hypertable:

```sql
SELECT create_hypertable(
  'perception.objects',
  by_range('event_start', INTERVAL '1 day'),
  if_not_exists => TRUE,
  create_default_indexes => FALSE
);
```

The universal row meaning does not change by data type:

```text
Codex session row     = temporal record in a Codex session series
Codex message row     = temporal record in the same Codex session series
Browser state row     = temporal record in a browser window series
File version row      = temporal record in a file path series
Screen chunk row      = temporal record in a screen display series
Audio transcript row  = temporal record in an audio input series
Metric sample row     = temporal record in a metric series
Memory summary row    = temporal record in a memory topic or summary series
```

### 3.3 `perception.lanes`

`perception.lanes` stores product tracks, not every session/window/thread/file
the viewer can render.

```sql
CREATE TABLE perception.lanes (
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
```

Lane keys should read like product ontology:

```text
agents.messages
browser.events
files.versions
messages.imessage
screen.main
system.metrics
```

They should not read like series IDs:

```text
agents.messages.codex.session-abc
browser.events.chrome.window-3
files.versions.path-hash-abc
```

### 3.4 `perception.sources`

A source is a producer or adapter identity. It owns source-record keys and
cursoring.

Examples:

```text
codex.local_sessions
claude.local_sessions
imessage.chat_db
desktop.semantic_observation
browser_extension.chrome.default_profile
file_watcher.local_repos
```

Sources can emit into many lanes and many series.

```text
source = codex.local_sessions
series = codex:session:019e...
lane   = agents.messages
record = codex.message
```

### 3.5 `perception.source_records`

`perception.source_records` is the idempotency table. It is not optional.

```sql
CREATE TABLE perception.source_records (
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
```

Stable ID rule:

```text
source_record_id = deterministic_uuid("source-record", source_id, source_record_key)
object_id        = deterministic_uuid("object",        source_id, source_record_key)
```

Source-record keys for Codex:

```text
codex:<session_id>:session
codex:<session_id>:line:<line_number>
codex:<session_id>:message:<message_id>
codex:<session_id>:tool:<tool_call_id>
codex:<session_id>:prompt:<turn_index>:<prompt_hash>
```

Same source key always means same record.

Hash conflict rule:

```text
If (source_id, source_record_key) is seen again with the same hash:
  return the existing object receipt and increment seen_count.

If (source_id, source_record_key) is seen again with a different hash:
  fail the write with SOURCE_IDENTITY_HASH_CONFLICT.
  increment conflict_count and record the competing hash in metadata.
```

Do not silently overwrite. A source identity collision is an adapter bug, not a
normal update path.

### 3.6 `perception.blobs`

`perception.blobs` stores descriptors for heavy bytes.

```sql
CREATE TABLE perception.blobs (
  blob_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  storage_backend TEXT NOT NULL,
  -- local, s3, r2, gcs, minio, test

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
  -- available, archived, deleted, missing

  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (user_id, uri)
);
```

Examples:

```text
video record      -> blob_id
audio record      -> blob_id
image record      -> blob_id
file record       -> blob_id
raw JSONL record  -> blob_id
```

Timeline queries should not need blob bytes.

### 3.7 `perception.object_edges`

Edges keep provenance and containment without changing row shape.

```sql
CREATE TABLE perception.object_edges (
  edge_id UUID PRIMARY KEY,

  user_id UUID NOT NULL,

  from_object_id UUID NOT NULL,
  from_object_event_start TIMESTAMPTZ NOT NULL,

  to_object_id UUID NOT NULL,
  to_object_event_start TIMESTAMPTZ NOT NULL,

  edge_kind TEXT NOT NULL,
  -- contains, derived_from, references, supersedes,
  -- annotates, caused_by, same_event_as, reply_to

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
```

For a Codex session:

```text
codex.session contains codex.message
codex.session contains codex.tool_call
codex.tool_call references file.diff
summary derived_from codex.session
memory references summary
```

The session is time-correlated as a record. The internal events are
time-correlated as records. Edges preserve structure.

The edge table stores object event-start timestamps because `perception.objects`
is keyed by `(event_start, object_id)` as a hypertable. That keeps edge
expansion from needing a source-record lookup on every hop.

### 3.8 `perception.timeline_projections`

`timeline_projections` stores named projection definitions. A projection may be
a saved user layout, an agent-generated investigation, or a rebuildable
materialized cache.

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

  UNIQUE (user_id, projection_key),

  CHECK (status IN ('draft', 'building', 'ready', 'stale', 'failed'))
);
```

### 3.9 `perception.timeline_projection_items`

`timeline_projection_items` lets the viewer explode layout lanes while the
canonical DB remains compact.

```sql
CREATE TABLE perception.timeline_projection_items (
  projection_id UUID NOT NULL,

  user_id UUID NOT NULL,

  object_id UUID NOT NULL,
  object_event_start TIMESTAMPTZ NOT NULL,

  series_id UUID NOT NULL,
  base_lane_id UUID NOT NULL,

  display_lane_key TEXT NOT NULL,
  -- examples:
  -- agents.session.codex.019e...
  -- browser.window.chrome.3
  -- files.repo.onecontext-memory-db
  -- imessage.thread.family

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

CREATE INDEX timeline_projection_items_time_idx
ON perception.timeline_projection_items
  (user_id, projection_id, event_start, display_lane_key);

CREATE INDEX timeline_projection_items_range_gist_idx
ON perception.timeline_projection_items
USING GIST (user_id, projection_id, event_range);
```

The range GiST index uses UUID equality columns, so the current schema creates
`btree_gist` before this index.

This table is allowed to be layout-specific because it is not truth. It is a
rendered cache over truth.

Projection policy must cap visible series. The default viewer should rank
candidate sublanes by recency, activity, importance, and explicit user pins,
then collapse overflow into a stable "Other" display lane. A browser profile
with hundreds of tabs or a repo with thousands of files should not produce
hundreds of visible lanes by default.

Projection rebuilds are driven by `definition_hash`, the source time bounds, and
invalidation timestamps. Mark a projection `stale` when records inside its
source bounds change, when the definition hash changes, or when its ranking
policy changes. Rebuild into `timeline_projection_items` only for saved,
expensive, or agent-authored layouts; cheap viewport splits can remain
on-demand queries.

## 4. Body Encoding

Every temporal record has the same body columns:

```text
body_type
text_value
number_value
bool_value
payload
blob_id
```

Body fields have simple invariants:

```text
body_type = none     -> body columns may all be empty
body_type = text     -> text_value is required
body_type = json     -> payload carries the body
body_type = number   -> number_value is required
body_type = boolean  -> bool_value is required
body_type = blob     -> blob_id is required
body_type = mixed    -> any meaningful combination is allowed
```

### Text Message

```text
body_type  = text
text_value = "I found the issue in the writer."
payload    = { role: "assistant", model: "gpt-5" }
```

### JSON Event

```text
body_type = json
payload   = { event_type: "tab_changed", title: "...", url_hash: "..." }
```

### Number Sample

```text
body_type    = number
number_value = 83.2
payload      = { unit: "ms", metric: "codex_latency" }
```

### Video

```text
body_type = blob
blob_id   = screen capture mp4
payload   = { width: 3024, height: 1964, fps: 10 }
```

### File

```text
body_type = blob
blob_id   = file content blob
payload   = { path_hash: "...", extension: "rs", language: "rust" }
```

### Mixed Object

```text
body_type  = mixed
text_value = summary text
payload    = structured metadata
blob_id    = optional raw evidence
```

This is flexible without turning source-specific fields into universal columns.

## 5. JSONB And Promoted Fields

Core timeline semantics should be columns:

```text
user_id
source_id
source_record_id
source_record_hash
series_id
lane_id
kind
role
privacy_class
modality
body_type
event_start
event_end
blob_id
text_value
number_value
bool_value
display_title
display_text
```

Source-specific details should stay in `payload JSONB` until they become hot
query primitives.

Agent payload example:

```json
{
  "agent_source": "codex",
  "session_id": "019e...",
  "turn_id": "turn-12",
  "model": "gpt-5",
  "tool_name": "exec_command",
  "exit_code": 0,
  "projections": ["model_visible", "ui_timeline"]
}
```

Browser payload example:

```json
{
  "browser": {
    "app": "Chrome",
    "profile": "Default",
    "window_id": "3",
    "tab_id": "128",
    "url_hash": "sha256:...",
    "title": "1Context docs"
  }
}
```

File payload example:

```json
{
  "file": {
    "path_hash": "sha256:...",
    "display_path": "crates/onecontext-memory-db/src/write_objects.rs",
    "extension": "rs",
    "language": "rust",
    "change_type": "modified"
  }
}
```

Promotion rule:

```text
Core timeline semantics = columns.
Source weirdness = JSONB.
Repeated query predicates = promoted columns or expression indexes.
```

## 6. Index Policy

Core viewport and identity indexes should prefer live rows and lightweight
viewport reads. If using a combined UUID plus range GiST index, enable
`btree_gist`; otherwise split equality and range indexes.

```sql
CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE INDEX objects_user_live_time_idx
ON perception.objects (user_id, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX objects_lane_live_time_idx
ON perception.objects (user_id, lane_id, event_start DESC)
INCLUDE (
  object_id,
  series_id,
  kind,
  role,
  privacy_class,
  event_end,
  display_title,
  display_text,
  body_type,
  blob_id,
  importance_score
)
WHERE valid_to IS NULL;

CREATE INDEX objects_series_live_time_idx
ON perception.objects (user_id, series_id, event_start DESC)
INCLUDE (object_id, kind, role, privacy_class, event_end, display_title, body_type)
WHERE valid_to IS NULL;

CREATE INDEX objects_lane_series_live_time_idx
ON perception.objects (user_id, lane_id, series_id, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX objects_kind_live_time_idx
ON perception.objects (user_id, kind, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX objects_source_live_time_idx
ON perception.objects (user_id, source_id, event_start DESC)
WHERE valid_to IS NULL;

CREATE INDEX objects_event_range_live_gist_idx
ON perception.objects
USING GIST (user_id, event_range)
WHERE valid_to IS NULL;

CREATE INDEX objects_payload_gin_idx
ON perception.objects
USING GIN (payload jsonb_path_ops);

CREATE INDEX objects_search_vector_live_idx
ON perception.objects
USING GIN (search_vector)
WHERE valid_to IS NULL;
```

Hot expression indexes are acceptable when a field is not yet worth promoting:

```sql
CREATE INDEX objects_payload_browser_app_idx
ON perception.objects ((payload->'browser'->>'app'))
WHERE payload ? 'browser' AND valid_to IS NULL;

CREATE INDEX objects_payload_file_language_idx
ON perception.objects ((payload->'file'->>'language'))
WHERE payload ? 'file' AND valid_to IS NULL;
```

Use `jsonb_path_ops` for containment-heavy JSONB queries. If a connector needs
operators that `jsonb_path_ops` does not support, add a narrow expression index
or a second GIN index for that adapter's hot field rather than making every
payload query pay the cost.

Avoid high-cardinality fields in default continuous aggregates:

```text
Do group default density by:
  user_id
  lane_id
  kind
  role
  privacy_class
  bucket

Do not group default density by:
  series_id
  source_id
  session_id
  window_id
  tab_id
  thread_id
  file_path
  url
```

Per-series density should be computed on demand or written into a projection
when the viewer needs it.

## 7. Density

Density powers zoomed-out timeline views. The default aggregate should group by
coarse dimensions only.

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

CREATE INDEX object_density_1m_lookup_idx
ON perception.object_density_1m
  (user_id, lane_id, bucket_start DESC, kind, role, privacy_class);

SELECT add_continuous_aggregate_policy(
  'perception.object_density_1m',
  start_offset => INTERVAL '7 days',
  end_offset => INTERVAL '1 minute',
  schedule_interval => INTERVAL '1 minute'
);
```

Do not group default density by `series_id`. A browser with many tabs, a repo
with many files, or an agent-heavy day would turn the aggregate into a
high-cardinality layout cache. That belongs in on-demand queries or projections.

## 8. Mutable Containers And Revisions

Some records are immutable source facts. Others are live container summaries
whose interval grows as more child records arrive.

Immutable source records:

```text
codex.rollout_event
codex.message
browser.navigation
file.version
screen.chunk
audio.chunk
metric.sample
```

Mutable or derived container records:

```text
codex.session
browser.window_state summary
terminal.session
meeting.summary
memory.summary
```

Rules:

```text
1. Raw source records are append-only. If the same source key appears with a
   different hash, use SOURCE_IDENTITY_HASH_CONFLICT.
2. Container records may grow while the container is active. Update
   event_end, display fields, payload counters, and series first/last/count in
   a controlled writer transaction.
3. Derived summaries and memories are revised by closing the old row with
   valid_to and inserting a new row linked with supersedes.
4. Maintain contains/derived_from edges as child records are added.
5. Preserve the raw evidence path, usually a blob-backed source record, so a
   mutable container can always be audited.
```

This gives live sessions/windows useful intervals without pretending that a
session summary and a raw JSONL line have the same mutability contract.

## 9. Canonical Row Examples

### 9.1 Codex Session Series

```text
series_kind: codex_session
series_key: codex:session:019e...
default_lane: agents.sessions
display_name: Codex session in onecontext-memory-db
modality: mixed
```

### 9.2 Codex Session Record

```text
series: codex:session:019e...
lane: agents.sessions
kind: codex.session
event_start: first rollout timestamp
event_end: last rollout timestamp
body_type: mixed
text_value: optional compact session summary
payload:
  agent_source: codex
  session_id: 019e...
  cwd: /Users/paulhan/dev/1context-public-launch
  model: gpt-5
  message_count: 42
  tool_call_count: 12
edges:
  codex.session derived_from codex.rollout_jsonl
```

### 9.3 Codex Raw JSONL Evidence Record

```text
series: codex:session:019e...
lane: agents.sessions
kind: codex.rollout_jsonl
event_start: first rollout timestamp
event_end: last rollout timestamp
body_type: blob
blob_id: raw rollout JSONL blob
payload:
  agent_source: codex
  session_id: 019e...
  line_count: 387
```

### 9.4 Codex Rollout Event Record

Every timestamped JSONL line should become a source-aligned record before any
higher-level message/tool projection is inferred.

```text
series: codex:session:019e...
lane: agents.sessions
kind: codex.rollout_event
event_start: line timestamp
event_end: line timestamp + 1 microsecond
body_type: json
source_sequence: line_number
payload:
  normalized_event: {...}
edges:
  codex.session contains codex.rollout_event
  codex.rollout_event derived_from codex.rollout_jsonl
```

### 9.5 Codex Message Record

```text
series: codex:session:019e...
lane: agents.messages
kind: codex.message
event_start: message timestamp
event_end: message timestamp + 1 microsecond, or message span end
body_type: text
text_value: assistant/user message text
payload:
  agent_source: codex
  session_id: 019e...
  turn_id: turn-12
  role: assistant
  projections: [model_visible, ui_timeline, memory_candidate]
edges:
  codex.session contains codex.message
  codex.message derived_from codex.rollout_event
```

### 9.6 Codex Tool Call Record

```text
series: codex:session:019e...
lane: agents.tools
kind: codex.tool_call
event_start: tool call start
event_end: tool call end
body_type: mixed
text_value: shell command or tool summary
payload:
  tool_name: exec_command
  exit_code: 0
  cwd_hash: sha256:...
  stdout_byte_count: 1200
  stderr_byte_count: 0
edges:
  codex.session contains codex.tool_call
  codex.tool_call derived_from codex.rollout_event
  codex.tool_call references file.diff
```

### 9.7 Browser Window, Tab, And App Series

Window series:

```text
series_kind: browser_window
series_key: browser:chrome:profile:Default:epoch:<source_epoch>:window:927:3
default_lane: browser.windows
display_name: Chrome window - 1Context docs
modality: mixed
```

The stable key should include browser app, profile, a source epoch or window
creation token when available, and the native window ID. Native IDs alone are
often reused.

Child tab series:

```text
series_kind: browser_tab
series_key: browser:chrome:profile:Default:epoch:<source_epoch>:tab:128
parent_series_id: browser window series while the tab is in that window
default_lane: browser.tabs
```

App-window series:

```text
series_kind: app_window
series_key: app:com.google.Chrome:pid:927:window:3
default_lane: ui.events
```

Records:

```text
kind: browser.window_state
body_type: json
payload: { app, profile, title, active_tab_id, tab_count }

kind: browser.tab_membership
event_start: tab joined window
event_end: tab left window
body_type: json
payload: { tab_id, window_id, membership_state }

kind: browser.tab_event
body_type: json
payload: { tab_id, url_hash, title, event_type }

kind: browser.navigation
body_type: json
payload: { tab_id, url_hash, title, referrer_hash }
```

Use `parent_series_id` for current stable hierarchy and records/edges for
temporal membership changes when tabs move between windows.

### 9.8 File Series And Records

Series:

```text
series_kind: file_path
series_key: file:path_hash:sha256...
default_lane: files.versions
display_name: crates/onecontext-memory-db/src/write_objects.rs
modality: mixed
```

Records:

```text
kind: file.version
event_start: modification timestamp
event_end: modification timestamp + 1 microsecond
body_type: blob
blob_id: file content blob
payload: { path_hash, content_hash, basename, extension, language, change_type }

kind: file.diff
body_type: text
text_value: compact diff summary
payload: { added, removed, changed_symbols }

kind: file.rename
body_type: json
payload: { old_path_hash, new_path_hash, display_old_path, display_new_path }

kind: file.delete
body_type: json
payload: { path_hash, last_content_hash }
```

Path history and content identity are related but not identical. A path series
tracks what happened at a normalized path. Use `content_hash` in payload and
`same_event_as` or `supersedes` edges when a move/rename connects histories.

### 9.9 Screen Series And Records

Series:

```text
series_kind: screen_display
series_key: screen:main
default_lane: screen.main
modality: video
```

Records:

```text
kind: screen.chunk
body_type: blob
blob_id: mp4 or image sequence
payload: { width, height, fps, active_app, window_title }

kind: screen.ocr_span
body_type: text
text_value: OCR text
payload: { bbox, confidence, app }
```

### 9.10 Audio Series And Records

Series:

```text
series_kind: audio_input
series_key: audio:mic:default
default_lane: audio.mic
modality: audio
```

Records:

```text
kind: audio.chunk
body_type: blob
blob_id: flac or wav chunk
payload: { sample_rate, channels, codec }

kind: audio.transcript_span
body_type: text
text_value: recognized phrase
payload: { speaker, confidence, language }
edges:
  audio.transcript_span derived_from audio.chunk
```

### 9.11 Scalar Metric Series And Records

Series:

```text
series_kind: metric
series_key: metric:codex:latency_ms
default_lane: system.metrics
modality: scalar
```

Record:

```text
kind: metric.sample
body_type: number
number_value: 123.4
payload: { unit: "ms", metric: "codex_latency" }
event_start: timestamp
event_end: timestamp + 1 microsecond
```

No special metric table is required for V0. If scalar volume becomes huge later,
we can optimize physical storage while preserving the logical protocol.

## 10. Query Shapes

### 10.1 Viewport Query

```sql
SELECT
  object_id,
  event_start,
  event_end,
  series_id,
  lane_id,
  kind,
  role,
  privacy_class,
  body_type,
  display_title,
  display_text,
  blob_id,
  importance_score
FROM perception.objects
WHERE user_id = $1
  AND event_start < $3
  AND event_end > $2
  AND event_range && tstzrange($2, $3, '[)')
  AND valid_to IS NULL
ORDER BY lane_id, event_start, object_id
LIMIT 5000;
```

### 10.2 Series Drilldown

```sql
SELECT
  object_id,
  event_start,
  event_end,
  lane_id,
  kind,
  role,
  privacy_class,
  body_type,
  text_value,
  payload,
  blob_id,
  display_title,
  display_text
FROM perception.objects
WHERE user_id = $1
  AND series_id = $2
  AND event_start < $4
  AND event_end > $3
  AND event_range && tstzrange($3, $4, '[)')
  AND valid_to IS NULL
ORDER BY event_start, object_id;
```

This answers:

```text
show this Codex session
show this browser window
show this file's history
show this screen stream
show this metric over time
```

### 10.3 Series List For A Lane

```sql
SELECT
  series_id,
  series_kind,
  series_key,
  display_name,
  first_event_start,
  last_event_end,
  record_count
FROM perception.series
WHERE user_id = $1
  AND default_lane_id = $2
ORDER BY COALESCE(last_event_end, updated_at) DESC;
```

### 10.4 Active Series For A Viewport

This is the replacement for a separate `lane_instances` table in the default
schema. It lets the viewer ask which series should become temporary sublanes in
a specific time window.

```sql
SELECT
  s.series_id,
  s.series_kind,
  s.series_key,
  s.display_name,
  min(o.event_start) AS first_seen_at,
  max(o.event_end) AS last_seen_at,
  count(*) AS object_count,
  max(o.importance_score) AS max_importance
FROM perception.objects o
JOIN perception.series s
  ON s.series_id = o.series_id
 AND s.user_id = o.user_id
WHERE o.user_id = $1
  AND o.lane_id = $2
  AND o.event_start < $4
  AND o.event_end > $3
  AND o.event_range && tstzrange($3, $4, '[)')
  AND o.valid_to IS NULL
GROUP BY s.series_id, s.series_kind, s.series_key, s.display_name
ORDER BY
  max(o.event_end) DESC,
  count(*) DESC,
  max(o.importance_score) DESC NULLS LAST
LIMIT $5;
```

Materialize this into `timeline_projection_items` only when the layout needs to
be saved or rebuilt quickly. The caller should pass a visible-series cap such
as `max_visible_series`; overflow is collapsed by projection policy.

### 10.5 Expand A Container Record

```sql
SELECT child.*
FROM perception.object_edges edge
JOIN perception.objects child
  ON child.object_id = edge.to_object_id
 AND child.event_start = edge.to_object_event_start
WHERE edge.user_id = $1
  AND edge.from_object_id = $2
  AND edge.from_object_event_start = $3
  AND edge.edge_kind = 'contains'
  AND child.valid_to IS NULL
ORDER BY child.event_start ASC;
```

### 10.6 Projection Items

```sql
SELECT
  display_lane_key,
  display_group_key,
  object_id,
  object_event_start,
  series_id,
  base_lane_id,
  event_start,
  event_end,
  rank,
  collapsed,
  metadata
FROM perception.timeline_projection_items
WHERE user_id = $1
  AND projection_id = $2
  AND event_start < $4
  AND event_end > $3
  AND event_range && tstzrange($3, $4, '[)')
ORDER BY display_lane_key ASC, event_start ASC, object_id ASC;
```

## 11. Ingestion Rules

Writers should follow these rules:

```text
1. Choose or create a coarse durable lane.
2. Choose or create a series for the logical stream/container.
3. Insert temporal records into that series.
4. Preserve native source IDs in payload.
5. Store large bytes in perception.blobs, not payload.
6. Use source_records for dedupe, not grouping.
7. Use edges for containment, provenance, references, and corrections.
8. Do not create physical lanes for ephemeral series.
9. Do not add SQL columns for source-specific IDs.
10. Promote a payload field only after it becomes a repeated product query.
```

Timestamp fallback policy:

```text
1. Use the native source timestamp or span when it exists.
2. If a child event lacks a timestamp, inherit the nearest enclosing source
   timestamp and use source_sequence/source_ordinal to preserve order.
3. If only file or log mtime exists, use mtime plus ordinal as a tiny ordered
   interval.
4. If no source time is available, use ingest_time as event_start, set
   alignment_method = ingest_time_fallback, set low alignment_confidence, and
   populate time_uncertainty_ns.
5. Never create a zero-length interval.
```

Agent-session ingestion should:

```text
1. Create or find series:
   series_kind = codex_session / claude_session
   series_key = <agent_source>:session:<session_id>
   default_lane_id = agents.sessions

2. Store raw JSONL as a blob when preserving source evidence.

3. Insert raw JSONL evidence record:
   kind = codex.rollout_jsonl / claude.rollout_jsonl
   body_type = blob
   blob_id = raw JSONL blob
   event_start = first source timestamp
   event_end = last source timestamp

4. Insert session record:
   kind = codex.session / claude.session
   event_start = first source timestamp
   event_end = last source timestamp
   edge = derived_from raw JSONL evidence

5. Insert per-line source records:
   kind = codex.rollout_event / claude.rollout_event
   series_id = same session series
   source_sequence = line number

6. Insert semantic child records:
   kind = codex.message / codex.tool_call / claude.message / claude.tool_call
   derive from or same_event_as the source rollout event

7. Add edges:
   session contains message
   session contains tool_call
   message/tool_call derived_from rollout_event
   tool_call references file_diff
```

Browser ingestion should:

```text
1. Create browser_window series with browser app, profile, source epoch, and
   native window ID in the series key.
2. Create browser_tab series with native tab ID and parent_series_id for the
   current window.
3. Insert tab_membership records when tabs enter, leave, or move windows.
4. Insert window_state, tab_event, and navigation records.
5. Keep URL and tab IDs in payload unless they become promoted query fields.
```

App-window ingestion should:

```text
1. Create app_window series when an OS/app window has a meaningful lifetime.
2. Use bundle ID, process ID, source epoch, and native window ID in the key
   when available.
3. Link browser windows, terminal windows, and document windows to app-window
   series with parent_series_id or object_edges depending on whether the
   relationship is stable or temporal.
```

File ingestion should:

```text
1. Create file_path series with a normalized path hash.
2. Insert file.version records for content snapshots.
3. Insert file.diff records for compact changes.
4. Insert file.rename, file.move, and file.delete records when path lifecycle
   changes are observed.
5. Store content_hash in payload so path identity and content identity can be
   joined without making path or content a column.
6. Use blobs for full file contents or large diffs.
```

Audio ingestion should:

```text
1. Create audio_input series for each microphone or input stream.
2. Insert audio.chunk records with blob_id.
3. Insert audio.transcript_span records with text_value.
4. Link transcripts to chunks with derived_from edges and source/media offsets.
```

Metric ingestion should:

```text
1. Create metric series.
2. Insert metric.sample records.
3. Use number_value for numeric samples.
4. Keep unit and metric metadata in payload.
```

## 12. Protocol Naming

The current public method name may remain:

```text
memory.writeObjects
```

The internal row type should be documented as:

```text
TemporalRecord
```

Compatibility path:

```text
method: memory.writeObjects
table: perception.objects
logical row type: TemporalRecord
canonical grouping: perception.series
```

`memory.writeObjects` should accept either an existing `series_id` or an
inline series descriptor. This avoids forcing adapters to pre-create series in
a separate round trip.

Request shape:

```json
{
  "objects": [
    {
      "source_id": "uuid",
      "source_record_key": "codex:abc:line:42",
      "source_record_hash": "sha256:...",
      "series": {
        "series_id": null,
        "series_kind": "codex_session",
        "series_key": "codex:session:abc",
        "display_name": "Codex session",
        "default_lane_key": "agents.sessions",
        "parent_series_key": null
      },
      "lane_key": "agents.messages",
      "kind": "codex.message",
      "event_start": "2026-05-25T18:00:00Z",
      "event_end": "2026-05-25T18:00:00.000001Z",
      "body_type": "text",
      "text_value": "..."
    }
  ]
}
```

Receipt shape:

```json
{
  "results": [
    {
      "object_id": "uuid",
      "object_event_start": "2026-05-25T18:00:00Z",
      "series_id": "uuid",
      "source_record_id": "uuid",
      "status": "inserted",
      "conflict": null
    }
  ]
}
```

Receipt statuses should distinguish `inserted`, `already_seen`, `superseded`,
and `conflict`. Hash conflicts are explicit failures, not quiet updates.

If we later want a clearer protocol, the natural alias is:

```text
memory.writeRecords
```

Do not rename protocol methods just to make the schema vocabulary prettier.

## 13. Current Schema Bootstrap

The current implementation already has much of this shape:

```text
perception.objects        = temporal rows
perception.lanes          = coarse presentation lanes
perception.blobs          = bytes
perception.source_records = dedupe
perception.object_edges   = meaning
```

The important schema change is to make series canonical:

```text
add perception.series
add perception.objects.series_id
add perception.source_records.series_id
add body_type / text_value / number_value / bool_value / modality
index perception.objects by (user_id, series_id, event_start)
update density to avoid source_id and series_id by default
```

The older `instance_kind`, `instance_id`, and `instance_display_name` fields
were the predecessor to `perception.series`. They are not part of the active V2
contract.

Current implementation checklist:

```text
1. Reset dev DBs that were created with transitional schemas.
2. Ship one current schema bootstrap that creates perception-only product
   tables for empty dev/test databases.
3. Create perception.series before perception.objects and
   perception.source_records require series_id.
4. Make objects.series_id and source_records.series_id required in the V2
   schema.
5. Update the writer to choose or create a series for every record before
   claiming perception.source_records.
6. Map adapter identities to series_kind/series_key:
   Codex/Claude sessions, browser windows/tabs, iMessage threads, file paths,
   screen displays, audio inputs, and scalar metrics.
7. Store objects.series_id and source_records.series_id in the same transaction
   as the source record claim.
8. Reset transitional dev DBs instead of backfilling prototype rows.
9. Validate that every object has a series and that objects.lane_id remains the
   concrete record lane while series.default_lane_id is only the home lane.
10. Update read APIs to expose series_id and support series-scoped ordered
   reads.
11. Keep viewport/density defaults lane-oriented; do not group density by
   series_id unless explicitly requested.
12. Keep the active protocol and current schema free of instance_* compatibility
   fields.
13. Pass write/read/idempotency contract tests and local benchmark gates before
   enabling the V2 bundle by default.
```

## 14. Acceptance Criteria

The schema implementation follows this document when:

```text
1. Every `perception.objects` row means one time-correlated record in one series.
2. Every record has [event_start, event_end).
3. Instant-like events use a tiny nonzero interval.
4. Codex session JSONL creates one session series, one blob-backed
   codex.rollout_jsonl evidence record, one session record, N per-line rollout
   records, and semantic message/tool records derived from those lines.
5. Querying by series_id returns the whole session/window/file/metric in order.
6. Screen, audio, file, JSONL, and image bytes live behind blob_id.
7. Scalar samples use number_value rather than a special metric table for V0.
8. Text search covers display_title, display_text, and text_value.
9. Default density aggregates do not group by series_id.
10. No source adapter requires a new SQL column.
11. No session, window, tab, file path, or thread becomes a durable physical
    lane by default.
12. The viewer can render series as virtual sublanes without mutating canonical
    storage.
13. Browser tabs/windows record temporal membership and use stable keys that
    account for app, profile, and source epoch when native IDs can be reused.
14. File rename, move, delete, version, and content-hash cases can be expressed
    without a new table or column.
15. Audio chunks and transcript spans share the same row model and are linked by
    derived_from edges.
16. Source-record hash conflicts are surfaced explicitly and never overwrite
    the existing row.
17. Mutable container records can grow or be superseded without mutating raw
    source records.
18. Viewport queries use chunk-prunable time bounds and lightweight column
    lists.
19. Saved projections have visible-series caps, overflow behavior, and rebuild
    invalidation metadata.
```

The desired shape is:

```text
coarse lane
+ logical series
+ universal temporal record
+ typed body columns
+ JSONB source detail
+ optional blob
+ source-record dedupe
+ edges for meaning
+ optional materialized projection
```
