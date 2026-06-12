# Retired Files

This ledger tracks files moved out of the active product tree during cleanup.

## 2026-05-27 Cleanup

- `docs/assets/attention-capture-mockup/` -> `recycle-bin/20260527/docs/assets/attention-capture-mockup/`
  - Generated attention capture screenshots, video, JSON snapshots, fixture session data, and proof outputs.
- `docs/attention-capture-mockup.html` -> `recycle-bin/20260527/docs/attention-capture-mockup.html`
  - Static mockup page tied to the retired generated attention asset pack.
- `docs/attention-runner-validation.md` -> `recycle-bin/20260527/docs/attention-runner-validation.md`
  - Fixture-proof validation loop that treated generated attention output as release evidence.
- `crates/onecontext-attention-runner/tests/current_fixture_smoke.rs` -> `recycle-bin/20260527/crates/onecontext-attention-runner/tests/current_fixture_smoke.rs`
  - Integration smoke that depended on the retired checked-in generated attention fixture.
- `crates/onecontext-attention-runner/tests/candidate_ingestion.rs` -> `recycle-bin/20260527/crates/onecontext-attention-runner/tests/candidate_ingestion.rs`
  - Candidate ingestion checks tied to generated attention mockup paths.
- `demos/agent-mail-triad/static/fixtures/latest.json` was removed from Git and ignored.
  - The triad generator may recreate it locally as disposable demo output.
- `demos/peekaboo-evidence-wall/` -> `recycle-bin/20260527/demos/peekaboo-evidence-wall/`
  - Experimental demo/lab surface with ignored evidence output.
- `release/tools/caddy/darwin-arm64/caddy-v2.11.2-darwin-arm64.tar.gz` -> `recycle-bin/20260527/release/tools/caddy/darwin-arm64/caddy-v2.11.2-darwin-arm64.tar.gz`
  - Vendored release binary replaced by checksum-pinned download/cache logic.

## 2026-05-28 Cleanup

