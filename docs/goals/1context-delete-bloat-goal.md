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
  env hooks, the runtime-support shim, structural grep tests, the dead chat API
  shell, old GUI harnesses, stale policy docs/assets, Sparkle retry env hooks,
  the old developer-port local-web mode, runtime path env overrides, detached
  LaunchAgent bypasses, fixture uninstall/RPC stress harnesses, local-web setup
  path env overrides, menu perf env logging, the remaining shipped
  `ONECONTEXT_*` product env hooks, and the obsolete public lifecycle/setup
  CLI: 17 files and 7,724
  nonblank lines, a
  53,585-line / 87.4%
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
- [x] Delete `scripts/audit-github-release-assets.sh` after asset checks move
  into `scripts/release-train.sh audit` or `scripts/release-manifest.py`.
- [x] Change `.github/workflows/self-hosted-mac-update-proof.yml` to enter
  through `scripts/release-train.sh prove`.
- [x] Absorb or move `scripts/self-hosted-update-proof.sh` under an internal
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

Evidence, 2026-05-13: moved the GitHub public asset audit into
`scripts/release-train.sh audit`/`publish`, deleted
`scripts/audit-github-release-assets.sh`, moved the real-Mac proof engine to
`scripts/release/internal/self-hosted-update-proof.sh`, and changed the
self-hosted workflow to execute it only through
`scripts/release-train.sh prove --runner-execute`. Proof:
`scripts/test-release-train.sh`, `bash -n`, and negative `rg` over active docs,
scripts, and workflows for the deleted public script paths.

### 5. Shipped App Backdoor Removal

- [x] Delete `ONECONTEXT_SMOKE_FIXTURE` from production app build and launch
  behavior.
- [x] Delete `ONECONTEXT_SHOW_SETUP_ON_LAUNCH`.
- [x] Delete `ONECONTEXT_VERSION_OVERRIDE`.
- [x] Delete product path overrides in `Paths.swift`.
- [x] Delete install prompt bypass and destination override hooks in
  `AppInstall.swift`.
- [x] Delete daemon/runtime override gates that are not needed by production.
- [x] Delete agent override gates that only exist for legacy harnesses.
- [x] Delete local-web setup path overrides that make the product depend on
  source-checkout layout.
- [x] Delete Sparkle retry/test env overrides once retry behavior is covered by
  reducer tests and release-train proof.
- [x] Delete public `--no-redact` diagnostic output from the product CLI.
- [x] Replace any remaining product test hooks with internal harness fixtures
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

Evidence, 2026-05-13: removed `ONECONTEXT_SMOKE_FIXTURE`,
`ONECONTEXT_SMOKE_STATE_DIR`, and `ONECONTEXT_SHOW_SETUP_ON_LAUNCH` from the app
build, menu launch path, and active scripts. Deleted old ad hoc GUI harnesses
that preserved those hooks. Removed `ONECONTEXT_ALLOW_DAEMON_OVERRIDE` and
`ONECONTEXT_DAEMON_PATH` from runtime daemon discovery. Proof:
`swift test --package-path macos`, `bash -n`, and negative `rg`.

Evidence, 2026-05-13: removed Sparkle retry-delay environment overrides from
the shipped updater policy. Retry timing is now a product constant covered by
pure reducer tests rather than a smoke-harness process variable. Proof:
`swift test --package-path macos` and negative `rg` for
`ONECONTEXT_SPARKLE_AUTOMATIC_RETRY_DELAYS_SECONDS` /
`ONECONTEXT_SPARKLE_MANUAL_RETRY_DELAYS_SECONDS` in active code.

Evidence, 2026-05-13: removed shipped runtime path overrides from
`RuntimePaths`, stopped writing `ONECONTEXT_*` path variables into LaunchAgent
plists, removed the detached `ONECONTEXT_LAUNCH_AGENT_DISABLED` lifecycle path,
and deleted fixture-only uninstall home overrides plus the old RPC stress and
fixture uninstall shell scripts. Tests now construct fixture `RuntimePaths`
directly, while destructive uninstall proof is reserved for the protected
release runner. Proof: `swift test --package-path macos`, `./scripts/test.sh`,
`./scripts/package-macos-smoke.sh`, `./scripts/test-launch-agent-package.sh`,
`cd memory-core && uv run --with pytest pytest`, `git diff --check`, and
negative `rg` for the deleted path/lifecycle/uninstall env names.

