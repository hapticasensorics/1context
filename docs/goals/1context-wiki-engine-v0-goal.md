---
title: 1Context Wiki Engine V0 Goal
slug: 1context-wiki-engine-v0-goal
section: development
access: private
summary: "A minimal production-shaped goal for restoring Open Wiki from user-owned source, last-good static renders, and the repo-local runtime mirror before reintroducing memory agents."
status: draft
last_updated: 2026-05-14
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Wiki Engine V0 Goal

## Goal

Restore the smallest production-shaped wiki engine slice: Open Wiki should serve
a last-good static website rendered from user-owned wiki source.

This goal proves the user-data contract before the memory engine returns. The
wiki source, talk pages, templates, render metadata, and static export should
live in the locations defined by [User Data Spec](../user-data-spec.md). The
repo-local development mirror should follow [Repo Runtime Layout](../../runtime/README.md).
The memory-authoring, Swift-publication, and JS-renderer handoff is defined by
[Wiki Memory Publication Contract](../wiki-memory-publication-contract.md).

## Runtime Architecture

V0 uses a Swift-supervised, bundled-JS renderer.

```text
Swift daemon
  -> owns paths, permissions, setup, page registry, page lifecycle, staging,
     atomic promotion, local-web status, and diagnostics
  -> invokes a bundled renderer helper

bundled JS wiki-engine
  -> owns Markdown/frontmatter/talk-folder rendering and static asset generation
  -> has no network access and no runtime dependency install step

Python memory-core
  -> remains development and memory infrastructure
  -> is not required by the installed app to open the wiki
```

This keeps the app loop small and knowable. Swift controls user data and
process lifecycle; the renderer remains the renderer. The installed product
must not require a source checkout, `uv run`, system Python, system Node,
`npm install`, or `npm ci` to open the wiki.

## Boundary

This is a wiki-engine goal, not a memory-engine goal.

Included:

- create `user-wiki/source` and optionally import local fixture data for testing
- keep templates and talk conventions under `user-wiki`
- render only accepted `user-wiki/source` for canonical Open Wiki/export output
- render `user-wiki/source` to `user-wiki/site`
- write `site/.1context/current-render.json`
- append `site/.1context/render-events.jsonl`
- serve or mirror the last successful `user-wiki/site` through Open Wiki
- use the repo-local runtime mirror for development proof

Excluded:

- curator and librarian execution
- agent hiring
- broad memory jobs
- embeddings
- vector/search indexing
- hidden context-engine hot patches in canonical renders
- bundled `memory-core` source checkout
- bundled Python memory-core runtime loop
- runtime `npm install` or `npm ci`
- system Node dependency
- long-running Python web server
- broad runtime path override environment hooks

Vector/search indexes are intentionally out of the first slice. When they
return, they must be derived Application Support machinery rebuilt from
`user-wiki` and `context-engine`, not canonical context-engine truth.

## Done When

- A clean dev runtime can be initialized with:

  ```text
  runtime-test/1Context/user-wiki/
  runtime-test/1Context/context-engine/
  runtime-test/Library/Application Support/1Context/
  ```

- The same code path can target installed production paths through typed path
  configuration rather than broad product fallback environment variables.
- A local fixture wiki page family can include article source, talk, local
  templates, and enough structure for renderable site output without tracking
  personal data in the public repo.
- Rendering is atomic: a failed render updates `latest_attempt` and the JSONL
  ledger without replacing `last_success` or serving a partial site.
- Canonical render/export uses accepted `user-wiki/source` only. Proposal or
  patch previews may exist under `context-engine/artifacts`, but they do not
  update `user-wiki/site` until accepted changes are promoted into source.
- `user-wiki/site/` can be copied as a standalone static website export.
- Open Wiki opens the last successful rendered site.
- Tests or smoke scripts prove the repo-local path and the installed-path
  contract separately.
- Package checks still forbid generated runtime state, source checkouts, runtime
  dependency installation, long-running Python web serving, and accidental
  reliance on host Python or host Node.

## Local Review Fixture Inventory

The first local fixture pass should be methodical, not a broad copy of the old
runtime. The fixture itself belongs in ignored local runtime state, not in the
public repo.

For local review, use an ignored fixture shaped exactly like `runtime-test/`.
That fixture may be copied from a private experiment or hand-authored locally,
but the fixture path and contents should not be tracked in this repo.

Expected fixture shape:

```text
<local-fixture>/
  1Context/
    user-wiki/
    context-engine/
```

