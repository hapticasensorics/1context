# Context Engine Orchestrator Port Checklist

This checklist tracks the native Rust wiki-company orchestrator.
It is intentionally evidence-based: a milestone is checked only when the current
worktree has code or tests that prove the behavior.

## Validation Milestones

- [x] **1. Config loads and validates**
  - Rust loads `packs/wiki-company-v1` and
    `orchestrators/wiki-company-orchestrator-v1`.
  - Every job references an existing agent.
  - Every agent references an existing harness and provider.
  - Every agent/job/harness prompt path resolves.
  - Evidence: focused Rust tests over the shipped runtime defaults.
  - Current evidence:
    - `cargo test -p onecontext-context-engine shipped_wiki_company_pack_and_orchestrator_validate -- --nocapture`
    - `cargo run -q -p onecontext-context-engine -- update-wiki --root <tmp-runtime-copy> ...`
    - Validator reports 15 agents, 16 harness-launchable jobs, 1 harness,
      30 prompts, and 66 prompt references.
    - Removed stale `memory.wiki.build_inputs` from the shipped pack because it
      was an old deterministic helper without an agent/harness.

- [x] **2. Packet planner works**
  - Fixture Perception events produce recent-3-day packets first.
  - Backfill covers 30 days oldest-to-newest after the recent-first pass.
  - Packets split by token budget and skip empty windows.
  - Evidence: `packet_planner.rs` unit tests with deterministic fixtures.
  - Current evidence:
    - `cargo test -p onecontext-context-engine`
    - `planner_prioritizes_recent_three_days_then_backfill`
    - `planner_skips_cached_packets_and_ignores_events_without_timestamps`
    - `planner_splits_large_hours_by_session_and_event_chunks`
    - `planner_default_context_fraction_targets_about_sixty_two_percent`
    - Configured scribe packet target is `160,000` tokens from
      `packet-policy.toml`.

- [x] **2a. Executable orchestration primitives exist**
  - `phases.toml` declares dependencies, phase strategies, completion policies,
    and phase-to-job bindings instead of only labels.
  - Rust builds an `ExecutableOrchestrationPlan` from pack + orchestrator TOML.
  - Harness request previews use orchestrator routing and receipt policy instead
    of hardcoded routes when config is available.
  - Packet planning dry-run policy is derived from `packet-policy.toml`.
  - Evidence:
    - `cargo test -p onecontext-context-engine`
    - `tests/orchestration_primitives.rs`
    - `dry_run_lists_agents_routes_packet_policy_and_publish_intent`

- [x] **2b. Scheduler scaffold plans phases without execution**
  - Rust builds a `SchedulerRunPlan` from the executable orchestration plan.
  - The scheduler topologically orders phase dependencies, reports missing
    dependencies/cycles, marks ready phases from completed phase receipts, and
    records planned jobs, routes, bindings, required artifacts, and receipt
    policy without launching models.
  - Dry-run output now includes the scheduler plan.
  - Evidence:
    - `cargo test -p onecontext-context-engine --test scheduler_scaffold -- --nocapture`
    - `tests/scheduler_scaffold.rs`
    - `src/scheduler.rs` unit tests for ready phases, missing dependencies,
      cycles, and all-complete status.

- [x] **2c. Runtime primitive scaffold exists**
  - Rust has stable scaffolds for source packet materialization, fanout
    expansion, condition evaluation, artifact storage, runtime turn
    descriptors, scheduler receipts, run state, and wiki page-write/publish
    proof.
  - The scaffold does not count JSONL audit mirrors as authoritative Agent Mail
    delivery.
  - Evidence:
    - `cargo test -p onecontext-context-engine`
    - `tests/runtime_primitives_scaffold.rs`
    - `tests/source_packets.rs`
    - `tests/artifacts.rs`
    - `tests/conditions.rs`
    - `tests/runtime_executor_scaffold.rs`
    - `tests/receipt_bridge.rs`
    - `tests/wiki_publish_run_state.rs`

- [x] **3. Historical behavior captured**
  - The old procedure is captured in the orchestration spec and the active Rust
    planner/scheduler tests cover the shipped packet/job policy.
  - The active tree does not require a Python source checkout for parity.
  - Evidence:
    - `docs/context-engine-orchestration-spec.md`
    - `cargo test -p onecontext-context-engine`

