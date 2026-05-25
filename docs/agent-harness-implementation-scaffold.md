# Agent Harness Implementation Scaffold

- Status: scaffold ready for parallel implementation
- Last updated: 2026-05-25
- Owner: 1Context agent harness runtime

This document is the handoff map for turning the experimental birth-certificate
lab into a reusable 1Context component.

The harness is the authority for an agent unit. It does not own mail, wiki,
memory, Codex, or MCP behavior. Those are capabilities bound into an agent at
birth and then proven through adapter events.

## Core Invariant

```text
Every harness call emits one durable birth certificate that combines requested
inputs, host-generated ids, runtime binding evidence, and the initial metadata
needed to manage that agent until retirement.
```

The harness owns:

- birth certificates
- host-generated ids
- active and retired agent inventory
- lifecycle and turn ledgers
- token and duration accounting
- capability declarations
- proof requirements and observed proof status
- adapter transport evidence
- append-only receipts

The harness must not own:

- mail message storage
- wiki page storage
- memory database internals
- MCP server implementations
- Codex app-server transport details beyond adapter evidence

## Current State

Rust workspace members:

- `crates/onecontext-agent-harness-core`
- `crates/onecontext-agent-harness-daemon`

Swift package target:

- `macos/Sources/OneContextAgentRuntime`

Swift test target:

- `macos/Tests/OneContextAgentRuntimeTests`

The Rust daemon currently implements:

- `onecontext-agent-harness ensure`
- `onecontext-agent-harness status`
- `onecontext-agent-harness describe`
- `onecontext-agent-harness call`
- `onecontext-agent-harness birth`
- `onecontext-agent-harness agents`
- `onecontext-agent-harness agent-status`
- `onecontext-agent-harness retire`

It declares and validates, but intentionally returns structured
`status: "scaffold"` feature-gated receipts for:

- `start-turn`
- `complete-turn`
- `observe-proof`
- `record-adapter-event`
- `transport-plan`

The Swift bridge maps app RPC-style methods to those commands:

- `agent.harness.status`
- `agent.harness.describe`
- `agent.harness.ensure`
- `agent.harness.call`
- `agent.harness.birth`
- `agent.harness.start-turn`
- `agent.harness.complete-turn`
- `agent.harness.observe-proof`
- `agent.harness.record-adapter-event`
- `agent.harness.transport-plan`
- `agent.harness.agents`
- `agent.harness.agent-status`
- `agent.harness.retire`

## Storage Contract

The runtime root is the user 1Context directory. The scaffold reserves:

```text
<1Context>/context-engine/agents/harness/
  agent-harness.json
  birth-certificates/
  units/
```

Implementation should use file locking and atomic writes. If a later SQLite
store is introduced, the JSON receipts and certificate artifacts should remain
exportable and replayable.

## Five Implementation Lanes

### 1. Rust Core Store And Invariants

Files:

- `crates/onecontext-agent-harness-core/src/lib.rs`

Build the store, locking, atomic write path, receipt replay, and invariant
checks. This lane owns the durable data model.

Done when:

- `call` creates a unit namespace and appends a receipt
- every call emits a birth certificate
- active and retired inventory can be reconstructed from receipts
- duplicate host-generated ids are impossible inside one store
- core tests cover replay, corrupt store recovery, idempotency, and retirement

### 2. Rust Daemon Protocol

Files:

- `crates/onecontext-agent-harness-daemon/src/main.rs`

Turn scaffold commands into stable JSON command receipts. Keep all nonzero
errors structured with machine-readable codes and repair hints.

Done when:

- each declared command is implemented or intentionally feature-gated
- request JSON can be passed inline or by file
- `agents` reports active, waiting, blocked, done, and retired summaries
- `agent-status` reports certificate, lifecycle, usage, capabilities, and proof
  status for one unit
- CLI tests cover success, bad input, missing unit, and retired unit paths

### 3. Adapter Evidence And Proof Gating

Files:

- `crates/onecontext-agent-harness-core/src/lib.rs`
- future adapter modules in the daemon or a sibling crate

Implement `AdapterEvent` intake and proof status. The harness should record
transport evidence without pretending to own the transport.

