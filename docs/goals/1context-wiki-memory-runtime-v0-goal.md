---
title: 1Context Wiki Memory Runtime V0 Goal
slug: 1context-wiki-memory-runtime-v0-goal
section: development
access: private
summary: "Close-loop checklist for shipping the user-owned wiki runtime, Swift publisher, bundled JS renderer, and memory-authoring contracts needed by the app and future memory core."
status: active
last_updated: 2026-05-14
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# Milestone: Wiki Memory Runtime V0

## Goal

Ship the first production-shaped wiki memory runtime: the installed app can
initialize user-owned wiki data, materialize configured pages, publish a
last-good static wiki through Open Wiki, and expose enough governed authoring
surfaces for the future Python memory core to create talk, proposals, decisions,
source edits, and render requests safely.

This milestone connects three contracts:

- [User Data Spec](../user-data-spec.md): persisted filesystem contract
- [Wiki Memory Publication Contract](../wiki-memory-publication-contract.md):
  agent/Swift/renderer behavior contract
- [Repo Runtime Layout](../../runtime/README.md): public-safe development mirror

The prior [Wiki Engine V0 Goal](1context-wiki-engine-v0-goal.md) is the smaller
renderer slice. This goal is the close-loop product milestone around it.

## Done When

- A clean installed app creates the production-shaped `~/1Context` and
  Application Support trees without requiring a source checkout.
- A clean dev runtime initializes from `runtime/`, materializes configured wiki
  pages, preserves edited files, respects tombstones, and records setup state.
- `wiki.toml` is the single installed-runtime site map for source-backed pages,
  generated pages, aliases, navigation, templates, and missing-route behavior.
- The bundled renderer runs outside `memory-core`, accepts explicit paths,
  returns structured JSON, and writes only to staging.
- Swift `wiki.refresh` runs a queued, debounced, single-flight render
  coordinator instead of placeholder publishing.
- Successful render atomically promotes `user-wiki/site`, mirrors it to
  Application Support, writes `current-render.json`, and appends
  `render-events.jsonl`.
- Failed render preserves the last-good served site and records actionable
  diagnostics.
- Open Wiki serves configured routes, missing-route diagnostics, valid markdown
  twins, and private talk pages with no hidden `/your-context` fallback.
- Browser-visible APIs expose no absolute local paths, usernames, raw prompts,
  raw observations, run transcripts, or private proposal previews.
- Package smoke proves the app does not require host Python, host Node, `uv run`,
  `npm install`, `npm ci`, generated `runtime-test`, or a bundled `memory-core`
  source checkout to open the wiki.
- Memory-core integration has a stable facade for route plans, talk appends,
  proposal/decision promotion, preview renders, and render requests without
  owning renderer internals.
- Legacy wiki-engine placement, compatibility paths, stale runtime layouts, and
  old memory-core code that no longer owns product behavior are deleted rather
  than kept as backward-compatibility ballast.

## Checklist

### 1. Baseline Contracts

- [x] Split storage and behavior into separate specs. Evidence:
  [User Data Spec](../user-data-spec.md) and
  [Wiki Memory Publication Contract](../wiki-memory-publication-contract.md).
- [x] Document the repo/runtime mirror contract. Evidence:
  [Repo Runtime Layout](../../runtime/README.md).
- [x] Keep the smaller renderer slice tracked separately. Evidence:
  [Wiki Engine V0 Goal](1context-wiki-engine-v0-goal.md).
- [x] Add this milestone to release planning status once the first coordinator
  implementation starts. Evidence: [Development Goals](README.md) active status
  note.

### 2. User Data And Runtime Roots

- [x] Track public-safe `runtime/1Context/user-wiki`,
  `runtime/1Context/context-engine`, and Application Support mirror folders.
  Evidence: `./scripts/test-wiki-runtime-v0.sh`.
- [x] Keep generated and personal runtime state out of git. Evidence:
  `./scripts/test-wiki-runtime-v0.sh` checks ignored and trackable paths.
