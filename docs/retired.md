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
