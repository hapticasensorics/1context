# 1Context User Data Spec

This document defines the production filesystem contract for user-owned
1Context data.

It answers:

- what folders exist
- who owns each folder
- what files are canonical
- what can be copied, backed up, exported, deleted, or rebuilt
- which persisted schemas must stay stable

It does not define how agents decide what to write, how renders are scheduled,
or how accepted changes are published. Those behaviors live in
[Wiki Memory Publication Contract](wiki-memory-publication-contract.md).

## Core Rule

```text
The readable wiki is the artifact.
The context engine is the workshop.
Application Support is the machinery.
```

Files are truth. JSONL is history. Derived indexes are rebuildable.

## Top-Level Ownership

```text
~/1Context/
  user-wiki/
    readable wiki source, talk, templates, assets, and static export

  context-engine/
    memory-system workspace: prompts, agents, jobs, proposals, decisions,
    runs, artifacts, observations, ledgers, and index manifests

~/Library/Application Support/1Context/
  app machinery: setup state, staging, local web mirrors, sockets, and
  derived indexes

~/Library/Logs/1Context/
  app and process logs for diagnostics

~/Library/Caches/1Context/
  disposable caches
```

`~/1Context` is durable user data. Normal uninstall preserves it. Delete-data
flows may remove it only after explicit user intent.

Application Support is private app machinery. It may contain mirrors, staging
directories, and derived indexes, but it is not the canonical editable wiki or
memory archive.

## Copy And Backup

```text
Copy ~/1Context/user-wiki/site/
  when you want the current static website export.

Copy ~/1Context/user-wiki/
  when you want an editable wiki backup.

Copy ~/1Context/context-engine/
  when you want agent history, proposals, decisions, runs, evidence, prompts,
  and index manifests.

Do not copy Application Support as the canonical record.
  It is app machinery and may contain disposable mirrors or indexes.
```

## User Wiki

`user-wiki` is the readable, exportable wiki. A user should be able to inspect
or copy this folder without knowing about daemon internals.

```text
~/1Context/user-wiki/
  README.md
  wiki.toml

  templates/
    pages/
    talk/
    site/

  assets/

  source/
    families/
      <family-group>/
        group.toml
        <family-id>/
          family.toml
          source/
            <page-slug>.md
            <page-slug>.tombstone.toml
          talk/
            <page-slug>.talk/
              _meta.yaml
              _conventions.md
              _curator.md
              <timestamp>.proposal.<short-title>.md
              <timestamp>.reply.<short-title>.md
              <timestamp>.close.<resolution>-<short-title>.md
          templates/
            page.template.md
            talk/
              _conventions.template.md
              _curator.template.md
              entry.template.md

  site/
    index.html
    <route>/
      index.html
    assets/
    markdown/
    .1context/
      current-render.json
      render-events.jsonl
      site-manifest.json
      content-index.json
```

`source/` is canonical editable truth. `site/` is the last successful static
render. Source may be newer than site. Site must never be half-rendered.

## Wiki Registry

`wiki.toml` is the user-owned page registry and site map. It names routes,
navigation, source-backed pages, generated pages, aliases, templates, access,
and materialization policy.

Use `[[pages]]` for pages that materialize editable source under
`source/families/**`.

Use `[[site_pages]]` for generated pages, aliases, and diagnostics that can
appear in the site map without editable source.

Example:

