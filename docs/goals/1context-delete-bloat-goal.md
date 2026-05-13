---
title: 1Context Delete Bloat Goal
slug: 1context-delete-bloat-goal
section: development
access: private
summary: "A destructive greenfield cleanup goal for deleting old release paths, compatibility shims, source-checkout packaging, stale docs, and product test hooks before the next release train."
status: development-doc
last_updated: 2026-05-13
former_wiki_route: /goal/delete-bloat
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Delete Bloat Goal

## Priority

This is the next active release-train priority. Cleanup and deletion come before
new release features, new compatibility paths, new proof UX, or additional docs
polish. If a piece of old code exists only to preserve a pre-Sparkle workflow,
support an obsolete harness, tolerate old local habits, or keep a redundant
operator entrypoint alive, the default action is to delete it.

Minimal cleanup is not acceptable for this goal. The goal is a thorough
greenfield deletion pass that changes the shape of the app, not a few script
renames or doc edits. The release train should get smaller before it gets more
capable.

The quantitative target is to reduce the shipped app code-line footprint by at
least 60% from the pre-cleanup baseline. The exact measurement command must be
checked into this goal before closure, and the same command must be used for
the baseline and final proof.

## Goal

Delete the old world before building more release machinery on top of it.
The release-lockdown work proved the Sparkle path, updater policy, real-Mac
proof, and app setup flywheel. This goal is the follow-on cleanup: remove
backwards-compatibility paths, old scripts, stale docs, product test hooks, and
source-checkout packaging assumptions even when that temporarily breaks old
developer habits.

The rule is aggressive and simple: delete first, then re-add only the pieces
that the greenfield release train proves are still needed.

## Operating Doctrine

- No backwards compatibility with pre-Sparkle release flows.
- No compatibility shims for old update-policy files, old proof scripts, old
  release wrappers, old wiki goal routes, or old package entrypoints.
- One release command: `scripts/release-train.sh`.
- One release source of truth: `release/release.toml`.
- One proof shape: normalized, redacted evidence owned by the release train.
- One shipped app shape: signed app bundle plus explicit runtime artifacts, not
  a copied source checkout with cleanup after the fact.
- User-facing app behavior stays simple. Operator detail belongs in logs,
  diagnostics, proof JSON, and release evidence.

## Done When

- Docs mention exactly one release command for production release work:
  `scripts/release-train.sh`.
- Workflows invoke production package, publish, proof, audit, and bless through
  `scripts/release-train.sh`.
- `release/release.toml` owns mandatory/optional update policy, proof matrix,
  appcast expectations, release notes policy, redaction policy, and release
  facts without `release/update-policy.*`.
- Old release wrappers, old proof request scripts, old release-lockdown
  diagnostic scripts, and old compatibility env knobs are deleted or moved
  behind clearly internal release-train implementation boundaries.
- The shipped app no longer exposes product test backdoors, fixture launch
  modes, high-port local-web product mode, source-checkout discovery, or public
  raw-diagnostic flags.
- The app bundle is built from allowlisted runtime artifacts instead of copying
  the entire `memory-core` source tree and pruning it afterward.
- Tracked runtime state, generated wiki source outputs, stale policy assets,
  stale release docs, and dead code are removed.
- The shipped app code-line footprint is reduced by at least 60% using the
  agreed repeatable measurement.
- Replacement tests prove the greenfield behavior directly instead of grepping
  for old implementation strings.
- A protected release can validate, package, publish, prove, audit, and bless
  from a clean tagged tree using only the new release train.

## Checklist

### 1. Baseline And Stance

- [x] The release-lockdown goal is complete enough to make Sparkle the release
  baseline rather than an experiment.
- [x] Backwards compatibility with old release paths is explicitly out of scope.
- [x] The cleanup target is destructive: it is acceptable for old local habits,
  stale scripts, and legacy docs to stop working.
- [x] Cleanup and deletion are the active priority before additional release
  features or compatibility work.
- [x] Minimal cleanup is explicitly out of scope; the goal requires a thorough
  deletion pass.
- [x] Capture a deletion inventory before the first code removal, including
  script paths, env knobs, docs, app hooks, generated state, and tests.
