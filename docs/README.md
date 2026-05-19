# 1Context Docs

This folder is the maintainer source for release, runtime, and product-boundary
decisions. The root README stays product-facing; this index points to the docs
that operators and contributors should use.

## Start Here

- [Wiki Publishing System API](wiki-publishing-system-api.md): canonical V0
  contract for user data, RuntimeDefaults, the JS renderer, Swift publication,
  daemon refresh, Local Web serving, package evidence, and freeze boundaries.
- [Wiki Publishing System Runbook](wiki-publishing-system-runbook.md):
  operating guide for local fixtures, custom pages, RuntimeDefaults proofs,
  browser validation, memory agents, Swift startup, and release packaging.
- [Wiki Memory Runtime V0 Goal](goals/1context-wiki-memory-runtime-v0-goal.md):
  historical close-loop checklist for the user-owned wiki runtime, Swift
  publisher, bundled renderer, local web proof, and future memory-core
  authoring surfaces.
- [Wiki Runtime Production Shape Goal](goals/1context-wiki-runtime-production-shape-goal.md):
  historical cleanup checklist for retiring duplicate shipped wiki runtime
  artifacts and proving the `RuntimeDefaults` plus `WikiEngine` app shape.
- [Archived Delete Bloat Goal](goals/archive/1context-delete-bloat-goal.md):
  historical cleanup evidence for deleting old release paths, compatibility
  shims, stale docs, generated state, and product test hooks.
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
- [Local Web Contract](local-web-contract.md): local HTTPS, Caddy, static wiki
  publication, daemon API, cloud compatibility, and user-wiki boundaries.
- [Wiki Productionization Plan](wiki-productionization.html): where wiki source,
  default templates, generated files, and served local-web artifacts live in the
  repo and on an installed Mac.
- [Wiki Publishing System API](wiki-publishing-system-api.md): canonical API and
  freeze boundary for the full wiki publishing subsystem.
- [User Data Spec](user-data-spec.md): filesystem and persisted-data contract
  for `~/1Context`, Application Support, first-run defaults, runtime mirrors,
  templates, source families, talk files, ledgers, indexes, and static export.
- [Wiki Memory Publication Contract](wiki-memory-publication-contract.md):
  behavior contract for Python memory authoring, Swift publication, render
  scheduling, proposal promotion, local APIs, and the bundled JS renderer.
- [Repo Runtime Layout](../runtime/README.md): repo-local development mirror for
  the production user-data and Application Support paths.
- [Permissions](../PERMISSIONS.md): ownership, consent, storage, privacy, and
  diagnostic invariants.

## CI And Proof

- [Self-hosted Mac Runner](ci/self-hosted-mac-runner.md): protected real-Mac
  updater proof for release hops that need installed-app evidence.
- [Archived Release Lockdown Goal](goals/archive/1context-release-lockdown-goal.md):
  historical Sparkle-transition evidence.
- [Goals Folder](goals/README.md): policy for keeping operator goals out of the
  installed user wiki.

## Assets

- `assets/readme/`: README screenshots.

## Retired Docs

The old professional-app milestone, remaining-work, and install-distribution
checklist docs have been merged into the macOS release runbook. Keep future
release operations there unless a new doc has a clearly separate owner.
