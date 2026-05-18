# 1Context Memory Core

This directory contains the Python memory engine being integrated behind the
public macOS shell.

The public Swift app owns installation, lifecycle, diagnostics, permissions,
hooks, update behavior, wiki rendering, and wiki publication. The memory core
owns storage semantics, memory ticks, replay dry-runs, route planning, and
future agentic memory logic.

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
memory-core/storage/lakestore/*.lance
```

Do not add user wiki content, screenshots, session images, generated storage, or
local runtime state to this directory.
