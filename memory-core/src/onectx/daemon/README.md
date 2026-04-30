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

The live daemon clock is intentionally slow by default:

```bash
uv run 1context daemon watch
```

That ticks every five minutes. Each tick scans enabled ports, imports only new
native session rows from the last cursor offset, refreshes the per-session
summary rows, and emits a `daemon.tick`. For smoke runs and development,
override the clock explicitly:

```bash
uv run 1context daemon watch --experience-source daemon-smoke --interval 2 --ticks 2
```

For deliberate catch-up work, use `backfill`. It runs the same bounded tick
primitive in a tight loop until no enabled port reports `limited`:

```bash
uv run 1context daemon backfill
uv run 1context daemon backfill --max-ticks 10
```

`backfill` is not a separate importer. It is just `daemon once` repeated without
the watch sleep, so cursor, cleanup, idempotency, and evidence behavior stay
identical.

The clock and source import horizon are local policy in root `ports.toml`.
By default, session ports are enabled and `since = "30d"` prevents accidental
full-history backfills. This is only an import filter for native transcripts;
it is not a retention policy. Imported lakestore rows do not age out when they
cross day 31. The explicit `--experience-source` path is for smoke and
lived-experience replay, so it bypasses the live-port import horizon.

Session import is intentionally chunked. Native Claude/Codex transcripts remain
read-only; the daemon reads from their last saved byte offset, writes cleaned
earlier prototype-style rows into the lakestore in batches, saves the cursor after each
durable slice, persists parser state (`session_id`, `cwd`) for resume, and
reports `limited` when a tick hit its configured caps. A large first backfill is
therefore many small daemon ticks, not one blocking transaction.

The future Swift menu-bar app should wrap this daemon instead of replacing it.
Swift is the native shell for macOS permissions and status. Python remains the
portable core for storage, ports, state machines, and app supervision.
