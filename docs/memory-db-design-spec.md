---
title: 1Context Memory DB Design Spec
slug: memory-db-design-spec
section: architecture
access: private
summary: "Rust-backed Postgres and TimescaleDB memory database contract for the 1Context temporal object store."
status: draft
last_updated: 2026-05-21
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Memory DB Design Spec

## 0. Thesis

1Context should be a temporal object store built on Postgres plus TimescaleDB.

Every lane of data becomes one kind of perception object:

```ts
PerceptionObject =
  time range
  + user
  + source
  + lane
  + kind
  + payload metadata
  + optional blob pointer
  + provenance edges
```

This gives the system one time spine for screen captures, audio chunks, UI
events, browser events, Codex logs, terminal sessions, file versions, agent
messages, OCR spans, transcript spans, summaries, and memories.

The database should feel like one NLE timeline, not a source-specific table
zoo. Timescale hypertables fit because they behave like normal Postgres tables
while splitting time-series data into internal chunks.

Internal slogan:

```text
Everything is a time-bounded object.
Everything has a lane.
Everything can point to bytes.
Everything can point to other objects.
```

## 1. System Requirements

The DB must support:

```text
1. Append observed data from heterogeneous sources.
2. Align all data on one shared timeline.
3. Query a viewport: show all lanes from t0 to t1.
4. Query by lane: screen, audio, Codex, files, UI, browser, memory.
5. Query by kind: transcript_span, agent_tool_summary, file_version.
6. Store arbitrary structured payloads.
7. Point at large media/files without storing bytes in Postgres rows.
8. Support derived artifacts: OCR, transcripts, summaries, embeddings.
9. Preserve event time versus record/ingest time.
10. Allow corrections without corrupting historical causality.
11. Support zoomed-out timeline summaries.
```

V0 non-goals:

```text
distributed Postgres
Kafka lakehouse pipeline
separate ClickHouse timeline engine
full data lake table format
one table per source type
sample-level audio/sensor rows
```

The first ship target is one strong keel: a single timeline hypertable plus
small support tables.

## 2. Chosen Stack

Target database:

```text
PostgreSQL 17 or 18
TimescaleDB
pgvector
pg_trgm
btree_gist
pgcrypto
pg_stat_statements
optional: pg_jsonschema, postgis, pg_cron
```

Use PostgreSQL 18 when the selected TimescaleDB distribution supports it.
PostgreSQL 18 has native UUIDv7 generation, which is useful for insert locality.
Until that support is assumed everywhere, the v0 migrations use
`gen_random_uuid()` from `pgcrypto`.

## 3. Core Design

The central table is:

```text
perception.objects
```

It contains every captured or derived thing that appears on the timeline:

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
terminal_command
terminal_output
file_version
ocr_span
transcript_span
summary
memory
```

The "one DB" idea does not mean one physical table for everything. It means one
timeline object model. Support tables keep the time spine clean:

```text
app.users
perception.sources
perception.lanes
perception.blobs
perception.source_records
perception.object_edges
search.object_embeddings
```

The older `capture.*` prototype migrations remain in the repo for existing DB
compatibility. New product reads, writes, and docs should use the
`perception.*` schema unless they are explicitly describing legacy migration
support.

## 4. Extension Setup

The migration set begins with:

```sql
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
```

Optional extensions are deferred until product lanes need them:

```sql
CREATE EXTENSION IF NOT EXISTS pg_jsonschema;
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS pg_cron;
```

Use `pg_jsonschema` only for DB-level validation of JSON payloads. Use
`postgis` only once location becomes a real first-class lane.

## 5. Core Tables

### 5.1 Users

`app.users` is included for a standalone v0. If another app DB already owns user
identity, this table can become an adapter boundary.

### 5.2 Lanes

A lane is a UI/rendering concept. It answers where an object appears in the
timeline.

Example lanes:

```text
screen.main
audio.mic
ui.events
browser.tabs
agents.sessions
agents.messages
agents.tools
files.versions
transcript.spans
ocr.spans
memory.summaries
```

### 5.2.1 Lane Cardinality Rule

Do not create a persisted lane for every channel, thread, tab, repo, or chat.
That would turn the viewer into hundreds of tracks and make the database harder
to reason about.

Persisted lanes should be coarse, stable product tracks:

```text
slack.messages
discord.messages
messages.imessage
browser.pages
agents.messages
terminal.output
files.versions
```

High-cardinality detail belongs elsewhere:

```text
Slack channel          payload.channel_id / source metadata
Discord guild/channel  payload.guild_id / payload.channel_id
iMessage thread        payload.chat_guid / object edges
browser tab            source instance / payload.tab_id
repo/file path         payload.path / file_version object
agent session          payload.session_id / source instance
```

The viewer may create virtual lanes from filters, such as "Slack #launch" or
"Codex sessions in this repo", but those are UI groupings over the same coarse
persisted lanes. The default persisted lane set should stay small enough that a
human can scan it without setup work.

### 5.3 Sources

A source is an adapter or observed source instance. It answers where the data
came from and owns cursor/source-record identity.

Example source types:

```text
screen
audio
ui
browser
codex
terminal
file_watcher
agent
phone
gps
```

A source can emit objects into more than one lane. For example, a browser
extension source may emit both `browser_event` and `ui_event` objects.

### 5.4 Source Groups

A source group or edge set groups things observed in the same ingestion window.
This preserves "these objects arrived together" without pretending they are the
same object.

Example:

```text
bundle: 10:03:20 to 10:03:30
  screen chunk
  audio chunk
  UI events
  browser tab updates
  coding-agent tool summary events