- [x] Define and record the repeatable shipped app code-line measurement used
  for the 60% reduction target.
- [x] Capture the pre-cleanup app code-line baseline with that measurement.
- [x] Choose the first deletion tranche and update this goal with evidence after
  it lands.

Measurement:

- Command: `scripts/measure-shipped-app-lines.sh`
- Scope: `macos/Sources` plus source-equivalent code/config files currently
  bundled by the app's `memory-core` runtime copy, excluding build caches,
  virtualenvs, node modules, runtime lake data, and generated markdown that the
  current package script already deletes.
- Baseline captured on 2026-05-13: 218 files and 61,309 nonblank lines.
- 60% reduction target: final measurement must be 24,523 nonblank lines or
  fewer.
- Current measurement after deleting generated outputs, agent integration,
  shipped memory-core packaging, public debug/permissions surfaces, install
  env hooks, the runtime-support shim, structural grep tests, and the dead chat
  API shell, and dead helpers: 17 files and 8,362 nonblank lines, a
  52,947-line / 86.36%
  reduction from the baseline.
  This passes the 24,523-line 60% reduction target.

First deletion tranche, 2026-05-13: old update-policy source, old lockdown
evidence scripts, and the grep-based upgrade-path compatibility test. The next
tranche is to make production packaging/proof enter through
`scripts/release-train.sh` only.

### 2. Single Release Truth

- [x] Delete `release/update-policy.toml`.
- [x] Delete `release/update-policy.schema.json`.
- [x] Delete `scripts/update-policy.py`.
- [x] Delete `scripts/check-update-policy.sh` or replace it with a
  `scripts/release-train.sh validate` call site only.
- [x] Delete `scripts/test-update-policy.sh` after equivalent manifest tests
  exist.
- [x] Move mandatory/optional release policy, appcast validation, Sparkle env
  derivation, release notes policy, and post-install message policy into
  `release/release.toml` plus `scripts/release-manifest.py`.
- [x] Remove the release-manifest validation dependency on
  `docs/goals/1context-release-lockdown-goal.md`; docs must not be release
  truth.
- [x] Decide whether `release/release.schema.json` is enforced or deleted. If
  kept, schema validation must run in the normal release train.
- [x] Delete the fallback homegrown TOML parser in `scripts/release-manifest.py`
  and require modern Python `tomllib` on release runners.
- [x] Add a negative check proving no production release code reads
  `release/update-policy.*`.

Evidence, 2026-05-13: update policy data now lives in `release/release.toml`;
`scripts/release-manifest.py` exports the Sparkle/update UI environment and
validates mandatory/optional appcast cases. Deleted files:
`release/update-policy.toml`, `release/update-policy.schema.json`,
`scripts/update-policy.py`, `scripts/check-update-policy.sh`, and
`scripts/test-update-policy.sh`. Proof:
`scripts/release-manifest.py validate`, `scripts/test-release-train.sh`,
`git diff --check`, and `rg` over production release code for old
`update-policy` paths.

Evidence, 2026-05-13: deleted documentation-only
`release/release.schema.json`; the enforced release manifest contract remains
`scripts/release-manifest.py validate` and `scripts/release-train.sh`.

### 3. One Production Release Command

- [x] Delete `scripts/package-macos-production-release.sh`.
- [x] Delete or demote `scripts/package-macos-release.sh` so it cannot be used
  as a production release path.
- [x] Move production package/sign/notarize/appcast behavior into
  `scripts/release-train.sh package`.
- [x] Keep any unsigned local app build path clearly named as smoke tooling,
  not as a release path.
- [x] Delete production uses of release bypass flags such as
  `ALLOW_UNNOTARIZED` and `NOTARIZE` outside smoke-only tooling.
- [x] Update `.github/workflows/release.yml` so secrets are scoped to the
  signing/publish steps, not validation.
- [x] Add an audit proving CI production release steps call only
  `scripts/release-train.sh`.

