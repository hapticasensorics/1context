# 1Context Wiki Publishing System API

- Status: canonical V0 publishing contract plus consumer API target
- Last updated: 2026-05-20
- Latest evidence: 2026-05-20 daemon JSON-RPC, Rust CLI, and the current repo
  Swift CLI expose page lifecycle, publish, talk, mail, notifications,
  attachment metadata, and structured wiki failure envelopes through the shared
  Rust core surface

This is the master contract for publishing and collaborating around 1Context
wiki memory from user-owned files into app-served static output. It covers
authoring inputs, page lifecycle, talk folders, mailboxes, agent directory,
notifications, template fallback, rendering, validation, last-good publication,
local web serving, RuntimeDefaults backfill, package evidence, and the freeze
boundary. It also distinguishes the current shipped trigger from the cleaner
consumer API the wiki system should grow into.

Architecture spine:
[Wiki System Architecture](wiki-system-architecture.md). The API below assumes
a portable Rust wiki core with one `WikiInventory`, one page ledger, one
lifecycle service, one agent directory, one talk/mail router, one notification
dispatcher, one renderer boundary, and one publisher. Swift is the macOS host,
not the permanent home for core wiki logic. Consumers should not path-walk the
wiki or call repo-only helper scripts for product behavior.

Older goal docs record how this shape was reached. This document is the
starting point for building against it.

## One-Line Shape

```text
page/talk/mail lifecycle + file edits -> portable publish preflight -> render -> validated site -> last-good publish
```

Target implementation shape:

```text
WikiInventory + page ledger + agent directory + mail delivery ledgers -> lifecycle/talk/mail/notify/publish APIs -> static reader surface
```

## Publishing Pipeline

The stable publishing pipeline is:

1. User-owned inputs live under `~/1Context/user-wiki` and
   `~/1Context/context-engine`.
2. `WikiInventory` compiles `wiki.toml`, source, talk, templates, tombstones,
   page ledgers, agent directory state, mail delivery state, notification
   cursors, RuntimeDefaults setup evidence, and render manifests into one
   resolved view.
3. Structure-changing operations update user-owned files and append ledgers
   through `wiki.page.create`, `wiki.page.delete`, `wiki.talk.append`,
   `wiki.mail.mark`, `wiki.mail.subscribe`, and agent directory calls.
4. RuntimeDefaults install copies only missing packaged defaults into user data
   and writes proposals for changed existing files.
5. The publisher validates the inventory and computes the publish fingerprint.
6. The renderer builds a complete static site from actual user source into a
   staging directory.
7. The portable publisher validates the staged site, promotes
   `~/1Context/user-wiki/site`, then publishes the last-good mirror to
   Application Support.
8. Local Web serves the Application Support mirror and redacted API routes.
9. Manifests, ledgers, route indexes, and harness artifacts record what
   happened.

## Ownership Rule

```text
User data is live truth.
RuntimeDefaults are seed and backfill material.
The portable wiki core owns lifecycle, mail, validation, and publication.
Swift hosts Apple-specific app and service behavior.
The renderer is a pure helper behind the publisher.
Memory agents author governed user-owned files.
```

This is a shared-core contract. Swift uses the core for app and daemon wiki
behavior. The memory system uses the core for page placement, fallback, talk,
mail, notifications, validation, and publication requests. Neither side should
grow a parallel implementation of those rules.

No component may silently overwrite user-owned wiki source, talk, templates,
prompts, `_curator.md`, or `wiki.toml`.

## Consumer Surface

This API is primarily for consumers that need to change or observe the wiki:

| Consumer | Reads | Writes | Calls | Success proof |
| --- | --- | --- | --- | --- |
| Memory agent | inventory rows, page handles, inbox headers, talk threads, proposals, render ledgers | user-owned source, talk entries, proposals, accepted template/site-map changes, mail marks | `wiki.agent.register`, `wiki.mail.inbox`, `wiki.talk.append`, `wiki.page.*`, `wiki.publish` | `wiki.status`, `wiki.page.status`, inbox delivery receipts, route manifest contains expected route |
| Operator/editor | markdown source, talk folders, inbox state | markdown source, talk entries, `wiki.toml`, decisions | app menu actions backed by wiki API | page appears at Local Web route, markdown twin exists, inbox state is explainable |
| Rust wiki core | RuntimeDefaults, user source, previous site, ledgers, mail state | setup ledgers, page ledger, mail ledgers, `user-wiki/site`, app-support mirror | internal services behind daemon/CLI/API | published result or failed result with last-good preserved |
| Swift app/host | app bundle resources, runtime paths, core status, notification counts | Apple-specific service state only | launch/supervise core, open local web, show menu/status | core is reachable, local web is reachable, permissions are clear |
| Local Web/browser | app-support `wiki-site/current` | local UI state only under `/api/wiki/state` | static HTTP plus `/api/wiki/*` | no source paths leak; current route manifest is valid |
| Release/package | repo `runtime/1Context`, `wiki-engine` | bundled `RuntimeDefaults`, bundled `WikiEngine`, manifests | release train build | package smoke and RuntimeDefaults scenario proof pass |

Consumers should treat this as a whole-site publishing system. V0 does not have
a page-scoped render API. To render one changed page, edit that page's
user-owned files and request `wiki.publish`; the publisher fingerprints the
accepted input set and either re-renders the site or skips the renderer when
inputs are unchanged and the existing site validates.

Target consumers should call the greenfield API rather than path-walking:
`wiki.list`, `wiki.page.open`, `wiki.page.status`, `wiki.page.create`,
`wiki.page.write_body`, `wiki.page.patch_body`, `wiki.page.delete`,
`wiki.page.restore`,
`wiki.validate`, `wiki.publish`, `wiki.agent.register`, `wiki.agent.heartbeat`,
`wiki.agent.retire`, `wiki.mail.inbox`, `wiki.mail.read`, `wiki.mail.mark`,
`wiki.mail.subscribe`, `wiki.page.watch`, `wiki.page.unwatch`,
`wiki.page.assign_role`, `wiki.list.create`, `wiki.lists`,
`wiki.list.status`, `wiki.list.members`, `wiki.talk.append`,
`wiki.talk.thread`, `wiki.notify.poll`, `wiki.notify.ack`, and tiny
`wiki.status`. Those calls
should all read from or update the same `WikiInventory`, page ledger, agent
directory, mail ledgers, and notification outbox.

Current V0 implementation: the Rust core owns page creation, page open, page
body write/patch, page delete/restore, publish status, publish trigger, status,
validation, agent directory, talk append, mail inbox/read/claim/mark,
subscription helpers, lists, and notification poll/ack through the
`onecontext-wiki` JSON CLI. The Swift daemon forwards supported JSON-RPC
methods through `WikiCoreRPCBridge`, and the installed `1context wiki` CLI
exposes the common app-facing workbench for page lifecycle, publish,
agent identity, inbox, lists, page watch/role assignment, talk, mail,
subscriptions, and notification work. Remaining target work is to remove the
transitional Swift render queue and make the Rust publisher path the only app
render path.

## Component Map

Repo source:

```text
1context-public-launch/
  runtime/                         public-safe shipped defaults source
    1Context/user-wiki/
    1Context/context-engine/

  runtime-test/                    ignored local/private scenario lab

  wiki-engine/                     deterministic renderer package
    tools/render-site.mjs
    tools/write-runtime-defaults-manifest.py
    src/renderer/
    theme/
    schemas/

  macos/Sources/
    OneContextPlatform/            runtime paths and permissions
    OneContextDaemon/              macOS host for the wiki core API
    OneContextLocalWeb/            Caddy/local API/static serving
    OneContextWikiRuntime/         transitional Swift defaults/render bridge

  crates/
    onecontext-wiki-core/          target portable Rust wiki core
    onecontext-wiki-daemon/        target JSON-RPC/CLI daemon surface
```

Installed app bundle:

```text
1Context.app/
  Contents/MacOS/1Context
  Contents/MacOS/1contextd
  Contents/MacOS/1context-cli
  Contents/MacOS/onecontext-wiki
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
    .1context/page-ledger.jsonl
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
  notifications/
  run/
  local-web/
```

## Stable Data API

### `wiki.toml`

`~/1Context/user-wiki/wiki.toml` is the user-owned site map.

Stable concepts:

- `[[pages]]` declare editable source pages under `source/families/**`.
- `[[site_pages]]` declare generated pages, aliases, and diagnostics.
- Generated `[[site_pages]]` are renderer-owned routes, not editable source
  pages. `wiki.page.open`, `wiki.page.status`, `wiki.page.write_body`,
  `wiki.page.delete`, and `wiki.page.create` return typed
  `generated_site_page` errors for generated ids or routes; inspect
  `wiki.publish.status`, `wiki.status`, or the route manifest instead.
- Navigation order lives in `wiki.toml`, not in folder-name prefixes.
- Missing unconfigured routes diagnose; they do not redirect to
  `/your-context` or hidden bundled pages.
- Tombstoned source is not recreated by page creation or publish preflight.

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

The page creation step creates or verifies this user-owned shape:

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

- New-page creation validates source, conventions, and curator templates before
  mutating `wiki.toml`, creating files, or adding navigation entries.
- Missing source is created from the configured page template.
- Missing talk metadata is generated from the page registry.
- Missing `_conventions.md` and `_curator.md` are created from their configured
  talk templates when present.
- Existing files are never overwritten by page creation or publish preflight.
- Existing differing files are recorded as lifecycle evidence with
  `skipped_existing` status.
- `source/<slug>.tombstone.toml` blocks recreation of that page.
- Tombstone delete also retires the page from active navigation arrays and marks
  its `[[pages]]` record disabled so future renders cannot leave stale menu
  links to the deleted route.
- `wiki.page.create` refuses tombstoned or disabled page ids. Use a new page id
  for replacement content, or call `wiki.page.restore` when intentionally
  returning the same page.
- While a tombstoned route is still present in the rendered site,
  `wiki.page.status` should surface `next_action = "publish"` and a warning.
  After publish removes the route, the same tombstone is clean terminal state
  with `next_action = "none"`.
- Tombstoned and disabled pages reject new `wiki.talk.append` calls by default.
  Archive-maintenance callers must opt in explicitly with `allow_tombstoned`.
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
  attachments/
    <message-id>/
      screenshot.png
      context.json
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
changes the registry. The page becomes renderable after `wiki.page.create`
creates the source and talk files.

During whole-site rendering, `render-site.mjs` also reads
`user-wiki/wiki.toml` to build the brand menu. That means page placement is not
only bookkeeping: `navigation` / `primary_navigation` and enabled `[[pages]]`
entries determine which pages appear in the site menu.

### Published Route Placement

The configured `route` in `wiki.toml` is the readable URL truth. The `slug`
remains the source filename and family-local identifier. During whole-site
rendering, `render-site.mjs` passes the configured route to each per-page
render so older source frontmatter cannot silently collapse a nested page back
to its slug-only URL.

For a source page whose configured route is `/topics`, the renderer writes:

```text
<site>/topics.html
<site>/topics/index.html
<site>/topics.md
```

For a source page whose configured route is `/topics/retrieval-shape`, the
renderer writes:

```text
<site>/topics/retrieval-shape.html
<site>/topics/retrieval-shape/index.html
<site>/topics/retrieval-shape.md
```

The `index.html` route duplicate sets its document `<base>` to the canonical
route, so relative links behave the same whether a reader opens
`/topics/retrieval-shape` or `/topics/retrieval-shape/index.html`.

For a talk folder whose page route is `/topics/retrieval-shape`, the renderer
writes:

```text
<site>/topics/retrieval-shape.talk.html
<site>/topics/retrieval-shape/talk/index.html
<site>/topics/retrieval-shape.talk.md
```

Article Agent view exposes the article HTML/markdown surfaces and, when talk
is enabled, the corresponding talk HTML and talk markdown surfaces. This keeps
page reading, source inspection, and talk/inbox movement discoverable from one
agent-facing view.

Section subpages declared by renderer-supported section metadata become nested
route and markdown twins such as:

```text
<site>/topics/engineering.html
<site>/topics/engineering.md
```

The source of truth for what actually rendered is the route manifest, not the
registry. Use the route manifest for route existence and link checks. Use the
content index for the full markdown twin inventory and export allowlist:

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

The publisher promotes only after validation. A failed render must preserve the
previous last-good mirror.

## Publishing Command API

### Page Lifecycle Service

The page lifecycle service is the only public way to create or remove
source-backed pages. It owns template fallback, source/talk placement, tombstone
checks, route/id collision checks, no-overwrite behavior, and page-ledger
evidence.

Public operations:

- `wiki.page.create` creates a source-backed page from templates and returns the
  handles a consumer should edit.
- `wiki.page.delete` tombstones or disables a page and records the decision.
- `wiki.page.open` resolves edit handles for an existing page without mutating
  files.
- `wiki.page.status` reports whether the page is template-derived, edited,
  missing source, tombstoned, stale, or already rendered.

Writes:

- source markdown
- talk folder metadata, conventions, and curator prompt
- generated `group.toml` and `family.toml`
- family-local page/talk templates
- `wiki.toml` page registry changes when creating or deleting pages
- `user-wiki/.1context/page-ledger.jsonl` provenance events

It must not overwrite existing user files. If a destination already exists, the
operation returns `already_current` or `skipped_existing` evidence instead of
rewriting it.

Receipt evidence paths are root-relative to the active `1Context` runtime
unless a field is explicitly named `absolute_path`. This keeps agent reports
portable across developer machines, app sandboxes, and packaged RuntimeDefaults.

Implementation note: the repo still has an older Python helper used by some
dev/build harnesses while this service is being moved into the portable wiki
core. That helper is not the consumer API and should not be called by agents or
app features.

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
- enabled generated `[[site_pages]]` from `wiki.toml`, such as the root home
  page
- non-tombstoned pages only
- `wiki.toml` navigation and page metadata for the rendered menu
- bundled engine theme assets

Generated site pages may participate in configured navigation. Talk markdown
twins use their `talk_route` in the route manifest, for example
`/topics/talk`, so they do not duplicate the parent page route.

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
  "site_input_count": 1,
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
  "site_input_count": 0,
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
  --runtime-defaults-root <app>/Contents/Resources/RuntimeDefaults/1Context \
  --wiki-engine-root <app>/Contents/Resources/WikiEngine \
  --wiki-core-bin <app>/Contents/MacOS/onecontext-wiki \
  --manifest-writer wiki-engine/tools/write-runtime-defaults-manifest.py \
  --manifest-writer-display-path wiki-engine/tools/write-runtime-defaults-manifest.py \
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
- `hashes.wiki_core`
- `hashes.renderer`
- `hashes.manifest_writer`
- `render_summary.status`
- `render_summary.route_count`
- `render_summary.markdown_twin_count`

The app build writes this after the bundled helper executables are signed and
before the final app signature, so `hashes.wiki_core` and `hashes.wiki_engine`
describe the shipped payload. `scripts/test-launch-agent-package.sh`
recomputes these hashes against the bundle during package smoke.

## Portable Core And Platform Host API

### Target Runtime Ownership

The target implementation is a portable Rust wiki core hosted by the macOS app.
The core owns wiki semantics; Swift owns Apple integration.

Rust core owns:

- RuntimeDefaults install policy and ledgers
- inventory compile
- page lifecycle
- agent directory, mailboxes, and notifications
- validation
- render orchestration
- last-good publish promotion
- JSON-RPC/CLI API behavior

Swift host owns:

- app UI and menu actions
- Apple permissions and privacy surfaces
- app bundle resource discovery
- launch/service lifecycle
- Caddy/local-web supervision
- opening the browser/app web view
- bridging user actions into the Rust-backed API

### Runtime Paths

Production paths come from `RuntimePaths.current()`.

Debug-only override:

```bash
ONECONTEXT_DEV_RUNTIME_HOME=/path/to/runtime-test
```

This override is ignored in release builds.

The Rust core should receive resolved paths from the host or a small platform
adapter. It should not bake in Swift-only path discovery.

### Defaults Installer

Target operation:

```json
{"action":"wiki.defaults.install_missing","mode":"safe_backfill"}
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

Current transitional Swift type:

```swift
WikiRuntimeDefaultsInstaller(runtimePaths: paths).installMissingDefaults()
```

### Renderer Bridge And Publish Core

Target operation:

```json
{"action":"wiki.publish","scope":{"kind":"site"},"wait":"completed"}
```

Renderer discovery:

- `ONECONTEXT_WIKI_ENGINE_DIR` override
- bundled `Contents/Resources/WikiEngine`

Node discovery:

- `ONECONTEXT_NODE`
- dev fallback: `/usr/bin/env node`

Production app/open-wiki paths must bundle or otherwise control the Node
runtime the same way the app controls Caddy. The `/usr/bin/env node` path is a
developer override/fallback, not a signed-app dependency.

Current transitional Swift type:

```swift
WikiRenderCoordinator(runtimePaths: paths).renderAndPublish(trigger: "wiki.prepare")
```

### Render Behavior

The publisher:

- fingerprints the accepted publish input set
- skips rendering if inputs are unchanged and the existing site validates
- renders into a private staging directory
- validates route manifest, markdown twins, and private/public boundaries
- atomically replaces `~/1Context/user-wiki/site`
- promotes that site to Application Support `wiki-site/current`
- preserves last-good output on failure

Current V0 split: explicit `wiki.publish` uses the Rust publisher fingerprint
and includes page source, tombstones, `wiki.toml`, renderer/core identity, and
publish preflight state. It intentionally excludes mail delivery ledgers and
notification churn. The transitional Swift `wiki.refresh` queue still exists
for startup/support behavior and should be retired once every host path calls
the Rust publisher directly.

Failure behavior:

- renderer failure removes the failed staging directory
- validation failure blocks promotion
- `~/Library/Application Support/1Context/wiki-site/current` remains the
  previously valid last-good site
- `~/1Context/user-wiki/site/.1context/current-render.json` records failure
  when a source site exists
- the render queue records the failed trigger, error string, and backoff state

CLI/API failures return structured JSON envelopes on stdout, including current
repo Swift CLI argument errors and daemon/Rust RPC errors:

```json
{
  "schema_version": 1,
  "status": "error",
  "operation": "wiki.mail.inbox",
  "command": "mail-inbox",
  "error": {
    "code": "invalid_address",
    "message": "invalid address not-an-address; expected agent://, role://, list://, or mailbox:// address"
  },
  "repair_hints": ["Use agent://, role://, list://, or mailbox:// addresses with no whitespace."]
}
```

Consumers should parse stdout first even on non-zero exit. Stderr is reserved
for crashes or tooling failures outside the wiki API contract. The installed
app may lag this repo behavior until the next signed release, but the target
contract is stdout JSON for every `1context wiki ...` consumer failure.
Page creation preflights the source template against the renderer contract
before mutating `wiki.toml`. A template that renders without required
frontmatter (`title`, `slug`, `section`, `access`) or with an invalid
`section`/`access` value returns `error.code="invalid_page_template"` and leaves
the page registry unchanged.

## Daemon API

The daemon executable is:

```text
1Context.app/Contents/MacOS/1contextd
```

It accepts newline-delimited JSON-RPC over the runtime Unix socket.

Target daemon shape: Swift may launch and supervise the daemon, but the wiki
methods should be implemented by the portable core. The daemon is the single
product API for agents, CLI, app actions, and local web redacted APIs.

Current methods:

- `health`
- `status`
- `version`
- `wiki.status`
- `wiki.list`
- `wiki.page.status`
- `wiki.page.open`
- `wiki.page.create`
- `wiki.page.write_body`
- `wiki.page.patch_body`
- `wiki.page.delete`
- `wiki.page.restore`
- `wiki.validate`
- `wiki.publish.status`
- `wiki.publish`
- `wiki.page.watch`
- `wiki.page.unwatch`
- `wiki.page.assign_role`
- `wiki.list.create`
- `wiki.lists`
- `wiki.list.status`
- `wiki.list.members`
- `wiki.agent.register`
- `wiki.agent.identify`
- `wiki.agent.heartbeat`
- `wiki.agent.retire`
- `wiki.agent.whoami`
- `wiki.agent.list`
- `wiki.agent.status`
- `wiki.agent.inbox`
- `wiki.agent.claim`
- `wiki.talk.append`
- `wiki.mail.inbox`
- `wiki.mail.read`
- `wiki.mail.mark`
- `wiki.mail.mark_all`
- `wiki.mail.claim`
- `wiki.mail.subscribe`
- `wiki.mail.unsubscribe`
- `wiki.mail.subscriptions`
- `wiki.notify.poll`
- `wiki.notify.ack`
- `wiki.start`
- `wiki.refresh`
- `wiki.stop`

The Swift daemon RPC remains the app-hosted control plane for local web start,
stop, health, and the current render queue. `wiki.refresh` is transitional: it
queues an asynchronous whole-site publish request in the Swift host.
`wiki.publish` is the preferred explicit publishing operation for agents and
automation that have just changed source, tombstones, or `wiki.toml`; it
delegates to the portable Rust publisher, then mirrors the validated
`user-wiki/site` into the app-visible Application Support site. The receipt
keeps the core publish result and an `app_publish` block for that mirror step.
Agent identity, talk-mail, inbox, claim, subscription, and notification calls
also delegate to the portable Rust core. Callers must distinguish agent ids
from mail addresses: use `agent_id` for control calls, and use the returned
`primary_address` or `addresses[0]` for direct mail delivery.

Transitional queue request example:

```json
{"jsonrpc":"2.0","id":1,"method":"wiki.refresh","params":{}}
```

Preferred explicit publish request example:

```json
{"jsonrpc":"2.0","id":2,"method":"wiki.publish","params":{"trigger":"agent-edit"}}
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

Current limitation: daemon JSON-RPC now covers inventory, page lifecycle,
explicit whole-site publish, page watch/unwatch, page role assignment, list
creation/status/member inspection, agent identity, talk-mail, inbox, claims,
mail marking, subscriptions, and notification poll/ack through the Swift
`WikiCoreRPCBridge`. The installed `1context wiki` CLI now exposes those common
workbench calls as app-facing wrappers, so agents can use the installed daemon
path for page watch/unwatch, list membership, subscriptions, inbox, mail
mark-all, and notification pressure without dropping to the Rust binary.
Curator apply remains target-only, and the remaining host work is to retire the
transitional `wiki.refresh` render bridge.

## Greenfield Consumer API

This is the API I would want to use if I were actively writing, editing, and
publishing my own wiki.

The filesystem remains the truth and the inspection interface. Markdown, talk
folders, templates, and `wiki.toml` are meant to be readable and hand-editable.
The app API should own lifecycle, common safe edits, checks, publication, and
evidence. In other words:

```text
create/open page -> write or patch body -> validate -> publish -> inspect status
```

Do not expose legacy template-fill helpers, `preview`, or `explain` as primary
operations. Template filling is internal to page creation and publish preflight.
Preview is too much surface for V0. Explanation belongs in `wiki.validate`
issues and render ledgers.

### North Star

The API should feel like an editor's bench, not a CMS. I should never need to
remember folder conventions, renderer internals, RuntimeDefaults behavior, or
which helper script is safe to run. I should be able to ask three questions:

- "What pages exist, and which ones need attention?"
- "Where do I edit this page or its talk?"
- "Can you publish this and prove what happened?"

Everything else is implementation detail.

The best version is boring in daily use:

```text
wiki.page.open("topics")
wiki.page.patch_body("topics", find: "Old sentence", replace: "New sentence")
wiki.publish(page: "topics", wait: true)
wiki.page.status("topics")
```

For a new page:

```text
wiki.page.create(title: "Jackie Oliver", type: "person", collection: "people")
wiki.page.write_body("jackie-oliver", body_markdown: "# Jackie Oliver\n...")
wiki.publish(page: "jackie-oliver", wait: true)
```

For a page that should disappear from the reader surface:

```text
wiki.page.delete("old-page", mode: "tombstone")
wiki.publish(wait: true)
```

For a tombstoned page that should intentionally return:

```text
wiki.page.restore("old-page")
wiki.publish(wait: true)
```

This should be simple enough that memory agents, the macOS menu, a CLI, local
web, and a future UI can all sit on top of the same contract.

### Design Rules

- Page structure changes are explicit: create, delete, and restore.
- Page content edits happen through frontmatter-preserving body operations or,
  when needed, by editing the file handles returned by `wiki.page.open`.
- Publish owns preflight checks, rendering, validation, last-good promotion,
  and failure preservation.
- Publish does not invent unregistered pages. If `wiki.toml` already contains an
  enabled page with a valid template, publish may safe-create the missing
  source/talk files as a preflight and record that in the receipt. New custom
  pages still belong to `wiki.page.create`.
- List and open tell agents where things are; agents should not guess
  `source/families/**` paths.
- Status stays small; validation carries details.
- The implementation may full-render internally, even when the request scope is
  one page. The scope is the caller's intent and evidence boundary, not a
  promise of incremental rendering.
- A missing page is a typed problem, not a redirect. The fallback template
  system creates files only through explicit page creation or safe publish
  preflight, never by sending the browser to a different page.
- The API returns relative user-data handles and app-support handles. Absolute
  local paths stay out of browser-visible output.
- Every mutating call is idempotent enough for agents: retrying after a crash
  should produce `already_current`, `skipped_existing`, `tombstoned`, or a
  typed validation failure rather than duplicate files.

### Preferred Agent Loop

This is the loop I would actually use:

1. Call `wiki.agent.identify` with my transport pointer, roles, capabilities,
   and lease. It registers missing sessions, refreshes active or stale
   sessions, and refuses explicitly retired sessions.
2. Call `wiki.mail.inbox` to see work already waiting for me before I start
   path-walking or reading whole pages.
3. Call `wiki.list` to see configured, source-backed, rendered, missing, stale,
   and tombstoned pages.
4. Call `wiki.page.open` for the page I intend to edit.
5. Use `wiki.page.write_body` or `wiki.page.patch_body` for ordinary article
   edits; edit returned source, talk, curator, or convention files directly
   only when the structured operation is too blunt.
6. For new pages, call `wiki.page.create` before writing content beyond the
   returned source/talk files.
7. For removed pages, call `wiki.page.delete` and let tombstones protect the
   user history.
8. Use `wiki.talk.append` for proposals, concerns, questions, replies,
   decisions, and attachments so recipients get deliveries. For replies, pass
   `reply_to` or an explicit `thread_id` instead of relying on subject matching.
9. Use `wiki.agent.claim` when I am acting from `wiki.agent.inbox` and want
   the core to choose the best matching delivery for this agent. Use
   `wiki.mail.claim` only when I already know the exact recipient mailbox.
10. Use `wiki.mail.mark` to complete, snooze, or archive work without changing
   message truth.
11. Call `wiki.validate` when I need detailed diagnostics before publishing.
12. Call `wiki.publish` with `wait: "completed"` when I want the app-visible
   site updated.
13. Read the publish evidence or `wiki.page.status`. On failure, repair the
   source or write a talk proposal; do not blindly retry.

### Operation Set

The first clean surface should include page lifecycle and collaboration from
day one:

- `wiki.agent.register`
- `wiki.agent.identify`
- `wiki.agent.heartbeat`
- `wiki.agent.retire`
- `wiki.agent.whoami`
- `wiki.agent.list`
- `wiki.agent.status`
- `wiki.agent.inbox`
- `wiki.agent.claim`
- `wiki.mail.inbox`
- `wiki.mail.read`
- `wiki.mail.claim`
- `wiki.mail.mark`
- `wiki.mail.mark_all`
- `wiki.mail.subscribe`
- `wiki.mail.unsubscribe`
- `wiki.page.watch`
- `wiki.page.assign_role`
- `wiki.list.create`
- `wiki.lists`
- `wiki.list.status`
- `wiki.list.members`
- `wiki.talk.append`
- `wiki.talk.thread`
- `wiki.notify.poll`
- `wiki.notify.ack`
- `wiki.page.open`
- `wiki.page.status`
- `wiki.page.create`
- `wiki.page.write_body`
- `wiki.page.patch_body`
- `wiki.page.delete`
- `wiki.page.restore`
- `wiki.publish`
- `wiki.list`
- `wiki.validate`
- `wiki.status`

Short CLI aliases can exist later, but the daemon/action names should stay
explicit. The page lifecycle verbs are separate from publish so callers can see
whether they are changing structure or merely publishing existing source.
`page.status` is acceptable as a UI/CLI shorthand, but the daemon/action name
should stay `wiki.page.status`.

`wiki.curator.apply` is also part of the target surface, but it can land after
the mail primitives because it depends on decisions, ownership checks, and
bounded source patches.

### `wiki.page.open`

Open is a read-only resolver for editing. It answers "where do I work?"

Request:

```json
{
  "action": "wiki.page.open",
  "page": {"id": "topics"}
}
```

Result:

```json
{
  "schema_version": 1,
  "status": "ok",
  "operation": "wiki.page.open",
  "page": {
    "id": "topics",
    "route": "/topics",
    "title": "Topics",
    "state": "rendered",
    "content_state": "edited"
  },
  "handles": {
    "source": "user-wiki://page/topics/source",
    "talk": "user-wiki://page/topics/talk",
    "curator": "user-wiki://page/topics/curator",
    "conventions": "user-wiki://page/topics/conventions",
    "published_html": "app-support://wiki/topics",
    "markdown_twin": "app-support://wiki/topics.md"
  },
  "resolved_paths": {
    "source": "user-wiki/source/families/reference/topics/source/topics.md",
    "talk": "user-wiki/source/families/reference/topics/talk/topics.talk/",
    "curator": "user-wiki/source/families/reference/topics/talk/topics.talk/_curator.md",
    "conventions": "user-wiki/source/families/reference/topics/talk/topics.talk/_conventions.md",
    "published_html": "app-support://wiki-site/current/topics.html",
    "markdown_twin": "app-support://wiki-site/current/topics.md"
  },
  "hashes": {
    "source_sha256": "..."
  },
  "resources": [
    {
      "surface": "source",
      "uri": "user-wiki://page/topics/source",
      "path": "user-wiki/source/families/reference/topics/source/topics.md",
      "absolute_path": "/Users/me/1Context/user-wiki/source/families/reference/topics/source/topics.md",
      "sha256": "...",
      "safe_to_edit": true,
      "write_mode": "hash_checked_patch"
    }
  ],
  "edit": {
    "expected_source_sha256": "...",
    "must_preserve_user_edits": true,
    "must_check_hash_before_write": true,
    "safe_to_edit": true,
    "recommended_write_mode": "hash_checked_patch",
    "direct_source_write_allowed": true,
    "recommended_operation": "wiki.page.patch_body",
    "required_preconditions": ["expected_source_sha256", "preserve_user_edits"],
    "proposal_required": false,
    "policy_reason": "Page has post-template edits; prefer a narrow hash-checked patch. Use a talk proposal for broad rewrites or unclear ownership."
  },
  "allowed_actions": [
    "wiki.page.open",
    "wiki.page.patch_body",
    "wiki.page.write_body",
    "wiki.talk.append",
    "wiki.validate",
    "wiki.publish",
    "wiki.page.delete"
  ],
  "metadata": {
    "origin": "created_from_template",
    "template_state": "edited_from_template",
    "dirty_since_publish": false,
    "talk_state": "ready"
  }
}
```

### `wiki.page.write_body`

Write replaces only the markdown body for an existing page. It preserves YAML
frontmatter and refuses to edit tombstoned, disabled, or source-missing pages.
This is the operation I would use for "make this page body exactly this."

Request:

```json
{
  "action": "wiki.page.write_body",
  "page": {"id": "topics"},
  "body_markdown": "## Engineering\n\nUpdated body...",
  "expected_source_sha256": "optional-current-source-hash",
  "actor": {"kind": "agent", "name": "memory-agent"}
}
```

Current CLI and Python adapter:

```text
onecontext-wiki --root ~/1Context page-write-body topics --body-file body.md --expected-source-sha256 ...
1context wiki page-write-body topics --body-file body.md --expected-source-sha256 ...
wiki_page_write_body(runtime_home, "topics", body_markdown=body, expected_source_sha256=hash)
```

Result:

```json
{
  "schema_version": 1,
  "status": "ok",
  "operation": "wiki.page.write_body",
  "id": "topics",
  "evidence": [
    {
      "path": "user-wiki/source/families/reference/topics/source/topics.md",
      "status": "updated"
    }
  ],
  "page_status": {
    "operation": "wiki.page.status",
    "id": "topics",
    "state": "needs_publish",
    "content_state": "edited",
    "next_action": "publish"
  },
  "edit": {
    "expected_source_sha256": "fresh-current-hash-after-write",
    "must_check_hash_before_write": true,
    "safe_to_edit": true,
    "recommended_operation": "wiki.page.patch_body"
  },
  "hashes": {
    "source_sha256": "fresh-current-hash-after-write",
    "talk_sha256": "current-talk-hash",
    "curator_sha256": "current-curator-hash",
    "conventions_sha256": "current-conventions-hash"
  },
  "render_required": true,
  "next_action": "publish",
  "repair_hints": []
}
```

If the supplied `expected_source_sha256` is stale, the operation fails before
writing. If the body is already identical, the receipt returns
`render_required=false` and `next_action=none`. Successful page lifecycle
receipts include `page_status` when the page can be inspected. Create, write,
patch, delete, and restore receipts also expose top-level `route`, `type`, and
`collection`, matching `wiki.page.open`, so agents can chain operations without
digging through nested status or URI handles. They also include the same `edit`
preconditions and `hashes` shape returned by `wiki.page.open`, so agents can
chain guarded edits with `edit.expected_source_sha256` without making an extra
`wiki.page.open` call.

### `wiki.page.patch_body`

Patch is the small safe edit operation. It performs an exact find/replace in
the markdown body only, preserves frontmatter, and requires the find text to
match exactly once. Zero matches or multiple matches are typed conflicts, not
best-effort edits.

Request:

```json
{
  "action": "wiki.page.patch_body",
  "page": {"id": "topics"},
  "find": "Named subjects, categorized",
  "replace": "Named subjects, categorized and linked",
  "expected_source_sha256": "optional-current-source-hash",
  "actor": {"kind": "agent", "name": "memory-agent"}
}
```

Current CLI and Python adapter:

```text
onecontext-wiki --root ~/1Context page-patch-body topics --find-file find.md --replace-file replace.md --expected-source-sha256 ...
1context wiki page-patch-body topics --find-file find.md --replace-file replace.md --expected-source-sha256 ...
wiki_page_patch_body(runtime_home, "topics", find=find, replace=replace, expected_source_sha256=hash)
```

Result shape is the same `OperationReceipt` as `wiki.page.write_body`, with
`operation="wiki.page.patch_body"`. The caller should publish only when the
receipt says `render_required=true`.

### `wiki.page.status`

Page status is the full status card for one page. It answers "what is this
page, where did it come from, has anyone changed it, and is the reader surface
current?"

Current app/daemon surface:

- daemon JSON-RPC method: `wiki.page.status`
- params: `{"page": "topics"}` or `{"route": "/topics"}`
- CLI wrapper: `1context wiki page-status topics`

Request:

```json
{
  "action": "wiki.page.status",
  "page": {"id": "topics"}
}
```

Result:

```json
{
  "schema_version": 1,
  "status": "ok",
  "page": {
    "id": "topics",
    "title": "Topics",
    "route": "/topics",
    "nav_section": "primary",
    "type": "index",
    "collection": "topics",
    "kind": "source_page",
    "family_group": "reference",
    "family_id": "topics",
    "state": "needs_publish",
    "content_state": "edited"
  },
  "flags": {
    "configured": true,
    "enabled": true,
    "source_backed": true,
    "rendered": true,
    "stale": true,
    "tombstoned": false,
    "talk_ready": true,
    "template_derived": true,
    "user_edited": true
  },
  "provenance": {
    "origin": {
      "kind": "created_from_template",
      "template_path": "user-wiki/templates/pages/topics.md",
      "template_sha256_at_create": "...",
      "created_source_sha256": "...",
      "actor": {"kind": "system", "name": "runtime-defaults"}
    },
    "created_by": {"kind": "system", "name": "runtime-defaults"},
    "created_at": "ISO-8601",
    "last_edited_by": {"kind": "agent", "name": "memory-agent"},
    "last_edited_at": "ISO-8601"
  },
  "template": {
    "configured": "pages/topics.md",
    "local_copy": "user-wiki://page/topics/template",
    "status": "available",
    "state": "edited_from_template",
    "template_sha256_at_create": "...",
    "current_template_sha256": "...",
    "created_source_sha256": "...",
    "current_source_sha256": "..."
  },
  "source": {
    "status": "present",
    "handle": "user-wiki://page/topics/source",
    "sha256": "...",
    "frontmatter_status": "ok",
    "dirty_since_publish": true,
    "matches_template": false,
    "user_edited": true
  },
  "talk": {
    "status": "ready",
    "folder": "user-wiki://page/topics/talk",
    "meta": "ok",
    "conventions": "present",
    "curator": "present",
    "open_entry_count": 2,
    "last_entry_at": "ISO-8601"
  },
  "mail": {
    "page_mailbox": "mailbox://page/topics",
    "curator_address": "role://topics.curator",
    "default_watchers_list": "list://topics.watchers",
    "associated_lists": [
      {
        "address": "list://wiki.reviewers",
        "page_id": "topics",
        "member_count": 1,
        "active_member_count": 1
      }
    ],
    "message_count": 3,
    "actionable_count": 2,
    "open_delivery_count": 2,
    "open_thread_count": 1,
    "unread_count": 1
  },
  "links": {
    "status": "warning",
    "broken_internal_count": 1,
    "broken_internal_targets": ["/old-page"],
    "checked_against": ".1context/route-manifest.json",
    "issues": [
      {
        "code": "broken_internal_link",
        "page_id": "topics",
        "route": "/topics",
        "markdown_path": "topics.md",
        "href": "/old-page",
        "target": "/old-page",
        "suggested_actions": ["edit_source", "replace_link", "publish"]
      }
    ]
  },
  "render": {
    "status": "stale",
    "last_rendered_at": "ISO-8601",
    "published_source_sha256": "...",
    "routes": ["/topics", "/topics/talk"],
    "markdown_twins": ["/topics.md", "/topics.talk.md"],
    "served_site": "last_good"
  },
  "freshness": {
    "stale": true,
    "reasons": ["source_changed"],
    "input_fingerprint": "..."
  },
  "validation": {
    "status": "warning",
    "issue_count": 1,
    "blocking_count": 0,
    "warning_count": 1,
    "highest_severity": "warning",
    "issue_codes": ["stale_served_output"]
  },
  "allowed_actions": [
    "wiki.page.open",
    "wiki.page.patch_body",
    "wiki.page.write_body",
    "wiki.talk.append",
    "wiki.validate",
    "wiki.publish",
    "wiki.page.delete"
  ],
  "next_action": {
    "action": "wiki.publish",
    "page": {"id": "topics"}
  }
}
```

Stable metadata fields:

- `page.state`: one canonical state for quick branching: `rendered`,
  `needs_publish`, `source_missing`, `talk_missing`, `invalid`, `tombstoned`,
  `orphan_source`, `disabled`, or `generated`.
- `page.content_state`: content state such as `template_unedited`,
  `edited`, `generated`, `missing_source`, `tombstoned`, or `unknown`.
- `flags`: boring booleans for cheap scanning. They intentionally duplicate the
  canonical state in a form agents can filter without enum logic.
- `provenance.origin.kind`: where the page came from:
  `runtime_default`, `created_from_template`, `operator_created`,
  `agent_created`, `imported`, `generated`, or `unknown`.
- `template.state`: template relationship: `not_template_backed`,
  `template_unedited`, `edited_from_template`, `template_changed`,
  `template_missing`, or `unknown`.
- `source.dirty_since_publish`: current source fingerprint differs from the
  fingerprint that produced the served output.
- `source.matches_template`: current source still equals the template output
  captured when the page was created. `unknown` is valid for older pages
  without a baseline.
- `talk.status`: `ready`, `missing`, `invalid`, or `has_open_entries`.
- `talk_ready`: true only when the talk folder exists, `_meta.yaml` validates,
  `_conventions.md` exists, and `_curator.md` exists.
- `render.status`: `current`, `stale`, `not_rendered`, `failed`, or
  `last_good`.
- `freshness.stale`: derived from publish input fingerprints, not timestamps
  alone.
- `links.status`: last publish link health for this page. Broken internal
  links are warnings with `next_action = "repair_links"` because the rendered
  site may still be served while agents repair source links.

This is the place for "is this still the template page?" The answer should not
be inferred by reading source text manually.
Template provenance should be tracked for source, `_conventions.md`,
`_curator.md`, and family-local templates, not only the article markdown.

### `wiki.page.create`

Create a page from templates and return the handles I should edit next.

Request:

```json
{
  "action": "wiki.page.create",
  "page": {
    "title": "Jackie Oliver",
    "type": "person",
    "collection": "people"
  },
  "open_after_create": true,
  "actor": {"kind": "agent", "name": "memory-agent"}
}
```

Responsibilities:

- derive the page id, slug, route, family group, and family id from the title,
  type, and collection unless explicitly overridden
- write the `wiki.toml` page entry
- create source, talk metadata, conventions, curator file, family metadata, and
  family-local templates
- never overwrite existing user files
- reject route/id collisions
- respect tombstones unless the caller explicitly requests restore
- keep tombstoned routes reserved until `wiki.page.restore` intentionally
  returns the page, so replacement pages should use a new id and a new route
- return created, unchanged, skipped, tombstoned, and opened handles

Optional advanced fields:

- `id`
- `slug`
- `route`
- `family_group`
- `family_id`
- `template`
- `talk_conventions_template`
- `talk_curator_template`

Current CLI and Python adapter:

```text
onecontext-wiki --root ~/1Context page-create jackie-oliver --title "Jackie Oliver" --route /people/jackie-oliver --family-group people --family-id jackie-oliver --type person --nav-section primary --nav-order 410
1context wiki page-create jackie-oliver --title "Jackie Oliver" --route /people/jackie-oliver --family-group people --family-id jackie-oliver --type person --nav-section primary --nav-order 410
wiki_page_create(runtime_home, "jackie-oliver", title="Jackie Oliver", route="/people/jackie-oliver")
```

Result:

```json
{
  "schema_version": 1,
  "status": "created",
  "page": {
    "id": "jackie-oliver",
    "route": "/people/jackie-oliver",
    "title": "Jackie Oliver",
    "type": "person",
    "collection": "people",
    "content_state": "template_unedited"
  },
  "created_handles": [
    "user-wiki://page/jackie-oliver/source",
    "user-wiki://page/jackie-oliver/talk",
    "user-wiki://page/jackie-oliver/conventions",
    "user-wiki://page/jackie-oliver/curator"
  ],
  "unchanged": [],
  "skipped_existing": [],
  "open": {
    "source": "user-wiki://page/jackie-oliver/source",
    "talk": "user-wiki://page/jackie-oliver/talk",
    "curator": "user-wiki://page/jackie-oliver/curator",
    "conventions": "user-wiki://page/jackie-oliver/conventions"
  },
  "metadata": {
    "origin": "created_from_template",
    "template_state": "template_unedited",
    "dirty_since_publish": true,
    "talk_state": "ready"
  },
  "page_status": {
    "operation": "wiki.page.status",
    "id": "jackie-oliver",
    "state": "needs_publish",
    "next_action": "publish"
  },
  "hashes": {
    "source_sha256": "fresh-created-source-hash"
  }
}
```

### `wiki.page.delete`

Delete is tombstone-first. It removes the page from the active rendered surface
without destroying the user's history by default.

Request:

```json
{
  "action": "wiki.page.delete",
  "page": {"id": "dummy-custom"},
  "mode": "tombstone",
  "publish": true,
  "actor": {"kind": "operator"}
}
```

Responsibilities:

- disable the active `wiki.toml` page entry or mark it tombstoned by policy
- write `source/<slug>.tombstone.toml`
- preserve source, talk, and proposal history unless an explicit destructive
  purge is approved
- scan source markdown for inbound links that will break when this route and
  its route descendants disappear
- make the next publish remove rendered routes for that page
- return the tombstone, affected routes, and a `link_impact` preview

Receipt excerpt:

```json
{
  "operation": "wiki.page.delete",
  "id": "dummy-custom",
  "route": "/dummy-custom",
  "type": "context-page",
  "collection": "custom",
  "next_action": "repair_links",
  "link_impact": {
    "status": "warning",
    "deleted_route": "/dummy-custom",
    "deleted_markdown_path": "/dummy-custom.md",
    "post_publish_expected_next_action": "repair_links",
    "inbound_link_count": 2,
    "source_page_count": 1,
    "issues": [
      {
        "code": "would_break_internal_link",
        "phase": "pre_delete_source_link_scan",
        "source_page_id": "topics",
        "href": "/dummy-custom",
        "target": "/dummy-custom",
        "target_kind": "route"
      }
    ]
  }
}
```

The preview is source-level evidence so an agent can repair obvious links
before publish. Post-render `link_diagnostics` remains the authoritative
reader-surface check after publish.

### `wiki.page.restore`

Restore is the explicit inverse of tombstone delete. It removes the page's
tombstone, re-enables the page record, restores the page to navigation according
to its saved `nav_section`, appends a `page.restored` ledger event, and returns
`next_action = "publish"`.

It does not recreate missing source. If the source markdown is gone, the caller
must create a new page or perform an explicit manual recovery.

Current CLI and Python adapter:

```text
onecontext-wiki --root ~/1Context page-restore topics
1context wiki page-restore topics
wiki_page_restore(runtime_home, "topics")
```

Receipt excerpt:

```json
{
  "operation": "wiki.page.restore",
  "id": "dummy-custom",
  "render_required": true,
  "next_action": "publish",
  "evidence": [
    {"path": "user-wiki/source/families/custom/dummy-custom/source/dummy-custom.tombstone.toml", "status": "removed_tombstone"},
    {"path": "user-wiki/wiki.toml", "status": "restored_to_navigation"}
  ]
}
```

### `wiki.publish`

Publish current user data to the served site. This is the operation I would
call after editing files.

Request:

```json
{
  "action": "wiki.publish",
  "scope": {"kind": "page", "id": "topics", "route": "/topics"},
  "mode": "if_changed",
  "wait": "completed",
  "actor": {"kind": "agent", "name": "memory-agent"}
}
```

Responsibilities:

- run publish preflight checks
- safe-fill missing configured source/talk files only when the page already
  exists in `wiki.toml`, templates validate, and no user file would be
  overwritten
- fail validation rather than silently inventing ambiguous pages
- fingerprint full publish inputs
- render if changed or forced
- validate staged output
- run post-render internal link diagnostics against the route manifest
- preserve last-good output on failure
- return structured evidence
- return a nonzero CLI/process status when JSON `status` is `failed`, while
  still printing the failure evidence as JSON

Current CLI and Python adapter:

```text
onecontext-wiki --root ~/1Context publish --wiki-engine wiki-engine --node node --trigger agent
1context wiki publish --trigger agent
wiki_publish(runtime_home, wiki_engine="wiki-engine", node="node", trigger="agent")
```

Daemon receipt note: `1context wiki publish` returns the Rust publish receipt
plus `app_publish`. `app_publish.status="published"` means the Local Web mirror
was updated from the validated `user-wiki/site`. If the core publish succeeds
but app mirroring fails, the daemon returns `status="failed"` and
`next_action="repair_publish_mirror"` so consumers do not mistake a hidden
source-site publish for an app-visible one.

