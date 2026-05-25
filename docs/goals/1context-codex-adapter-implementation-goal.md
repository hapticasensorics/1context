# Milestone: 1Context Codex Adapter Implementation

## Goal

Build the first Rust-owned Codex adapter so 1Context can bind a durable agent
identity to a Codex thread, expose the two generic toolsets, wake active and
inactive Codex agents from durable mail notifications, deliver opened mail
bodies through `thread/inject_items`, and record redacted proof through the
agent harness.

The milestone is about the adapter spine, not a polished app UI. Swift may
later supervise or configure the adapter, but the portable control semantics
belong in Rust.

## Done When

- A `onecontext-codex-adapter` crate exists in the workspace and compiles.
- The crate has typed records for schema status, bindings, wake attempts,
  injection jobs, hook intents, event mirror plans, policy decisions, and proof
  records.
- A local proof can bind `agent_id` plus Codex `thread_id` without treating the
  thread id as durable identity.
- Wake planning chooses active-turn steering, idle-thread wake, resume/start,
  new-thread recovery, suppression, or poll-only from explicit state.
- `wiki.mail.open` style content delivery can be converted into a redacted
  injection job and proof record without persisting body text.
- Hook intent helpers cover `SessionStart`, `PreToolUse`, `PostToolUse`, and
  `Stop` first.
- Event mirror and proof helpers can emit harness-compatible redacted adapter
  evidence.
- Harness birth certificates can represent spawned child agents with
  parent/root lineage, so Codex subagents can be wrapped as durable 1Context
  child units instead of hidden runtime-only children.
- The adapter harness bridge validates governed child-spawn requests before
  birth using a permissive default policy and explicit rejection checks.
- Unit tests cover the pure planning paths without requiring a live Codex app
  server.

## Checklist

### 1. Scaffold

- [x] Codex adapter target spec exists in `docs/1context-codex-adapter-spec.md`.
- [x] Workspace crate seam exists at `crates/onecontext-codex-adapter`.
- [x] Public module seams match the adapter spec: schema, app-server client,
  binding, wake, injection, hooks, event mirror, policy, proof.
- [x] README explains the crate boundary and local proof ownership.
- [x] README explains local proof commands once they exist.

### 2. Schema And App-Server Client

- [x] Represent generated app-server schema status and required method
  capability checks.
- [x] Add pure tests for missing/available required methods.
- [x] Add a placeholder command path for future `codex app-server
  generate-json-schema` integration without running it in unit tests.

### 3. Agent Binding And Wake Dispatch

- [x] Validate `agent_id`, harness unit, and Codex transport binding records.
- [x] Plan wake strategy from lease, loaded state, thread id, active turn id,
  suppression state, and supervisor policy.
- [x] Record wake attempts as transport evidence, not mail state transitions.

### 4. Injection Bridge

- [x] Model bodyless `content_delivery` requests from `wiki.mail.open`.
- [x] Queue injection jobs only when agent, delivery, thread, and body hash
  match.
- [x] Produce injection receipt/proof plans without storing body text.

### 5. Hook Manager

- [x] Model hook registry observations and trust state.
- [x] Generate `SessionStart`, `PreToolUse`, `PostToolUse`, and `Stop` intents.
- [x] Cap Stop-hook continuation loops so unfinished mail cannot trap forever.

### 6. Event Mirror And Proof Recorder

- [x] Map runtime event families to mail control events and harness adapter
  events.
- [x] Redact body-like evidence before persistence.
- [x] Add proof summaries for wake, injection, hook, and toolset visibility
  families.

### 7. Integration Proof

- [x] `cargo test -p onecontext-codex-adapter` passes.
- [x] `cargo check -p onecontext-codex-adapter` passes.
- [ ] A dogfood harness can prove toolset visibility, active wake, open/inject,
  Stop guard, and redacted evidence.
- [x] The agent harness can birth child units with parent/root lineage and
  reject spawns from retired parents.
- [x] The Codex adapter has an in-process harness bridge for redacted proof
  recording and child birth.
- [x] The Codex adapter bridge has a pure governed child-spawn policy covering
  parent allowlists, model allowlists, active child limits from inventory, and
  simple metadata/capability budgets.
- [x] `scripts/test-codex-adapter-harness-dogfood.mjs` proves parent birth,
  adapter-driven child spawn, parent/root lineage, turn accounting, and
  redacted proof recording through the bridge.

### 8. Live App-Server Lane

- [x] `live-server-plan` emits deterministic JSON for the runner instead of
  prose-only usage output.
- [x] The plan includes the real `codex app-server` startup command, schema
  generation command, required method list, proof categories, artifact paths,
  and `cli_contract_version`.
- [x] The CLI exposes `--listen-url` and `--codex-bin` so scripts can select
  the transport and executable without rewriting JSON.
- [x] The plan treats model-consuming `turn/start` and `turn/steer` as the
  default live proof lane, with `--skip-model-turns` reserved for non-model
  debugging.
- [x] Worker A runner starts the real app-server first, generates schema,
  initializes, proves `thread/start` and `thread/loaded/list`, and records
  redacted harness evidence.
- [x] Worker A runner proves non-model `thread/inject_items` against the live
  server with redacted synthetic context.
- [x] Worker A runner attempts `turn/start` and `turn/steer` by default, waits
  for the matching `turn/started` notification before steering, and records
  the redacted wake proof when both live operations pass.
  Evidence: `test-results/codex-adapter-live-server-dogfood-20260525T111353Z/proof-summary.json`.
- [x] Live mail flow dogfood creates real mail, dispatches the durable
  notification through a local command bridge into live Codex `turn/steer`,
  opens the delivery, injects the authorized body through `thread/inject_items`,
  records injection receipt, marks the delivery done, and acks notification.
  Evidence: `test-results/codex-adapter-live-mail-flow-20260525T112717Z/proof-summary.json`.

## Worker Slices

- Worker A: schema registry, app-server capability reports, and live
  app-server runner.
- Worker B: agent binding validation and wake strategy planning.
- Worker C: injection bridge, content-delivery receipt planning, and
  live-server CLI/documentation surface.
- Worker D: hook manager, hook registry, and Stop guard budget.
- Worker E: event mirror, policy bridge, and harness proof recorder.

## Notes

- Current baseline: the spec, crate seams, pure planning helpers, harness
  bridge CLI, governed spawn policy, deterministic adapter/harness dogfood,
  default live Codex app-server dogfood with model turn and steering attempts,
  and a live mail wake/open/inject dogfood are present.
- Evidence: reviewed parent run
  `CARGO_TARGET_DIR=/tmp/onecontext-codex-adapter-review-1779701691 cargo test -p onecontext-codex-adapter`
  passed 49 tests; `cargo check -p onecontext-codex-adapter` and
  `cargo clippy -p onecontext-codex-adapter --all-targets -- -D warnings`
  passed in the same isolated target dir.
- Immediate next step: move the live mail dogfood bridge from a test-local
  dispatcher helper into the adapter/harness runtime boundary so production
  supervisors can dispatch without a script-owned HTTP bridge.
