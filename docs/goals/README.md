# 1Context Development Goals

This folder holds operator and release-train goals for the 1Context app.

These documents are not shipped as local wiki pages. The installed user wiki
should stay focused on user-facing templates and user content. If a goal needs a
live view during development, use a docs or preview workflow rather than copying
it into the installed app's `wiki-site/current` release site.

Current goals:

- [1Context Wiki Runtime Production Shape Goal](1context-wiki-runtime-production-shape-goal.md)
  - Status: complete. Retired the duplicate `release/memory-runtime` artifact
    path and proved the single `RuntimeDefaults` plus `WikiEngine` bundle
    shape.
- [1Context Wiki Engine V0 Goal](1context-wiki-engine-v0-goal.md)
  - Status: renderer slice extracted into `wiki-engine/`; remaining closure is
    tracked by current contracts and production-shape proof.
- [1Context Agent Mail Protocol Goal](1context-agent-mail-protocol-goal.md)
  - Status: planning. Defines the implementation boundary for bringing agent
    mail back beside the wiki runtime as a clean transport kernel.
- [1Context Codex Adapter Implementation Goal](1context-codex-adapter-implementation-goal.md)
  - Status: active. Tracks the Rust adapter spine for Codex app-server binding,
    hooks, wake dispatch, body injection, event mirroring, and redacted proof.

Archived goals:

- [1Context Delete Bloat Goal](archive/1context-delete-bloat-goal.md)
- [1Context Release Lockdown Goal](archive/1context-release-lockdown-goal.md)