- [x] Expose typed Swift runtime roots for `user-wiki`, `context-engine`,
  Application Support, logs, and caches. Evidence:
  `macos/Tests/OneContextPlatformTests/PathAndPermissionTests.swift`.
- [x] Add a production-path smoke that runs without
  `ONECONTEXT_DEV_RUNTIME_HOME`. Evidence:
  `PathAndPermissionTests.testProductionRuntimePathsDoNotRequireDebugRuntimeHomeOverride`
  asserts production-shaped `~/1Context` and `~/Library/...` roots without
  writing to them.
- [x] Add setup-state schema validation for default install and page
  materialization ledgers. Evidence: `./scripts/test-wiki-setup-state-schema.sh`
  validates setup TOML for default materialization and dev fixture imports.

### 3. Site Map And Source Families

- [x] Define source-backed pages and target site pages in `wiki.toml`. Evidence:
  `runtime/1Context/user-wiki/wiki.toml` parses with four `[[pages]]` and three
  `[[site_pages]]`.
- [x] Use semantic family groups instead of menu-order folders. Evidence:
  specs define `context`, `work`, `reference`, `for-you`, and `system`.
- [x] Materialize V0 source families for `for-you`, `your-context`,
  `projects`, and `topics`. Evidence: `./scripts/test-wiki-runtime-v0.sh` and
  `./scripts/test-wiki-render-contract.sh`.
- [x] Validate `wiki.toml` for duplicate routes, duplicate ids, invalid
  `kind`, disabled alias targets, path escapes, absolute template paths, and
  symlink escapes. Evidence: `./scripts/test-wiki-materializer-validation.sh`.
- [x] Decide whether `home`, `for-you`, `this-week`, and `open-questions` are
  enabled in V0 or remain declared-but-disabled until generated-page support
  lands. Evidence: `for-you` is source-backed and enabled, `home` remains the
  generated root, and `this-week`/`open-questions` remain disabled in
  `runtime/1Context/user-wiki/wiki.toml`.

### 4. Templates And Materialization

- [x] Materializer creates missing configured pages and talk folders from
  templates. Evidence: `./scripts/test-wiki-runtime-v0.sh`.
- [x] Prove user-configured custom pages can use generic fallback templates
  without adding blessed public content. Evidence:
  `./scripts/test-wiki-custom-pages-contract.sh`.
- [x] Materializer preserves edited files and tombstones. Evidence:
  `./scripts/test-wiki-runtime-v0.sh`.
- [x] Align runtime template frontmatter with renderer validation. Evidence:
  `./scripts/test-wiki-render-contract.sh` and
  `npm test` in `wiki-engine`.
- [x] Make talk `_meta.yaml` include page route, talk route, access, timestamps,
  and schema fields required by the renderer. Evidence:
  `./scripts/test-wiki-render-contract.sh`.
- [x] Ensure talk access inherits page access by default. Evidence:
  `./scripts/test-wiki-render-contract.sh` checks private talk output.
- [x] Fix markdown twin URLs so browser/Agent surfaces can fetch valid markdown
  for every configured page and talk page. Evidence:
  `./scripts/test-wiki-browser-contract.sh`.

### 5. Wiki Engine Extraction

- [x] Move the renderer out of `memory-core/wiki-engine` into a first-class
  `wiki-engine/` package or bundled helper source. Evidence:
  `wiki-engine/package.json` plus `./scripts/test-wiki-render-contract.sh`.
- [x] Delete `memory-core/wiki-engine` after the first-class `wiki-engine/`
  package has equivalent render proof. Evidence: removed legacy directory and
  passing renderer/browser contract tests.
- [x] Remove compatibility paths instead of preserving long-term shims. Evidence:
  harnesses call `wiki-engine/` directly.
- [x] Add structured renderer CLI: explicit roots, staging output, result JSON,
  no source mutation. Evidence: `wiki-engine/tools/render-site.mjs` and
  `./scripts/test-wiki-render-site-cli.sh`.