Evidence, 2026-05-13: production package/sign/notarize/appcast logic moved into
`scripts/release-train.sh package`. Deleted:
`scripts/package-macos-production-release.sh` and
`scripts/package-macos-release.sh`. Added `scripts/package-macos-smoke.sh` for
local/CI unsigned package smoke only. CI now uses the smoke script; the protected
release workflow validates before credential-bearing steps and production
release steps enter through `scripts/release-train.sh`. Proof:
`scripts/release-manifest.py validate`, `scripts/test-release-train.sh`,
`actionlint`, `git diff --check`, and `rg` for old package wrappers and bypass
env.

### 4. Proof And Evidence Consolidation

- [x] Delete `scripts/request-release-proof.sh`.
- [x] Delete `scripts/test-release-proof-request.sh` after proof request
  coverage moves into `scripts/test-release-train.sh`.
- [ ] Delete `scripts/audit-github-release-assets.sh` after asset checks move
  into `scripts/release-train.sh audit` or `scripts/release-manifest.py`.
- [ ] Change `.github/workflows/self-hosted-mac-update-proof.yml` to enter
  through `scripts/release-train.sh prove`.
- [ ] Absorb or move `scripts/self-hosted-update-proof.sh` under an internal
  implementation path that is not documented as a release command.
- [x] Delete `scripts/collect-release-lockdown-evidence.sh`.
- [x] Delete `scripts/classify-release-lockdown-diagnostics.py`.
- [x] Delete `scripts/test-release-lockdown-diagnostics.sh`.
- [x] Delete `scripts/test-upgrade-paths.sh`, the grep-based compatibility test
  that preserved old lockdown evidence scripts.
- [x] Remove fake or single-case proof generation from `release-train.sh prove`;
  every matrix case in `release/release.toml` must be real, skipped with an
  explicit policy reason, or absent from the manifest.
- [x] Make evidence immutable: producers write proof JSON, redactors write
  redaction reports, and auditors verify without mutating proof files.

Evidence, 2026-05-13: old lockdown evidence scripts and the old upgrade-path
grep test were deleted, and CI no longer calls `scripts/test-upgrade-paths.sh`.
Proof: `scripts/release-manifest.py validate`, `scripts/test-release-train.sh`,
and `git diff --check`.

Evidence, 2026-05-13: `scripts/release-train.sh prove` now owns proof request
construction, dry-run output, dispatch/watch/download behavior, and evidence
redaction entry. Deleted `scripts/request-release-proof.sh` and
`scripts/test-release-proof-request.sh`. The prior synthetic
`mandatory_automatic_success.json` proof was removed; bless now still requires
real matrix proof JSON. Proof: `scripts/test-release-train.sh`.

Evidence, 2026-05-13: `scripts/audit-evidence-redaction.sh` no longer edits
`proof-results/*.json`; it only writes `redaction-report.json` and fails on
forbidden evidence. Proof: `scripts/test-release-train.sh`.

### 5. Shipped App Backdoor Removal

- [ ] Delete `ONECONTEXT_SMOKE_FIXTURE` from production app build and launch
  behavior.
- [ ] Delete `ONECONTEXT_SHOW_SETUP_ON_LAUNCH`.
- [x] Delete `ONECONTEXT_VERSION_OVERRIDE`.
- [ ] Delete product path overrides in `Paths.swift`.
- [x] Delete install prompt bypass and destination override hooks in
  `AppInstall.swift`.
- [ ] Delete daemon/runtime override gates that are not needed by production.
- [x] Delete agent override gates that only exist for legacy harnesses.
- [ ] Delete local-web setup path overrides that make the product depend on
  source-checkout layout.
- [ ] Delete Sparkle retry/test env overrides once retry behavior is covered by
  reducer tests and release-train proof.
- [x] Delete public `--no-redact` diagnostic output from the product CLI.
- [ ] Replace any remaining product test hooks with internal harness fixtures
  outside the shipped app surface.

