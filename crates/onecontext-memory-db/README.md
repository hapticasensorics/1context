# onecontext-memory-db

Rust contract crate for the 1Context temporal object store.

This crate intentionally starts below the live capture sources. It defines the
shared `CaptureEnvelope` and carries the current Postgres/Timescale schema so
screen capture, Codex/Claude logs, terminal output, files, browser events,
iMessages, and future agent outputs can wire into one timeline model as they
come online.

The design spec lives at:

```text
docs/memory-db-design-spec.md
```

V0 crate surface:

```rust
use onecontext_memory_db::{current_schema_sql, CaptureEnvelope, probe_local_sources};
```

The crate also ships the first local source adapter CLI. It does not require a
running database; it probes local sources, samples source shape, and can run a
bounded cursorized ingest tick that emits validated capture envelopes for the
future TimescaleDB writer.

```bash
cargo run -p onecontext-memory-db --bin onecontext-memory-db -- probe
cargo run -p onecontext-memory-db --bin onecontext-memory-db -- sample --source codex --limit 5
cargo run -p onecontext-memory-db --bin onecontext-memory-db -- sample --source claude --limit 5
cargo run -p onecontext-memory-db --bin onecontext-memory-db -- sample --source imessage --limit 5
cargo run -p onecontext-memory-db --bin onecontext-memory-db -- ingest --source codex --cursor-file /tmp/onecontext-codex-cursors.json --max-events 1000
cargo run -p onecontext-memory-db --bin onecontext-memoryd -- bench --sources codex,claude,imessage --max-events 1000
```

iMessage sample output redacts message text by default. Pass
`--include-sensitive-text` only when intentionally inspecting local sensitive
content.

Codex and Claude session ingest defaults to `--profile hot_memory`, matching the
agent-session profile in `docs/coding-agent-ingest-spec.md`: user/assistant
messages stay on the hot path, while full tool calls/results are intentionally
skipped. Use `--profile compact_audit` for compact tool summaries in
viewer/debug paths, and reserve `--profile forensic` for explicit investigation
flows as the compiler grows raw evidence/blob preservation.

`onecontext-memoryd` is the daemon binary. In the macOS app it is supervised by
Swift `1contextd`; standalone commands are available for local benchmarking and
one-shot daemon ticks before Timescale insertion is wired in.