`--node` / `node=` accepts an executable name or path. It is not a shell command
string; use `node` or `/absolute/path/to/node`, not `/usr/bin/env node`.

Consumer rule: a fresh/default runtime may call `wiki.publish` directly. If
configured pages are missing source or talk files, publish first runs a
`wiki.publish.preflight` lifecycle pass that safe-creates already configured
page files with the same rules as `wiki.page.create`, then renders from the
created user-owned files. The receipt keeps both states:

- `before`: the status before any publish preflight. For a raw default runtime,
  this has `next_action="publish"` and lists `pages_missing_source`.
- `preflight`: ordered lifecycle actions publish performed before rendering.
  The top-level preflight operation is `wiki.publish.preflight`; nested
  per-page receipts keep the canonical `operation="wiki.page.create"`.
- `render_input`: the status after preflight and before rendering. This can
  still have `next_action="publish"` because newly backfilled pages now exist
  and need the render pass.
- `after`: the status after render, validation, and link diagnostics.

Target preflight checks:

- `wiki.toml` parses and has no duplicate ids or routes
- every enabled page is either backed by source, tombstoned, generated, or a
  valid alias
- template paths stay under `user-wiki/templates`
- new page templates render valid renderer frontmatter before registry mutation
- talk folders have valid `_meta.yaml`
- browser-visible output will not expose local paths
- source/talk/template fingerprints are recorded for skip decisions
- V0 publish freshness writes a full-site fingerprint plus
  `.1context/page-fingerprints.json`. Page fingerprints include the page
  registry row, source markdown, and tombstone state. The full-site fingerprint
  catches site-map, navigation, and any other render input changes without
  pretending every page source is dirty. Talk-message, mailbox, and
  notification churn is inbox state and does not force a page-content publish.
  A caller may still force a render when it intentionally wants rendered talk
  output refreshed.
- `publish-status.pages_needing_publish` is page-scoped: editing one page
  should not make untouched pages report `dirty_since_publish=true`.
- `publish-status.site_needs_publish=true` means the rendered site is stale
  even if `pages_needing_publish` only names the page-local source actions.
  A raw fixture or first-run user wiki with no published site fingerprint also
  reports `site_needs_publish=true`.
- `publish-status.link_health.fresh=false` means the link report is from the
  last publish and should not be treated as post-render truth until
  `wiki.publish` runs.
- `publish-status` is operation-shaped: `operation="wiki.publish.status"` and
  `surface="wiki_publish_status"`.

Target success:

```json
{
  "schema_version": 1,
  "status": "published",
  "site_needs_publish": false,
  "action_id": "uuid",
  "scope": {"kind": "page", "id": "topics", "route": "/topics"},
  "preflight": {"status": "passed", "issue_count": 0},
  "render": {
    "status": "published",
    "skipped": false,
    "routes_changed": ["/topics", "/topics/talk"],
    "route_count": 8,
    "markdown_twin_count": 8
  },
  "published": {
    "source_site": "user-wiki://site",
    "served_site": "app-support://wiki-site/current"
  },
  "link_health": {
    "status": "warning",
    "fresh": true,
    "broken_internal_count": 1,
    "pages_with_broken_links": ["topics"]
  },
  "link_diagnostics": {
    "status": "warning",
    "issue_count": 1,
    "broken_internal_count": 1,
    "issues": [
      {
        "code": "broken_internal_link",
        "severity": "warning",
        "phase": "post_render_link_check",
        "page_id": "topics",
        "route": "/topics",
        "source_path": "topics.html",
        "markdown_path": "topics.md",
        "href": "/old-page",
        "target": "/old-page",
        "manifest_path": ".1context/route-manifest.json",
        "suggested_actions": ["edit_source", "replace_link", "publish"]
      }
    ]
  }
}
```

Target failure:

```json
{
  "schema_version": 1,
  "status": "failed",
  "action_id": "uuid",
  "failed_stage": "validate",
  "last_good_preserved": true,
  "served_site": "app-support://wiki-site/current",
  "issues": [
    {
      "code": "page_missing_slug",
      "severity": "error",
      "message": "topics.md is missing slug frontmatter",
      "paths": ["user-wiki/source/families/reference/topics/source/topics.md"]
    }
  ]
}
```

### `wiki.list`

Return the site map I need as a consumer: configured pages, source files, talk
folders, tombstones, rendered routes, collections, page types/templates, and
compact per-page metadata.

Current app/daemon surface:

- daemon JSON-RPC method: `wiki.list`
- params: `{}`
- CLI wrapper: `1context wiki list`

```json
{
  "action": "wiki.list",
  "include": [
    "configured",
    "source",
    "talk",
    "rendered",
    "tombstoned",
    "collections",
    "page_types"
  ]
}
```

This replaces path guessing. A caller should be able to ask "what pages are
available?" and "what can I create?" without walking `source/families/**` or
reading template directories.

Result receipts are operation-shaped like the rest of the API:

```json
{
  "schema_version": 1,
  "status": "ok",
  "operation": "wiki.list",
  "surface": "wiki_inventory",
  "page_count": 7,
  "source_page_count": 4,
  "generated_page_count": 3,
  "pages": []
}
```

Each page row should carry:

- `schema_version`, `status="ok"`, `operation="wiki.page.status"`, and
  `surface="wiki_page_status"` so a row can be handled like a direct
  `wiki.page.status` receipt
- stable `id`, `title`, `route`, `type`, and `collection`
- `nav_section` when explicitly placed in primary, utility, hidden, or another
  configured navigation section
- `kind`: `source_page`, `generated_site_page`, or `alias`
- one canonical `state`: `rendered`, `needs_publish`, `source_missing`,
  `talk_missing`, `invalid`, `tombstoned`, `orphan_source`, `disabled`, or
  `generated`
- boring `flags` for cheap scanning: `configured`, `enabled`,
  `source_backed`, `rendered`, `stale`, `tombstoned`, `talk_ready`,
  `template_derived`, `runtime_default`, `custom_created`, and `user_edited`
- `content_state` such as `template_unedited`, `edited`, `generated`,
  `missing_source`, or `unknown`
- `origin` such as `runtime_default`, `created_from_template`,
  `operator_created`, `agent_created`, `imported`, `generated_site_page`, or
  `unknown`. Packaged source pages should declare `runtime_default`; pages born
  through `wiki.page.create` should declare `created_from_template`; generated
  `[[site_pages]]` should declare `generated_site_page`, so consumers can tell
  shipped defaults, custom editable pages, and generated navigation pages apart
  without reading `wiki.toml`.
- `template_state` such as `template_unedited`, `edited_from_template`,
  `not_template_backed`, `template_changed`, or `unknown`
- `template` with `relative_path`, user-data-relative `path`, current template
  `sha256` when the template file exists, and `baseline_sha256` when the page
  was created from a template baseline
- source, talk, and rendered handles when present
- `dirty_since_publish` and `talk_state`
- open issue count when validation has recent results

Example compact row:

```json
{
  "id": "topics",
  "title": "Topics",
  "route": "/topics",
  "nav_section": "primary",
  "type": "index",
  "collection": "topics",
  "kind": "source_page",
  "state": "needs_publish",
  "content_state": "edited",
  "origin": "created_from_template",
  "template_state": "edited_from_template",
  "template": {
    "relative_path": "pages/e08/topics.md",
    "path": "user-wiki/templates/pages/e08/topics.md",
    "sha256": "hex-sha256",
    "baseline_sha256": "hex-sha256"
  },
  "dirty_since_publish": true,
  "talk_state": "has_open_entries",
  "flags": {
    "configured": true,
    "enabled": true,
    "source_backed": true,
    "rendered": true,
    "stale": true,
    "tombstoned": false,
    "talk_ready": true,
    "template_derived": true,
    "runtime_default": false,
    "custom_created": true,
    "user_edited": true
  },
  "last_publish": {
    "status": "published",
    "at": "ISO-8601"
  },
  "validation": {
    "status": "warning",
    "issue_count": 1,
    "blocking_count": 0,
    "warning_count": 1,
    "highest_severity": "warning"
  },
  "issue_count": 1,
  "handles": {
    "source": "user-wiki://page/topics/source",
    "talk": "user-wiki://page/topics/talk",
    "published": "app-support://wiki/topics"
  },
  "allowed_actions": [
    "wiki.page.open",
    "wiki.page.patch_body",
    "wiki.page.write_body",
    "wiki.talk.append",
    "wiki.validate",
    "wiki.publish",
    "wiki.page.delete"
  ],
  "next_action": "publish"
}
```

Generated site-page rows are deliberately visible in `wiki.list` and
`wiki.page.status` even though they are not editable source pages. They carry
`kind="generated_site_page"`, `origin="generated_site_page"`,
`flags.source_backed=false`, `talk_state="not_applicable"`, and only
`wiki.validate` / `wiki.publish` as allowed actions. Their `template` points at
the generated site template and has no page baseline. `wiki.page.open`,
`wiki.page.patch_body`, `wiki.page.write_body`, talk append, and delete remain
source-page operations.

`wiki.list` should stay compact. If a caller needs hashes, timestamps, template
baseline details, or render route lists, it should call `wiki.page.status` for
that page.

### `wiki.validate`

Run checks without publishing. This is the detailed diagnostic surface. The
current Rust CLI is:

```bash
onecontext-wiki --root ~/1Context validate
```

Current V0 validation reports:

- missing configured source files
- missing configured talk folders
- pages that need publish
- stale or missing site output
- last-publish broken internal links

The broader target validation should also report:

- registry errors
- missing templates
- route collisions
- tombstones
- renderer-frontmatter errors
- unsafe public output

The output is a typed issue list with severities and paths. That is enough; V0
does not need a separate `wiki.explain`.

Validation result headers should include:

- `scope`
- `checked_at`
- `input_fingerprint`
- `can_publish`
- `status`
- `issue_count`
- `blocking_count`
- `warning_count`
- `highest_severity`

Useful validation issues include a suggested next action, but they should not
become a separate repair engine:

```json
{
  "schema_version": 1,
  "operation": "wiki.validate",
  "status": "warning",
  "scope": "site",
  "can_publish": true,
  "issue_count": 1,
  "blocking_count": 0,
  "warning_count": 1,
  "highest_severity": "warning",
  "next_action": "publish",
  "issues": [{
    "code": "configured_page_missing_source",
    "severity": "warning",
    "page": {"id": "jackie-oliver", "route": "/people/jackie-oliver"},
    "paths": ["user-wiki/wiki.toml"],
    "next_action": "publish"
  }]
}
```

For broken internal links, `wiki.validate` embeds the same post-render
diagnostics that were written to the publish receipt, so an agent can repair
from one response:

```json
{
  "code": "page_has_broken_internal_links",
  "severity": "warning",
  "page": {"id": "topics", "route": "/topics"},
  "paths": ["/missing-diagnostic-target"],
  "next_action": "repair_links",
  "diagnostics": [{
    "href": "./missing-diagnostic-target",
    "target": "/missing-diagnostic-target",
    "markdown_path": "topics.md",
    "source_path": "topics.html",
    "route": "/topics",
    "suggested_actions": ["edit_source", "replace_link", "publish"]
  }]
}
```

### `wiki.status`

Status is the tiny top-level card an agent should read before deciding what to
do next. The current Rust CLI is:

```bash
onecontext-wiki --root ~/1Context status
```

Result shape:

```json
{
  "schema_version": 1,
  "operation": "wiki.status",
  "status": "ok",
  "state": "idle",
  "next_action": "none",
  "page_count": 7,
  "source_page_count": 4,
  "generated_page_count": 3,
  "agents_summary": {"active_count": 0, "stale_count": 0},
  "mail_summary": {"delivery_count": 0, "unread_count": 0, "notification_count": 0},
  "publish": {"next_action": "none", "render_required": false},
  "last_publish": {"status": "published", "at": "ISO-8601", "route_count": 11},
  "validation": {"status": "ok", "issue_count": 0}
}
```

No renderer logs, no verbose path dump, no explanation engine. `state` is
`idle`, `attention`, or `blocked`. If the user or agent wants details, call
`wiki.validate`, `wiki.page.status`, or read the publish receipt.

