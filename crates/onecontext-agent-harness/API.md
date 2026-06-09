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
| `describe` | `{}` | `owned_by_harness[]`, `external_product_layers[]`, `adapter_event_families[]` |
| `ensure` | `{}` | `paths` |
| `status` | `{}` | path fields, `store_exists`, `harness_root_exists` |
| `call` | `AgentCallRequest` | `unit_id`, `unit`, latest `receipt` |
| `birth` | `AgentCallRequest` | same as `call` |
| `start-turn` | `StartTurnRequest` | updated `unit`, latest `receipt` |
| `complete-turn` | `CompleteTurnRequest` | updated `unit`, latest `receipt` |
| `record-adapter-event` | `RecordAdapterEventRequest` | updated `unit`, latest `receipt` |
| `observe-proof` | `ObserveProofRequest` | `status: "scaffold"`, compatibility `receipt` |
| `transport-plan` | `TransportPlanRequest` | `request`, `transport_plan`, compatibility `receipt` |
| `agents` | `{}` | grouped inventory plus `counts` |
| `agent-status` | `{ "unit_id": "..." }` | `certificate`, `lifecycle`, `lineage`, `turns`, `usage`, `capabilities`, `adapter_evidence`, `proof_status`, `receipts` |
| `retire` | `{ "unit_id": "...", "reason": "..." }` | updated `unit`, latest `receipt` |

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

`RecordAdapterEventRequest`

Required: `unit_id`, `adapter`, `kind`, `status`.

Optional: `event_id`, `id`, `at`, `correlation`, `evidence`, `redaction`.

`ObserveProofRequest`

Required: `unit_id` and one of `proof_key`, `proof`, or `kind`.

Optional: `status`, `evidence`, `metadata`.

`TransportPlanRequest`

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

`record-adapter-event` is the durable proof write path. `observe-proof` only
returns a scaffold compatibility receipt today.