- [x] Add renderer schemas for page result, error result, route manifest,
  content index, and markdown twins. Evidence:
  `wiki-engine/schemas/*.schema.json` plus
  `./scripts/test-wiki-render-schemas.sh`; `render-site.mjs` writes
  `.1context/route-manifest.json` and `.1context/content-index.json`.
- [x] Add renderer package smoke proving no network fetch and no runtime
  dependency install. Evidence: `./scripts/test-wiki-engine-package-smoke.sh`
  copies `wiki-engine/` without `node_modules`, runs `npm ci --offline`,
  renders a fixture source tree, and fails on obsolete runtime-coupled text.

### 6. Swift Render Coordinator

- [x] Create `OneContextWikiRuntime` or equivalent Swift module for render
  coordination and validators. Evidence:
  `swift test --package-path macos --filter OneContextWikiRuntimeTests`.
- [x] Replace `ensurePlaceholderSite()` in `wiki.refresh` with the coordinator.
  Evidence: `macos/Sources/OneContextDaemon/main.swift` calls
  `WikiRenderCoordinator.renderAndPublish(trigger: "wiki.refresh")`.
- [x] Stage render output under Application Support before promotion. Evidence:
  `OneContextWikiRuntimeTests` failure-injection preserves served current site.
- [x] Atomically promote successful output from `user-wiki/site` and mirror to
  `Application Support/1Context/wiki-site/current`. Evidence:
  `OneContextWikiRuntimeTests` verifies published files in current site.
- [x] Write `site/.1context/current-render.json` and append
  `site/.1context/render-events.jsonl`. Evidence:
  `OneContextWikiRuntimeTests` verifies source and mirrored render state.
- [x] Invoke the extracted `wiki-engine/` renderer from Swift before
  promotion. Evidence:
  `swift test --package-path macos --filter OneContextWikiRuntimeTests`
  exercises `WikiEngineRenderer` against the root `wiki-engine/` package.
- [x] Validate generated routes, markdown twins, manifests, access labels, and
  export allowlists before promotion. Evidence: `WikiSiteValidator` validates
  `.1context/route-manifest.json`, `.1context/content-index.json`, route-index
  files, markdown twin hashes, `data-tier` labels, and export allowlists;
  `swift test --package-path macos --filter OneContextWikiRuntimeTests`
  rejects bad output while preserving last-good.

### 7. Render Queue And CPU Backpressure

- [x] Implement queued `wiki.refresh` semantics: debounced, single-flight,
  coalesced, and hash-aware. Evidence: `WikiRenderQueue` is wired into
  `OneContextDaemon`; `WikiRenderFingerprint` lets the coordinator skip
  unchanged accepted inputs; `swift test --package-path macos --filter
  OneContextWikiRuntimeTests`.
- [x] Prove 100 rapid refresh requests produce no overlapping renderer helpers
  and a bounded number of canonical renders. Evidence:
  `WikiRenderQueueTests.testRapidRequestsAreSingleFlightAndCoalesced`.
- [x] Prove no-op refresh skips the renderer when accepted inputs are unchanged.
  Evidence:
  `WikiRenderCoordinatorTests.testNoOpRefreshSkipsRendererWhenInputsAreUnchanged`.
- [x] Add failure backoff while preserving manual refresh priority. Evidence:
  `WikiRenderQueueTests.testFailureBackoffDelaysAutomaticButManualRunsImmediately`.
- [x] Record render duration, renderer duration, dirty pages, trigger, and
  coalesced request count in render events. Evidence: `WikiRenderQueueRecord`
  records trigger, queue delay, render duration, renderer duration, dirty pages,
  skip reason, outcome, and `WikiRenderQueueSnapshot.coalescedCount`; covered by
  `WikiRenderQueueTests.testRecordsDurationsDirtyPagesAndSkipReason`.

### 8. Local Web And User App Experience

- [x] Open Wiki serves the last-good rendered site, not hardcoded placeholder
  routes. Evidence:
  `LocalWebTests.testStaticSupportFilesPreserveLastGoodRenderedSite` and
  `testStaticSupportFilesDoNotPublishPlaceholderPages`; failed renders preserve
  `Application Support/1Context/wiki-site/current` in
  `OneContextWikiRuntimeTests`.