Required evidence families:

- transport identity: thread, session, turn, and generated ids
- wake steering: attempted, accepted, failed, or suppressed
- context injection: requested and executed
- hooks: registry observed and decision observed
- tool conformance: allowlist checked, call observed, denied, or native extra
  observed
- dispatch and lease liveness: supervisor dispatch, heartbeat, lease expiry

Done when:

- capability bindings can require proof categories
- missing required proof keeps an agent in blocked or degraded state
- observed native extra tools are surfaced as warnings, not silently ignored
- tests can replay adapter events into deterministic proof status

### 4. Swift Runtime Integration

Files:

- `macos/Sources/OneContextAgentRuntime`
- `macos/Tests/OneContextAgentRuntimeTests`
- later: daemon RPC registration in `macos/Sources/OneContextDaemon`

Wire the harness into the macOS daemon as a process-backed runtime component.
The Swift side should stay thin: discover the binary, call commands, surface
receipts, and expose daemon RPC methods.

Done when:

- the daemon can answer harness status through its existing RPC path
- `agent.harness.call` can be invoked from the app layer
- errors preserve Rust error codes and repair hints where possible
- Swift tests cover process discovery, command mapping, and daemon routing

### 5. Product Dogfood And Capability Boundaries

Files:

- docs and scripts that exercise the installed app
- future MCP/tool gateway integration points

Current proof:

- `docs/agent-harness-boundary-dogfood.md`
- `scripts/test-agent-harness-boundary-dogfood.mjs`

The script currently exercises real `ensure`, `status`, `describe`, `birth`,
`agents`, `agent-status`, `start-turn`, `complete-turn`,
`record-adapter-event`, `transport-plan`, and `retire` behavior. It asserts
usage accounting, adapter-event persistence, proof status from persisted
events, transport planning, the external mail/wiki body boundary, and external
Codex skill/plugin/connector/app registry bindings.

Prove the harness can create isolated agent units and bind external toolsets
without hard-coding those tools into the harness.

Done when:

- one agent can be born with no product toolsets
- one agent can be born with `toolset-mail`, `toolset-wiki`, a Codex skill, a
  Codex plugin, a Codex connector, and a Codex app
- three agents can be born in separate rooms with separate certificates,
  namespaces, session histories, metadata, and parent/root lineage
- capability bindings cite the external MCP/tool or Codex registry source
- dogfood receipts show communication through the mail system without the
  harness storing mail bodies
- one disposable room unit can be retired after the boundary proof without
  mutating unrelated units

Latest lane-5 evidence, 2026-05-25:
`test-results/agent-harness-boundary-dogfood-20260525T100317Z/proof-summary.json`
shows real birth/status/inventory/retire behavior plus real turn lifecycle,
usage accounting, adapter-event persistence, proof status from persisted
events, transport planning, Codex skill/plugin/connector/app registry
attachment proof, and parent/root lineage for spawned child units.

Latest adapter bridge evidence, 2026-05-25:
`scripts/test-codex-adapter-harness-dogfood.mjs` now drives parent birth through
the harness CLI, child spawn through the Codex adapter CLI, turn accounting
through the harness, and redacted proof recording back through the adapter
bridge. The remaining harness deepening work is daemon RPC dogfood, live
Codex app-server dogfood, and a dedicated durable `observe-proof` API.

## Parallel Work Rules

- Each agent should choose one lane and avoid editing another lane's files
  unless they first report a blocking contract mismatch.
- Code-bearing changes need deterministic tests in the same lane.
- New abstractions must name the invariant they protect.
- Do not port code wholesale from private experiments. Port contracts,
  schemas, tiny utilities, and proven behavior only.
- Keep the harness core small. Toolsets, MCP servers, mail, wiki, and memory
  integrations should plug in through capability bindings and adapter events.

## Baseline Checks

Run these before handing a lane back:

```bash
cargo test -p onecontext-agent-harness-core -p onecontext-agent-harness-daemon
swift test --package-path macos --filter OneContextAgentRuntimeTests
git diff --check
```

When a lane starts touching app daemon routing, also run the daemon test filter
for the affected target.