```

This is the time-correlated primitive.

### 5.5 Object Blobs

Large bytes live outside Postgres. Postgres stores the pointer and metadata.

Rule:

```text
small event metadata: inline JSONB
large video/audio/file/log: perception.blobs row + blob_id pointer
```

Blob state:

```text
available
archived
deleted
missing
```

When a blob is deleted but the timeline record remains, `perception.objects`
still says what happened and `perception.blobs` records that the heavy payload
is gone.

## 6. Main Hypertable

### 6.1 Object Registry

Timescale hypertables require any unique index on the hypertable to include all
partitioning columns. Since `perception.objects` is partitioned by
`event_start`, a clean global `UNIQUE(object_id)` does not belong on the
hypertable itself.

`perception.source_records` is the non-hypertable source-identity table that
gives the app global source dedupe and stable object lookup while Timescale gets
clean partitioning.

### 6.2 Perception Objects

`perception.objects` is the product hypertable. It carries:

```text
event_start / event_end   shared corrected timeline range
object_id                 object identity linked from source_records
user_id                   owner
source_id                 source adapter or observed source
lane_id                   timeline lane
kind                      object type
role                      raw, derived, synthetic, annotation, memory
source_record_key         stable external record key
blob_id                   optional perception.blobs pointer
payload                   structured source metadata
display_title/text        human-readable timeline/search text
event_range               generated range for overlap queries
source_*                  source clock metadata
ingest_time               when 1Context learned it
valid_from/valid_to       current-version window
privacy_class             normal, sensitive, secret, redacted
```

Instants must be expanded by ingestion code to a tiny nonzero duration, such as
one microsecond, before insertion. Empty `[start,end)` ranges do not behave well
as timeline objects.

## 7. Indexes

Viewport indexes:

```text
objects_user_time_idx
objects_lane_time_idx
objects_source_time_idx
objects_kind_time_idx
```

Interval overlap index:

```text
objects_event_range_gist_idx
```

JSONB indexes:

```text
objects_payload_gin_idx
objects_payload_app_idx
objects_payload_session_idx
```

Text indexes:

```text
objects_search_vector_idx
objects_display_text_trgm_idx
```

Hot JSON fields should get expression indexes instead of forcing every access
through one giant JSONB index.

## 8. Relationship Table

Relationships are edges, not arrays inside perception objects.

Initial edge kinds:

```text
contains
derived_from
supersedes
contradicts
references
same_event_as
caused_by
annotates
```

Examples:

```text
ocr_span derived_from screen_chunk
transcript_span derived_from audio_chunk
agent_tool_summary references terminal_command
agent_compaction derived_from agent_message
agent_prompt_snapshot references model-visible agent_message objects
memory references many non-contiguous source objects
new_summary supersedes old_summary
```

This is where nonlinear editing becomes real. A memory can point to scattered
clips without being forced into one contiguous time span.

## 9. Embeddings

Embeddings live in `search.object_embeddings`, not inside the main timeline
table.

V0 uses:

```text
embedding vector(1536)
HNSW cosine index
UNIQUE (object_id, embedding_model)
```

For larger embedding dimensions, choose `halfvec`, dimensionality reduction, or
a dedicated vector store after measurement.

## 10. Continuous Aggregates

The raw timeline can be dense. Zoomed-out NLE views use Timescale continuous
aggregates:

```text
perception.object_density_1m
```

The UI chooses:

```text
seconds view: perception.objects
minutes view: perception.object_density_1m plus selected raw expansion
hours view: perception.object_density_1m with coarser query windows
days view: summaries/memories as perception.objects
```

Important v0 rule: do not enable row-level security on `perception.objects` while
continuous aggregates are part of the design. Enforce user isolation in the
Rust service/application layer, DB roles, or physical deployment boundaries
until the tradeoff is tested.

## 11. Hypercore / Columnstore Lifecycle

Fresh data stays in rowstore. Cold data converts to columnstore.

V0 policy:

```text
hot rowstore: 0 to 7 days
columnstore: older than 7 days
segmentby: user_id, lane_id
orderby: event_start DESC
```

For a single-user local product, `segmentby` may become `lane_id, kind` after
measurement.

## 12. Ingestion Contract

Every source adapter emits a Perception DB object input:

```ts
type PerceptionObjectInput = {
  user_id: string
  source_id: string
  source_record_key: string
  source_record_hash?: string
  lane_id: string
  kind: string

  event_start: string
  event_end: string

  payload?: object
  blob?: {
    uri: string
    content_type: string
    sha256?: string
    byte_count?: number
  }

  source_clock_id?: string
  source_start_ns?: number
  source_end_ns?: number
  source_sequence?: number

  display_title?: string
  display_text?: string

  schema_name?: string
  schema_version?: number
  confidence?: number
  privacy_class?: string
}
```

Rules:

```text
1. No object enters without event_start and event_end.
2. Instants use a tiny nonzero duration.
3. event_start/event_end are normalized UTC event time.
4. source_start_ns/source_end_ns preserve device/source clock time.
5. Large blobs are written first, then the DB row references the blob.
6. Raw evidence rows are append-only.
7. Derived rows are append-only.
8. Corrections use valid_to + supersedes edges.
```

Insert sequence:

```text
1. Write blob to object storage, if needed.
2. Insert perception.blobs row.
3. Claim perception.source_records identity.
4. Insert perception.objects row.
5. Insert perception.object_edges rows, if derived/related.
6. Insert embedding row asynchronously, if text/semantic object.
```

The Rust crate `onecontext-memory-db` owns this object input as compile-time
contract surface before live source adapters exist.

## 13. Timeline Query API

Raw viewport:

```sql
SELECT *
FROM perception.get_timeline_viewport(
  $user_id,
  $window_start,
  $window_end,
  $lane_ids,
  5000
);
```

Zoomed viewport:

```sql
SELECT *
FROM perception.object_density_1m
WHERE user_id = $1
  AND bucket_start >= $2
  AND bucket_start < $3
  AND lane_id = ANY($4)