### Talk, Mail, Directory, Notifications, And Curators

The page API should not pretend talk is only a rendered companion page. Talk,
mail, agent directory, and notifications are part of the first wiki API because
agents need a way to coordinate without reading entire talk folders. The
private-4 system points at the better model: a talk folder is an append-only
workbench and mailing-list archive for proposals, concerns, questions,
decisions, deferrals, redactions, contradictions, and curator closures.

Talk has two separate surfaces:

- Source/workflow surface:
  `user-wiki/source/families/<group>/<family>/talk/<slug>.talk/*.md`
- Reader surface:
  `/page/talk`, `/page.talk.html`, and `/page.talk.md`

Agents write the source/workflow surface. Users read the reader surface. The
renderer connects them, but the app should never ask agents to edit rendered
talk output.

The files stay first-class:

- `_meta.yaml` says which page the talk folder belongs to.
- `_conventions.md` says how this page's discussions should be written.
- `_curator.md` is the page-specific job prompt for deciding what lands in the
  article.
- Timestamped entries carry typed frontmatter such as `proposal`, `concern`,
  `question`, `decided`, `deferred`, `contradiction`, `redacted`, or `reply`.
- `parent:` links entries into threads without needing a database table first.

The first publishing API should be designed as mail, not as "read the whole
talk page":

- `wiki.agent.register`: bind a live agent to roles, capabilities, and a lease.
- `wiki.agent.heartbeat`: keep that lease alive.
- `wiki.agent.retire`: remove the live agent from active routing.
- `wiki.agent.whoami`: resolve a thread/session pointer or agent id into the
  directory entry an agent should use.
- `wiki.agent.list`: list active agents by default, with explicit stale/retired
  inclusion for audits and handoffs.
- `wiki.agent.status`: inspect one agent's liveness, mailboxes,
  subscriptions, and next action without requiring it to be currently active.
- `wiki.agent.inbox`: union direct role/agent mail, subscribed mailbox/list
  mail, and pending notifications for one live agent.
- `wiki.agent.claim`: claim one actionable message from an agent's unified
  inbox without making the caller choose the underlying role, list, or page
  delivery.
- `wiki.talk.append`: append one validated typed entry to a page talk folder,
  optionally with `reply_to`, explicit `thread_id`, and attachments.
- `wiki.mail.inbox`: list pending headers by recipient, page, kind, parent,
  target section, and decision state without forcing agents to scan every file.
  Archived messages are hidden by default, but callers can request
  `include_archived` when proving durability, auditing deleted-page mail, or
  recovering prior work.
- `wiki.mail.read`: hydrate one message or thread on demand.
- `wiki.mail.claim`: atomically claim a message for one active agent and
  return a conflict if another active agent already claimed it.
- `wiki.mail.mark`: seen, claimed, done, timed snoozed, or archived.
- `wiki.mail.subscribe`: route list, role, or mailbox wakeups to a live agent
  without copying or duplicating the underlying mail.
- Page-recipient aliases are accepted wherever an operation expects a mail
  recipient address. `page://topics` and `page:///topics` normalize to the
  durable `mailbox://page/topics` address in delivery records, inbox receipts,
  claim receipts, and subscription records.
- `wiki.notify.poll`: read wakeups for transports that cannot receive direct
  push.
- `wiki.notify.ack`: record notification delivery or dismissal.
- `wiki.curator.apply`: apply an accepted decision through an owned section,
  sandbox/diff check, operator-touched gate, source promotion, and publish.

The talk workflow I would want:

```text
wiki.agent.register(thread_id, roles: ["role://topics.curator"])
wiki.agent.whoami(thread_id)
wiki.mail.subscribe(agent_id, address: "list://wiki.reviewers", kinds: ["review"])
wiki.agent.inbox(agent_id)
wiki.mail.inbox(recipient: "role://topics.curator", state: "unread")
wiki.talk.append(page: "topics", kind: "proposal", target_section: "Tools", attachments: ["screenshot.png"])
curator reads _curator.md + relevant open entries
wiki.mail.mark(message: "talkmsg_...", state: "claimed")
wiki.curator.apply(decision: "2026-05-19T08-20Z.decided.tools-cleanup.md")
wiki.publish(page: "topics", wait: true)
wiki.agent.retire(agent_id)
```

Kind and state are separate. `kind` describes what the message is:
`conversation`, `proposal`, `question`, `concern`, `reply`, `decision`,
`deferral`, `contradiction`, or `redaction`. `state` describes lifecycle:
`open`, `accepted`, `rejected`, `resolved`, `withdrawn`, `superseded`,
`blocked`, or `archived`.

All mutating collaboration calls accept an optional `idempotency_key`; retries
with the same key must return the same operation receipt or a typed conflict.

### Target Talk, Mail, Agent, And Notification Schemas

`wiki.agent.register`:

```json
{
  "action": "wiki.agent.register",
  "idempotency_key": "agent-start-019e3f72",
  "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01",
  "roles": ["role://topics.curator"],
  "capabilities": ["wiki.mail", "wiki.curator.apply"],
  "ttl_seconds": 1800,
  "push_transport": "codex-thread"
}
```

Result:

```json
{
  "schema_version": 1,
  "status": "registered",
  "operation": "wiki.agent.register",
  "agent_id": "agent_codex_019e3f72",
  "primary_address": "agent://codex/019e3f72",
  "addresses": ["agent://codex/019e3f72", "role://topics.curator"],
  "mailboxes": [
    {"address": "role://topics.curator", "unread_count": 2}
  ],
  "operation_id": "op_..."
}
```

Use `primary_address` when sending direct mail to the agent. `agent_id` is a
directory/control key, not a mailbox. Addresses like
`agent://agent_codex_...` are rejected because they usually mean the caller
confused the id with the agent's actual `agent://codex/<thread-id>` mailbox
address.

`wiki.agent.identify` is the normal waking-agent entrypoint:

```json
{
  "action": "wiki.agent.identify",
  "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01",
  "roles": ["role://topics.curator"],
  "capabilities": ["wiki.mail", "wiki.curator.apply"],
  "ttl_seconds": 1800
}
```

Result:

```json
{
  "schema_version": 1,
  "status": "registered | identified | refreshed | retired",
  "action": "registered | identified | refreshed | retired",
  "operation": "wiki.agent.identify",
  "agent_id": "agent_codex_019e3f72",
  "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01",
  "primary_address": "agent://codex/019e3f72",
  "addresses": ["agent://codex/019e3f72", "role://topics.curator"],
  "liveness_before": "stale",
  "liveness_after": "active",
  "agent": {"agent_id": "agent_codex_019e3f72", "liveness": "active"},
  "mailboxes": [],
  "subscriptions": [],
  "next_action": "none",
  "repair_hints": []
}
```

Missing sessions are registered, active sessions are identified and lease
refreshed, stale sessions are refreshed directly, and retired sessions refuse
silent resurrection with `next_action="agent_register_new_thread"`.
The older low-level `wiki.agent.register` call is create-only. It refuses both
retired identities and already-known live/stale identities; use
`wiki.agent.identify` to refresh a lease or merge roles/capabilities. A retired
thread/session is a durable clean-exit record, and a new live agent needs a new
transport pointer.

`wiki.agent.heartbeat`:

```json
{
  "action": "wiki.agent.heartbeat",
  "agent_id": "agent_codex_019e3f72",
  "lease_extend_seconds": 1800,
  "last_seen": {
    "inbox_cursor": "mailcur_...",
    "active_page": "topics"
  }
}
```

`wiki.agent.retire`:

```json
{
  "action": "wiki.agent.retire",
  "agent_id": "agent_codex_019e3f72",
  "reason": "completed"
}
```

`wiki.agent.whoami`:

```json
{
  "action": "wiki.agent.whoami",
  "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01"
}
```

Result:

```json
{
  "schema_version": 1,
  "status": "ok",
  "operation": "wiki.agent.whoami",
  "surface": "agent_whoami",
  "resolved_by": "thread_id",
  "matches": [
    {
      "agent_id": "agent_codex_019e3f72",
      "liveness": "active",
      "next_action": "check_inbox",
      "active_subscription_count": 2,
      "actionable_count": 1
    }
  ],
  "next_action": "check_inbox"
}
```

`wiki.agent.list` returns live agents by default. Stale or retired entries are
opt-in so normal dispatch surfaces do not accidentally treat old sessions as
awake:

```json
{
  "action": "wiki.agent.list",
  "include_stale": true,
  "include_retired": true
}
```

Result excerpt:

```json
{
  "schema_version": 1,
  "status": "ok",
  "surface": "agent_list",
  "counts": {"active_count": 1, "stale_count": 1, "retired_count": 1, "total_count": 3},
  "agents": [
    {
      "agent_id": "agent_codex_019e3f72",
      "liveness": "active",
      "primary_address": "agent://codex/019e3f72",
      "owned_addresses": ["agent://codex/019e3f72", "role://topics.curator"],
      "subscribed_addresses": ["list://topics.watchers"],
      "actionable_count": 1,
      "next_action": "check_inbox"
    }
  ]
}
```

`wiki.agent.status` is the detailed card for one known `agent_id`. It reports
`liveness`, `primary_address`, `lease_expires_at`, `retired_at`, `retire_reason`,
`owned_addresses`, `subscribed_addresses`, active subscription count, mailbox
counts, and `next_action`. Stale agents return
`next_action="agent_identify"` because identify is the safe wakeup/lease
refresh path. Retired agents keep durable mailbox and subscription evidence but
return `next_action="agent_register_new_thread"` because they should not be
silently revived.

Result excerpt:

```json
{
  "schema_version": 1,
  "status": "ok",
  "surface": "agent_status",
  "exists": true,
  "agent": {"agent_id": "agent_codex_019e3f72", "liveness": "active"},
  "mailboxes": [{"address": "role://topics.curator", "actionable_count": 1}],
  "subscriptions": [{"address": "list://topics.watchers", "agent_liveness": "active"}],
  "next_action": "check_inbox"
}
```

`wiki.talk.append`:

```json
{
  "action": "wiki.talk.append",
  "idempotency_key": "topics-tools-proposal-2026-05-19",
  "page": {"id": "topics"},
  "message": {
    "kind": "proposal",
    "state": "open",
    "subject": "Split tools into verified and unverified",
    "target_section": "Tools",
    "from": "agent://codex/019e3f72",
    "to": ["role://topics.curator"],
    "cc": ["list://topics.watchers"],
    "body_markdown": "Proposal text..."
  },
  "attachments": [
    {
      "source": {"kind": "file", "path": "staging/screenshot.png"},
      "filename": "screenshot.png",
      "media_type": "image/png",
      "caption": "Dropdown link did not navigate",
      "alt_text": "Topics dropdown menu with inactive links"
    }
  ]
}
```

V0 attachment rule: attachments are copied into
`<slug>.talk/attachments/<message-id>/` in user-owned talk source. The
attachment record keeps that safe source-relative `path` and a
`user-wiki://...` handle, never an absolute local path. Rendered talk Markdown
links use the page talk route, for example
`/topics/talk/attachments/<message-id>/screenshot.png`, and the renderer copies
the attachment files into the matching published talk asset path so link
diagnostics can prove they resolve. Diagnostics resolve relative talk links
against the rendered document base href, so `attachments/evidence.txt` on
`/topics/talk/` is checked as `/topics/talk/attachments/evidence.txt`, not as a
source-file sibling. Attachment filenames are normalized to safe route/file
tokens: path separators are rejected, duplicate basenames get numeric suffixes,
and punctuation runs collapse into a single dash. Optional attachment
`caption` and `alt_text` metadata are stored in talk frontmatter, returned in
mail/read receipts, and rendered into both HTML and markdown talk twins. When
frontmatter attachments exist, the renderer suppresses the generated markdown
body's trailing attachment section so a talk entry does not show duplicate
attachment links.

Threading rule: if neither `reply_to` nor `thread_id` is supplied, V0 preserves
the existing subject-derived page thread. `reply_to` must target an existing
message on the same page and resolves that parent message's thread even when
the reply subject changes. `thread_id` must target an existing thread on the
same page.