- [x] **4. Harness request builder**
  - Rust composes one job + one agent + prompts + mail context into the expected
    harness turn request.
  - Evidence: test asserts birth/start-turn/context-injection/final-message
    requirements without starting a model.
  - Current evidence:
    - `cargo test -p onecontext-context-engine`
    - `builds_codex_app_server_harness_turn_request_for_for_you_curator`
    - `harness_request_rejects_unknown_jobs`
    - The request includes the `codex-app-server` harness, `onecontext-codex-adapter`
      command, `gpt-5.5` / `xhigh`, preserved prompt stack, Agent Mail appendix,
      `final-message.md`, talk/mail delivery, and required harness receipts.

- [x] **4a. Swift product entrypoints use Context Engine**
  - The menu bar Refresh Wiki action calls daemon RPC `context_engine.update_wiki`.
  - Manual refresh uses `execute_agents=true`, `max_concurrent=5`, and
    `source_window_days=3` for the quick demo path.
  - The daemon exposes `context_engine.update_wiki`, queues the run, calls
    `ContextEngineProcessClient.updateWiki`, and publishes the wiki afterward.
  - The automatic daemon timer runs the same Context Engine path every 12 hours:
    first pass as recent-first backfill, later passes as incremental.
  - Evidence:
    - `swift build --package-path macos --product 1contextd`
    - `swift build --package-path macos --product OneContextMenuBar`
    - `swift test --package-path macos --filter OneContextAgentRuntimeTests`

- [x] **5. Fake executor gate**
  - Fake harness/adapter/wiki-core prove Rust refuses to mark a turn done unless
    final message, talk/mail receipt, and harness completion are present.
  - Evidence: fake-executor tests covering success and missing-receipt failure.
  - Current evidence:
    - `cargo test -p onecontext-context-engine`
    - `harness_completion_requires_all_required_receipts`
    - `harness_completion_rejects_missing_final_message`
    - `harness_completion_rejects_missing_talk_or_mail_receipt`
    - `harness_completion_rejects_codex_exit_only`

- [x] **6. Mail-first trace**
  - Every update appends to `context-engine/live/mail/threads/wiki-company.jsonl`.
  - No `context-engine/live/runs` folder is created.
  - Evidence: Context Engine tests and smoke output.
  - Current evidence:
    - `cargo test -p onecontext-context-engine`
    - Temp-runtime CLI smoke verified `mail/threads/wiki-company.jsonl` exists
      and `context-engine/live/runs` does not.

- [x] **7. Dry-run local update**
  - Real Perception metadata, no model calls.
  - Output lists planned packets, agents, routes, and publish intent.
  - Evidence: CLI dry-run command and captured JSON receipt.
  - Current evidence:
    - `scripts/build-managed-postgres-runtime.sh`
    - `scripts/release-train.sh build --channel dev`
    - `cargo build -p onecontext-memory-db --bin onecontext-memoryd`
    - `cargo run -q -p onecontext-context-engine -- update-wiki --root runtime/1Context --run-id dry-run-real-metadata-1m --trigger menu.update_wiki --max-concurrent 5 --source-window-days 3 --mode dry-run --no-agents --json`
    - `source_metadata.status = ok`, `object_count = 37210`,
      `bucket_count = 3891`, `active_day_count = 3`,
      `active_hour_count = 32`.
    - Planner produced 99 total packets, selected 20 recent packets,
      16 harness previews, 36 routes, and a wiki publish intent.

- [ ] **8. Tiny live run**
  - One scribe and one editor/curator run through the harness and Codex
    app-server path.
  - Evidence: harness birth certificates, mail/talk receipts, and visible wiki
    talk entries.

- [ ] **9. Demo run**
  - Last 3 days, max concurrent 5.
  - The wiki visibly changes, stale claims are removed when contradicted, and
    the mail thread records what the company did.
  - Evidence: installed dev app refresh, Context Engine JSON, wiki page/talk
    output, and no fake planned-only completion.

## Current Scope Rule

Keep `packs/wiki-company-v1` as the company definition and
`orchestrators/wiki-company-orchestrator-v1` as the run policy. Rust code lives
in stable modules under `crates/onecontext-context-engine/src/`.

Do not reintroduce a filesystem `runs/` hierarchy while the chosen history
direction is Agent Mail first and Postgres/Timescale for rich execution history.