- [x] Missing configured source materializes or shows repair diagnostics.
  Evidence: `./scripts/test-wiki-runtime-v0.sh` materializes configured source,
  and `./scripts/test-wiki-custom-pages-contract.sh` materializes a dummy custom
  page from the generic fallback template.
- [x] Unconfigured routes show diagnostics and never redirect to
  `/your-context`. Evidence: `./scripts/test-wiki-browser-contract.sh` checks
  `/definitely-missing` returns 404 without redirect, and
  `./scripts/test-wiki-custom-pages-contract.sh` checks `/not-configured`.
- [x] Tombstoned pages are not recreated and show tombstone diagnostics.
  Evidence: `./scripts/test-wiki-tombstone-contract.sh` writes a configured
  tombstone, verifies materialization state reports `status = "tombstoned"`,
  verifies source/talk files and rendered routes are absent, and confirms the
  route 404s without redirecting.
- [x] Browser-visible `/api/wiki/*` responses expose logical ids or redacted
  relative paths, never absolute local paths. Evidence:
  `swift test --package-path macos --filter OneContextLocalWebTests`.
- [x] Agent/browser markdown twin links resolve for pages and talk pages.
  Evidence: `./scripts/test-wiki-browser-contract.sh`.
- [x] Menu-bar Open Wiki or refresh path reports current, stale, failed,
  rendering, or uninitialized state clearly. Evidence: daemon `wiki.status` now
  includes a structured `render` payload with active trigger, queue state,
  backoff, last status, durations, dirty pages, skip reason, and errors;
  `WikiMenuSnapshot` parses render state for menu/open-wiki timeout and refresh
  reporting.

### 9. Wiki Interface For Memory-Core

- [x] Define route plan schema for target, ownership, hashes, validators,
  expected outputs, idempotency key, and promotion preconditions. Evidence:
  `onectx.wiki_interface.authoring.write_route_plan` plus
  `test_wiki_authoring_facade_writes_route_plan_records_and_preview`.
- [x] Add talk append API or file helper that writes schema-valid talk entries
  with provenance and page identity. Evidence: `append_talk_entry` covered by
  `test_wiki_authoring_facade_appends_schema_valid_talk_entry`.
- [x] Add proposal and decision record schemas compatible with private-4 style
  promotion. Evidence: `write_proposal` and `write_decision` fixture checks in
  `test_wiki_authoring_facade.py`.
- [x] Add promotion receipt format for accepted source, template, prompt, and
  `wiki.toml` changes. Evidence: `write_promotion_receipt` fixture checks in
  `test_wiki_authoring_facade.py`.
- [x] Add preview render path under `context-engine/artifacts` that never
  updates canonical `user-wiki/site`. Evidence: `write_preview_render_request`
  writes under `context-engine/artifacts/wiki/previews/**`; test asserts
  `user-wiki/site` is untouched.
- [x] Add render request facade that calls Swift/daemon `wiki.refresh` rather
  than invoking renderer internals directly. Evidence: `request_wiki_refresh`
  sends JSON-RPC over the daemon Unix socket; mocked daemon test asserts method
  `wiki.refresh` and no `wiki-engine` invocation.
- [x] Remove legacy memory-core wiki rendering/discovery paths that duplicate
  `wiki.toml`, Swift publication, or the extracted renderer. Evidence:
  deleted `memory-core/src/onectx/wiki/**`, `memory-core/wiki/**`, old
  `wiki list/ensure/render/routes/stats` CLI paths, and old wiki-renderer tests;
  `uv run --with pytest pytest tests` passes in `memory-core`.
- [x] Move Python wiki controls out of `onectx.memory` into a single
  `onectx.wiki_interface` boundary with no compatibility wrappers. Evidence:
  `memory-core/src/onectx/wiki_interface/README.md` defines the boundary;
  the active interface is narrowed to authoring records, accepted source edits,
  and Swift `wiki.refresh` requests; the deleted planning/executor/apply/
  validator compatibility modules must not import; `wc -l` reports 472 lines
  for `onectx.wiki_interface` plus `source_freshness.py`, and 792 lines when
  including the runtime invariant checker; `tests/test_wiki_interface_boundary.py`
  fails if old compatibility modules return; `uv run --with pytest pytest tests`
  passes in `memory-core` with 61 tests.