Result:

```json
{
  "schema_version": 1,
  "status": "appended",
  "message_id": "talkmsg_...",
  "thread_id": "thread_...",
  "source": "user-wiki://page/topics/talk/messages/talkmsg_...",
  "attachments": [
    {
      "filename": "screenshot.png",
      "media_type": "image/png",
      "path": "attachments/talkmsg_.../screenshot.png",
      "handle": "user-wiki://page/topics/talk/attachments/talkmsg_.../screenshot.png",
      "caption": "Dropdown link did not navigate",
      "alt_text": "Topics dropdown menu with inactive links"
    }
  ],
  "deliveries": [
    {"recipient": "role://topics.curator", "state": "unread"}
  ],
  "operation_id": "op_..."
}
```

`wiki.mail.inbox` returns headers first:

```json
{
  "action": "wiki.mail.inbox",
  "recipient": "role://topics.curator",
  "state": ["unread", "claimed"],
  "limit": 50
}
```

```json
{
  "schema_version": 1,
  "status": "ok",
  "surface": "agent_action_queue",
  "mailbox": {
    "address": "role://topics.curator",
    "surface": "agent_action_queue",
    "total_count": 5,
    "actionable_count": 4,
    "unread_count": 2,
    "archived_count": 1
  },
  "messages": [
    {
      "message_id": "talkmsg_...",
      "thread_id": "thread_...",
      "reply_to": "talkmsg_parent...",
      "page": {"id": "topics", "route": "/topics"},
      "kind": "proposal",
      "state": "open",
      "subject": "Split tools into verified and unverified",
      "excerpt": "Proposal text...",
      "attachment_count": 1,
      "delivery_state": "unread"
    }
  ]
}
```

`wiki.mail.read` hydrates one message or thread:

```json
{
  "action": "wiki.mail.read",
  "thread_id": "thread_...",
  "include": ["body", "attachments"]
}
```

Result:

```json
{
  "schema_version": 1,
  "status": "ok",
  "operation": "wiki.mail.read",
  "surface": "mail_thread",
  "resolved_by": "thread_id",
  "thread_id": "thread_...",
  "message_count": 1,
  "delivery_count": 2,
  "messages": [
    {
      "message_id": "talkmsg_...",
      "thread_id": "thread_...",
      "reply_to": "talkmsg_parent...",
      "page_id": "topics",
      "route": "/topics",
      "subject": "Proposal",
      "body_markdown": "## Proposal\n\nFull talk body...",
      "attachments": [
        {
          "filename": "context.json",
          "media_type": "application/json",
          "path": "attachments/talkmsg_.../context.json",
          "handle": "user-wiki://page/topics/talk/attachments/talkmsg_.../context.json",
          "caption": "Captured context for the proposal",
          "alt_text": "JSON context attachment"
        }
      ],
      "deliveries": [
        {"recipient": "role://topics.curator", "state": "unread"},
        {"recipient": "list://topics.watchers", "state": "unread"}
      ]
    }
  ]
}
```

Use this after `wiki.notify.poll` or `wiki.agent.inbox` when an agent needs
the full body for one item. It is intentionally narrower than reading the talk
folder directly.

`wiki.mail.mark` changes delivery state, not message truth:

```json
{
  "operation": "wiki.mail.mark",
  "message_id": "talkmsg_...",
  "recipient": "mailbox://page/topics",
  "state": "snoozed",
  "snoozed_until": "<future RFC3339>"
}
```

`recipient` in the receipt is always canonical. For example,
`page://topics` is accepted as input and returns `mailbox://page/topics` so the
caller can continue with a stable mailbox key.

Agent-facing mail commands may also resolve through the agent workbench. If an
active agent sees a role/list/page delivery in `wiki.agent.inbox`, then
`wiki.mail.claim` and `wiki.mail.mark` may be called with that agent's
`primary_address` even when the exact delivery row lives in the canonical
role/list/page mailbox. The receipt still returns the canonical recipient that
was updated. If the address belongs to no active agent, or ambiguously belongs
to more than one active identity, callers must use `wiki.agent.claim` or the
canonical mailbox recipient from `agent_inbox.threads[].recipients[]`.

`snoozed` is a timed overlay, not terminal mail. It requires a future
`snoozed_until` value. While the time is in the future, normal
`wiki.agent.inbox`, `wiki.mail.inbox`, `wiki.notify.poll`, and
`wiki.page.status.mail.open_*` surfaces exclude that delivery. Audit callers
can request snoozed deliveries explicitly. When the time is due, the same
delivery becomes actionable again and existing notification wakeups can surface
through `wiki.notify.poll`.

`wiki.mail.mark_all` changes every delivery for the same message. Use it when a
curator or resolver has decided the work is done everywhere, especially for one
talk entry that was delivered to a role, list, and page mailbox:

```json
{
  "action": "wiki.mail.mark_all",
  "message_id": "talkmsg_...",
  "state": "done"
}
```

Target receipt:

```json
{
  "operation": "wiki.mail.mark_all",
  "message_id": "talkmsg_...",
  "state": "done",
  "recipients": ["list://topics.watchers", "mailbox://page/topics", "role://topics.curator"],
  "changed_delivery_count": 3,
  "before": {"delivery_count": 3, "open_delivery_count": 3, "unread_count": 3},
  "after": {"delivery_count": 3, "open_delivery_count": 0, "unread_count": 0}
}
```

`changed_delivery_count` counts only deliveries whose state actually changed.
If one recipient was already `done`, `delivery_count` still includes it in
`before`/`after`, but `changed_delivery_count` excludes it and that mailbox
evidence is reported as `unchanged`.

`wiki.mail.unsubscribe` cancels matching active subscriptions for one live
agent. Use it when an agent leaves a list, stops watching a page, or narrows
which message kinds should wake it. It does not delete durable list mail; it
only removes future live wakeups for that subscription filter:

```json
{
  "action": "wiki.mail.unsubscribe",
  "agent_id": "agent_codex_019e3f72",
  "address": "list://topics.watchers",
  "relation": "watcher",
  "kinds": ["proposal"]
}
```

Target receipt:

```json
{
  "operation": "wiki.mail.unsubscribe",
  "status": "unsubscribed",
  "cancelled_count": 1,
  "remaining_count": 0,
  "next_action": "none"
}
```

`remaining_count` is scoped to the same unsubscribe filter, not the agent's
entire subscription inventory. Call `wiki.mail.subscriptions` or
`wiki.list.members` when the agent needs a full roster/audit view.

Agents should use `wiki.agent.claim` from a unified `wiki.agent.inbox` result.
It resolves the best actionable delivery for that agent, preferring direct
owned addresses, then role assignments, then member/subscriber/list/page
watcher deliveries:

```json
{
  "action": "wiki.agent.claim",
  "agent_id": "agent_codex_019e3f72",
  "message_id": "talkmsg_..."
}
```

Use `wiki.mail.claim` only when the caller already knows the exact mailbox
recipient it intends to claim:

```json
{
  "action": "wiki.mail.claim",
  "message_id": "talkmsg_...",
  "recipient": "mailbox://page/topics",
  "agent_id": "agent_codex_019e3f72"
}
```

The first claimant receives `status="claimed"`. Repeating the same claim
returns `status="already_claimed"`. A competing agent receives a typed
`mail_already_claimed` error and should read `wiki.agent.inbox` or
`wiki.mail.inbox` to see `claimed_by`. `done` and `archived` are terminal for
page open-thread counts. Terminal mail does not keep notification pressure in
`wiki.agent.inbox`; archived mail is hidden unless `include_archived` is
requested.
`wiki.agent.inbox.summary.pages_with_open_mail_count` is mail-specific. It
answers "how many pages still have open deliveries for me?" separately from
`pages_requiring_action`, which can be true because a page itself needs publish
or repair work.

`wiki.mail.subscribe` is an agent wakeup rule, not a second mailbox. List
addresses must be created first with `wiki.list.create`; subscribing to an
unknown `list://` address is a typed `unknown_list` error so agents do not
silently invent phantom routing surfaces:

```json
{
  "action": "wiki.mail.subscribe",
  "agent_id": "agent_codex_019e3f72",
  "address": "list://wiki.reviewers",
  "relation": "member",
  "kinds": ["review"],
  "ttl_seconds": 1800
}
```

The durable mail still lands in `list://wiki.reviewers`. While the agent lease
and subscription lease are active, matching deliveries also create
notifications for that agent. Kind filters are exact tokens; an empty `kinds`
list means all kinds for that address. V0 stores subscription events in
`context-engine/mail/subscriptions.jsonl`. Subscribe receipts include a
backfill summary for existing mailbox messages surfaced by the new
subscription. Historical mail becomes visible in `wiki.agent.inbox`, but V0
does not create retrospective notifications; notifications are for future
deliveries.

For page work, agents should prefer page-aware helpers over hand-typed routing
addresses:

```json
{
  "action": "wiki.page.watch",
  "page": "topics",
  "agent_id": "agent_codex_019e3f72",
  "kinds": ["review"]
}
```

`wiki.page.watch` creates or reuses the default `list://topics.watchers` list
and subscribes the agent as a watcher. It also subscribes the agent to the
page mailbox, such as `mailbox://page/topics`, so direct page talk reaches the
same watcher. The receipt includes `unsubscribe_plan` so agents do not have to
reverse-engineer both cleanup addresses:

```json
{
  "operation": "wiki.page.watch",
  "unsubscribe_plan": {
    "operation": "wiki.page.unwatch",
    "page": "topics",
    "list_address": "list://topics.watchers",
    "page_mailbox_address": "mailbox://page/topics",
    "relation": "watcher",
    "kinds": ["review"]
  }
}
```

`wiki.page.unwatch` is the page-aware cleanup pair. It cancels matching watcher
subscriptions from both the default watchers list and the page mailbox, then
returns per-address unsubscribe receipts and fresh page subscription state:

```json
{
  "action": "wiki.page.unwatch",
  "page": "topics",
  "agent_id": "agent_codex_019e3f72",
  "kinds": ["review"]
}
```

If `kinds` are provided, they match exact kind sets, just like
`wiki.mail.unsubscribe`. If broader page-mailbox watches remain, the receipt
returns `next_action="mail_subscriptions"`; a broad unwatch with no kind filter
clears all page watcher subscriptions for that agent.

For `wiki.talk.append`, `page://topics` and `page:///nested/route` are accepted
as page-recipient shorthand. The core resolves them to configured pages and
stores/delivers the canonical `mailbox://page/<page-id>` address in receipts,
frontmatter, mailboxes, and notifications. Raw inbox/control commands still
use `mailbox://page/<page-id>`.

```json
{
  "action": "wiki.page.assign_role",
  "page": "topics",
  "agent_id": "agent_codex_019e3f72",
  "role": "curator",
  "kinds": ["proposal"]
}
```

`wiki.page.assign_role` expands role shorthand to `role://topics.curator` and
subscribes the agent as an assignee. Fully qualified page roles still work, and
`role://curator` is normalized as page-local shorthand for
`role://<page>.curator`.

`wiki.list.create` gives a list address durable metadata without changing how
mail delivery works. Recreating an existing list is idempotent in spirit but not
destructive: V0 returns `already_exists` and preserves the original metadata.

```json
{
  "action": "wiki.list.create",
  "address": "list://topics.watchers",
  "title": "Topics Watchers",
  "description": "Agents watching the Topics page and nearby topic links.",
  "page_id": "topics",
  "owner": "agent_codex_019e3f72"
}
```

`owner` may be a concrete mail address or an active `agent_id`. The core saves
the list owner as the agent's primary `agent://...` address so later consumers
do not have to remember whether the list was created by id or by address.

`wiki.lists` reads the list directory from
`context-engine/mail/lists.jsonl` and includes durable list metadata plus active
member counts:

```json
{
  "operation": "wiki.lists",
  "status": "ok",
  "lists": [{
    "address": "list://topics.watchers",
    "created_at": "2026-05-20T10:22:00Z",
    "member_count": 2,
    "active_member_count": 1,
    "inactive_member_count": 1
  }]
}
```

