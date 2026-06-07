# 1Context Memory Core

This directory contains the remaining Python memory helpers used behind the
public macOS shell.

The public Swift app owns installation, lifecycle, diagnostics, permissions,
hooks, update behavior, wiki rendering, and wiki publication. Rust 1Context
components own the durable infrastructure: Perception DB via
`onecontext-memoryd`, wiki page lifecycle via `onecontext-wiki-core`, and agent
provenance via the agent harness.

The Python memory core no longer owns the wiki-company update path. The app
calls `onecontext-context-engine update-wiki`; Python remains for narrow
developer-facing memory helpers such as ticks, cycles, and replay dry runs.

The boundary is a subprocess contract. For local development:

```bash
uv run --project memory-core 1context-memory-core status --json
```

The executable wrapper used by the Swift adapter is:

```bash
memory-core/bin/1context-memory-core
```

It intentionally exposes only the narrow public command shapes needed by the
app-facing subprocess contract:

```text
status --json
storage init --json
memory tick --wiki-only --json
memory cycles list --json
memory cycles show <cycle-id> --json
memory cycles validate <cycle-id> --json
memory replay-dry-run --start <ts> --end <ts> [--sources a,b] [--replay-run-id id] --json
```

Successful output is wrapped as:

```json
{
  "status": "ok",
  "schema_version": 1
}
```

Runtime outputs are intentionally ignored by git:

```text
memory-core/memory/runtime/
memory-core/storage/lakestore/
```

Do not add user wiki content, screenshots, session images, generated storage, or
local runtime state to this directory.