```toml
schema_version = 1
title = "1Context"
source_dir = "source"
site_dir = "site"
templates_dir = "templates"
assets_dir = "assets"

[site]
home_route = "/"
missing_route_behavior = "diagnose"
navigation = ["your-context", "projects", "topics"]
primary_navigation = ["for-you", "your-context", "projects", "topics"]
utility_navigation = ["this-week", "open-questions"]

[defaults]
operator_name = "Operator"
access_tier = "private"
template_pack = "e08"

[materialization]
enabled = true
create_talk = true
overwrite_user_files = false

[[site_pages]]
id = "home"
enabled = true
kind = "generated"
route = "/"
template = "site/e08/index.md"

[[site_pages]]
id = "for-you"
enabled = false
kind = "alias"
route = "/for-you"
family_group = "for-you"
target_policy = "latest-accepted"

[[pages]]
id = "your-context"
enabled = true
title = "Your Context"
slug = "your-context"
route = "/your-context"
family_group = "context"
family_id = "your-context"
type = "context-page"
template = "pages/e08/your-context.md"
talk_conventions_template = "talk/conventions/your-context.md"
talk_curator_template = "talk/curators/your-context.md"
```

The behavior of routes, generated pages, aliases, missing pages, and render
fallbacks is defined by the publication contract.

## Source Families

`source/families/` is source ownership, not menu structure. Navigation order
belongs in `wiki.toml`, not folder names.

Use semantic groups:

| Group | Purpose |
| --- | --- |
| `for-you` | rolling orientation and time-window summaries |
| `context` | durable operator and collaboration context |
| `work` | projects, repos, goals, releases, and work state |
| `reference` | concepts, topics, people, organizations, and tools |
| `system` | wiki operations, diagnostics, and generated worklists |

Do not use menu-order prefixes such as `10-for-you/20-your-context` in durable
source paths.

A family is the smallest editable ownership package. Most families begin with
one page, but a family may later contain related companion pages, templates, and
local policy.

```text
source/families/context/your-context/
source/families/work/projects/
source/families/reference/topics/
source/families/work/guardian-app/
source/families/reference/lancedb/
```

`family.toml` binds the family to logical page ids and route ids. It is not a
second site map.

## Talk Files

Talk belongs beside the page it discusses. It is part of the user's wiki, not
hidden engine state.

Talk folders are durable page history:

```text
source/families/<group>/<family>/talk/<page-slug>.talk/
  _meta.yaml
  _conventions.md
  _curator.md
  2026-05-14T18-10Z.proposal.short-title.md
  2026-05-14T18-22Z.reply.short-title.md
  2026-05-14T18-40Z.close.accepted-short-title.md
  archive/
```

`_meta.yaml` stores page identity, route identity, status, access, and schema
version. `_conventions.md` stores page-local discussion rules.
`_curator.md` stores page-local curator instructions.

Talk entries should carry stable logical ids, timestamps, authorship,
provenance, and references to evidence or artifacts. Use logical ids such as
`page://your-context`, `family://context/your-context`, and
`evidence://observation/event_123` instead of raw local paths.

Talk entry state names should be explicit: `proposed`, `accepted`, `rejected`,
`resolved`, `withdrawn`, `superseded`, `blocked`, or `archived`.

## Templates

Templates that shape the readable wiki live in `user-wiki/templates`.

```text
templates/
  pages/
    context-page.md
    project-index.md
    topic-index.md
    e08/
      your-context.md
      projects.md
      topics.md
  talk/
    entry.md
    conventions.md
    conventions/
    curators/
  site/
    home.md
    nav.md
    e08/
      index.md
      this-week.md
      open-questions.md
```

Templates initialize pages and talk. They do not own those files after
materialization. Once a template has created a user file, the user file wins.

Global agent prompts belong in `context-engine/prompts`, not
`user-wiki/templates`. Page-local talk conventions and curator instructions
belong with the page's talk folder because they define how that page should be
discussed.

## Static Site Export

`user-wiki/site/` is the copyable static website export.

It may contain:

- rendered HTML
- markdown twins for inspection and agent reading
- static assets
- route and content manifests
- redacted render metadata under `.1context/`

It must not contain:

- `context-engine` internals
- raw prompts unless explicitly published by a user-owned page
- raw observations
- run transcripts
- private proposal previews
- local absolute paths
- usernames or home-directory fragments in browser-visible JSON

