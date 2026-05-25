# 1Context Docs

This folder is the maintainer source for release, runtime, and product-boundary
decisions. The root README stays product-facing; this index points to the docs
that operators and contributors should use.

## Start Here

- [Wiki Publishing System API](wiki-publishing-system-api.md): canonical V0
  contract for user data, RuntimeDefaults, page lifecycle, page assets, page
  talk, the renderer boundary, portable publication, Local Web serving,
  package evidence, and freeze boundaries.
- [Wiki System Architecture](wiki-system-architecture.md): internal shape for
  the portable wiki core, inventory compiler, page ledger, lifecycle service,
  renderer, publisher, and host boundaries.
- [Wiki Publishing System Runbook](wiki-publishing-system-runbook.md):
  operating guide for local fixtures, custom pages, RuntimeDefaults proofs,
  browser validation, memory agents, macOS startup, and release packaging.
- [Wiki Agent Use Story](wiki-agent-use-story.md): narrative walkthrough of
  how agents should inspect wiki state, edit pages, add assets, append talk,
  publish, validate, and leave evidence without path guessing.
- [Agent Tool Gateway](agent-tool-gateway.md): generic agent-facing tool
  contract with one consolidated backend and two visible toolsets,
  `toolset-mail` and `toolset-wiki`.
- [1Context Codex Adapter Spec](1context-codex-adapter-spec.md): implementation
  contract for the Codex-specific runtime bridge that owns app-server calls,
  hook installation, wake dispatch, body injection, event mirroring, and
  adapter proof.
- [Codex Hook Control And Mail Wakeup Spec](codex-hook-control-spec.md):
  control-plane contract for Codex steering, lifecycle hooks, notification
  dispatch, and autonomous mail correctness.
- [Agent Mail Protocol](agent-mail-protocol.md): design contract and V0 proof
  surface for durable agent mail, async ready/valid semantics, backpressure,
  talk-page inboxes, notifications, and governance workflows over proposals and
  artifacts.
- [Release Factory Goal](goals/1context-release-factory-goal.md): active
  priority for channel-aware dev, prototype, private, and official release
  builds with no backwards-compatible shims, no Homebrew dependency for
  distributable artifacts, and no revival of deleted release paths.
- [macOS Release Runbook](macos-release-runbook.md): current release packaging,
  local proof, update policy, and self-hosted Mac proof guidance.
- [Development And Release Notes](development.md): maintainer details for local
  files, local web, tests, and packaging.
- [Roadmap](../ROADMAP.md): current product and release-train priorities.

## Contracts

- [macOS App Architecture](macos-app-architecture.md): app-owned setup,
  permissions, update, install, and local-web boundaries.
- [macOS Capture System](macos-capture-system.md): native window graph,
  ScreenCaptureKit/CoreGraphics/AX capture spine, dynamic capture policy, and
  current daemon/CLI snapshot surface.
- [Capture System Implementation Spec](capture-system-implementation-spec.md):
  implementation plan for the native capture runtime from macOS sensors through
  short-lived READY capture bundles, stopping before attention filtering and
  Timescale memory writes.
- [Capture Window Bundle Spec](capture-window-bundle-spec.md): ephemeral
  Application Support file-bundle contract for handing time-aligned capture
  evidence to the attention-filter agent before durable selected output is
  written to memory DB.
- [Memory DB Design Spec](memory-db-design-spec.md): Postgres plus TimescaleDB
  Perception DB temporal object store with `perception.objects` as the product
  time spine.
- [Perception DB Schema Layout](perception-db-schema-layout.md): canonical
  schema layout where lanes are presentation, series are identity, objects are
  temporal records, blobs are bytes, and edges are meaning.
- [Memory DB API And Protocol Spec](memory-db-api-protocol-spec.md): canonical
  Rust-owned read/write protocol for writers, viewers, agents, and local web
  adapters.
- [Memory DB Infra And Viewer Spec](memory-db-infra-viewer-spec.md):
  infrastructure and viewer contract for local memory operation.
- [Memory Source Connectors Spec](memory-source-connectors-spec.md): connector
  discovery, access, cursoring, and source-record rules for app data imports.
- [Coding Agent Ingest Spec](coding-agent-ingest-spec.md): Codex, Claude, and
  future 1Context agent-session reduction into one compact Perception DB format.
- [Semantic Observation System](semantic-observation-system.md): downstream
  reconstruction layer for per-minute attention captures and seen-surface
  composites.
- [Semantic Observation Output Contract](semantic-observation-output-contract.md):
  visual-first contract for attention-highlighted screenshots, full development
  composites, and up to three attended items per minute.
- [Attention Capture Mockup](attention-capture-mockup.html): standalone HTML
  mockup of the per-minute final output and development/debug view.
- [Attention Dashboard Skeleton Schema](attention-dashboard-skeleton-schema.md):
  native Rust/egui judge dashboard contract for video-side review, attention
  output inspection, timeline lanes, review labels, and four-agent
  implementation split.
- [Local Web Contract](local-web-contract.md): local HTTPS, Caddy, static wiki
  publication, daemon API, cloud compatibility, and user-wiki boundaries.
- [User Data Spec](user-data-spec.md): filesystem and persisted-data contract
  for `~/1Context`, Application Support, first-run defaults, runtime mirrors,
  templates, source families, talk files, ledgers, indexes, and static export.
- [Repo Runtime Layout](../runtime/README.md): repo-local development mirror for
  the production user-data and Application Support paths.
- [Permissions](../PERMISSIONS.md): ownership, consent, storage, privacy, and
  diagnostic invariants.

## CI And Proof

- [Self-hosted Mac Runner](ci/self-hosted-mac-runner.md): protected real-Mac
  updater proof for release hops that need installed-app evidence.
- [Archived Release Lockdown Goal](goals/archive/1context-release-lockdown-goal.md):
  historical Sparkle-transition evidence.
- [Wiki Memory Runtime V0 Goal](goals/1context-wiki-memory-runtime-v0-goal.md):
  historical close-loop checklist for the user-owned wiki runtime, publisher,
  bundled renderer, local web proof, and future memory authoring surfaces.
- [Wiki Runtime Production Shape Goal](goals/1context-wiki-runtime-production-shape-goal.md):
  historical cleanup checklist for retiring duplicate shipped wiki runtime
  artifacts and proving the `RuntimeDefaults` plus `WikiEngine` app shape.
- [Archived Delete Bloat Goal](goals/archive/1context-delete-bloat-goal.md):
  historical cleanup evidence for deleting old release paths, compatibility
  shims, stale docs, generated state, and product test hooks.
- [Goals Folder](goals/README.md): policy for keeping operator goals out of the
  installed user wiki.

## Assets

- `assets/readme/`: README screenshots.

## Retired Docs

- [Wiki Memory Publication Contract](wiki-memory-publication-contract.md) has
  been folded into the current wiki API, architecture, runbook, agent story,
  and user-data spec.
- [Wiki Productionization Plan](wiki-productionization.html) has been folded
  into the current wiki API, architecture, runbook, and user-data spec.
- [Archived Docs](archive/README.md) keeps historical source copies for
  provenance.
- The old professional-app milestone, remaining-work, and install-distribution
  checklist docs have been merged into the macOS release runbook. Keep future
  release operations there unless a new doc has a clearly separate owner.
