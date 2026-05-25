# Storage

This package owns the local 1Context lakestore.

The storage direction is LanceDB/Lance, not a pile of one-off SQLite files.
Earlier prototype proved the value of a queryable event archive (`sessions.db` plus a
small query tool). 1Context generalizes that idea: the same store should hold
raw session events, state-machine events, artifacts, evidence, documents, and
later embeddings.

The v0 store is intentionally plain:

```text
storage/lakestore/
  events      append-only facts and state-machine ticks
  sessions    session summaries, replacing the old sessions table
  artifacts   durable things the system can point at
  evidence    proof that an artifact is valid enough to advance state
  documents   readable text surfaces and future embedding inputs
```

This is a lake-style store, not a relational domain model. We keep the first
schema shallow and JSON-backed where the shape is still moving. LanceDB gives
us local embedded tables now and leaves room for vectors, full-text, filters,
and larger Lance datasets later.

## Rule

The state machine should advance from evidence in storage, not from hidden
chat state.

```text
agent output -> artifact row/file -> evidence row -> state transition
```

Files can still exist when they are the best human-readable artifact. The
lakestore records where they are, what they mean, and which evidence allowed
the system to trust them.

## Remembering Input Contract

Screen, accessibility, input-monitoring, browser-extension, microphone,
Automation, and Full Disk Access source data belongs in the lakestore once the
macOS app or source-specific setup flow has granted the required permission.
Capture code should not invent a parallel memory database.

Use the tables this way:

```text
events      capture ticks, active app/window facts, OCR text, input summaries
artifacts   durable frame files, OCR sidecars, and materialized observation windows
evidence    checks proving a capture artifact is readable, scoped, and usable
documents   readable text packets that memory jobs can cite or summarize
```

Raw files may live beside runtime support state when they are too large or too
binary for table rows. Their stable paths, hashes, and provenance still belong
in `artifacts`, and state-machine progress should still flow through `evidence`.

## Runtime Experience Projections

Rendered lived experience is first-class in storage, but it is not the source
of truth.

```text
events table
  raw session facts from Codex, Claude Code, and future sources
  includes tool calls and tool results

artifacts table
  materialized runtime experience packets
  records mode, path, content hash, byte size, source window, and projection policy
  uses a stable artifact id per experience projection, so rerenders update the row
  can be reused when mode/window/source-event hash match

runtime/experiences/
  human-readable files loaded into hired agents at birth
```

So `braided_lived_messages` is a materialized projection artifact, not a
destructive rewrite and not a separate memory store. It means:

```text
raw Lance events remain complete
agent-facing packet includes all user and assistant messages
tool_use/tool_result events stay in Lance for retry/audit/forensic jobs
```

The scribe jobs use `braided_lived_messages` by default because it preserves
the conversational lived context at much lower context cost. Jobs that need
exact command output or logs can request `braided_lived_transcript`.

## Current API

Use `LakeStore` directly from Python:

```python
from onectx.storage import LakeStore

store = LakeStore(system.storage_dir)
store.ensure()
store.append_event("state_machine.tick", state_machine="for_you_day")
store.append_artifact("talk_entry_file", path="memory/runtime/...")
store.append_evidence("talk_entry.valid", artifact_id="artifact_...")
```

The app-facing storage command only initializes the local tables:

```bash
uv run --project memory-core 1context-memory-core storage init --json
```