Evidence, 2026-05-13: deleted `ONECONTEXT_MENU_PERF_LOG` and the menu-app
performance logging scaffolding from shipped product code and docs. Proof:
`swift test --package-path macos`, `./scripts/test.sh`,
`./scripts/package-macos-smoke.sh`, `./scripts/test-launch-agent-package.sh`,
`./scripts/test-release-train.sh`, `git diff --check`, and negative `rg` for
the deleted perf env/logging names.

Evidence, 2026-05-13: removed the remaining shipped `ONECONTEXT_*` product env
hooks from `macos/Sources`. Caddy/local-web tests now inject fixture binaries
and setup paths through typed constructors, local-web setup no longer fakes
ServiceManagement/keychain/Applications state through env vars, and version
resolution no longer accepts a process environment parameter. Build/release
scripts still use explicit release env vars as operator tooling. Proof:
`swift test --package-path macos`, `./scripts/test.sh`,
`./scripts/package-macos-smoke.sh`, `./scripts/test-launch-agent-package.sh`,
`./scripts/test-release-train.sh`,
`cd memory-core && uv run --with pytest pytest`, `git diff --check`, and
`rg -n "ONECONTEXT_[A-Z0-9_]+" macos/Sources` returning no matches.

### 6. Local Web And Permissions Cleanup

- [x] Delete high-port HTTP as a shipped product mode. Public 1Context should
  have one local-web mode: portless local HTTPS.
- [x] Remove setup readiness bypasses tied to high-port HTTP.
- [x] Remove future sensitive permission scaffolding from shipped setup until
  the corresponding capture feature is actually shipping.
- [x] Remove `permissions --all` behavior if it exposes unshipped permissions.
- [x] Keep updater release notes hidden by policy and keep post-install popup
  disabled by default, with founder-controlled manifest copy for rare use.

Evidence, 2026-05-13: deleted the `OneContextPermissions` target and tests,
removed future Screen Recording/Accessibility rows from setup, and removed the
public `permissions` CLI command entirely. Current Local Wiki Access status is
reported through setup and redacted `diagnose`.

Evidence, 2026-05-13: deleted `high-port-http` / `LocalWebURLMode.highPortHTTP`
from shipped local-web code and tests. The CLI smoke now proves that start and
wiki commands require Local Wiki Access setup instead of bypassing setup with a
developer port. Proof: `swift test --package-path macos`, `./scripts/test.sh`,
and negative `rg` for `high-port-http`, `highPortHTTP`, and
`ONECONTEXT_WIKI_URL_MODE` in active code/scripts/docs.

Evidence, 2026-05-13: removed local-web setup path environment overrides from
`LocalWebSetupSystemPaths` and replaced them with typed fixture path injection
for tests and diagnostics. Production setup now derives the app bundle, helper
plist, support directory, certificate files, and proxy log paths from the signed
app and the user's standard 1Context folders instead of
`ONECONTEXT_LOCAL_WEB_*` path variables. Proof:
`swift test --package-path macos`, `./scripts/test.sh`,
`./scripts/package-macos-smoke.sh`, `./scripts/test-launch-agent-package.sh`,
`./scripts/test-release-train.sh`,
`cd memory-core && uv run --with pytest pytest`, `git diff --check`, and
negative `rg` for the deleted local-web setup path env names.

Evidence, 2026-05-13: release notes are intentionally hidden from the Sparkle
update window by `release/release.toml` (`show_in_update_window = false`), and
the rare post-install popup remains founder-controlled but disabled by default
(`enabled = false`, title default `1Context Improved!`). The manifest validator
enforces both policies and rejects appcasts that include a description while
release notes are hidden. Proof: `./scripts/test-release-train.sh` and
`./scripts/release-manifest.py validate`.

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

