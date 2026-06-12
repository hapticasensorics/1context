# Agent Harness API

JSON command surface for agent unit lifecycle and adapter proof evidence.

## Invocation

```bash
onecontext-agent-harness [--root <1Context-root>] <command> \
  [--request-json '<json>' | --request-file <path>]
```

`--root` defaults to `runtime-test/1Context`. Missing request JSON is `{}`.

## Envelopes

Success returns:

```json
{ "schema_version": 1, "status": "ok", "surface": "...", "operation": "...", "at": "..." }
```

Failure returns:

```json
{ "schema_version": 1, "status": "error", "surface": "agent_harness", "error": { "code": "...", "message": "...", "details": {} }, "repair_hints": [] }
```

## Commands

| Command | Request | Return |
| --- | --- | --- |
| `help` | `{}` | `commands[]` |
| `birth-live` | `AgentCallRequest` | `status: "scaffold"` until live runtime binding lands |
| `start-turn` | `StartTurnRequest` | updated `unit`, latest `receipt` |
| `complete-turn` | `CompleteTurnRequest` | updated `unit`, latest `receipt` |
| `record-proof` | `RecordAdapterEventRequest` | updated `unit`, latest `receipt` |
| `heartbeat` | `{ "unit_id": "..." }` | updated `unit`, heartbeat proof receipt |
| `retire` | `{ "unit_id": "...", "reason": "..." }` | updated `unit`, latest `receipt` |
| `status` | `{}` or `{ "unit_id": "..." }` | path status, or unit status with `certificate`, `lifecycle`, `lineage`, `turns`, `usage`, `capabilities`, `adapter_evidence`, `proof_status`, `receipts` |
| `inventory` | `{}` | grouped inventory plus `counts` |
| `replay` | `{}` | replayed `snapshot`, `inventory`, and `counts` |

Support commands:

| Command | Request | Return |
| --- | --- | --- |
| `describe` | `{}` | `owned_by_harness[]`, `external_product_layers[]`, `adapter_event_families[]` |
| `ensure` | `{}` | `paths` |

Compatibility aliases:

| Alias | Use Instead |
| --- | --- |
| `call` | `birth-live` once live birth lands; currently legacy ledger birth |
| `birth` | `birth-live` once live birth lands; currently legacy ledger birth |
| `record-adapter-event` | `record-proof` |
| `agents` | `inventory` |
| `agent-status` | `status` with `unit_id` |
| `observe-proof` | `record-proof`; currently scaffold only |
| `transport-plan` | future `birth-live` tool resolution |

## Requests

`AgentCallRequest`

Required: `role`, `model`, `visibility`.

Optional: `unit_id`, `parent_unit_id`, `spawn_request_id`, `identity`,
`instructions`, `runtime`, `capabilities`, `metadata`.

`StartTurnRequest`

Required: `unit_id`.

Optional: `turn_id`, `parent_turn_id`, `reason`, `expected_transport`,
`context`, `metadata`.

`CompleteTurnRequest`

Required: `unit_id`, `turn_id`.

Optional: `outcome`, `status`, `input_tokens`, `output_tokens`,
`total_tokens`, `usage`, `duration_ms`, `error`, `metadata`.

`outcome`/`status`: `completed`, `waiting`, `done`.

`RecordAdapterEventRequest` / `record-proof`

Required: `unit_id`, `adapter`, `kind`, `status`.

Optional: `event_id`, `id`, `at`, `correlation`, `evidence`, `redaction`.

`ObserveProofRequest` compatibility alias

Required: `unit_id` and one of `proof_key`, `proof`, or `kind`.

Optional: `status`, `evidence`, `metadata`.

`TransportPlanRequest` compatibility alias

Optional: `unit_id`, `capability_id`, `requested_transports`, `intent`,
`metadata`.

## Enums

`visibility`: `private`, `team`, `public`, `hidden`

`transport`: `mcp`, `codex_app_server_dynamic_tool`, `codex_skill`,
`codex_plugin`, `codex_connector`, `codex_app`, `host_hook`, `local_test`

`adapter`: `codex_app_server`, `codex_cli`, `codex_skill`, `codex_plugin`,
`codex_connector`, `codex_app`, `mcp`, `host_hook`, `local_test`

`adapter status`: `observed`, `accepted`, `failed`, `blocked`, `suppressed`,
`missing`

`lifecycle`: `new`, `born`, `ready`, `running`, `waiting`, `blocked`, `done`,
`error`, `retired`

`adapter event kind`: `transport_identity_observed`,
`runtime_wakeup_attempted`, `runtime_wakeup_accepted`,
`runtime_wakeup_failed`, `context_injection_requested`,
`context_injection_executed`, `hook_registry_observed`,
`hook_decision_observed`, `skill_registry_observed`,
`plugin_registry_observed`, `connector_registry_observed`,
`app_registry_observed`, `tool_allowlist_checked`, `tool_call_observed`,
`tool_call_denied`, `native_extra_tool_observed`,
`supervisor_dispatch_attempted`, `supervisor_dispatch_suppressed`,
`agent_lease_expired`, `agent_heartbeat_observed`

## Current Caveat

`record-proof` is the durable proof write path. `observe-proof` only returns a
scaffold compatibility receipt today. `birth-live` is fail-closed scaffold until
the harness owns real Codex runtime binding.