ORDER BY lane_id, bucket_start;
```

## 14. Clock Synchronization

V0 stores normalized UTC event time and preserves source clock metadata:

```text
event_start/event_end = corrected shared timeline
source_start_ns/source_end_ns = original source clock
```

Add `perception.source_clocks` and `perception.clock_sync_samples` once multiple
devices matter. This keeps alignment debuggable when a browser extension,
phone, or remote service drifts.

## 15. Mutability And Corrections

Raw capture data is immutable except for privacy deletion/redaction.

Use bitemporal-ish fields:

```text
event_start/event_end: when the thing happened
ingest_time: when 1Context learned it
valid_from/valid_to: whether this row is the current version
```

Correction flow:

```text
1. Set valid_to on the old object row.
2. Insert the corrected object as a new object.
3. Add object_edges row: new supersedes old.
```

This prevents quiet history rewrites. The system can say: the summary was
corrected later.

## 16. Data Kind Conventions

Use `kind` as text, not an enum, because new capture types will appear
constantly.

Suggested initial kinds:

```text
screen_chunk
screen_frame
audio_chunk
ui_event
browser_event
browser_page
terminal_command
terminal_output
agent_session
agent_turn
agent_message
agent_tool_summary
agent_compaction
agent_prompt_snapshot
file_version
file_diff
ocr_span
transcript_span
memory
manual_note
calendar_event
location_point
```

Suggested payloads:

```json
{
  "screen_chunk": {
    "display": "main",
    "width": 3024,
    "height": 1964,
    "fps": 1,
    "active_app": "Chrome",
    "window_title": "OpenAI Docs"
  },
  "agent_message": {
    "agent_source": "codex",
    "session_id": "abc123",
    "role": "assistant",
    "model": "gpt-5",
    "text": "..."
  },
  "file_version": {
    "path": "/repo/src/app.ts",
    "language": "typescript",
    "sha256": "...",
    "change_type": "modified"
  },
  "ui_event": {
    "app": "Chrome",
    "event_type": "click",
    "x": 1212,
    "y": 844,
    "target_hint": "Submit"
  }
}
```

## 17. Partitioning Policy

Start with:

```text
hypertable: perception.objects
partition column: event_start
chunk interval: 1 day
space partition: none
```

Legacy note: `capture.captured_objects` was the prototype hypertable. The
product hypertable for new work is `perception.objects`; keep legacy migrations
only for existing database compatibility.

Do not immediately hash partition by `user_id`. For early local/single-user v0,
one time partitioning dimension is simpler and easier to reason about.

## 18. Retention And Storage

Do not delete timeline metadata early. Metadata is the map.

Blob retention can vary by kind:

```text
screen video blobs: maybe 30 to 180 days
audio blobs: maybe 30 to 180 days
raw files: content-addressed, longer
OCR/transcripts/summaries: long-term
timeline metadata: long-term
embeddings: rebuildable, medium-term
```

## 19. Operational Configuration

For self-hosted Postgres:

```conf
shared_preload_libraries = 'timescaledb,pg_stat_statements'