- [x] Slim memory-core to memory ownership: route plans, jobs, state machines,
  proposals, decisions, artifacts, observations, indexes, and render requests.
  Evidence: memory tick now writes only source freshness, migration, invariant,
  and Swift `wiki.refresh` request records for wiki publication; stale route
  hire/promotion invariant paths were removed; state-machine DSL and diagrams
  use `write_wiki_refresh_request` / `notify_wiki_render_queue`; full
  `memory-core` pytest suite passes.

### 10. Bloat Deletion And No Backward Compatibility

- [x] Inventory legacy wiki/memory code paths before deletion: old
  `wiki/menu/**`, old generated outputs, compatibility CLIs, placeholder local
  web paths, dead docs, and source-tree runtime fallbacks. Evidence:
  [Wiki Memory Runtime Bloat Inventory](evidence/wiki-memory-runtime-bloat-inventory-2026-05-14.md).
- [x] Delete obsolete docs and plans that describe the old architecture once
  their durable truth is captured in the two specs and this goal. Evidence:
  deleted the stale release-lockdown talk artifact that seeded `/goal`, removed
  old wiki route frontmatter from the delete-bloat goal, updated local-web docs
  to describe last-good render publication instead of placeholder shells, and
  refreshed `docs/goals/README.md` around the current wiki-runtime closure.
- [x] Delete old renderer/package scripts that require host Node, runtime
  `npm install`, or source-tree assumptions in the installed app path. Evidence:
  `scripts/build-macos-app.sh` pre-materializes and pre-renders
  `RuntimeDefaults/1Context/user-wiki/site` at build time; production-shape
  cleanup vendors the small `WikiEngine` production dependencies while package
  smoke still forbids package locks, executable npm shims, runtime package
  installs, and a bundled `memory-core` source checkout.
- [x] Delete placeholder publication behavior after Swift coordinator lands.
  Evidence: `ensurePlaceholderSite()` and `placeholderHTML()` are gone from
  `macos/Sources` and `macos/Tests`; `wiki.prepare` now calls
  `WikiRenderCoordinator.renderAndPublish(trigger: "wiki.prepare")`. Proof:
  `swift test --package-path macos --filter OneContextLocalWebTests` and
  `swift test --package-path macos --filter OneContextWikiRuntimeTests`.
- [x] Remove old fallback behavior that routes missing pages to
  `/your-context`, `/for-you`, or hidden bundled content. Evidence:
  `./scripts/test-wiki-browser-contract.sh` proves `/definitely-missing` stays
  404 without redirect, `./scripts/test-wiki-custom-pages-contract.sh` proves
  `/not-configured` stays 404, and `rg` over active local-web source finds no
  placeholder-page fallback publisher.
- [x] Keep no long-term backward-compatibility layer for old public-launch wiki
  layouts. Evidence: old `onectx.memory.wiki*` imports now fail instead of
  routing through wrappers; runtime-default installation copies missing packaged
  defaults, preserves changed user files, and writes explicit conflict
  proposals under `1Context/context-engine/proposals/wiki/runtime-defaults/`;
  `swift test --package-path macos --filter OneContextWikiRuntimeTests` passes.
- [x] Package smoke fails if bundled artifacts include obsolete runtime state,
  private fixtures, `runtime-test`, legacy generated pages, or a `memory-core`
  source checkout. Evidence: `ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1
  ./scripts/test-launch-agent-package.sh` rejects `runtime-test`,
  `1context-private`, local developer paths, generated source output,
  raw observations, run transcripts, private previews, dead chat/provider
  surfaces, `node_modules`, package locks, and bundled `memory-core`; it passes
  against `dist/1Context.app`.

### 11. Privacy And Export Guardrails

