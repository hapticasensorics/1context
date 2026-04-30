# 1Context Memory Core

This directory contains the Python memory engine being integrated behind the
public macOS shell.

The public Swift app owns installation, lifecycle, diagnostics, permissions,
hooks, and update behavior. The memory core owns storage semantics, wiki
rendering, memory ticks, replay dry-runs, route planning, and future agentic
memory logic.

The boundary is a subprocess contract. For local development:

```bash
uv run --project memory-core 1context-memory-core status --json
```

The executable wrapper used by the Swift adapter is:

```bash
memory-core/bin/1context-memory-core
```

It accepts the narrow public command shapes documented in
`docs/memory-core-contract.md` and wraps successful output as:

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
