# 1Context Wiki Publishing System API

- Status: canonical V0 publishing contract plus consumer API target
- Last updated: 2026-05-19

This is the master contract for publishing 1Context wiki memory from
user-owned files into app-served static output. It covers authoring inputs,
site-map materialization, rendering, validation, Swift last-good publication,
local web serving, RuntimeDefaults backfill, package evidence, and the freeze
boundary. It also distinguishes the current shipped trigger from the cleaner
consumer API the wiki system should grow into.

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

## Consumer Surface

This API is primarily for consumers that need to change or observe the wiki:

| Consumer | Reads | Writes | Calls | Success proof |
| --- | --- | --- | --- | --- |
| Memory agent | `wiki.toml`, source, talk, proposals, render ledgers | user-owned source, talk entries, proposals, accepted template/site-map changes | `wiki.refresh` through the daemon after durable edits | `wiki.status`, `site/.1context/current-render.json`, route manifest contains expected route |
| Operator/editor | markdown source and talk folders | markdown source, talk entries, `wiki.toml` | app menu refresh, daemon `wiki.refresh`, or fixture render command | page appears at Local Web route and markdown twin exists |
| Swift app/daemon | RuntimeDefaults, user source, previous site | setup ledgers, `user-wiki/site`, app-support mirror | `installMissingDefaults()`, `renderAndPublish(trigger:)` | published result or failed result with last-good preserved |
| Local Web/browser | app-support `wiki-site/current` | local UI state only under `/api/wiki/state` | static HTTP plus `/api/wiki/*` | no source paths leak; current route manifest is valid |
| Release/package | repo `runtime/1Context`, `wiki-engine` | bundled `RuntimeDefaults`, bundled `WikiEngine`, manifests | release train build | package smoke and RuntimeDefaults scenario proof pass |

Consumers should treat this as a whole-site publishing system. V0 does not have
a page-scoped render API. To render one changed page, edit that page's
user-owned files and request `wiki.refresh`; Swift fingerprints the whole
source tree and either re-renders the site or skips the renderer when the
accepted inputs are unchanged and the existing site validates.

Current implementation gap: the daemon trigger publishes whatever source files
already exist. It does not yet own materialization from `wiki.toml`, and the
Python materializer is used by build/test/dev harnesses rather than bundled as
the production daemon's page creation engine. The target consumer API below
closes that gap by making materialization an explicit publish-stage option.

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

### Configured Page Placement

A source-backed `[[pages]]` entry is the registry record. It is not the
rendered page itself. Given:

```toml
[[pages]]
id = "topics"
slug = "topics"
route = "/topics"
family_group = "reference"
family_id = "topics"
template = "pages/e08/topics.md"
talk_conventions_template = "talk/conventions/topics.md"
talk_curator_template = "talk/curators/topics.md"
```

The materializer creates or verifies this user-owned shape:

```text
~/1Context/user-wiki/source/families/reference/group.toml
~/1Context/user-wiki/source/families/reference/topics/family.toml
~/1Context/user-wiki/source/families/reference/topics/source/topics.md
~/1Context/user-wiki/source/families/reference/topics/talk/topics.talk/_meta.yaml
~/1Context/user-wiki/source/families/reference/topics/talk/topics.talk/_conventions.md
~/1Context/user-wiki/source/families/reference/topics/talk/topics.talk/_curator.md
~/1Context/user-wiki/source/families/reference/topics/templates/page.template.md
~/1Context/user-wiki/source/families/reference/topics/templates/talk/_conventions.template.md
~/1Context/user-wiki/source/families/reference/topics/templates/talk/_curator.template.md
~/1Context/user-wiki/source/families/reference/topics/templates/talk/entry.template.md
```

Template lookup is always relative to `~/1Context/user-wiki/templates`.
Absolute template paths, `..`, empty path segments, and paths outside
`templates/` are invalid.

### Template Fallback And Tombstones

Configured pages use templates as a fallback only when the user-owned source or
talk file is missing.

Rules:

- Missing source is created from the configured page template.
- Missing talk metadata is generated from the page registry.
- Missing `_conventions.md` and `_curator.md` are created from their configured
  talk templates when present.
- Existing files are never overwritten by materialization.
- Existing differing files are recorded as `skipped_existing` in
  `wiki-page-materialize.toml`.
- `source/<slug>.tombstone.toml` blocks recreation of that page.
- Templates are copied into the family-local `templates/` folder so future
  agents can inspect the shape that created the page.

This is the fallback system for user-configured pages. It is different from
RuntimeDefaults. RuntimeDefaults backfill packaged defaults; page templates
create missing user-owned pages from the user's `wiki.toml`.

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

### Render Discovery

The renderer does not read `wiki.toml` to discover render inputs. It renders the
source tree shape:

```text
source/families/<group>/<family>/source/*.md
source/families/<group>/<family>/talk/*.talk/_meta.yaml
```

The current renderer skips:

- source files with a sibling `<slug>.tombstone.toml`
- files named `*.tombstone.md`
- talk folders whose page has a matching source tombstone

This separation matters for agents: adding `[[pages]]` to `wiki.toml` only
changes the registry. The page becomes renderable after materialization creates
the source and talk files.

### Published Route Placement

For a source page whose rendered frontmatter has `slug: topics`, the renderer
writes:

```text
<site>/topics.html
<site>/topics/index.html
<site>/topics.md
```