| Original path | Recycle-bin path | Reason |
| --- | --- | --- |
| `scripts/generate-agent-mail-triad-demo.mjs` | `recycle-bin/20260528/scripts/generate-agent-mail-triad-demo.mjs` | Generated dogfood demo snapshot writer; generated fixtures are no longer an active contract. |
| `scripts/test-agent-harness-boundary-dogfood.mjs` | `recycle-bin/20260528/scripts/test-agent-harness-boundary-dogfood.mjs` | MJS dogfood harness for stale boundary proof surface. |
| `scripts/test-agent-mail-dogfood.mjs` | `recycle-bin/20260528/scripts/test-agent-mail-dogfood.mjs` | MJS mail dogfood harness retired in favor of current typed product tests. |
| `scripts/test-codex-adapter-live-server-dogfood.mjs` | `recycle-bin/20260528/scripts/test-codex-adapter-live-server-dogfood.mjs` | Live dogfood proof runner generated evidence outside the launch product contract. |
| `scripts/test-codex-adapter-live-mail-flow.mjs` | `recycle-bin/20260528/scripts/test-codex-adapter-live-mail-flow.mjs` | Live mail dogfood bridge retired with the dogfood MJS cluster. |
| `scripts/test-codex-adapter-harness-dogfood.mjs` | `recycle-bin/20260528/scripts/test-codex-adapter-harness-dogfood.mjs` | Adapter harness dogfood proof moved out of active scripts. |
| `scripts/test-wiki-core-dogfood.mjs` | `recycle-bin/20260528/scripts/test-wiki-core-dogfood.mjs` | Wiki dogfood proof runner retired from active scripts. |
| `scripts/verify-agent-mail-triad-mcp-realism.mjs` | `recycle-bin/20260528/scripts/verify-agent-mail-triad-mcp-realism.mjs` | Generated demo realism verifier retired with the demo fixture. |
| `scripts/onecontext-wiki-mcp-server.mjs` | `recycle-bin/20260528/scripts/onecontext-wiki-mcp-server.mjs` | Demo-only MCP shim retired with the MJS dogfood cluster. |
| `demos/agent-mail-triad` | `recycle-bin/20260528/demos/agent-mail-triad` | Generated static dogfood demo and checked-in `latest.json` fixture retired from active demos. |
| `docs/agent-harness-boundary-dogfood.md` | `recycle-bin/20260528/docs/agent-harness-boundary-dogfood.md` | Dogfood command wrapper for retired harness. |
| `docs/agent-harness-implementation-scaffold.md` | `recycle-bin/20260528/docs/agent-harness-implementation-scaffold.md` | Scaffold-era implementation note retired under cleanup policy. |
| `docs/codex-adapter-harness-dogfood.md` | `recycle-bin/20260528/docs/codex-adapter-harness-dogfood.md` | Dogfood command wrapper for retired adapter harness. |
| `docs/codex-adapter-live-mail-flow.md` | `recycle-bin/20260528/docs/codex-adapter-live-mail-flow.md` | Dogfood command wrapper for retired live mail flow script. |
| `docs/codex-adapter-live-server-dogfood.md` | `recycle-bin/20260528/docs/codex-adapter-live-server-dogfood.md` | Dogfood command wrapper for retired live-server script. |
| `docs/goals/1context-agent-mail-dogfood-goal.md` | `recycle-bin/20260528/docs/goals/1context-agent-mail-dogfood-goal.md` | Historical dogfood harness milestone retired with the harness. |
| `docs/attention-bundle-migration-notes.md` | `recycle-bin/20260528/docs/attention-bundle-migration-notes.md` | Migration-era attention bundle notes retired; READY bundle validation is now the active contract. |
| `scripts/test-capture-audit-regenerated-bundle.sh` | `recycle-bin/20260528/scripts/test-capture-audit-regenerated-bundle.sh` | Saved-audit regeneration proof wrapper with old capability fallback behavior and seeded synthetic media; current bundle contracts live in Rust tests. |
| `scripts/benchmark-capture-bundle-large-windows.sh` | `recycle-bin/20260528/scripts/benchmark-capture-bundle-large-windows.sh` | Synthetic capture bundle performance wrapper; spool index and bracketing behavior are covered by capture-core tests. |
| `scripts/benchmark-capture-bundle-media-export.sh` | `recycle-bin/20260528/scripts/benchmark-capture-bundle-media-export.sh` | Saved-audit media export timing wrapper; media index and READY bundle behavior are covered by capture-core tests. |
| `scripts/test-capture-dashboard-metadata.sh` | `recycle-bin/20260528/scripts/test-capture-dashboard-metadata.sh` | Live installed-app permission evidence wrapper; capture dashboard metadata parsing and permission-derived fields are covered by typed Rust tests. |
| `scripts/launch-attention-dashboard.sh` | `recycle-bin/20260528/scripts/launch-attention-dashboard.sh` | Local app-bundle scaffold for launching the attention dashboard; the dashboard binary accepts sessions directly and typed tests cover session contracts. |
| `scripts/benchmark-memory-backfill.sh` | `recycle-bin/20260528/scripts/benchmark-memory-backfill.sh` | Memory DB backfill benchmark retired; current schema and writer behavior are covered by typed `onecontext-memory-db` tests. |
| `scripts/summarize-memory-benchmarks.sh` | `recycle-bin/20260528/scripts/summarize-memory-benchmarks.sh` | Summarizer retired with the backfill benchmark result format. |
| `scripts/test-memory-local-web-e2e.sh` | `recycle-bin/20260528/scripts/test-memory-local-web-e2e.sh` | Manual local-web memory viewer harness retired from active scripts; local-web contracts remain in typed macOS tests. |
| `scripts/test-installed-app-permission-capabilities.sh` | `recycle-bin/20260528/scripts/test-installed-app-permission-capabilities.sh` | Non-live installed-app package checker duplicated permission proof surface; keep one timestamped dev live TCC probe path. |
| `scripts/test-browser-extension-native-host.sh` | `recycle-bin/20260528/scripts/test-browser-extension-native-host.sh` | Synthetic native-host smoke wrapper was weak proof and not a current product gate; browser extension proof remains dev-scoped. |
| `scripts/test-wiki-runtime-defaults-scenarios.sh` | `recycle-bin/20260528/scripts/test-wiki-runtime-defaults-scenarios.sh` | Opt-in RuntimeDefaults upgrade/backfill and legacy talk-alias scenario wrapper retired; current contracts live in Swift/wiki-engine tests and the compact wiki smoke script. |
| `docs/goals/1context-portable-wiki-core-goal.md` | `recycle-bin/20260528/docs/goals/1context-portable-wiki-core-goal.md` | Superseded historical wiki-core milestone retired from active goals. |
| `docs/goals/1context-wiki-memory-runtime-v0-goal.md` | `recycle-bin/20260528/docs/goals/1context-wiki-memory-runtime-v0-goal.md` | Superseded wiki runtime milestone retired; current contracts and production-shape proof own the active surface. |
| `docs/goals/evidence/wiki-memory-runtime-bloat-inventory-2026-05-14.md` | `recycle-bin/20260528/docs/goals/evidence/wiki-memory-runtime-bloat-inventory-2026-05-14.md` | Historical bloat inventory tied to the retired wiki runtime milestone. |
| `scripts/test-release-train.sh` | `recycle-bin/20260528/scripts/test-release-train.sh` | Fixture-heavy release policy shell test retired; release runner typed tests now own manifest, appcast, dry-run, and command-surface coverage. |
| `scripts/test-launch-agent-package.sh` | `recycle-bin/20260528/scripts/test-launch-agent-package.sh` | Packaged-app smoke wrapper retired from active scripts; build validation remains behind `scripts/release-train.sh` and typed macOS/runtime tests. |
| `docs/goals/1context-professional-release-runner-goal.md` | `recycle-bin/20260528/docs/goals/1context-professional-release-runner-goal.md` | Historical release runner milestone retired; current release contracts live in `release/runner` tests and the macOS release runbook. |
| `docs/goals/1context-release-factory-goal.md` | `recycle-bin/20260528/docs/goals/1context-release-factory-goal.md` | Historical release factory proof journal retired from active goals after shell proof wrappers moved into typed release runner tests. |
| `docs/goals/evidence/delete-bloat-audit-2026-05-13.md` | `recycle-bin/20260528/docs/goals/evidence/delete-bloat-audit-2026-05-13.md` | Historical bloat audit referenced retired shell proof paths; cleanup guard and current typed tests now own active verification. |