- [x] Collapse the public CLI to only the commands the product truly needs:
  `version`, redacted `diagnose`, `uninstall`, and possibly `wiki local-url`.
- [x] Delete public lifecycle commands if they duplicate menu or launch-agent
  behavior: `start`, `stop`, `restart`, `quit`, and `logs`.
- [x] Delete public debug commands, especially raw debug paths and
  `debug --no-redact`.
- [x] Delete public `permissions`, `agent`, and `memory-core` command surfaces
  unless they are part of the current product contract.
- [x] Move any necessary operator-only behavior into release/proof tooling, not
  the shipped app CLI.

Evidence, 2026-05-13: removed the standalone `debug` command, removed
`--debug` from lifecycle/status commands, removed `diagnose --no-redact`,
removed `permissions`, and previously removed `agent` and `memory-core`.
Operator evidence scripts now call redacted `diagnose` instead of public debug
flags. Proof: `scripts/test.sh` negative checks and negative `rg`.

Evidence, 2026-05-13: removed public `start`, `stop`, `quit`, `restart`,
`status`, `logs`, `update`, `setup local-web`, and `wiki refresh` from the
shipped CLI. The CLI help now exposes only `version`, redacted `diagnose`,
`uninstall`, and `wiki local-url`. The self-hosted update proof no longer calls
public `quit` or `setup local-web status`; it stops app processes through
operator script internals and preflights Local Wiki setup with redacted
`diagnose`. Proof: `swift build --package-path macos`,
`swift test --package-path macos`, `./scripts/test.sh`, `bash -n
scripts/test.sh scripts/test-release-app-product-https.sh
scripts/release/internal/self-hosted-update-proof.sh`, negative `rg` over the
deleted CLI functions, and `scripts/measure-shipped-app-lines.sh` reporting
7,724 shipped-app nonblank lines.

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

Evidence, 2026-05-13: after the public CLI/env-hook, chat/API, dead-helper,
old harness, stale policy-doc, Sparkle retry-env, and developer-port local-web
cleanup, `scripts/measure-shipped-app-lines.sh` reports 8,191 nonblank
shipped-app lines, an 86.64% reduction from baseline.

Evidence, 2026-05-13: after deleting runtime path env overrides, the detached
LaunchAgent bypass, and obsolete fixture uninstall/RPC stress harnesses,
`scripts/measure-shipped-app-lines.sh` reports 8,014 nonblank shipped-app
lines after the remaining shipped `ONECONTEXT_*` product env hooks were removed,
an 86.93% reduction from
baseline.

Evidence, 2026-05-13: after collapsing the public CLI to support-only commands,
`scripts/measure-shipped-app-lines.sh` reports 7,724 nonblank shipped-app lines,
a 53,585-line / 87.4% reduction from baseline.

### 11. Dead Code, Assets, And Docs

- [x] Delete high-confidence dead Swift helpers:
  `presentSameVersionInstallPrompt`, `runtimeProofLine`,
  `registerMenu(appPath:)`, and `hasPublishedSite(at:)`.
- [x] Delete high-confidence dead Python helpers:
  `load_content_index`, `normalized_scope`, and `find_transition_contract` if
  live tests confirm they are unused.
- [x] Delete the `OneContextRuntimeSupport` re-export module if direct imports
  are cleaner after the package graph is simplified.
- [x] Delete stale update-policy screenshots and assets such as
  `current-update-prompt.png` when no doc uses them.
- [x] Delete unused wiki icon assets if no shipped surface references them.
- [x] Rewrite `docs/macos-release-runbook.md` around only the release train.
- [x] Rewrite `docs/development.md` so local packaging points at smoke tooling,
  not release tooling.
- [x] Rewrite `docs/ci/self-hosted-mac-runner.md` around release-train proof,
  runner attestation, and evidence.
- [x] Delete or rewrite `docs/update_policy.html` after policy lives in the
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