Evidence, 2026-05-13: removed `ONECONTEXT_VERSION_OVERRIDE`,
`ONECONTEXT_SKIP_APP_INSTALL_PROMPT`, `ONECONTEXT_APP_INSTALL_DESTINATION`,
`ONECONTEXT_TEST_APP_BUNDLE_PATH`, `ONECONTEXT_APP_TRASH_DESTINATION`, and
`ONECONTEXT_ALLOW_NON_APPLICATIONS_APP_TRASH` from shipped app code. Public
`diagnose --no-redact`, `status --debug`, and `debug` are now rejected by
`scripts/test.sh`. App install/trash tests inject fixture paths directly
instead of environment variables. Deleted the old install-prompt and local
install-move harnesses. Proof: `swift test --package-path macos`,
`./scripts/test.sh`, `./scripts/package-macos-smoke.sh`,
`./scripts/test-launch-agent-package.sh`, `actionlint`, `bash -n`,
`git diff --check`, and negative `rg` over the deleted env names.

### 6. Local Web And Permissions Cleanup

- [ ] Delete high-port HTTP as a shipped product mode. Public 1Context should
  have one local-web mode: portless local HTTPS.
- [ ] Remove setup readiness bypasses tied to high-port HTTP.
- [x] Remove future sensitive permission scaffolding from shipped setup until
  the corresponding capture feature is actually shipping.
- [x] Remove `permissions --all` behavior if it exposes unshipped permissions.
- [ ] Delete updater release-notes and post-install-popup branches if the final
  product policy remains "no release notes, no post-update popup by default."

Evidence, 2026-05-13: deleted the `OneContextPermissions` target and tests,
removed future Screen Recording/Accessibility rows from setup, and removed the
public `permissions` CLI command entirely. Current Local Wiki Access status is
reported through setup and redacted `diagnose`.

### 7. First-Party Agent Integration Deletion

- [x] Delete `OneContextAgent` if agent hook installation is not a proven
  `0.1.x` product surface.
- [x] Delete Claude/Codex config mutation and hook/statusline installation from
  shipped app behavior.
- [x] Delete agent env override gates, tests, CLI commands, README install
  instructions, and uninstall cleanup paths for agent integrations.
- [x] Add a negative check proving the shipped app no longer edits first-party
  agent configs.

Evidence, 2026-05-13: removed the `OneContextAgent` target, source file, tests,
CLI command surface, uninstall cleanup, docs references, and harness envs. Proof:
negative `rg` over shipped app/test/script/docs surfaces for `OneContextAgent`,
`AgentConfigStore`, `agent hook`, `agent statusline`, `ONECONTEXT_AGENT`,
`CLAUDE_SETTINGS`, and `CODEX_CONFIG`; `swift test --package-path macos` passed.

### 8. CLI Collapse

- [ ] Collapse the public CLI to only the commands the product truly needs:
  `version`, redacted `diagnose`, `uninstall`, and possibly `wiki local-url`.
- [ ] Delete public lifecycle commands if they duplicate menu or launch-agent
  behavior: `start`, `stop`, `restart`, `quit`, and `logs`.
- [x] Delete public debug commands, especially raw debug paths and
  `debug --no-redact`.
- [x] Delete public `permissions`, `agent`, and `memory-core` command surfaces
  unless they are part of the current product contract.
- [ ] Move any necessary operator-only behavior into release/proof tooling, not
  the shipped app CLI.

Evidence, 2026-05-13: removed the standalone `debug` command, removed
`--debug` from lifecycle/status commands, removed `diagnose --no-redact`,
removed `permissions`, and previously removed `agent` and `memory-core`.
Operator evidence scripts now call redacted `diagnose` instead of public debug
flags. Proof: `scripts/test.sh` negative checks and negative `rg`.

### 9. Wiki Chat And Provider Deletion

- [x] Delete unshipped wiki chat/provider API routes if chat is not shipping for
  real in this release train.
- [x] Delete disabled chat responses and static placeholder JSON from the Swift
  local API.
- [x] Delete matching frontend chat/provider/reset calls from shipped wiki
  resources.
- [x] Delete chat docs and tests that preserve a nonfunctional UI surface.
- [x] Add a package smoke assertion proving the shipped wiki has no dead chat
  controls or provider calls.

Evidence, 2026-05-13: removed `/api/wiki/chat/*` routes from the shipped Swift
local API, removed placeholder `api/wiki/chat/config.json` publication, updated
docs to stop describing chat as a product API, and changed tests to assert
unshipped chat routes return 404. Package smoke now fails if packaged resources
contain `/api/wiki/chat`, `chat_available`, `ai-provider`, or "Chat about this
page". Proof: `swift test --package-path macos`, `./scripts/test.sh`,
`./scripts/package-macos-smoke.sh`, and `./scripts/test-launch-agent-package.sh`.

