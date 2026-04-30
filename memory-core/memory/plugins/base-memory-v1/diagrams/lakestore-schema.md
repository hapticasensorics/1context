## What This Map Shows

This is the current 1context lakestore format and the first daemon import path.
The store is a local LanceDB/Lance directory at `storage/lakestore/`. It is the
queryable substrate underneath ports, daemon ticks, state machines, app viewers,
and later agent context.

## How to read it

Start on the left. Codex and Claude produce native JSONL session logs. The
real ports point at `~/.codex/sessions/**/rollout-*.jsonl` and
`~/.claude/projects/**/*.jsonl`, but those ports are disabled by default until
we intentionally enable local import. The committed `daemon-smoke`
lived-experience has tiny native-shaped session logs under `source-sessions/`
so the same adapter path can be tested without reading private local history.

The daemon does not interpret these sessions as agent cognition. It runs a
classical importer on a five-minute default clock: read the local `ports.toml`
switchboard, scan enabled port paths, read from the last cursor offset, apply
the earlier prototype session cleanup contract, write lake rows, materialize inline
session images into `storage/artifacts/session-images/`, update
`storage/cursors/daemon.json`, and emit one `daemon.tick`.

The cleanup contract matters. Claude and Codex logs do not label reality quite
the way a memory system wants to read it. The importer re-shapes native rows
into honest session kinds:

- `user` - the human's words.
- `assistant` - model prose.
- `tool_use` - the model's proposed action, formatted for quick reading.
- `tool_result` - shell, file, browser, or other environment output, with
  noisy envelopes stripped and huge outputs collapsed.

The center is the schema:

- `events` is the main fact stream. Individual Codex/Claude turns, tool uses,
  daemon ticks, app lifecycle events, future state-machine ticks, and job
  outcomes land here. For session imports it keeps the earlier prototype columns
  directly: `hash`, `session_id`, `kind`, `cwd`, and `char_count`.
- `sessions` summarizes a source session by `session_id`, `source`, cwd, first
  timestamp, last timestamp, and event count. The importer refreshes this row
  per session, mirroring earlier prototype's SQLite upsert behavior.
- `artifacts` records durable things by URI/path/hash. The importer writes one
  artifact row for each observed raw JSONL source file.
- `evidence` records validation proof. State machines should advance from
  evidence, not from an agent saying something is done.
- `documents` is the readable-text surface for later context views,
  summarization, chunking, and embeddings.

## Why It Matters

This makes Codex/Claude import a plain software loop instead of a special
agent feature. Once we enable the real ports, the pipeline should be:

```text
native session file changes
daemon imports new rows
lakestore records facts
swim-lane viewer shows context
state machines can react to events and evidence
```

The schema is intentionally small. It is not trying to be the final warehouse.
It is the stable first place where live observations, durable artifacts, and
validation evidence can meet.
