# 1Context Docs

This folder is the maintainer source for release, runtime, and product-boundary
decisions. The root README stays product-facing; this index points to the docs
that operators and contributors should use.

## Start Here

- [Delete Bloat Goal](goals/1context-delete-bloat-goal.md): active priority for
  deleting old release paths, compatibility shims, stale docs, generated state,
  and product test hooks before adding more release machinery.
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
- [Wiki Productionization Spec](wiki-productionization.md): where wiki source,
  default templates, generated files, and served local-web artifacts live in the
  repo and on an installed Mac.
- [Permissions](../PERMISSIONS.md): ownership, consent, storage, privacy, and
  diagnostic invariants.

## CI And Proof

- [Self-hosted Mac Runner](ci/self-hosted-mac-runner.md): protected real-Mac
  updater proof for release hops that need installed-app evidence.
- [Release Lockdown Goal](goals/1context-release-lockdown-goal.md): historical
  and current release-train checklist evidence.
- [Goals Folder](goals/README.md): policy for keeping operator goals out of the
  installed user wiki.

## Assets

- `assets/readme/`: README screenshots.

## Retired Docs

The old professional-app milestone, remaining-work, and install-distribution
checklist docs have been merged into the macOS release runbook. Keep future
release operations there unless a new doc has a clearly separate owner.