### 10. Memory-Core Packaging Cleanup

- [x] Stop copying the whole `memory-core` source checkout into the app bundle.
- [x] Replace copy-then-scrub packaging with explicit app-bundle resources.
- [x] Delete the shipped `OneContextMemoryCore` target if the app can ship a
  smaller runtime artifact instead.
- [x] Delete `memory-core/bin/1context-memory-core` from the shipped runtime if
  release packaging no longer runs source-checkout Python.
- [x] Delete runtime dependency installation from the shipped app, including
  user-machine `npm ci` setup.
- [x] Delete shipped `uv run --project "$ROOT"` source-checkout launcher paths.
- [x] Delete runtime publishing through `WikiSitePublisher` from the installed
  daemon if the release can ship one prebuilt public wiki artifact.
- [x] Split or trim the broad memory CLI so the public bundle exposes only the
  contract surface it needs.
- [x] Delete source-tree seed crawlers that scan bundled
  `memory-core/wiki/**/generated`.
- [x] Replace source-tree wiki scanning with the app-owned local wiki shell.
- [x] Remove tracked runtime state under `memory-core/memory/runtime/**`.
- [x] Remove tracked generated wiki outputs under `memory-core/wiki/generated/**`
  and `memory-core/wiki/menu/**/generated/**`.
- [x] Keep renderer and memory-engine code as build-time tooling or separate
  source modules only when they are not copied into the shipped app.

Evidence, 2026-05-13: removed all tracked files under
`memory-core/memory/runtime/**`, `memory-core/wiki/generated/**`, and
`memory-core/wiki/menu/**/generated/**`. Proof: `git ls-files` for those globs
returns no tracked files, `scripts/measure-shipped-app-lines.sh` reports 51,989
nonblank lines, and `scripts/test-release-train.sh` still passes.

Evidence, 2026-05-13: removed `OneContextMemoryCore`,
`MemoryCoreAdapter`, `MemoryCoreSetup`, `WikiSitePublisher`, the memory-core CLI
command, the memory-core contract fixture/test, `docs/memory-core-contract.md`,
and the app-bundle memory-core copy block. The daemon now prepares the small
app-owned wiki shell directly; the package smoke asserts
`Contents/Resources/memory-core` is absent. Proof: `swift test --package-path
macos`, `./scripts/test.sh`, `./scripts/package-macos-smoke.sh`,
`./scripts/test-launch-agent-package.sh`, `./scripts/release-manifest.py
validate`, `./scripts/test-release-train.sh`, `bash -n`, and `git diff --check`
passed. `scripts/measure-shipped-app-lines.sh` reports 8,796 nonblank
shipped-app lines, an 85.65% reduction from baseline.

Evidence, 2026-05-13: after the public CLI/env-hook, chat/API, and dead-helper
cleanup, `scripts/measure-shipped-app-lines.sh` reports 8,362 nonblank
shipped-app lines, an 86.36% reduction from baseline.

### 11. Dead Code, Assets, And Docs

- [x] Delete high-confidence dead Swift helpers:
  `presentSameVersionInstallPrompt`, `runtimeProofLine`,
  `registerMenu(appPath:)`, and `hasPublishedSite(at:)`.
- [x] Delete high-confidence dead Python helpers:
  `load_content_index`, `normalized_scope`, and `find_transition_contract` if
  live tests confirm they are unused.
- [x] Delete the `OneContextRuntimeSupport` re-export module if direct imports
  are cleaner after the package graph is simplified.
- [ ] Delete stale update-policy screenshots and assets such as
  `current-update-prompt.png` when no doc uses them.
- [ ] Delete unused wiki icon assets if no shipped surface references them.
- [x] Rewrite `docs/macos-release-runbook.md` around only the release train.
- [x] Rewrite `docs/development.md` so local packaging points at smoke tooling,
  not release tooling.
- [x] Rewrite `docs/ci/self-hosted-mac-runner.md` around release-train proof,
  runner attestation, and evidence.