wal_compression = on
checkpoint_timeout = '15min'
max_wal_size = '8GB'

pg_stat_statements.track = all
pg_stat_statements.max = 10000

timescaledb.max_background_workers = 16
```

For pgvector, build large HNSW indexes after bulk loading when possible. In
production, build vector indexes concurrently when avoiding write blocking
matters.

## 20. Minimal Migration Order

The Rust crate carries versioned SQL migrations:

```text
001_extensions.sql
002_schemas.sql
003_app_users.sql
004_capture_support_tables.sql through 012_source_records.sql
  legacy prototype compatibility
013_perception_support_tables.sql
014_perception_objects.sql
015_perception_indexes.sql
016_perception_edges.sql
017_perception_density.sql
018_perception_embeddings.sql
019_perception_source_cursors.sql
020_reconcile_perception_embeddings.sql
```

## 21. V0 Acceptance Tests

### 21.1 Insert One Bundle

Insert:

```text
1 screen chunk
1 audio chunk
10 UI events
1 agent message
1 file version
```

All share a source/bundle identity in payload or edge metadata.

Expected:

```text
query by bundle returns all objects
query by viewport returns all objects ordered by lane/time
query by lane returns only that lane
```

### 21.2 Interval Overlap

Query:

```sql
SELECT *
FROM perception.get_timeline_viewport(
  $user_id,
  '2026-05-21 10:03:25Z',
  '2026-05-21 10:03:35Z',
  NULL,
  5000
);
```

Expected:

```text
objects overlapping the window appear
objects ending before the window do not
objects starting after the window do not
```

### 21.3 Large Blob

Insert a screen capture with `blob_id`.

Expected:

```text
DB row is small
blob metadata exists
UI can lazy-load blob by URI
timeline query does not need blob bytes
```

### 21.4 Derived Artifact

Insert OCR span derived from screen chunk.

Expected:

```text
OCR row appears on OCR lane
object_edges has OCR derived_from screen_chunk
semantic/text search can find OCR text
```

### 21.5 Correction

Insert a summary, then corrected summary.

Expected:

```text
old row has valid_to set
new row is current
object_edges has new supersedes old
normal viewport shows only current unless asked for history
```

## 22. References

- [Timescale hypertables](https://www.tigerdata.com/docs/use-timescale/latest/hypertables/)
- [Timescale CREATE TABLE hypertable syntax](https://www.tigerdata.com/docs/api/latest/hypertable/create_table)
- [PostgreSQL UUID functions](https://www.postgresql.org/docs/current/functions-uuid.html)
- [PostgreSQL JSON types](https://www.postgresql.org/docs/current/datatype-json.html)
- [PostgreSQL range types](https://www.postgresql.org/docs/current/rangetypes.html)
- [pgvector](https://github.com/pgvector/pgvector)
- [Timescale continuous aggregates](https://www.tigerdata.com/docs/use-timescale/latest/continuous-aggregates/)
- [Timescale continuous aggregate creation](https://www.tigerdata.com/docs/use-timescale/latest/continuous-aggregates/create-a-continuous-aggregate)
- [Timescale Hypercore](https://www.tigerdata.com/docs/use-timescale/latest/hypercore/real-time-analytics-in-hypercore)
