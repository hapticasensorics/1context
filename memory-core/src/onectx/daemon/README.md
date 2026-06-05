# Daemon

The daemon is the local pulse.

It should stay boring:

```text
scan ports
advance cursors
write storage rows
emit daemon.tick
start/stop supervised apps
```

It should not know prompt internals, agent psychology, or memory-system policy.
State-machine definitions decide what observations mean. The daemon only keeps
local loops alive and inspectable.

Classical software loops belong here:

```text
Codex/Claude session import
screen capture import
viewer lifecycle
render/export refresh
health checks
```

Agent work is different. It should appear to the daemon as a command/process
plus expected artifacts and evidence, not as a special ontology baked into the
daemon.

The live daemon clock is intentionally slow by default. Each tick scans enabled
ports, imports only new
native session rows from the last cursor offset, refreshes the per-session
summary rows, and emits a `daemon.tick`. The broad developer daemon CLI was
removed; future app integration should call a narrow contract surface rather
than revive ad hoc shell verbs.

The clock and source import horizon are local policy in root `ports.toml`.
By default, session ports are enabled and `since = "30d"` prevents accidental
full-history backfills. This is only an import filter for native transcripts;
it is not a retention policy. Imported Perception DB rows do not age out when they
cross day 31. The explicit `--experience-source` path is for smoke and
lived-experience replay, so it bypasses the live-port import horizon.

Session import is intentionally chunked. Native Claude/Codex transcripts remain
read-only; the daemon reads from their last saved byte offset, writes cleaned
agent-message rows into Perception DB in batches, saves the cursor after each
durable slice, persists parser state (`session_id`, `cwd`) for resume, and
reports `limited` when a tick hit its configured caps. A large first backfill is
therefore many small daemon ticks, not one blocking transaction.

Swift is the native shell for macOS permissions, status, local web, and update
behavior. Python remains the thin portable coordinator for ports, state machines,
and memory planning.
