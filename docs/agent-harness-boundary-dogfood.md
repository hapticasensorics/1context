# Agent Harness Boundary Dogfood

Status: lane-5 dogfood proof with real behavior probing

This dogfood scenario proves the product boundary the harness must preserve:
agent certificates, generated ids, parent-child lineage, lifecycle metadata,
capability declarations, proof status, and adapter evidence belong to the
harness; mail, wiki, memory, MCP implementations, Codex skills, Codex plugins,
Codex connectors, and Codex apps remain external capability bindings.

## Scenario

The deterministic verifier creates a disposable runtime and represents five
agent units:

- `agent-no-tool-001`: a control agent born with no product toolsets.
- `agent-mail-wiki-001`: an agent born with `toolset-mail`, `toolset-wiki`, a
  Codex skill, a Codex plugin, a Codex connector, and a Codex app as external
  capability bindings.
- `agent-room-red-001`, `agent-room-green-001`, and `agent-room-blue-001`:
  three child agents spawned under `agent-mail-wiki-001` in separate rooms with
  separate certificates, namespaces, session histories, and metadata.

The mail/wiki-capable agent performs one dogfood proof turn and sends one
dogfood mail delivery to the red room agent. Harness artifacts record only
labels, ids, external source URIs, redaction flags, a `mail://` body reference,
and a body digest. The message body is written only to
`external-capability-fixture.json`, which represents the mail system rather
than harness-owned state.

The five agents are born through the real `onecontext-agent-harness birth`
command. The verifier then starts and completes a real turn with usage, records
adapter events, asks `agent-status` for proof/usage from persisted evidence,
queries `transport-plan`, and retires the disposable blue-room unit.

## Run

```bash
node scripts/test-agent-harness-boundary-dogfood.mjs
```

The script writes evidence under:

```text
test-results/agent-harness-boundary-dogfood-<timestamp>/
```

Useful outputs:

- `commands.jsonl`: harness command calls and structured results.
- `harness-describe.json`: current harness contract description.
- `harness-agents.json`: pre-retire inventory returned by the harness.
- `harness-agents-after-retire.json`: inventory after retiring
  `agent-room-blue-001`.
- `harness-agent-statuses-before-frontier.json`: status before the frontier
  proof probes.
- `harness-agent-statuses.json`: per-agent certificate, lifecycle, usage, and
  proof summaries returned after the frontier probes and retire.
- `agent-status-after-frontier.json`: focused mail/wiki agent status after turn
  and adapter-event probes.
- `turn-lifecycle.json`: real turn lifecycle/usage result.
- `adapter-event-recording.json`: adapter event intake results for transport,
  wake steering, allowlist, skill/plugin/connector/app registry, mail send, and
  context injection evidence.
- `transport-plan.json`: transport-plan response.
- `harness-retire.json`: real retire response for the disposable blue-room
  unit.
- `dogfood-boundary-scenario.json`: certificates, metadata, receipts, proof
  status, and adapter evidence owned by the harness.
- `external-capability-fixture.json`: mail-system-owned message body fixture.
- `proof-summary.json`: compact pass/fail summary and implementation gaps.

## Current Behavior

The verifier exercises the implemented commands:

- `onecontext-agent-harness ensure`
- `onecontext-agent-harness status`
- `onecontext-agent-harness describe`
- `onecontext-agent-harness birth`
- `onecontext-agent-harness agents`
- `onecontext-agent-harness agent-status`
- `onecontext-agent-harness start-turn`
- `onecontext-agent-harness complete-turn`
- `onecontext-agent-harness record-adapter-event`
- `onecontext-agent-harness transport-plan`
- `onecontext-agent-harness retire`

Turn completion must be visible in usage status, adapter events must affect
proof status, and transport planning must return a real plan. The current run
at `test-results/agent-harness-boundary-dogfood-20260525T100317Z/` proved real
birth/status/inventory/retire behavior plus real turn lifecycle, usage
accounting, adapter event persistence, proof status from persisted events, and
transport planning across MCP toolsets and Codex skill/plugin/connector/app
attachments. It also proves child units carry parent/root lineage in their
birth certificates and inventory. `proof-summary.json` reports no remaining
implementation gaps for this dogfood boundary.

## Boundary Assertions

The verifier fails if:

- the scenario does not contain exactly five units;
- the no-tool agent declares any product toolset;
- the mail/wiki agent lacks `toolset-mail`, `toolset-wiki`, the Codex skill,
  the Codex plugin, the Codex connector, or the Codex app binding;
- capability bindings do not cite the expected external MCP or Codex registry
  sources;
- the three room agents share a room id, namespace, certificate, unit id, or
  session history;
- spawned room agents do not cite `agent-mail-wiki-001` as parent and root
  lineage;
- the disposable blue-room unit is not safely retired by the real harness;
- harness-owned artifacts contain exact mail body fields such as
  `body_markdown`;
- harness-owned artifacts contain the external mail body literal;
- mail adapter evidence lacks a body ref, body digest, or redaction flag.

## Deepening Path

The remaining deepening work is no longer this boundary proof; it is broader
runtime integration:

1. Route the same scenario through the installed macOS daemon RPC, not only the
   Rust CLI binary.
2. Add a dedicated durable `observe-proof` core API for manual/operator proof
   observations that are not raw adapter events.
3. Connect the real MCP `toolset-mail` and `toolset-wiki` servers plus real
   Codex skill/plugin/connector/app registries when those are packaged, while
   keeping `external-capability-fixture.json` outside harness state so the test
   continues proving that mail bodies are mail-owned, not harness-owned.
