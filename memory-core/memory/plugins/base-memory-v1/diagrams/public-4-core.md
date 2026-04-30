## What This Map Shows

This is the broad 1context memory system as it exists right now. The root files choose the active plugin, define local accounts, and define host permissions. The plugin describes shareable design. The runtime folder records local life.

## How to read it

- Start at `1context.toml`. It selects `base-memory-v1`, the runtime folder, and the accounts file.
- The plugin folders define the pieces that can be hired or controlled: agents, jobs, state machines, harnesses, prompts, custom tools, lived-experiences, provider routes, and linking policy.
- `hire_agent` is the central product action. It resolves job ids plus concrete job params plus agent configuration into one concrete hired agent.
- The birth certificate is the durable record: `hired_agent.created` with `hired_agent_uuid`, job params, config snapshot, versions, hashes, and memory attachment.
- Runtime experience is local and harness-native. Codex gets a real isolated `CODEX_HOME`; Claude gets its native project memory surface.

## Why it matters

The important boundary is plugin versus runtime. Plugins can be shared, edited, and swapped. Runtime experience stays local and is linked through the ledger, so experiments can change without losing what happened.