## 2026-06-11 Docs Sweep

| Original path | Disposition | Reason |
| --- | --- | --- |
| `docs/1context-codex-adapter-spec.md` | deleted | Codex adapter crate deleted; spec retired with it. |
| `docs/codex-hook-control-spec.md` | deleted | Hook/wake control plane retired with the Codex adapter. |
| `docs/context-engine-orchestration-spec.md` | deleted | TOML pack/policy orchestration layer ripped out. |
| `docs/context-engine-orchestrator-port-checklist.md` | deleted | Port checklist for the removed orchestration layer. |
| `docs/agent-tool-gateway.md` | deleted | Gateway never shipped; zero code references. |
| `docs/attention-dashboard-skeleton-schema.md` | deleted | Contract for the deleted attention-dashboard crate. |
| `docs/attention-source-arbitration-scaffold.md` | deleted | Scaffold for the deleted attention prototype crates. |
| `docs/schemas/attention-dashboard-session.schema.json` | deleted | Session schema for the deleted attention dashboard. |
| `docs/cleanup-deletion-program.md` | deleted | Cleanup program complete; superseded by the FSM-DSL rip-out. |
| `docs/cleanup-policy.md` | deleted | Superseded with the cleanup program. |
| `docs/cleanup-verification-matrix.md` | deleted | Primary gate (cleanup-guard) was deleted. |
| `docs/coding-agent-cleanup-questions.md` | deleted | Working notes for the finished cleanup program. |
| `docs/handrolled-legacy-cleanup-plan.md` | deleted | Plan executed; surfaces removed. |
| `docs/low-end-loc-cleanup-plan.md` | deleted | Plan executed; surfaces removed. |
| `docs/public-export-workflow.md` | deleted | Private-to-public export pipeline retired. |
| `docs/goals/1context-codex-adapter-implementation-goal.md` | deleted | Goal for the deleted Codex adapter crate. |
| `docs/review-packets/codex-agent-harness-mail-flow-20260527/` | deleted | Review packet for the retired adapter mail flow. |
| `docs/screencap-attention-algorithm-notes.md` | `docs/archive/screencap-attention-algorithm-notes.md` | Attention algorithm design notes kept for provenance. |
