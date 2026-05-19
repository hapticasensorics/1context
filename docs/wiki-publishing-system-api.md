# 1Context Wiki Publishing System API

- Status: canonical V0 API contract
- Last updated: 2026-05-19

This is the master contract for publishing 1Context wiki memory from
user-owned files into app-served static output. It covers authoring inputs,
site-map materialization, rendering, validation, Swift last-good publication,
local web serving, RuntimeDefaults backfill, package evidence, and the freeze
boundary.

Older goal docs record how this shape was reached. This document is the
starting point for building against it.

## One-Line Shape

```text
wiki.toml + templates -> materialized source -> JS render -> validated site -> Swift last-good publish
```

## Publishing Pipeline

The stable publishing pipeline is:

1. User-owned inputs live under `~/1Context/user-wiki` and
   `~/1Context/context-engine`.
2. `wiki.toml` declares available pages, navigation, aliases, and generated
   site pages.
3. The materializer creates only missing editable source pages, talk folders,
   and family-local templates.
4. RuntimeDefaults install copies only missing packaged defaults into user data
   and writes proposals for changed existing files.
5. The renderer builds a complete static site from actual user source into a
   staging directory.
6. Swift validates the staged site, promotes `~/1Context/user-wiki/site`, then
   publishes the last-good mirror to Application Support.
7. Local Web serves the Application Support mirror and redacted API routes.
8. Manifests, ledgers, route indexes, and harness artifacts record what
   happened.

## Ownership Rule

```text
User data is live truth.
RuntimeDefaults are seed and backfill material.
Swift publishes.
The JS wiki engine renders.
Memory agents author governed user-owned files.
```

No component may silently overwrite user-owned wiki source, talk, templates,
prompts, `_curator.md`, or `wiki.toml`.

## Component Map

Repo source:

```text
1context-public-launch/
  runtime/                         public-safe shipped defaults source
    1Context/user-wiki/
    1Context/context-engine/

  runtime-test/                    ignored local/private scenario lab

  wiki-engine/                     deterministic renderer package
    tools/materialize-wiki-pages.py
    tools/render-site.mjs
    tools/write-runtime-defaults-manifest.py
    src/renderer/
    theme/
    schemas/

  macos/Sources/
    OneContextPlatform/            runtime paths and permissions
    OneContextDaemon/              JSON-RPC daemon and render queue entrypoint
    OneContextLocalWeb/            Caddy/local API/static serving
    OneContextWikiRuntime/         defaults installer, renderer bridge, validator
```

Installed app bundle:

```text
1Context.app/
  Contents/MacOS/1Context
  Contents/MacOS/1contextd
  Contents/MacOS/1context-cli
  Contents/Resources/WikiEngine/
    tools/render-site.mjs
    src/
    theme/
    node_modules/
  Contents/Resources/RuntimeDefaults/
    1Context/user-wiki/
    1Context/context-engine/
    1Context/.1context/runtime-defaults-manifest.json
```

Installed user data:

```text
~/1Context/
  user-wiki/
    wiki.toml
    templates/
    assets/
    source/
    site/
  context-engine/

~/Library/Application Support/1Context/
  setup/
  wiki-site/current/
  wiki-site/previous/
  wiki-site/next/
  run/
  local-web/
```

## Stable Data API

### `wiki.toml`

`~/1Context/user-wiki/wiki.toml` is the user-owned site map.

Stable concepts:

- `[[pages]]` materialize editable source pages under `source/families/**`.
- `[[site_pages]]` declare generated pages, aliases, and diagnostics.
- Navigation order lives in `wiki.toml`, not in folder-name prefixes.
- Missing unconfigured routes diagnose; they do not redirect to
  `/your-context` or hidden bundled pages.
- Tombstoned source is not recreated by materialization.

### Source Families

Canonical source pages live under:

```text
~/1Context/user-wiki/source/families/<family-group>/<family-id>/source/<slug>.md
```

Talk folders live next to them:

```text
~/1Context/user-wiki/source/families/<family-group>/<family-id>/talk/<slug>.talk/
  _meta.yaml
  _conventions.md
  _curator.md
  <timestamp>.proposal.<title>.md
  <timestamp>.reply.<title>.md
  <timestamp>.close.<resolution>-<title>.md
```

Family-local templates live under:

```text
~/1Context/user-wiki/source/families/<family-group>/<family-id>/templates/
```

### Static Site

`~/1Context/user-wiki/site` is the latest successful render from canonical
source. It is user-copyable export output, but not the editing source of truth.

The app-served mirror is:

```text
~/Library/Application Support/1Context/wiki-site/current
```

Swift publishes only after validation. A failed render must preserve the
previous last-good mirror.

## Publishing Command API

### Materialize Configured Pages

Entrypoint:

```bash
python3 wiki-engine/tools/materialize-wiki-pages.py <runtime-home> [--dry-run]
```

`<runtime-home>` is shaped like `runtime-test/`:

```text
<runtime-home>/1Context/user-wiki/wiki.toml
<runtime-home>/1Context/user-wiki/templates/
<runtime-home>/Library/Application Support/1Context/setup/
```

Writes:

- missing source pages
- missing talk folders
- generated `group.toml` and `family.toml`
- family-local page/talk templates
- `Library/Application Support/1Context/setup/wiki-page-materialize.toml`

Does not overwrite existing files. Existing differing files are recorded as
`skipped_existing` in materialization state.

Current stdout:

```text
materialized_pages=<count> state=<state-path>
```

### Render Site

Entrypoint:

```bash
node wiki-engine/tools/render-site.mjs \
  --source-root <runtime-home>/1Context/user-wiki/source \
  --output <staging-site> \
  --result-json <result.json>
```

Inputs:

- source pages matching `source/families/<group>/<family>/source/*.md`
- talk folders ending in `.talk` with `_meta.yaml`
- non-tombstoned pages only
- bundled engine theme assets

Writes only to `--output`.

Successful result JSON:

```json
{
  "schema_version": 1,
  "status": "published",
  "rendered_at": "ISO-8601",
  "source_root": "/absolute/source/root",
  "output": "/absolute/staging/site",
  "route_manifest": ".1context/route-manifest.json",
  "content_index": ".1context/content-index.json",
  "route_count": 8,
  "markdown_twin_count": 8,
  "source_input_count": 4,
  "talk_input_count": 4,
  "assets": ["assets/theme.css"],
  "logs": []
}
```

Failed result JSON:

```json
{
  "schema_version": 1,
  "status": "failed",
  "rendered_at": "ISO-8601",
  "source_root": "/absolute/source/root",
  "output": "/absolute/staging/site",
  "source_input_count": 0,
  "talk_input_count": 0,
  "error": "human readable reason"
}
```

Stable output metadata:

```text
<site>/.1context/route-manifest.json
<site>/.1context/content-index.json
<site>/.1context/current-render.json
<site>/.1context/render-events.jsonl
```

### RuntimeDefaults Manifest

Build-only entrypoint:

```bash
python3 wiki-engine/tools/write-runtime-defaults-manifest.py \
  --runtime-defaults-root <dist/runtime-defaults/1Context> \
  --wiki-engine-root wiki-engine \
  --render-result <render-site-result.json> \
  --version <app-version> \
  --git-commit <commit-or-unknown> \
  [--git-dirty] \
  --output <runtime-defaults-root>/.1context/runtime-defaults-manifest.json
```

Stable manifest fields:

- `schema_version`
- `release_version`
- `source_control.git_commit`
- `source_control.git_dirty`
- `hashes.runtime_defaults_source`
- `hashes.runtime_defaults_site`
- `hashes.wiki_engine`
- `hashes.materializer`
- `hashes.renderer`
- `hashes.manifest_writer`
- `render_summary.status`
- `render_summary.route_count`
- `render_summary.markdown_twin_count`

## Swift API

### Runtime Paths

