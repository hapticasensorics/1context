# onecontext-agent-harness-daemon

Command protocol for the 1Context Agent Harness.

The daemon owns CLI/RPC JSON plumbing around the harness core. It accepts
request JSON inline or from a file, validates each declared command shape, and
returns structured JSON for both success and error paths.

## Request Input

Request-bearing commands accept any one of:

```bash
onecontext-agent-harness birth-live --request-json '{"role":"researcher","model":"gpt-5","visibility":"private"}'
onecontext-agent-harness record-proof --json '{"unit_id":"agent-1","adapter":"local_test","kind":"context_injection_executed","status":"observed"}'
onecontext-agent-harness status --request-file request.json
onecontext-agent-harness inventory '{}'
```

`--root <path>` selects the 1Context runtime root and defaults to
`runtime-test/1Context`.

## Commands

Primary harness API:

- `birth-live`
- `start-turn`
- `complete-turn`
- `record-proof`
- `heartbeat`
- `retire`
- `status`
- `inventory`
- `replay`

Support commands:

- `ensure`
- `describe`

Compatibility aliases:

- `call`
- `birth`
- `record-adapter-event`
- `agents`
- `agent-status`
- `transport-plan`
- `observe-proof`

`call` and `birth` still perform the legacy ledger-only birth. `birth-live`
currently fails closed with `status: "scaffold"` until the harness owns live
Codex runtime binding.

`transport-plan` is a deterministic projection over declared capability
bindings. It returns `status: "ok"`, a stable daemon receipt, and a
`compatibility.core_method_expected_for_persistence` field for a future durable
core planner:

```rust
AgentHarnessStore::plan_transport(TransportPlanRequest) -> Result<TransportPlan, HarnessError>
```

`observe-proof` returns `status: "scaffold"` with a stable receipt and a
`feature_gate.expected_core_method` field:

```rust
AgentHarnessStore::observe_proof(ObserveProofRequest) -> Result<AgentHarnessUnit, HarnessError>
```

Unit-scoped compatibility commands still verify that the referenced unit exists
and is not retired before returning a scaffold receipt.

## Frontier Request Shapes

`start-turn`:

```json
{
  "unit_id": "agent-1",
  "turn_id": "turn-1",
  "metadata": {}
}
```

`complete-turn`:

```json
{
  "unit_id": "agent-1",
  "turn_id": "turn-1",
  "usage": { "input_tokens": 7, "output_tokens": 11 },
  "duration_ms": 30,
  "outcome": "done"
}
```

`record-proof`:

```json
{
  "unit_id": "agent-1",
  "adapter": "local_test",
  "kind": "context_injection_executed",
  "status": "accepted",
  "correlation": { "turn_id": "turn-1" },
  "evidence": {}
}
```

`transport-plan`:

```json
{
  "unit_id": "agent-1",
  "requested_transports": ["mcp", "codex_skill", "codex_plugin", "local_test"]
}
```

`status` with `unit_id` returns persisted turn, usage, adapter evidence, and proof status
from the core store. Adapter proof status is derived from
`unit.adapter_events` plus adapter events replayed from unit receipt evidence.
Registry proof events include `skill_registry_observed`,
`plugin_registry_observed`, `connector_registry_observed`, and
`app_registry_observed`.

## Errors

All nonzero exits print an object with:

- `schema_version`
- `status: "error"`
- `surface: "agent_harness"`
- `error.code`
- `error.message`
- `error.details`
- `repair_hints`
