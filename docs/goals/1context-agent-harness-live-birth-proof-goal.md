# Milestone: Agent Harness Live Birth Proof

## Goal

Make agent birth a harness-owned live transaction: a real Codex agent is born
with identity, prompts, tools, lived experience, place/source context, runtime
binding, and ledgered lifecycle proof.

The acceptance path is one repeatable live proof test. Dummy agents, mocked
app-server responses, and local-only ledger tests do not count as milestone
completion.

## Done When

- A fresh proof root can birth a real Codex-backed harness agent.
- The live agent receives and echoes the expected tools, prompt ids, lived
  experience id, source/place packet id, unit id, and runtime thread id.
- The harness writes a complete birth envelope, certificate, runtime binding,
  manifests, receipts, adapter events, and ledger entries.
- One clean harness API owns birth, turn lifecycle, runtime binding, proof
  recording, inventory, replay, and retirement.
- A replay from harness ledger state reconstructs the same agent state.
- Ledger rollover is exercised during the live proof and replay still matches.
- The live proof can pass repeatedly from clean proof roots without dummy
  fallback.

## Checklist

### 1. Baseline

- [x] Agent harness crates are grouped under `crates/onecontext-agent-harness/`.
  Evidence: `core/`, `daemon/`, and `adapters/codex/` exist under that folder.
- [x] Agent Mail is separated from wiki core.
  Evidence: `crates/onecontext-agent-mail/`.
- [x] Wiki runtime crates are grouped under `crates/onecontext-wiki/core/` and
  `crates/onecontext-wiki/cli/`.
- [x] Harness command surface has a compact API reference.
  Evidence: `crates/onecontext-agent-harness/API.md`.
- [x] A live Codex app-server execution path exists.
  Evidence: `crates/onecontext-context-engine/src/agent_execution.rs` spawns
  `codex app-server --stdio` and calls `thread/start` or `thread/resume`.
- [ ] Harness exposes one clean single API for birth, turns, proof, inventory,
  replay, and retirement.

### 2. Single Harness API

- [ ] Keep the harness API small enough to understand from one command/return
  table, with no second public orchestration vocabulary.
- [ ] Replace scattered `call`, context-engine launch, adapter spawn, and proof
  side doors with one harness-owned API surface.
- [x] Define the public commands in `crates/onecontext-agent-harness/API.md`
  before implementing new behavior.
  Evidence: `cargo test -p onecontext-agent-harness-daemon` covers help output.
- [x] Include only these top-level lifecycle commands: `birth-live`,
  `start-turn`, `complete-turn`, `record-proof`, `heartbeat`, `retire`,
  `status`, `inventory`, and `replay`.
  Evidence: `help_names_clean_api_and_compatibility_aliases`.
- [x] Mark old aliases such as `call`, `birth`, `observe-proof`, and
  adapter-specific launch commands as compatibility wrappers or remove them.
  Evidence: `help_names_clean_api_and_compatibility_aliases`.
- [ ] Make Context Engine call the harness API, not Codex app-server directly.
- [ ] Make Codex adapter implement the adapter contract behind the harness API,
  not a parallel public orchestration surface.
- [ ] The live proof script uses only the single harness API entrypoint.

### 3. Birth Transaction

- [ ] Replace ledger-only `call` semantics with an explicit live birth request.
- [x] Add a fail-closed `birth-live` command so live birth has one public
  entrypoint before runtime binding is implemented.
  Evidence: `birth_live_fails_closed_without_creating_ledger_unit`.
- [ ] Define `AgentBirthRequest` with identity, model, role, adapter, prompts,
  tools, lived experience, source/place context, permissions, and required
  receipts.
- [ ] Define `AgentBirthEnvelope` as the durable birth truth.
- [ ] Write `birth-envelope.json` before the agent is considered live-born.
- [ ] Record `agent.birth.failed` when runtime binding fails.
- [ ] Record `agent.born` only after the live Codex runtime binding exists.

### 4. Context And Tool Manifests

- [ ] Resolve prompt parts into a manifest with ids, paths, byte counts, and
  sha256 hashes.
- [ ] Resolve lived-experience parts into a manifest and inject them into the
  live turn.
- [ ] Resolve place/source packet parts into a manifest and inject them into
  the live turn.
- [ ] Resolve requested tools into requested, visible, hidden host, denied, and
  gateway-bound tool lists.
- [ ] Persist all manifests under `units/<unit-id>/manifests/`.
- [ ] Record context/tool hash summaries in the birth envelope.

### 5. Runtime Binding

- [ ] Move Codex `thread/start` and `thread/resume` binding under the harness
  live birth operation.
- [ ] Persist `runtime-bindings/codex-app-server.json` with thread id, model,
  cwd, app-server transport, and resume/start status.
- [ ] Record adapter evidence for runtime binding creation.
- [ ] Reject or fail closed when the live Codex thread id cannot be observed.

### 6. Lifetime Ledger

- [ ] Add global harness ledger segmentation:
  `ledger/current.jsonl`, `ledger/segments/`, `ledger/checkpoints/`.
- [ ] Add per-agent lifecycle ledger under `units/<unit-id>/ledger/`.
- [ ] Emit ledger events for birth requested, context resolved, tools resolved,
  runtime started, born, turn started, context injected, turn completed,
  heartbeat observed, lease expired, and retired.
- [ ] Keep `agent-harness.json` as a rebuildable snapshot, not primary truth.
- [ ] Add replay from latest checkpoint plus later segments plus current log.

### 7. Live Proof Script

- [ ] Add `scripts/test-live-agent-harness-birth.sh`.
- [ ] The script creates a fresh proof root under
  `dist/live-agent-harness-proof/<timestamp>/`.
- [ ] The script fails if a real Codex app-server cannot be used.
- [ ] The script births one real Codex-backed harness agent.
- [ ] The live agent receives a tiny verification task and returns JSON with
  unit id, thread id, prompt ids, tool names, lived experience id, and
  source/place packet id.
- [ ] The script verifies birth envelope, certificate, manifests, runtime
  binding, receipts, adapter events, ledger entries, and rebuilt snapshot.
- [ ] The script forces tiny ledger rollover and verifies replay still matches.
- [ ] The script writes `proof-summary.json` and raw app-server transcript.

### 8. Repeated Proof

- [ ] Run the live proof three times from clean proof roots.
- [ ] Each run uses different runtime ids but proves the same required birth
  context and ledger invariants.
- [ ] Preserve proof bundles when `ONECONTEXT_LIVE_HARNESS_KEEP_PROOF=1`.
- [ ] Document the passing proof bundle paths in this file.

## Notes

- Current baseline: the repo has a live Codex app-server worker loop, but the
  harness does not yet own live runtime birth.
- Current gap: tools and context are declared around the harness request, but
  tool installation, lived experience injection, source/place packet injection,
  runtime binding, and ledger replay are not one harness-owned transaction.
- Immediate next step: implement the smallest `birth-live` path that starts a
  real Codex app-server thread and writes a birth envelope plus runtime binding.