`wiki.mail.subscriptions` can be filtered by agent or address. Filtering by
address is the V0 roster view for page watchers, role assignees, and list
members:

```json
{
  "operation": "wiki.mail.subscriptions",
  "status": "ok",
  "subscription_count": 2,
  "liveness_counts": {
    "active_agent_count": 2,
    "inactive_agent_count": 0
  },
  "next_action": "none",
  "subscriptions": [{
    "address": "list://wiki.reviewers",
    "agent_liveness": "active"
  }]
}
```

Rows are liveness-enriched in the same shape as list/page rosters:
`agent_liveness`, `agent_lease_expires_at`, `agent_retired_at`, and
`agent_retire_reason` are present even when the subscribed agent is stale or
retired. Filtering by `agent_id` is therefore an audit/read operation, not an
active-agent requirement.

`wiki.list.members` is the ergonomic alias for the same address-filtered
roster:

```json
{
  "operation": "wiki.list.members",
  "status": "ok",
  "exists": true,
  "list": {
    "address": "list://topics.watchers",
    "created_at": "2026-05-20T10:22:00Z"
  },
  "member_count": 2,
  "active_member_count": 1,
  "inactive_member_count": 1,
  "next_action": "none"
}
```

`wiki.list.status` is the one-call list workbench view. It returns list
metadata, liveness-enriched subscriptions, mailbox counts, and recent list
messages. By default it hides archived mail and future-snoozed mail, but it
still exposes top-level audit flags and hidden counts so agents can tell the
difference between "empty" and "hidden by audit filter." Pass
`--include-archived` and/or `--include-snoozed` to include those messages in
the returned `messages` array. It also repeats the top-level member counts from
`wiki.list.members` so callers can branch without knowing the nested `list`
shape:

```json
{
  "operation": "wiki.list.status",
  "status": "ok",
  "exists": true,
  "list": {
    "address": "list://topics.watchers",
    "created_at": "2026-05-20T10:22:00Z"
  },
  "member_count": 2,
  "active_member_count": 1,
  "inactive_member_count": 1,
  "include_archived": false,
  "include_snoozed": false,
  "has_archived": true,
  "has_snoozed": true,
  "hidden_archived_count": 1,
  "hidden_snoozed_count": 1,
  "audit_flags": ["archived_hidden", "snoozed_hidden"],
  "mailbox": {"actionable_count": 1, "unread_count": 1},
  "next_action": "mail_claim_or_mark"
}
```

If `mailbox.next_action` is otherwise `none` but archived or future-snoozed
mail is hidden by the default filters, `wiki.list.status` returns
`next_action="include_hidden_mail"` so agents can choose an audit read with
`--include-archived` and/or `--include-snoozed` instead of mistaking the list
for clean empty work.

`wiki.page.status`, `wiki.page.open`, and `wiki.list` expose actionable page
mail addresses directly: `mail.page_mailbox`, `mail.curator_address`, and
`mail.default_watchers_list`. They also expose page mailbox watchers plus
obvious page-scoped list subscriptions such as `list://topics.watchers` and
page role addresses such as `role://topics.curator` under
`mail.watcher_count` and `mail.subscriptions`, so an agent can see who is
watching, assigned to, or following a page before editing it.
Explicit list objects whose metadata says `page_id = "topics"` are also exposed
under `mail.associated_lists` and included in page mail pressure, even when the
list address is not page-shaped, such as `list://wiki.reviewers`.
`mail.watcher_count` is unique by agent, so a page watcher subscribed to both
the watchers list and page mailbox is still counted once.
Watcher and member counts are liveness-aware. `watcher_count` / `member_count`
remain total unique subscribed agents for the roster, while
`active_watcher_count`, `inactive_watcher_count`, `active_member_count`,
`inactive_member_count`, and `subscription_liveness_counts` distinguish active,
stale, retired, and unknown agents. On `wiki.page.status`,
`mail.subscription_liveness_counts` covers every page-related subscription,
including explicit lists whose metadata points at the page; watcher counts are
still watcher-only. `wiki.list.members` and
`wiki.list.status` include `subscriptions[].agent_liveness`,
`agent_lease_expires_at`, `agent_retired_at`, and `agent_retire_reason`; use
the active counts before assuming a page or list is staffed right now.
`mail.actionable_count` mirrors `mail.open_delivery_count` on page surfaces so
agents can use the same pressure field they use on inboxes. `open_delivery_count`
is recipient pressure; `open_thread_count` is conversation pressure. One talk
entry delivered to a role, list, and page mailbox can therefore show three open
deliveries but one open thread until `wiki.mail.mark_all` resolves it
everywhere.

After publish, broken internal links are available in
`user-wiki/site/.1context/link-diagnostics.json` and are also annotated in the
affected rendered HTML. The reader-visible annotation is post-render output
only; source markdown is left untouched. The checker handles root-relative
links such as `/topics/foo` and relative links such as `./topics/foo`.
The route manifest also carries a compact `link_diagnostics` pointer and health
summary so rendered-site consumers can discover diagnostics from
`.1context/route-manifest.json` without already knowing the sibling file name.

`wiki.agent.inbox` is the normal "what is waiting for me?" call:

```json
{
  "schema_version": 1,
  "status": "ok",
  "surface": "agent_inbox",
  "agent_id": "agent_codex_019e3f72",
  "summary": {
    "actionable_count": 2,
    "claimable_count": 2,
    "unread_count": 2,
    "message_count": 2,
    "thread_count": 1,
    "actionable_thread_count": 1,
    "claimable_thread_count": 1,
    "notification_count": 2,
    "notification_thread_count": 1,
    "pages_with_open_mail_count": 1,
    "pages_requiring_action": 1
  },
  "owned_addresses": ["agent://codex/019e3f72"],
  "subscribed_addresses": ["role://topics.curator", "list://wiki.reviewers"],
  "effective_mailboxes": ["agent://codex/019e3f72", "role://topics.curator", "list://wiki.reviewers"],
  "addresses": ["agent://codex/019e3f72", "role://topics.curator", "list://wiki.reviewers"],
  "subscriptions": [
    {
      "address": "list://wiki.reviewers",
      "relation": "member",
      "agent_liveness": "active",
      "agent_lease_expires_at": "ISO-8601"
    }
  ],
  "mailboxes": [],
  "pages": [
    {
      "id": "topics",
      "route": "/topics",
      "state": "needs_publish",
      "next_action": "publish",
      "actionable_count": 2,
      "claimable_count": 2,
      "unread_count": 2,
      "message_count": 2
    }
  ],
  "threads": [
    {
      "thread_id": "thread_topics_example",
      "message_id": "talkmsg_...",
      "page_id": "topics",
      "subject": "Proposal",
      "delivery_count": 3,
      "actionable_delivery_count": 2,
      "claimable_delivery_count": 2,
      "notification_count": 2,
      "attachment_count": 1,
      "recipients": [
        {"recipient": "role://topics.curator", "state": "claimed"},
        {"recipient": "mailbox://page/topics", "state": "read"},
        {"recipient": "list://topics.watchers", "state": "snoozed"}
      ]
    }
  ],
  "messages": [],
  "notifications": []
}
```

`messages` remains the raw per-delivery list. `threads` is the default
workbench shape for agents: one talk thread delivered to a role, page mailbox,
and list appears as one thread with expandable recipient state. Thread summaries
include `attachment_count` so an agent can prioritize attachment-bearing work
before expanding raw messages.
`actionable_count` means open work still exists. `claimable_count` means this
specific agent can claim or continue claiming it. A shared delivery already
claimed by another agent remains actionable history, but drops to
`claimable_count=0` for competitors and changes the inbox `next_action` to
`mail_read_or_watch_claim` instead of inviting a failing claim attempt.

`wiki.notify.poll` returns wakeups, not full mail bodies:

```json
{
  "action": "wiki.notify.poll",
  "agent_id": "agent_codex_019e3f72",
  "cursor": "notifcur_...",
  "limit": 50
}
```

`wiki.notify.poll` is also delivery-state and subscription aware. Unacked
wakeups stop surfacing once their underlying delivery is terminal (`done` or
`archived`), and list/role/page-mailbox wakeups are filtered through the
agent's active owned addresses plus active subscription rules. Expired,
cancelled, or kind-mismatched subscriptions do not keep stale wakeups alive, so
callers do not need to ack old notifications just to keep a quiet inbox after
`wiki.mail.mark_all` or `wiki.mail.unsubscribe`.

```json
{
  "schema_version": 1,
  "status": "ok",
  "notifications": [
    {
      "notification_id": "notif_...",
      "recipient": "agent://codex/019e3f72",
      "agent_address": "agent://codex/019e3f72",
      "delivery_recipient": "role://topics.curator",
      "mailbox": "role://topics.curator",
      "message_id": "talkmsg_...",
      "thread_id": "thread_...",
      "page_id": "topics",
      "route": "/topics",
      "kind": "proposal",
      "subject": "Proposal",
      "excerpt": "Short proposal preview...",
      "attachment_count": 1,
      "urgency": "normal",
      "cursor": "notifcur_..."
    }
  ]
}
```

`recipient` is retained as the wakeup transport address for compatibility.
New consumers should read `agent_address` for the agent's direct address and
`delivery_recipient` / `mailbox` for the mailbox that actually received the
talk delivery. Notifications include `subject`, `excerpt`, and
`attachment_count` so a pushed agent can decide whether to open
`wiki.agent.inbox` without first loading the full mailbox.

`wiki.notify.ack` only records wakeup handling. It is not a mail-state change.
If work is still open, `wiki.agent.inbox` and `wiki.page.status` can continue
to report actionable mail after the notification disappears from
`wiki.notify.poll`. Use `wiki.mail.mark`, `wiki.mail.claim`, or
`wiki.mail.mark_all` to change the underlying work state.

`wiki.notify.ack` records wakeup handling:

```json
{
  "action": "wiki.notify.ack",
  "agent_id": "agent_codex_019e3f72",
  "notification_id": "notif_...",
  "state": "delivered"
}
```

Target error codes should be stable:

- `unknown_page`
- `unknown_agent`
- `stale_agent`
- `invalid_agent_address`
- `unknown_list`
- `unexpected_arguments`
- `mail_already_claimed`
- `invalid_address`
- `invalid_kind`
- `invalid_state`
- `attachment_copy_failed`
- `subscription_conflict`
- `notification_delivery_failed`
- `idempotency_conflict`
- `source_hash_mismatch`
- `body_patch_not_found`
- `body_patch_ambiguous`
- `body_edit_refused`
- `index_stale`
- `needs_manual_edit`

`wiki.curator.apply` should not be a hidden autonomous editor. It should be the
mechanical bridge from a concrete accepted decision to a bounded source patch:
copy to sandbox, apply only the owned section, reject operator-touched spans,
record diff/evidence, promote only when checks pass, and then publish. If the
decision is not structured enough for safe application, it should return
`needs_manual_edit` rather than guessing.

That keeps the wiki publishing core boring while still giving the memory system
a real governance surface. Talk remains durable source, not hidden chat state.

This target is deliberately not just a wiki detail. It is the pattern the rest
of the app can reuse: accept an intent, resolve durable file ownership, create
or delete user-owned structure through explicit operations, run bounded work,
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
2. creates configured page source using the current lifecycle/internal adapter
3. scrubs generated `context-engine/runs` receipts from the defaults work tree
4. renders `RuntimeDefaults/1Context/user-wiki/site`
5. writes `runtime-defaults-manifest.json`
6. bundles `RuntimeDefaults` and `WikiEngine`
7. validates the app and DMG

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
- page lifecycle no-overwrite behavior
- `render-site.mjs` CLI arguments
- render result JSON shape
- route manifest and content index names
- RuntimeDefaults manifest fields
- Swift installer ledger shape
- conflict proposal location
- `onecontext-wiki` JSON CLI as the shared V0 consumer bridge
- `wiki.refresh` only as a transitional Swift host trigger

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