The canonical render ledger lives with the export:

```text
site/.1context/current-render.json
site/.1context/render-events.jsonl
```

The event schema and render lifecycle are defined by the publication contract.

## Context Engine

`context-engine` is the user-owned memory-system workplace. It is inspectable
and backup-worthy, but it is not the clean static wiki export.

```text
~/1Context/context-engine/
  agents/
    roles/
    tools/
    policies/

  jobs/

  prompts/
    shared/
    e08-for-you/

  inbox/
  proposals/
  decisions/
  runs/
  artifacts/
  observations/
  ledgers/
  indexes/
```

Directory meanings:

| Directory | Stores |
| --- | --- |
| `agents/roles` | reusable role definitions |
| `agents/tools` | tool contracts and allowlists |
| `agents/policies` | operator-touched rules, safety policy, edit rules |
| `jobs/` | reusable job definitions |
| `prompts/` | global prompt files and prompt packs |
| `inbox/` | machine-readable work requests |
| `proposals/` | immutable proposed changes and patch series |
| `decisions/` | accepted, rejected, deferred, withdrawn, or superseded outcomes |
| `runs/` | replay/debug records for agent work |
| `artifacts/` | previews, patches, validations, and proof bundles |
| `observations/` | source material and captured inputs |
| `ledgers/` | append-only operational JSONL |
| `indexes/` | user-owned manifests and rebuild state for derived indexes |

The context engine can reference wiki objects with logical ids. Canonical wiki
publication happens only after accepted changes materialize into `user-wiki`.

## Context Engine File Classes

Inbox task files are queued work requests. Claims should be lease-based and
include target, input hash, expected outputs, and allowed paths.

Proposals are immutable suggested changes. New versions create new files rather
than overwriting old versions. Decisions record whether a proposal is accepted,
rejected, deferred, withdrawn, or superseded.

Runs are replay/debug records. Artifacts are outputs, not conversation.
Observations are source material for memory work. Ledgers are append-only JSONL.

Detailed write protocol, proposal promotion, route plans, and render requests
belong to the publication contract.

## Indexes And LanceDB

LanceDB is a derived retrieval index, not canonical memory.

Canonical records remain ordinary files:

```text
user-wiki/source/**/*.md
user-wiki/source/**/*.toml
user-wiki/source/**/*.yaml
user-wiki/site/.1context/*.json*
context-engine/**/*.md
context-engine/**/*.toml
context-engine/**/*.yaml
context-engine/**/*.json
context-engine/**/*.jsonl
```

Heavy derived indexes belong under Application Support:

```text
~/Library/Application Support/1Context/indexes/lancedb/
```

User-owned index manifests and rebuild state live under:

```text
~/1Context/context-engine/indexes/
  index-manifest.toml
  lancedb.state.json
  rebuilds.jsonl
```

The app must be able to delete and rebuild Application Support indexes from
`user-wiki`, `context-engine`, and index manifests. A missing or corrupt index
may degrade retrieval, but it must not erase memory or block Open Wiki.

## Application Support

Application Support is app-owned machinery.

```text
~/Library/Application Support/1Context/
  setup/
  staging/
  wiki-site/
    current/
    previous/
  indexes/
    lancedb/
  local-web/
    caddy/
  sockets/
  run/
  state/
```

Rules:

- `staging/` is disposable.
- `wiki-site/current/` is a mirror of the last-good export for local serving.
- `wiki-site/current/` is not the canonical export.
- `indexes/` are rebuildable.
- `setup/` records default installation and migration state.
- App Support must not become the only copy of user memory.

## Logs And Caches

Logs live under:

```text
~/Library/Logs/1Context/
```

Caches live under:

```text
~/Library/Caches/1Context/
```

Logs and caches are not user memory. They may aid diagnostics, but the product
must not rely on them as canonical state.

## First-Run Defaults And Updates

The public-launch repo tracks reviewed, public-safe default user data under:

