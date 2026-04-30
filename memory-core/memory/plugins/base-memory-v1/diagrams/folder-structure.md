## What This Map Shows

This is the current 1context folder structure. It is intentionally small: a tiny Python loader/linker, a shareable memory plugin, and a runtime folder for local state.

## How to read it

- `src/onectx` holds the code that loads config, links runtime experience, writes ledger events, routes native memory, and renders birth certificates.
- `memory/plugins/base-memory-v1` holds shareable definitions.
- `agents`, `jobs`, and `state_machines` are plugin-owned definition folders.
- `harnesses`, `prompts`, `dependencies`, `custom-tools`, `native-memory.toml`, `providers.toml`, and `linking.toml` are active design surfaces.
- `memory/runtime` holds the local ledger and runtime experiences.

## Why it matters

The structure keeps the plugin system isolated inside `memory/`. That makes it easier to change the rest of 1Context later without spreading plugin-specific assumptions across the whole repo.