Current-repo shell sources:

- `runtime/1Context/user-wiki/wiki.toml`
- `runtime/1Context/user-wiki/source/families/**`
- `runtime/1Context/user-wiki/templates/**`
- `wiki-engine/src/renderer/**`
- `wiki-engine/tools/render-site.mjs`

The public-launch repo should track the folder contract, helper scripts,
synthetic tests, and docs, not personal wiki content.

Explicitly excluded from tracked V0 files:

- private experiment scripts, run outputs, and generated site artifacts
- `memory-core/memory/runtime/**`
- `wiki-engine/node_modules/**`
- raw observations, run transcripts, private previews, and generated index data

The only agent prompt copied in Chunk 2 is the page-local `_curator.md`, because
that file is part of the user-editable talk/page policy surface.

## Implementation Chunks

### Chunk 1: Path Roots And Blessed Runtime Folders (Review Slice)

Create the typed path layer and initializer for:

```text
runtime/1Context/user-wiki/
runtime/1Context/context-engine/
runtime/Library/Application Support/1Context/

runtime-test/1Context/user-wiki/
runtime-test/1Context/context-engine/
runtime-test/Library/Application Support/1Context/
```

Acceptance:

- repo `runtime/` contains the tracked production-shaped, public-safe data tree
- dev mode can initialize those folders in `runtime-test`
- production mode resolves to the real installed paths
- only a debug-build runtime switch is introduced
- generated `runtime-test/**` state remains ignored by git

Review implementation:

- `RuntimePaths` exposes typed roots for `user-wiki`, `context-engine`, and
  Application Support derived indexes.
- debug builds may use `ONECONTEXT_DEV_RUNTIME_HOME` to point at `runtime-test`
- the daemon creates those roots during startup preparation
- `scripts/init-dev-wiki-runtime.sh` copies the `runtime/` tree into
  `runtime-test`
- `.gitignore` keeps generated `runtime-test/**` out of source control
- `runtime/` is the tracked blessed public-safe data tree, including reviewed
  wiki source, talk, templates, static site output, and context-engine
  configuration
- `runtime/.gitignore` blocks scratch import folders, private probes, app
  machinery, logs, caches, and heavy derived indexes, while leaving the blessed
  `runtime/1Context` user-data surfaces trackable after review

### Chunk 2: Local User-Wiki Fixture Import (Review Slice)

When a review fixture is needed, create the smallest neutral sample wiki through
the page lifecycle into `user-wiki/source`:

```text
user-wiki/source/families/<family-group>/<family-id>/
  family.toml
  source/<page-slug>.md
  talk/<page-slug>.talk/
    _meta.yaml
    _conventions.md
    _curator.md
    <timestamp>.synthesis.<short-title>.md
  templates/
    page.template.md
    talk/
      _meta.template.yaml
      _conventions.template.md
      _curator.template.md
      entry.template.md
```

Acceptance:

- local fixture import writes only missing files or unchanged imported files
- edited user files are not overwritten
- the fixture is inspectable as normal Markdown/TOML/YAML under ignored
  `runtime-test`

Review implementation:

- no personal wiki content is tracked in this repo
- local review fixtures can be passed to `scripts/init-dev-wiki-runtime.sh`
- synthetic fixture data in the smoke test proves the copy/import behavior
- the smoke test uses a temporary runtime root and writes a namespaced smoke
  fixture there instead of contaminating the persistent development runtime
- the smoke test runs the initializer twice to prove idempotent install

Current proof:

```bash
./scripts/test-wiki.sh
swift test --package-path macos --filter PathAndPermissionTests
```

### Chunk 3: Template Normalization

Decide and enforce the template locations before adding more families:

```text
user-wiki/templates/pages/
user-wiki/templates/talk/
user-wiki/templates/site/
user-wiki/source/families/<group>/<family>/templates/
```

Acceptance:

- page-local templates stay beside their family when they express local policy
- reusable wiki templates live under `user-wiki/templates`
- talk conventions and curator files remain part of `user-wiki`
- global agent prompts stay under `context-engine`, not `user-wiki`

### Chunk 3.5: Page Registry And First-Run Defaults

Make `user-wiki/wiki.toml` the user-owned site map.

Acceptance:

- configured pages define id, route, family, source template, talk convention
  template, and curator template
- initializing dev runtime creates missing configured source files from
  templates
- initializing dev runtime creates the matching talk folder, `_meta.yaml`,
  `_conventions.md`, and `_curator.md`
