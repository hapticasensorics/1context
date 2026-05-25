# Codex Adapter Harness Dogfood

Status: deterministic proof script for the adapter harness bridge

This proof verifies that the Codex adapter can use the agent harness boundary
to create and record a governed child agent. It intentionally drives the
adapter through a CLI contract instead of calling Rust internals directly, so
the same script can become release evidence for the adapter bridge.

## Run

```bash
node scripts/test-codex-adapter-harness-dogfood.mjs
```

The verifier writes a temporary result directory under:

```text
test-results/codex-adapter-harness-dogfood-<timestamp>/
```

Useful outputs:

- `proof-summary.json`: compact pass/fail proof summary with only redacted
  evidence.
- `commands.jsonl`: structured command log for harness and adapter CLI calls.
- `requests/*.json`: request payloads sent to the harness and adapter bridge.
- `runtime/1Context/`: disposable harness runtime created for this proof.

## Expected Adapter CLI

The script calls these adapter bridge commands:

```bash
onecontext-codex-adapter spawn-child \
  --root <1Context-root> \
  --request-json <governed-child-agent-request-json>

onecontext-codex-adapter record-proof \
  --root <1Context-root> \
  --request-json <harness-proof-record-plan-json>
```

If the adapter binary is not already built at
`target/debug/onecontext-codex-adapter`, the script falls back to:

```bash
cargo run -q -p onecontext-codex-adapter -- <adapter-args>
```

Set `ONECONTEXT_CODEX_ADAPTER_BIN` to test another binary path. Set
`ONECONTEXT_AGENT_HARNESS_BIN` to test another harness CLI path.

## Assertions

The proof fails unless all of these conditions hold:

- the parent unit is born through the real harness CLI;
- the child unit is spawned through the Codex adapter bridge CLI;
- the child certificate records `agent-codex-parent-001` as both parent and
  root lineage;
- the child certificate records the adapter spawn request id;
- proof events are recorded through the adapter bridge;
- child `agent-status` reports observed proof for `transport_identity`,
  `context_injection`, and `tool_conformance`;
- persisted adapter evidence count is at least three events;
- `proof-summary.json` does not contain the raw prompt sentinel and only
  includes hashes or redacted markers.

## Current Scaffold Behavior

If `crates/onecontext-codex-adapter/src/main.rs` is still at the older scaffold
placeholder, or if the command names drift, the script fails with
`adapter_harness_cli_missing` or `adapter_harness_command_failed` after proving
parent birth. That failure is intentional and actionable.

The expected next implementation step is to route those two adapter commands to
the existing in-process bridge:

- `GovernedChildAgentRequest -> InProcessHarnessBridge::spawn_child`
- redacted proof event/plan -> `InProcessHarnessBridge::record_proof_plan`

The current CLI emits harness unit JSON directly for both successful commands,
which the verifier accepts.