Evidence, 2026-05-13: deleted stale `docs/update_policy.html`, its obsolete
update-prompt screenshot asset, and unused wiki theme icon variants. Updated
`ROADMAP.md`, `docs/README.md`, and `docs/macos-release-runbook.md` so active
docs point at `release/release.toml` and `scripts/release-train.sh` instead of
a separate policy page. Proof: negative `rg` over active docs for
`update_policy.html`, `current-update-prompt.png`, and `assets/update-policy`.

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
- [x] Add release-train tests for dirty-tree failure, deleted-script absence,
  manifest mismatch failure, missing artifact failure, and proof matrix
  completeness.
- [x] Run `scripts/release-train.sh validate`.
- [x] Run the Swift test suite.
- [x] Run the Python test suite.
- [x] Run package smoke for the allowlisted app bundle.
- [ ] Run self-hosted real-Mac proof through the release train after the cleanup
  is packaged.
- [x] Capture a final deletion audit artifact showing removed files, negative
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

Evidence, 2026-05-13: captured the deletion audit artifact at
`docs/goals/evidence/delete-bloat-audit-2026-05-13.md`, including removed file
families, negative checks for deleted public surfaces, proof commands, and the
current 87.4% shipped-app line reduction result.

Evidence, 2026-05-13: product path override deletion tranche passed with
`swift test --package-path macos`, `./scripts/test.sh`,
`./scripts/package-macos-smoke.sh`, `./scripts/test-launch-agent-package.sh`,
`cd memory-core && uv run --with pytest pytest`, `git diff --check`, and a
negative `rg` for `RuntimePaths.current(environment`,
`ONECONTEXT_USER_CONTENT_DIR`, `ONECONTEXT_APP_SUPPORT_DIR`,
`ONECONTEXT_CACHE_DIR`, `ONECONTEXT_LOG_DIR`, `ONECONTEXT_SOCKET_PATH`,
`ONECONTEXT_LOG_PATH`, `ONECONTEXT_PREFERENCES_PATH`,
`ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES`, `ONECONTEXT_LAUNCH_AGENT_DISABLED`,
`ONECONTEXT_UNINSTALL_HOME_DIR`, `ONECONTEXT_ALLOW_UNINSTALL_HOME_OVERRIDE`,
`stress-runtime-rpc`, and `test-macos-uninstall-command`.

Evidence, 2026-05-13: after the CLI collapse landed, `scripts/release-train.sh
prove --dry-run` produced the expected self-hosted proof dispatch for
`0.1.64 -> 0.1.65`. A clean detached worktree validation from current `main`
correctly failed because `release/release.toml` still points at already-tagged
`v0.1.65` while current `HEAD` is `40e2196`, post-release cleanup work. The
remaining closure now requires choosing and tagging the next release, expected
to be `0.1.66`, then running
`scripts/release-train.sh validate/package/publish/prove/audit/bless` from that
clean tag.

Evidence, 2026-05-13: prepared `0.1.66` release metadata for the next clean
tag: `VERSION`, `Core.swift`, `release/release.toml`, `RELEASE_NOTES.md`, and
the self-hosted proof workflow default now agree on `0.1.65 -> 0.1.66`.
Proof: `./scripts/check-version-consistency.sh`,
`./scripts/release-manifest.py validate`, `swift test --package-path macos`,
`./scripts/test.sh`, `./scripts/test-release-train.sh`,
`./scripts/release-train.sh prove --dry-run`,
`./scripts/package-macos-smoke.sh`, `git diff --check`, and packaged
`1context-cli --version` reporting `0.1.66`.

### 13. Exit

- [x] No documented production release path exists except
  `scripts/release-train.sh`.
- [x] No old update-policy files or scripts remain.
- [x] No old proof request scripts remain as documented user/operator entry
  points.
- [x] No shipped app product code depends on source-checkout discovery or test
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

Tag the clean `0.1.66` tree, then run the greenfield release train end to end:
`validate`, `package`, `publish`, `prove`, `audit`, and `bless`.
