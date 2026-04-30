# Ports

Ports are the system boundary.

A port can be input, output, or both. This is why the folder is not split into
hard `inputs/` and `outputs/`: real surfaces loop.

```text
Codex session log      output of Codex, input to 1Context
Claude session log     output of Claude, input to 1Context
screen capture         output of recorder, input to memory agents
wiki page              output when published, input when read later
talk entry             output of one agent, input to another
report/export          output for humans, input to later review
```

The wiki has a first-class root folder (`wiki/`) because it is the durable
human-readable memory workspace, not merely an import/export boundary. Wiki
ports observe that workspace: source pages, talk folders, generated renders,
and discovery files should be indexed into storage, but the product
organization and editable truth live under `wiki/`.

The port definition says how the daemon should observe a boundary surface. The
daemon should stay boring: scan ports, import rows, emit ticks, and let
state-machine definitions decide what those facts mean.

Current v0 port adapter definition shape:

```toml
id = "codex_sessions"
label = "Codex Sessions"
kind = "session_log"
adapter = "codex_rollout_jsonl"
enabled = true
directions = ["input"]
paths = ["~/.codex/sessions/**/rollout-*.jsonl"]
stores = ["storage.events", "storage.sessions", "storage.artifacts"]
```

The user-facing switchboard is the root `ports.toml`, modeled after
`accounts.toml`:

```toml
enabled = true
watch_interval_seconds = 300

[defaults]
enabled = true
since = "30d"
max_events_per_tick = 5000
max_lines_per_tick = 25000

[[ports]]
id = "codex_sessions"
enabled = true
paths = ["~/.codex/sessions/**/rollout-*.jsonl"]
```

The files in `ports/` are the reusable defaults. Root `ports.toml` is where a
local user or future menu-bar panel changes whether a port is live and what
paths it reads.

The default `since = "30d"` is a **source import horizon**, not a retention
policy. It keeps a fresh daemon from importing years of local Codex/Claude
history by accident, and on later ticks it only affects which native transcript
rows are eligible to be imported. It never prunes `storage/lakestore`; rows
already imported remain in storage when they age past the horizon. Use
`since = "all"` for a deliberate full backfill; if cursors have already
advanced, clear `storage/cursors/daemon.json` or reset the lake before rerunning
that history.

The tick caps are the other half of the safety model. Native Claude and Codex
session files can be tens of megabytes; the daemon must not treat one large
file as a single heroic transaction. Session ports therefore import bounded
file slices:

```toml
max_events_per_tick = 5000   # imported useful events; 0 means no cap
max_lines_per_tick = 25000   # raw JSONL rows scanned; 0 means no cap
```

After each changed file slice, the adapter batch-writes rows, advances the
byte-offset cursor, saves parser state (`session_id`, `cwd`), and saves
`storage/cursors/daemon.json`. If a tick stops because a cap was reached, the
daemon reports `limited`; the next tick resumes from the saved offset with the
same parser state. This matters for Codex rollouts, where `session_meta` can
appear before the slice being resumed. This makes first backfill ordinary
software: interruptible, repeatable, and proportional to the new work.

Keep adapters idempotent. Running `uv run 1context daemon once` ten times
should not duplicate imported rows. Session adapters also own cleanup: native
Codex and Claude rows are reshaped into the same human-readable contract proven
in the earlier prototype (`user`, `assistant`, `tool_use`, `tool_result`; Codex command
envelopes stripped; large tool output collapsed; transient Claude plumbing
tools ignored).

In the live system this is clocked by the daemon:

```bash
uv run 1context daemon watch
```

The default watch interval is five minutes. Each tick appends eligible new
normalized events from the native transcript sources, observes raw session-log
artifacts by content hash when useful rows were imported, materializes inline
session images as artifacts, and replaces the matching `sessions` summary row
so the lake keeps one current summary per native session. The `sessions` table
is the only session-import table that is replaced; `events` are append-only and
are never deleted because of `since`.

The code mirrors the earlier prototype split:

```text
src/onectx/ports/session_extract.py   pure native-log cleanup and shaping
src/onectx/ports/sessions.py          file cursors, lake writes, summaries
```

For a source-controlled smoke run, use the plugin's mock lived-experience:

```bash
uv run 1context daemon --experience-source daemon-smoke
```

For real local use, enable the relevant port and let the daemon read the native
Codex or Claude path directly.