```text
runtime/
  1Context/
    user-wiki/
    context-engine/
  Library/
```

Private or experimental review data belongs in ignored `runtime-test/` or
another ignored fixture path. Promote files into `runtime/` only after they are
scrubbed and intended to ship.

First-run defaults are copied only into missing destinations. After a file is
materialized into `~/1Context`, the user owns it.

The shipped defaults live in the app bundle under
`Contents/Resources/RuntimeDefaults/1Context`. The build also writes
`Contents/Resources/RuntimeDefaults/1Context/.1context/runtime-defaults-manifest.json`
with the release version, defaults source hash, pre-rendered site hash, renderer
hash, and sanitized render counts. This manifest is package evidence and setup
ledger input; it is not an editable user wiki page.

Application Support records setup state:

```text
~/Library/Application Support/1Context/setup/runtime-defaults-install.json
~/Library/Application Support/1Context/setup/wiki-page-materialize.toml
```

Update matrix:

| Case | Behavior |
| --- | --- |
| User file unchanged from old default | App may update it to the new default. |
| User file modified | Preserve it and write a proposal or diagnostic. |
| User file missing with no tombstone | Offer restore; do not assume deletion was accidental. |
| User file tombstoned | Respect the tombstone. |
| User file moved or renamed | Record the migration only if identity can be proven. |
| Schema version changes | Write a migration proposal or safe additive patch. |
| Merge conflict | Preserve user file and create a proposal with old, new, and user versions. |

No app update may silently overwrite user-edited source, talk, templates,
prompts, `_curator.md`, or `wiki.toml`.

## Development Runtime Mapping

Development mirrors production folder names:

```text
runtime/1Context/user-wiki/
runtime/1Context/context-engine/
runtime/Library/Application Support/1Context/
runtime/Library/Logs/1Context/
runtime/Library/Caches/1Context/

runtime-test/1Context/user-wiki/
runtime-test/1Context/context-engine/
runtime-test/Library/Application Support/1Context/
runtime-test/Library/Logs/1Context/
runtime-test/Library/Caches/1Context/
```

`runtime/` is the public-safe tracked default tree. `runtime-test/` is ignored
local state and may contain personal data.

Debug builds may use `ONECONTEXT_DEV_RUNTIME_HOME` to point at `runtime-test`.
Release builds must ignore that switch.

The repo-local runtime contract is documented in
[Repo Runtime Layout](../runtime/README.md).

## Required Persisted Schemas

These persisted files need explicit schemas and tests:

- `wiki.toml`
- `wiki.toml` `[[pages]]`
- `wiki.toml` `[[site_pages]]`
- `group.toml`
- `family.toml`
- page frontmatter
- talk `_meta.yaml`
- talk entry frontmatter
- proposal records
- decision records
- run manifests
- artifact manifests
- observation records
- append-only ledgers
- index manifests
- setup state TOML
- `current-render.json`
- `render-events.jsonl`
- `site-manifest.json`
- `content-index.json`

Schema drift between defaults, materializers, renderers, Swift validators, and
tests is a release blocker.

## Minimal V0 Data Contract

The first wiki integration only needs to prove the data shape:

- initialize `user-wiki` and `context-engine`
- materialize configured source-backed pages from templates
- preserve edited user files
- respect tombstones
- render accepted source into `user-wiki/site`
- write `site/.1context/current-render.json`
- append `site/.1context/render-events.jsonl`
- mirror the last-good export into Application Support for local web serving
- prove `runtime/` is public-safe and `runtime-test/` is ignored

The first slice should not require a bundled `memory-core` source checkout,
long-running Python web server, LanceDB, embeddings, broad memory jobs, runtime
`npm install`, runtime `uv run`, or host Python/Node to open the wiki.

Those are implementation constraints for V0, but they exist to protect the data
contract: user-owned files must be enough to understand and rebuild the wiki.