- edited source and talk files are not overwritten
- tombstoned pages are not silently recreated
- unconfigured routes diagnose missing pages instead of falling back to
  `/your-context`
- page lifecycle state is recorded under Application Support setup

### Chunk 4: Render To Last-Good Static Site

Bundle the existing JS wiki renderer as a small app resource and wrap it with a
Swift render coordinator.

Renderer resource contract:

```text
1Context.app/Contents/Resources/wiki-engine/
  package.json
  tools/render-site.mjs
  src/renderer/**
  node_modules/**          # vendored at build time or replaced by a single bundled executable
```

The runtime invocation must be deterministic:

```text
render-site
  --source-root <user-wiki>
  --output <staging-site>
  --result-json <render-result.json>
```

If the renderer stays as JS, the package must include either a bundled Node
runtime or a single packaged helper executable. The app must never search for
developer-local Node or run package installation on the user's machine.

Swift render coordinator responsibilities:

- read `user-wiki/wiki.toml`
- create configured missing source and talk files through page lifecycle before rendering
- create a unique staging directory under Application Support
- invoke the renderer once per configured source page and talk folder
- collect renderer JSON output, stdout, stderr, exit status, and file hashes
- synthesize `.1context/route-manifest.json`, `.1context/content-index.json`, and minimal
  `api/wiki/pages.json`
- validate the staged site before promotion
- write render events and current-render state
- atomically promote staging to `user-wiki/site`
- mirror last-good `user-wiki/site` to Application Support local-web current

Acceptance:

- render reads accepted `user-wiki/source` only
- render writes to staging first
- successful render atomically updates `user-wiki/site`
- failed render records `latest_attempt` and leaves `last_success` untouched
- `site/.1context/render-events.jsonl` is appended
- `site/.1context/current-render.json` is updated
- proposal previews, if any, write only under `context-engine/artifacts`
- renderer failures preserve the previous served site and record stdout, stderr,
  exit code, and failed input path
- render output contains no unresolved `{{ placeholder }}` strings in source
  pages, talk metadata, or generated navigation
- no host Python, host Node, source-checkout path, or runtime package install is
  needed for the installed app path

### Chunk 5: Open Wiki Integration

Wire Open Wiki to the last successful site and delete hardcoded placeholder
routes.

Acceptance:

- Open Wiki serves or mirrors `user-wiki/site`
- Application Support contains only disposable local-web serving mirrors and
  metadata
- copied `user-wiki/site/` works as a standalone static website export
- diagnostics can explain current, stale, failed, or uninitialized render state
- `LocalWebDefaults` does not hardcode `/your-context` as a hidden fallback for
  arbitrary missing routes
- missing configured pages trigger page creation or a repair diagnostic
- missing unconfigured routes show a missing-page diagnostic rather than
  redirecting to another page
- `wiki.refresh` runs the Swift render coordinator and returns render status

### Chunk 6: Proof And Packaging Guardrails

Add proof before expanding the system.

Acceptance:

- dev smoke initializes `runtime-test`, imports fixture data, renders, and
  verifies exported HTML plus render metadata
- package smoke confirms no generated runtime state, generated wiki outputs,
  source checkouts, runtime dependency install, broad path overrides, or host
  interpreter dependence ship in the app
- docs point from the goal to the user-data spec and runtime layout

Proof commands for V0:

```bash
./scripts/test-wiki.sh
swift test --package-path macos --filter PathAndPermissionTests
swift test --package-path macos --filter WikiRenderCoordinatorTests
./scripts/release-train.sh build --channel dev
```

Browser proof should open the local wiki after `wiki.refresh`, verify the home
route, the configured page routes, rendered talk pages, and a deliberately
missing route.

### Later: Retrieval And Memory Engine

Only after V0 is solid:

- add vector/search indexes as Application Support derived indexes
- add `context-engine/indexes` manifests and rebuild ledgers
- add curator/librarian jobs
- add proposal preview renders
- allow accepted proposals to promote source edits
- reintroduce Python memory-core orchestration as a development and memory-layer
  subsystem once the app-native wiki loop is stable

## Open Questions

- Should the JS renderer ship with a bundled Node runtime, or should the build
  produce a single renderer helper executable?
- What is the smallest local fixture page family that meaningfully proves talk,
  templates, markdown twins, and static export?
- Should Open Wiki render on explicit refresh only for V0, or also on source
  change detection?
- What diagnostic should the menu show when source is newer than the last
  successful site?
