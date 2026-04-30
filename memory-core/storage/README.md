# Storage

This folder is the first-class storage substrate for 1Context.

The memory plugin says what the system should do. Runtime records say what
happened. Storage is the durable local place where records can be queried,
indexed, embedded, compacted, and inspected.

1Context uses a LanceDB/Lance lakestore:

```text
storage/
  lakestore/       generated LanceDB tables, ignored by git
  artifacts/       generated files materialized from session/event payloads
```

Current table family:

```text
events      normalized facts, ticks, job outcomes, imported source events
sessions    summaries for Codex, Claude, screen, manual, or other sessions
artifacts   durable outputs such as talk entries, rendered pages, transcripts
evidence    validation rows that allow state machines to advance
documents   readable text surfaces and future embedding/chunk inputs
```

For the wiki, storage is an index and evidence substrate, not the only source of
truth. Live wiki source and talk folders live under `wiki/`; the lakestore keeps
searchable `documents`, durable `artifacts`, render/import `events`, and
validation `evidence` derived from those files. This preserves human-editable
files while still making the wiki queryable by state machines, agents, and
viewers.

For Codex and Claude imports, `events` intentionally keeps the earlier prototype session
columns as first-class fields: `hash`, `session_id`, `kind`, `cwd`, and
`char_count`. The broader event fields (`event`, `actor`, `subject`,
`state_machine`, `artifact_id`, `payload_json`) remain for daemon ticks, app
events, state-machine runs, and future non-session observations.

`sessions` is a summary table, not the source of truth. The daemon refreshes a
single row per `session_id` from imported `events` on every session-import tick.
That mirrors the earlier prototype SQLite upsert behavior while keeping append-friendly
event history in the lake.

This is not "memory" in the agent-experience sense. It is storage: the stable
local lake that memory systems, state machines, viewers, importers, and
publishers can all use.

Use:

```bash
uv run 1context storage init
uv run 1context storage smoke
uv run 1context storage search "daemon" --table events
uv run 1context storage export --output apps/swim-lane-context-viewer/public/context.json
```

The daemon also keeps small local cursor files under `storage/cursors/`. Those
are process bookkeeping, not shareable memory-system definitions.
