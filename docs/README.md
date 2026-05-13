# 1Context Docs

This folder is the maintainer source for release, runtime, and product-boundary
decisions. The root README stays product-facing; this index points to the docs
that operators and contributors should use.

## Start Here

- [macOS Release Runbook](macos-release-runbook.md): current release packaging,
  local proof, update policy, and self-hosted Mac proof guidance.
- [Development And Release Notes](development.md): maintainer details for local
  files, hooks, memory-core adapter, local web, tests, and packaging.
- [Update Policy](update_policy.html): founder-controlled policy for mandatory
  and optional updates, updater copy, failure copy, menu behavior, and rare
  post-install messages.
- [Roadmap](../ROADMAP.md): current product and release-train priorities.

## Contracts

- [macOS App Architecture](macos-app-architecture.md): app-owned setup,
  permissions, update, install, and local-web boundaries.
- [Local Web Contract](local-web-contract.md): local HTTPS, Caddy, static wiki
  publication, daemon API, cloud compatibility, and user-wiki boundaries.
- [Memory Core Contract](memory-core-contract.md): bounded JSON subprocess
  bridge between the public Swift shell and the Python memory engine.
- [Permissions](../PERMISSIONS.md): ownership, consent, storage, privacy, and
  diagnostic invariants.

## Subsystems

- [Memory Core](../memory-core/README.md): Python memory engine boundary and
  public Swift adapter entry point.
- [Wiki Workspace](../memory-core/wiki/README.md): file layout, publication
  boundary, and user-wiki policy.
- [Wiki Engine](../memory-core/wiki-engine/README.md): Node renderer, theme,
  manifests, and future package boundary.

## CI And Proof

- [Self-hosted Mac Runner](ci/self-hosted-mac-runner.md): protected real-Mac
  updater proof for release hops that need installed-app evidence.
- [Release Lockdown Goal](goals/1context-release-lockdown-goal.md): historical
  and current release-train checklist evidence.
- [Goals Folder](goals/README.md): policy for keeping operator goals out of the
  installed user wiki.

## Assets

- `assets/readme/`: README screenshots.
- `assets/update-policy/`: updater policy screenshots.

## Retired Docs

The old professional-app milestone, remaining-work, and install-distribution
checklist docs have been merged into the macOS release runbook. Keep future
release operations there unless a new doc has a clearly separate owner.