- [x] Static export allowlist excludes `context-engine`, raw prompts, raw
  observations, run transcripts, private previews, and local paths. Evidence:
  `./scripts/test-wiki-render-schemas.sh` and `WikiSiteValidator` reject
  `context-engine`, `source/families`, `_curator`, `_conventions`, prompts,
  observations, runs, previews, `runtime-test`, and `/Users/` in export
  allowlists.
- [x] Local APIs redact usernames and home-directory fragments. Evidence:
  `swift test --package-path macos --filter OneContextLocalWebTests` covers
  browser-visible health/state payloads and removed unshipped chat routes.
- [x] Package smoke forbids generated `runtime-test`, private fixtures,
  `node_modules` unless intentionally vendored, runtime package installs, and
  source checkouts. Evidence: `./scripts/package-macos-smoke.sh` plus
  `ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh`
  pass after rebuilding the app bundle and DMG.
- [x] Update migration preserves user-edited defaults and writes proposals for
  conflicts. Evidence:
  `WikiRuntimeDefaultsInstallerTests.copiesMissingDefaultsAndPreservesExistingFiles`
  preserves an edited `user-wiki/wiki.toml`, copies missing default files,
  writes `runtime-defaults-install.json`, writes a conflict proposal without
  absolute local paths, and passes in
  `swift test --package-path macos --filter OneContextWikiRuntimeTests`.
- [x] Delete/reinstall without zap preserves `~/1Context` and does not confuse
  app machinery with user memory. Evidence:
  `./scripts/test-wiki-reinstall-preserves-user-data.sh` deletes and reinstalls
  a packaged app fixture while preserving edited `user-wiki/wiki.toml`, edited
  topic source, and Application Support machinery separately.

### 12. End-To-End Proof Bundle

- [x] Clean dev runtime proof: initialize, materialize, render, serve, refresh,
  fail render, recover last-good. Evidence:
  `./scripts/test-wiki-runtime-e2e.sh`.
- [x] Installed-path proof: same workflow against production-shaped paths
  without source-tree fallbacks. Evidence:
  `./scripts/package-macos-smoke.sh`,
  `./scripts/test-wiki-installed-path-smoke.sh`, and
  `ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh`.
- [x] Browser proof: open local wiki, verify primary routes, talk routes,
  UI Talk navigation, brand-menu navigation, TOC anchor targets, Agent view,
  markdown twins, resources, console errors, and missing-route no-redirect.
  Evidence: `./scripts/test-wiki-browser-contract.sh` plus an in-app Browser
  pass on a live local server with 39 assertions covering `/topics`,
  `/topics/talk`, `/for-you`, visible TOC contents, Agent view markdown load,
  no absolute local path leak, and missing-route no-redirect.
- [x] Agent proof: append talk, create proposal, accept decision, promote
  source, request render, observe updated page. Evidence:
  `./scripts/test-wiki-agent-authoring-e2e.sh`.
- [x] Performance proof: render queue handles agent write burst without CPU
  storm or overlapping helpers. Evidence:
  `./scripts/test-wiki-render-queue-stress.sh` and
  `swift test --package-path macos --filter OneContextWikiRuntimeTests`.

## Notes

- Current baseline: storage/behavior specs, runtime layout, typed Swift paths,
  tracked defaults, materializer, materializer validation, render-contract
  smoke, browser-contract smoke, custom-page fallback smoke, setup-state schema
  smoke, local API redaction tests, wiki-engine unit tests, package smoke,
  installed-path smoke, reinstall preservation, agent authoring, and render
  queue stress proof are in place.
- Known biggest gaps: none for this V0 checklist after the 2026-05-14 proof
  bundle; later work should focus on product page design, custom generated
  pages, and the next memory-core authoring layer rather than restoring deleted
  renderer/planner compatibility paths.
- Deletion policy: once a new path has proof, remove the old path. Do not keep
  broad backward-compatibility shims for pre-contract wiki layouts in the
  installed app.
- Immediate next step: cut over to follow-up product work only after reviewing
  the final diff for accidental old-path references and keeping the heartbeat
  disabled once this checklist is verified clean.
