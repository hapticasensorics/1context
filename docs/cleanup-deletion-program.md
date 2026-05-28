# Cleanup Deletion Program

This program turns the cleanup audits into gated, PR-sized deletion slices.
Read `docs/cleanup-policy.md` before editing and use
`docs/coding-agent-cleanup-questions.md` when launching investigation agents.

## Execution Order

| Slice | Goal | Primary Gate |
| --- | --- | --- |
| 0 | Policy, question packet, stale-reference guard, verification matrix. | `npm run cleanup:guard` reports current state. |
| 1 | Script inventory and first deletion pass. | Every script has keep/delete/replace classification. |
| 2 | Dogfood MJS cluster deletion or replacement. | `scripts/` moves toward 2,860 LOC and no deleted references remain. |
| 3 | Memory DB migration deletion. | No migration runner, migration SQL, repair path, dirty recovery, or migration-only tests. |
| 4 | Wiki route and parser cleanup. | Canonical routes only; no legacy `.talk` aliases or fallback templates. |
| 5 | Capture/attention READY contract cleanup. | Missing required bundle fields fail as typed errors. |
| 6 | macOS process and CLI cleanup. | One process runner, typed CLI, no silent process/protocol fallback. |
| 7 | Docs/tests final sweep and size gate enforcement. | Final matrix in `docs/cleanup-verification-matrix.md` passes. |

## Slice Rules

- Delete first, then type what survives.
- Prefer removing references over preserving a stale path.
- Do not move every dogfood behavior into tests. Keep only current-contract
  assertions.
- Do not create a grand shared schema crate until at least two active surfaces
  need the same current contract.
- Treat generated artifacts as outputs, not product inputs.

## First Deletion Targets

The first deletion pass should focus on `scripts/`, because it concentrates
dogfood proof generation, duplicated command runners, hand-rolled JSON parsing,
and stale evidence output.

Prioritize:

- `scripts/generate-agent-mail-triad-demo.mjs`
- `scripts/test-agent-harness-boundary-dogfood.mjs`
- `scripts/test-agent-mail-dogfood.mjs`
- `scripts/test-codex-adapter-live-server-dogfood.mjs`
- `scripts/test-codex-adapter-live-mail-flow.mjs`
- `scripts/test-codex-adapter-harness-dogfood.mjs`
- `scripts/test-wiki-core-dogfood.mjs`
- `scripts/verify-agent-mail-triad-mcp-realism.mjs`
- `scripts/onecontext-wiki-mcp-server.mjs`

Keep only thin operator shims unless a script has unique current-contract
coverage that cannot yet live in Rust, Swift, Playwright, or release-runner
tests.

## Final Product Nucleus

The low-end cleanup target keeps:

- signed macOS app
- setup/readiness UI
- daemon health/status
- local web/wiki opens
- CLI `version`, `diagnose`, and `uninstall`
- release train build/install/proof path
- exactly one current wiki renderer/runtime

Everything else is devtool, lab, sample pack, archive, or deleted unless it is
explicitly readmitted after the LOC gates pass.
