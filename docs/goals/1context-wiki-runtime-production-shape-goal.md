---
title: 1Context Wiki Runtime Production Shape Goal
slug: 1context-wiki-runtime-production-shape-goal
section: development
access: private
summary: "Completed cleanup checklist for retiring duplicate wiki runtime artifacts and shipping one RuntimeDefaults plus WikiEngine production shape."
status: complete
last_updated: 2026-05-18
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# Milestone: Wiki Runtime Production Shape

## Goal

Collapse the wiki runtime into one knowable production shape:

- `RuntimeDefaults` is the shipped seed/backfill tree for user-owned
  `~/1Context` data.
- `WikiEngine` is the bundled renderer source/artifact Swift invokes to render
  user data.
- Swift installs missing defaults, preserves user-owned files, records conflicts
  as proposals, renders from `~/1Context/user-wiki/source`, and publishes the
  last-good static site to Application Support.
- The old `release/memory-runtime` static fallback artifact is retired.

## Done When

- The app bundle contains no `Contents/Resources/memory-runtime`.
- The tracked `release/memory-runtime` source tree and builder script are gone.
- The build writes a `RuntimeDefaults` freshness manifest that ties the shipped
  defaults to the release version, git commit/dirty bit, source hash,
  wiki-core hash, renderer hash, and render proof.
- The RuntimeDefaults installer ledger records the packaged manifest/hash while
  preserving user-owned files and writing proposals for conflicts.
- Local Web no longer seeds `wiki-site/current` from a bundled static fallback;
  it serves the published last-good site produced from installed user data.
- Package and release tests reject the old artifact and validate the
  RuntimeDefaults/WikiEngine bundle shape.
- Active docs/specs describe only the RuntimeDefaults/WikiEngine shape; any
  memory-runtime mentions are historical or explicitly retired.

## Checklist

### 1. Remove The Duplicate Runtime Artifact

- [x] Delete `release/memory-runtime/` and
  `scripts/build-memory-runtime-artifact.sh`. Evidence: tracked files removed
  with `git rm`; `release/` now contains only `release.toml` and release-owned
  tool artifacts.
- [x] Remove memory-runtime build/copy steps from the app packaging pipeline.
  Evidence: `scripts/build-macos-app.sh` now builds `RuntimeDefaults`, writes
  the freshness manifest, and copies `WikiEngine`.
- [x] Remove Local Web's bundled memory-runtime seeding path. Evidence:
  `LocalWeb.ensureStaticSupportFiles()` no longer copies bundled pages and full
  `swift test --package-path macos` passes.

### 2. Prove Default Freshness

- [x] Generate a build-time `RuntimeDefaults` manifest with version, git commit,
  source hash, site hash, wiki-core hash, renderer hash, and render counts. Evidence:
  `wiki-engine/tools/write-runtime-defaults-manifest.py`; rebuilt app manifest reports
  schema `1context.runtime-defaults-manifest.v1`, version `0.1.87`, clean
  commit `4dfe5722c136b36f59f9f68ded26e0efec98ab92`, `git_dirty=false`,
  8 routes, 8 markdown twins, wiki-core hash prefix `78ae10f53900`, and
  renderer hash prefix `0aeac13b4e1e`.
- [x] Validate that manifest in package and installed-path smoke tests.
  Evidence: `ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1
  ./scripts/test-launch-agent-package.sh` and
  `./scripts/test-wiki.sh` pass.
- [x] Record the manifest identity in the first-run/defaults install ledger.
  Evidence: `WikiRuntimeDefaultsInstallerTests.copiesMissingDefaultsAndPreservesExistingFiles`
  asserts `packagedManifest` survives into `runtime-defaults-install.json`.

### 3. Keep The Public Bundle Lean

- [x] Package smoke rejects `memory-core`, `runtime-test`, generated private
  state, and the old `memory-runtime` artifact.
- [x] Release-train tests no longer call or require the retired artifact
  builder. Evidence: `./scripts/test-release-train.sh` passes and scans active
  build/runtime code for retired `build-memory-runtime-artifact` and
  `release/memory-runtime` references.
- [x] Active docs/specs point operators to `RuntimeDefaults` and `WikiEngine`
  as the only shipped wiki runtime components. Evidence:
  `docs/user-data-spec.md`, `docs/wiki-memory-publication-contract.md`, and
  `docs/wiki-productionization.html` describe the single bundle shape.

### 4. Verification Evidence

- [x] `./scripts/test-release-train.sh`
- [x] `swift test --package-path macos`
- [x] `npm test --prefix wiki-engine`
- [x] `/usr/bin/time -p ./scripts/release-train.sh build --channel dev` completed in
  73.00 seconds and produced `dist/1Context-0.1.87-macos-arm64.dmg`.
- [x] `ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh`
- [x] `./scripts/test-wiki.sh`
- [x] `./scripts/test-wiki-runtime-defaults-scenarios.sh`
- [x] `git diff --check`

## Evidence Log

- 2026-05-18: Goal opened after identifying the duplicate
  `release/memory-runtime` artifact path as transitional release-factory
  residue now that RuntimeDefaults and WikiEngine are the desired production
  shape.
- 2026-05-18: Closed the cleanup. The rebuilt app bundle contains no
  `Contents/Resources/memory-runtime`, includes a 1.4 MB `RuntimeDefaults`
  bundle with a freshness manifest, and includes a 2.0 MB `WikiEngine` bundle
  with vendored production renderer dependencies and no package lock or npm bin
  shims.
- 2026-05-19: Strengthened proof for the clean production shape. A clean dev
  build stamped RuntimeDefaults with commit
  `4dfe5722c136b36f59f9f68ded26e0efec98ab92`, `git_dirty=false`, app version
  `0.1.87`, and successful render summary. Runtime-test scenarios now prove
  fresh backfill, user-edit preservation with conflict proposals, and custom
  fallback-template pages with talk routes.