For a talk folder `topics.talk`, the renderer writes:

```text
<site>/topics.talk.html
<site>/topics/talk/index.html
<site>/topics.talk.md
```

Section subpages declared by renderer-supported section metadata become nested
route and markdown twins such as:

```text
<site>/topics/engineering.html
<site>/topics/engineering.md
```

The source of truth for what actually rendered is the route manifest, not the
registry:

```text
<site>/.1context/route-manifest.json
<site>/.1context/content-index.json
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

Current production caveat: this command is Python and is not bundled into
`Contents/Resources/WikiEngine` today. Build and test harnesses call it; the
greenfield production shape should move materialization into the bundled wiki
engine or a Swift-owned materialization bridge so daemon consumers can add
configured pages without reaching into repo-only tooling.

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

Fingerprint scope is currently the source directory only. Template or
`wiki.toml` edits affect a render only after they change materialized source or
talk files. The target API should fingerprint the complete publish input set:
`wiki.toml`, source, talk, templates, assets, renderer version, and
materialization state.

Failure behavior:

- renderer failure removes the failed staging directory
- validation failure blocks promotion
- `~/Library/Application Support/1Context/wiki-site/current` remains the
  previously valid last-good site
- `~/1Context/user-wiki/site/.1context/current-render.json` records failure
  when a source site exists
- the render queue records the failed trigger, error string, and backoff state

## Daemon API

The daemon executable is:

```text
1Context.app/Contents/MacOS/1contextd
```

It accepts newline-delimited JSON-RPC over the runtime Unix socket.

Current methods:

- `health`
- `status`
- `version`
- `wiki.status`
- `wiki.start`
- `wiki.refresh`
- `wiki.stop`

`wiki.refresh` currently ignores params and queues an asynchronous whole-site
publish request. It returns a status snapshot immediately; callers must poll
`wiki.status` or inspect render ledgers to know whether publication completed.

Current request example:

```json
{"jsonrpc":"2.0","id":1,"method":"wiki.refresh","params":{}}
```

Current status result shape includes:

```json
{
  "running": true,
  "url": "https://localhost/your-context",
  "health": "refreshing",
  "render": {
    "state": "refreshing",
    "running": false,
    "scheduled": true,
    "pending": false,
    "accepted_count": 1,
    "coalesced_count": 0,
    "completed_count": 0,
    "failed_count": 0,
    "skipped_count": 0,
    "backing_off": false,
    "last": {
      "trigger": "wiki.refresh",
      "status": "published",
      "dirty_pages": 1,
      "skip_reason": null,
      "error": null
    }
  }
}
```

The render queue:

- runs at most one render at a time
- runs manual `wiki.refresh` immediately when idle
- debounces automatic `wiki.prepare`
- coalesces extra requests while scheduled or running
- keeps the manual request when manual and automatic requests collide
- records up to 50 history entries
- backs off automatic retries after failures

Current limitation: the daemon RPC is a trigger, not the final wiki consumer
API. It has no route/page scope, no materialization option, no wait mode, no
structured page-dirty list, and no typed repair hints.

## Target Consumer Action API

Greenfield wiki work should converge on an app-wide action API that can later
serve the rest of 1Context, with wiki publishing as the first clean target.

Primary request:

```json
{
  "action": "wiki.publish",
  "scope": {
    "kind": "page",
    "id": "topics",
    "route": "/topics"
  },
  "options": {
    "materialize": true,
    "render": "if_changed",
    "publish": true,
    "wait": "completed"
  },
  "actor": {
    "kind": "agent",
    "name": "memory-agent"
  }
}
```

Target operations:

- `wiki.resolve`: page id or route to source, talk, template, and published
  paths.
- `wiki.materialize`: create missing configured pages from templates, never
  overwrite user files.
- `wiki.publish`: materialize if requested, render if changed or forced,
  validate, publish last-good, and return structured evidence.
- `wiki.status`: queue state, last successful publish, last failure, current
  route counts, and backoff state.
- `wiki.explain`: explain why a route exists, is missing, is tombstoned, or
  failed validation.

Target `wiki.publish` result:

```json
{
  "schema_version": 1,
  "status": "published",
  "action_id": "uuid",
  "trigger": "agent.accepted-edit",
  "scope": {"kind": "page", "id": "topics", "route": "/topics"},
  "materialization": {
    "status": "applied",
    "created": ["user-wiki/source/families/reference/topics/source/topics.md"],
    "skipped_existing": []
  },
  "render": {
    "status": "published",
    "skipped": false,
    "dirty_pages": ["topics"],
    "route_count": 8,
    "markdown_twin_count": 8
  },
  "published": {
    "source_site": "user-wiki://site",
    "served_site": "app-support://wiki-site/current"
  }
}
```

Target failure result:

```json
{
  "schema_version": 1,
  "status": "failed",
  "action_id": "uuid",
  "failed_stage": "render",
  "last_good_preserved": true,
  "served_site": "app-support://wiki-site/current",
  "repair": {
    "kind": "template_frontmatter_error",
    "message": "topics.md is missing slug frontmatter",
    "paths": ["user-wiki/source/families/reference/topics/source/topics.md"]
  }
}
```

This target is deliberately not just a wiki detail. It is the pattern the rest
of the app can reuse: accept an intent, resolve durable file ownership,
materialize missing user-owned structure when allowed, run bounded work,
publish only validated output, and return evidence.

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