Production paths come from `RuntimePaths.current()`.

Debug-only override:

```bash
ONECONTEXT_DEV_RUNTIME_HOME=/path/to/runtime-test
```

This override is ignored in release builds.

### Defaults Installer

Type:

```swift
WikiRuntimeDefaultsInstaller(runtimePaths: paths).installMissingDefaults()
```

Inputs:

- discovered `Contents/Resources/RuntimeDefaults`
- or `ONECONTEXT_RUNTIME_DEFAULTS_DIR` override

Ledger:

```text
~/Library/Application Support/1Context/setup/runtime-defaults-install.json
```

Statuses:

- `missing_defaults`
- `already_current`
- `installed`
- `installed_with_conflicts`
- `failed`

Conflict proposals:

```text
~/1Context/context-engine/proposals/wiki/runtime-defaults/*.proposal.json
```

### Renderer Bridge

Type:

```swift
WikiRenderCoordinator(runtimePaths: paths).renderAndPublish(trigger: "wiki.prepare")
```

Renderer discovery:

- `ONECONTEXT_WIKI_ENGINE_DIR` override
- bundled `Contents/Resources/WikiEngine`

Node discovery:

- `ONECONTEXT_NODE`
- otherwise `/usr/bin/env node`

This is the main remaining production hardening point: public app builds should
eventually bundle or otherwise control the Node runtime the same way the app
now controls Caddy.

### Render Behavior

The coordinator:

- fingerprints `user-wiki/source`
- skips rendering if inputs are unchanged and the existing site validates
- renders into a private staging directory
- validates route manifest, markdown twins, and private/public boundaries
- atomically replaces `~/1Context/user-wiki/site`
- promotes that site to Application Support `wiki-site/current`
- preserves last-good output on failure

## Daemon API

The daemon executable is:

```text
1Context.app/Contents/MacOS/1contextd
```

It accepts newline-delimited JSON-RPC over the runtime Unix socket.

Stable methods:

- `health`
- `status`
- `version`
- `wiki.status`
- `wiki.start`
- `wiki.refresh`
- `wiki.stop`

`wiki.refresh` queues a render request. The render queue debounces, coalesces,
and backs off after failures.

## Local Web API

Local Web serves the app-support mirror, not live source:

```text
~/Library/Application Support/1Context/wiki-site/current
```

Core routes:

- wiki page routes such as `/for-you`, `/topics`, `/topics/talk`
- markdown twins such as `/topics.md`, `/topics.talk.md`
- health route `__1context/health`
- redacted local API routes under `/api/wiki/*`

Browser-visible output must not expose absolute local paths.

## Release And Package API

Build command:

```bash
./scripts/release-train.sh build --channel dev
```

The build:

1. copies public-safe `runtime/1Context` into `dist/runtime-defaults`
2. materializes configured pages
3. renders `RuntimeDefaults/1Context/user-wiki/site`
4. writes `runtime-defaults-manifest.json`
5. bundles `RuntimeDefaults` and `WikiEngine`
6. validates the app and DMG

Package proof:

```bash
ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh
```

RuntimeDefaults scenario proof:

```bash
./scripts/test-wiki-runtime-defaults-scenarios.sh
```

Browser route proof:

```bash
./scripts/test-wiki.sh
```

## Freeze Boundary

Freeze now:

- `wiki.toml` page registry semantics
- materializer no-overwrite behavior
- `render-site.mjs` CLI arguments
- render result JSON shape
- route manifest and content index names
- RuntimeDefaults manifest fields
- Swift installer ledger shape
- conflict proposal location
- `wiki.refresh` as the render trigger

Do not freeze yet:

- internal renderer module layout
- visual theme implementation
- `enhance.js` internals
- dev-only tools other than the documented commands
- Node runtime packaging strategy

## Compatibility Rule

Until this API is declared V1, the repo may break old internal paths and old
experimental helpers. It must not break the documented V0 inputs and outputs
without updating this file and the scenario tests in the same change.
