# Base Memory V1

This is the seed memory plugin for 1Context.

The plugin defines memory agents, jobs, prompts, state-machine source, provider
policy, and tool contracts. It does not own wiki rendering, local web serving,
installation, updates, or the public app shell. Those live in Swift,
`RuntimeDefaults`, and `WikiEngine`.

## Folder Map

```text
plugin.toml        plugin identity
linking.toml       versioned linker policy
native-memory.toml native memory surfaces
providers.toml     provider/model routing
dependencies/      declared local and account needs
agents/            agent profiles
harnesses/         agent loop backends
prompts/           prompt text and prompt fragments
jobs/              reusable work contracts
state_machines/    scoped control definitions
custom-tools/      plugin tool contracts and optional implementations
migrations/        idempotent memory contract migrations
```

Runtime output belongs under ignored `memory/runtime/`. Durable source memory is
owned by Perception DB through `onecontext-memoryd`; the old LanceDB
`storage/lakestore/` tree is archived only.

## Runtime Contract

The app-facing executable is intentionally narrow:

```bash
uv run --project memory-core 1context-memory-core status --json
uv run --project memory-core 1context-memory-core storage init --json
uv run --project memory-core 1context-memory-core memory tick --wiki-only --json
uv run --project memory-core 1context-memory-core memory cycles list --json
uv run --project memory-core 1context-memory-core memory cycles show <cycle-id> --json
uv run --project memory-core 1context-memory-core memory cycles validate <cycle-id> --json
uv run --project memory-core 1context-memory-core memory replay-dry-run --start <ts> --end <ts> --json
uv run --project memory-core 1context-memory-core memory update-wiki --provider codex --json
```

Do not add a broad developer CLI back for convenience. Add explicit contract
verbs only when the Swift app or a production memory loop actually needs them.

## Boundaries

- Memory-core owns prompt/job definitions and thin wiki-update orchestration.
- `onecontext-memoryd` owns durable Perception DB ingestion and query.
- `onecontext-wiki-core` owns wiki page lifecycle, talk folders, and publish
  receipts.
- The agent harness owns birth certificates, capabilities, and proof status.
- Swift owns first-run defaults, user-file preservation, render scheduling,
  local-web publication, and update/install behavior.

## Rule

Keep this plugin small enough that an operator can inspect it. Prompts and job
definitions should be explicit files. Generated diagrams, historical plans, and
large exploratory architecture notes should stay out of the shipped plugin.
