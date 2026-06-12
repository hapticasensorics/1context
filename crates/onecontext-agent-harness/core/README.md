# onecontext-agent-harness-core

Rust core for the 1Context Agent Harness.

This crate owns the portable agent-unit contract and durable store:

- birth certificate schemas
- host-owned generated IDs
- lifecycle and turn ledger types
- parent-child lineage for spawned agent units
- capability bindings
- adapter/transport evidence
- active agent inventory and retirement authority
- token/duration usage accounting
- append-only receipt/event path layout

First-class capability transports are `mcp`, `codex_app_server_dynamic_tool`,
`codex_skill`, `codex_plugin`, `codex_connector`, `codex_app`, `host_hook`,
and `local_test`.

## Store Shape

The store lives under:

```text
<1Context>/context-engine/live/state/harness/
  agent-harness.json
  agent-harness.lock
  birth-certificates/<certificate-id>.json
  units/<unit-id>/adapter-events/<adapter-event-id>.json
  units/<unit-id>/receipts/<receipt-id>.json
```

`agent-harness.json` is an atomic JSON snapshot for fast reads. Birth
certificates, receipts, and redacted adapter events are also written as separate
durable artifacts so inventory and proof evidence can be reconstructed if the
snapshot is corrupt.

`AgentHarnessStore::call` creates a unit namespace, allocates host-generated
IDs from the store sequence, emits a birth certificate, appends an
`agent_called` receipt, and persists the snapshot atomically. Child calls may
cite `parent_unit_id` and `spawn_request_id`; the harness records parent/root
lineage in both the certificate and inventory, and rejects children spawned
from retired parents. Explicit `unit_id` calls are idempotent when the
requested inputs match the existing birth certificate and are rejected when the
same unit ID is reused for different inputs.

`AgentHarnessStore::retire` marks a unit retired and appends an
`agent_retired` receipt. Inventory is derived from the stored units or replayed
receipts.

`AgentHarnessStore::start_turn` moves an active unit to `running`, records the
active turn ID, increments `turns_started`, refreshes `last_active_at`, and
appends a `turn_started` receipt. `AgentHarnessStore::complete_turn` validates
the active turn ID, clears it, increments `turns_completed`, accumulates
input/output/total tokens and duration, moves the unit to `ready`, `waiting`, or
`done`, and appends a `turn_completed` receipt. Retired units reject turn
mutations.

`AgentHarnessStore::record_adapter_event` accepts an `AdapterEventRequest`,
allocates a host-owned adapter event ID, redacts body-like evidence fields,
writes `units/<unit-id>/adapter-events/<adapter-event-id>.json`, appends a
`proof_observed` receipt, and persists the event in both the unit and snapshot
indexes. Adapter evidence is only proof metadata: mail, wiki, memory, raw
prompt, tool output, token, and secret-like bodies are replaced with
`[redacted]` before durable writes.

`AgentHarnessUnit::proof_status`, `AgentHarnessSnapshot::proof_status`,
`AgentHarnessStore::proof_status`, and `AgentHarnessStore::agent_status`
compute proof readiness from persisted adapter events rather than transient
daemon state. Status-ready payloads include the stored redacted adapter events
and their `ProofStatusSummary`.

`replay_snapshot_from_receipts` rebuilds a deterministic snapshot from durable
receipts and adapter event artifacts and is used when the JSON snapshot cannot
be read.

Still out of scope for this crate: mail/wiki/memory/MCP implementations and
transport ownership. Codex skill, plugin, connector, and app implementations
are also external. The harness stores the declaration and proof ledger; runtime
loading stays in the owning registry or host.
