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
- [1Context Wiki Memory Runtime V0 Goal](1context-wiki-memory-runtime-v0-goal.md)
  - Status: superseded for final cleanup by the production-shape goal. Swift render coordination, runtime
    materialization, browser route proof, package smoke, wiki-interface cleanup,
    and custom-page fallback proof are in place.
- [1Context Wiki Engine V0 Goal](1context-wiki-engine-v0-goal.md)
  - Status: renderer slice extracted into `wiki-engine/`; current closure work
    is tracked by the Wiki Memory Runtime V0 goal.
- [1Context Release Factory Goal](1context-release-factory-goal.md)

Archived goals:

- [1Context Delete Bloat Goal](archive/1context-delete-bloat-goal.md)
- [1Context Release Lockdown Goal](archive/1context-release-lockdown-goal.md)