- [ ] Delete or rewrite `docs/update_policy.html` after policy lives in the
  release manifest and control panel plan.
- [x] Update `ROADMAP.md`, `PERMISSIONS.md`, and `docs/local-web-contract.md`
  so they no longer teach old architecture.
- [x] Delete `docs/memory-core-contract.md` after removing the shipped
  memory-core subprocess bridge.

Evidence, 2026-05-13: deleted the `OneContextRuntimeSupport` module and moved
CLI/daemon/menu imports to direct module dependencies. Updated `ROADMAP.md`,
`PERMISSIONS.md`, `docs/local-web-contract.md`,
`docs/macos-app-architecture.md`, `docs/development.md`,
`docs/macos-release-runbook.md`, and `docs/ci/self-hosted-mac-runner.md` so the
current docs no longer teach agent hooks, shipped memory-core, public raw
diagnostics, or old install harnesses.

Evidence, 2026-05-13: deleted unreferenced helpers
`presentSameVersionInstallPrompt`, `LaunchAgentManager.registerMenu`,
`load_content_index`, `normalized_scope`, and `find_transition_contract`.
`runtimeProofLine` and `hasPublishedSite(at:)` were already removed with the
agent/memory-core cuts. Proof: negative `rg` over `macos/`, `memory-core/`, and
`scripts/`, plus Swift and Python suites.

### 12. Replacement Tests And Proof Gates

- [x] Replace grep-based structural tests with behavior tests and proof JSON
  validation where practical.
- [x] Keep negative `rg` audits for deleted public surfaces and env knobs.
- [ ] Add release-train tests for dirty-tree failure, deleted-script absence,
  manifest mismatch failure, missing artifact failure, and proof matrix
  completeness.
- [x] Run `scripts/release-train.sh validate`.
- [x] Run the Swift test suite.
- [x] Run the Python test suite.
- [x] Run package smoke for the allowlisted app bundle.
- [ ] Run self-hosted real-Mac proof through the release train after the cleanup
  is packaged.
- [ ] Capture a final deletion audit artifact showing removed files, negative
  compatibility checks, and the current release command surface.
- [x] Capture the final shipped app code-line measurement and prove at least a
  60% reduction from the baseline.

Evidence, 2026-05-13: deleted the stale grep-based
`scripts/test-menu-lifecycle-deterministic.sh`; the surviving tests cover
behavioral CLI/setup/update/package contracts and release proof JSON. Current
proof: `swift test --package-path macos`, `./scripts/test.sh`,
`./scripts/test-release-train.sh`, `./scripts/package-macos-smoke.sh`,
`./scripts/test-launch-agent-package.sh`, `./scripts/release-manifest.py
validate`, `cd memory-core && uv run --with pytest pytest`, `actionlint`,
`bash -n`, `git diff --check`, and `scripts/measure-shipped-app-lines.sh`.

### 13. Exit

- [ ] No documented production release path exists except
  `scripts/release-train.sh`.
- [x] No old update-policy files or scripts remain.
- [x] No old proof request scripts remain as documented user/operator entry
  points.
- [ ] No shipped app product code depends on source-checkout discovery or test
  fixture env vars.
- [x] No shipped app product code installs first-party agent hooks unless agent
  integration is explicitly re-approved as a current product surface.
- [x] No shipped app exposes dead wiki chat/provider UI.
- [x] The public CLI is intentionally small and redacted by default.
- [x] No tracked runtime state or generated wiki source output remains.
- [x] The shipped app code-line footprint is at least 60% smaller than the
  baseline captured at the start of this goal.
- [ ] A clean tagged release can validate, package, publish, prove, audit, and
  bless with the greenfield train.
- [ ] This goal is closed with evidence paths and any intentionally deferred
  non-goals.

## Immediate Next Step

Start with the release surface, because it has the highest leverage and the
least product ambiguity:

1. Delete the old update-policy system.
2. Delete or absorb the old release wrappers.
3. Route production release docs and workflows through `scripts/release-train.sh`.
4. Run release-train validation and focused release tests.
5. Update this goal with the exact deleted files and proof output.
