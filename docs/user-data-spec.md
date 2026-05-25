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
[Wiki Publishing System API](wiki-publishing-system-api.md),
[Wiki System Architecture](wiki-system-architecture.md), and
[Wiki Agent Use Story](wiki-agent-use-story.md).

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
    runs, artifacts, observations, talk-derived mail, notifications, ledgers,
    and index manifests

~/Library/Application Support/1Context/
  app machinery: setup state, staging, local web mirrors, sockets, and
  derived indexes

~/Library/Preferences/com.haptica.1context.plist
  per-device app settings such as automatic wiki publish cadence

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

Preferences are per-device app settings. They may control how aggressively the
daemon publishes source changes, for example `no_limit`, `1_minute`, or
`30_minute`, but they are not user-wiki source and should not be required to
copy or export the wiki.

Current wiki publish cadence setting:

```text
~/Library/Preferences/com.haptica.1context.plist
WikiAutomaticPublishCadence = no_limit | 1_minute | 30_minute
```

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
  .1context/
    page-ledger.jsonl

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
            <page-slug>.assets/
              image.png
              notes.pdf
          talk/
            <page-slug>.talk/
              _meta.yaml
              _conventions.md
              _curator.md
              <timestamp>.proposal.<short-title>.md
              <timestamp>.reply.<short-title>.md
              <timestamp>.close.<resolution>-<short-title>.md
              attachments/
                <message-id>/
                  screenshot.png
                  context.json
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
      route-manifest.json
      content-index.json
```

`source/` is canonical editable truth. `site/` is the last successful static
render. Source may be newer than site. Site must never be half-rendered.

Page-local embedded files live beside the source page under
`source/<page-slug>.assets/`. Use this for images, PDFs, screenshots, CSVs, and
other files that are part of the readable article body. Talk-only supporting
evidence belongs in the talk folder's `attachments/` subtree instead.

## Wiki Registry

`wiki.toml` is the user-owned page registry and site map. It names routes,
navigation, source-backed pages, generated pages, aliases, templates, access,
and page lifecycle policy.

Use `[[pages]]` for pages that create editable source under
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

[site.home_feed]
enabled = true
max_items = 30
sources = ["page_ledger", "render_events", "decisions", "link_diagnostics"]
include_talk = "decisions_only"

[defaults]
operator_name = "Operator"
access_tier = "private"
template_pack = "e08"

[page_lifecycle]
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
source/families/reference/search-indexes/
```

`family.toml` binds the family to logical page ids and route ids. It is not a
second site map.

## Page Assets And Embedded Files

Global theme or shared wiki assets may live under `user-wiki/assets/`. Files that
belong to one article should live beside that article:

```text
source/families/<group>/<family>/source/<page-slug>.assets/
  hero.png
  diagram.svg
  evidence.pdf
```

The intended authoring API is:

```text
wiki.asset.add(page, file, purpose, caption, alt_text)
wiki.asset.list(page)
wiki.page.patch_body(page, find, replace_with_markdown)
```

The asset add operation copies the file into the page-local asset folder, applies
safe filename rules, records hash/media metadata, appends a page-ledger event,
and returns the markdown or
download link the agent should insert. The renderer publishes page-local assets
as route-sibling assets, for example `/topics.assets/hero.png`, and records
them in `.1context/reference-index.json` with hashes, media types,
published hrefs, source markdown references, and stable `user-wiki://`
citation URIs. Agents should not guess these paths from filesystem layout.

Fenced Markdown code blocks are inline page resources too. They remain normal
Markdown in source, render with deterministic HTML anchors, and appear in the
reference index with language, source line, content hash, and citation URI.
Source code files that should be downloaded or cited as files belong in the
same page-local asset folder and should be added through `wiki.asset.add`.

Wikipedia-style Markdown footnotes are also supported:

```markdown
This claim has a source.[^source]

[^source]: Source details or a Markdown link.
```

The renderer turns footnote uses into superscript numbers, appends a References
section at the bottom of the page, and publishes citation records in
`.1context/reference-index.json` with stable `user-wiki://` citation URIs.

Rules:

- Asset filenames are sanitized; path separators and `..` are invalid.
- Browser-visible output must not expose local absolute paths.
- Images require useful alt text unless explicitly marked decorative.
- Downloadable files should have a caption or label.
- Source code files may be attached as page assets; fenced snippets may stay
  inline in the Markdown body.
- Page assets are reader content and can affect publish freshness.
- Talk attachments are workflow/evidence content and do not make page source
  dirty unless a source page links to them intentionally.

## Talk Files, Mailboxes, And Notifications

Talk belongs beside the page it discusses. It is part of the user's wiki, not
hidden engine state. It is also the durable source for the inbox system.

Talk folders are durable page-local mailing-list history:

```text
source/families/<group>/<family>/talk/<page-slug>.talk/
  _meta.yaml
  _conventions.md
  _curator.md
  2026-05-14T18-10Z.proposal.short-title.md
  2026-05-14T18-22Z.reply.short-title.md
  2026-05-14T18-40Z.close.accepted-short-title.md
  attachments/
    <message-id>/
      screenshot.png
      context.json
  archive/
```

`_meta.yaml` stores page identity, route identity, status, access, and schema
version. `_conventions.md` stores page-local discussion rules.
`_curator.md` stores page-local curator instructions.

Talk entries should carry stable logical ids, timestamps, authorship,
provenance, and references to evidence, artifacts, or attachments. Use
logical ids such as `page://your-context`, `family://context/your-context`,
`artifact://run_123/screenshot`, and `evidence://observation/event_123`
instead of raw local paths.

Talk attachments are user wiki files. They may include images, screenshots,
logs, patches, JSON context, PDFs, or other supporting artifacts under the
talk folder's `attachments/` subtree. Rendered talk pages may link to or
thumbnail safe attachments, but must never expose local absolute paths.

Talk entry `kind` names should describe the message type: `conversation`,
`proposal`, `question`, `concern`, `reply`, `decision`, `deferral`,
`contradiction`, or `redaction`.

Talk entry `state` names should describe lifecycle: `open`, `accepted`,
`rejected`, `resolved`, `withdrawn`, `superseded`, `blocked`, or `archived`.

Current wiki V0 talk is page-local collaboration context. By default, `to` and
`cc` are metadata labels only. Agent mail V0 may explicitly deliver addressed
talk into `context-engine/mail`, but only when a caller requests mail delivery
through the Agent Mail Protocol. Talk entry files and talk attachments remain
the durable wiki discussion facts.

Mail delivery is operational state. It must not make the static wiki require
publication unless page source, page assets, tombstones, templates, or
`wiki.toml` also changed.

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

Templates initialize pages and talk. They do not own those files after page
creation. Once a template has created a user file, the user file wins.

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

The canonical page provenance ledger lives with editable user wiki source:

```text
user-wiki/.1context/page-ledger.jsonl
```

It records append-only page lifecycle facts such as creation, template
baselines, observed edits, tombstones, restores, and publishes. It is user data,
not Application Support state.

## Context Engine

`context-engine` is the user-owned memory-system workplace. It is inspectable
and backup-worthy, but it is not the clean static wiki export.

```text
~/1Context/context-engine/
  agents/
    directory/

  jobs/

  prompts/
    shared/
    e08-for-you/

  mail/
    messages/
    bodies/
    deliveries.jsonl
    mailboxes/
    claims.jsonl
    idempotency.jsonl
    injection-receipts.jsonl
    control-events.jsonl
    dead-letter.jsonl

  notifications/
    outbox.jsonl
    attempts.jsonl
    cursors/

  agents/deferred/
    roles/
    tools/
    policies/
    subscriptions/
  mail/deferred/
    subscriptions.jsonl
    lists.jsonl
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
| `agents/directory` | live and recently-live agent registrations, transport pointers, leases, and retirement events |
| `jobs/` | reusable job definitions |
| `prompts/` | global prompt files and prompt packs |
| `mail/messages` | immutable accepted mail envelopes, partitioned by date |
| `mail/bodies` | immutable accepted markdown bodies, partitioned by date |
| `mail/deliveries.jsonl` | append-only delivery truth for each recipient |
| `mail/mailboxes` | rebuildable recipient inbox indexes keyed by safe address encodings |
| `mail/claims.jsonl` | append-only claim and mark events for individual deliveries |
| `mail/idempotency.jsonl` | stable operation keys and payload fingerprints for safe retries |
| `mail/injection-receipts.jsonl` | host-facing records that an authorized `wiki.mail.open` body was injected or failed to inject into a Codex thread |
| `mail/control-events.jsonl` | hook, app-server, injection, and supervisor decisions that shape agent runtime behavior without becoming message truth |
| `mail/dead-letter.jsonl` | inspectable failed or exhausted delivery attempts |
| `notifications/outbox.jsonl` | durable wakeup hints for eligible agents; contains envelope metadata, not full bodies |
| `notifications/attempts.jsonl` | dispatch attempts, steering outcomes, retry evidence, and failures |
| `notifications/cursors` | optional per-agent notification cursors |
| `agents/deferred` | later role/tool/policy/subscription shapes after V0 delivery is stable |
| `mail/deferred` | later list and subscription indexes after V0 delivery is stable |
| `proposals/` | immutable proposed changes and patch series |
| `decisions/` | accepted, rejected, deferred, withdrawn, or superseded outcomes |
| `runs/` | replay/debug records for agent work |
| `artifacts/` | previews, patches, validations, and proof bundles |
| `observations/` | source material and captured inputs |
| `ledgers/` | append-only operational JSONL |
| `indexes/` | user-owned manifests and rebuild state for derived indexes |

The context engine can reference wiki pages and artifacts with logical ids.
Canonical wiki publication happens only after accepted changes land in
`user-wiki`.

## Context Engine File Classes

The current context engine folder is reserved for memory/runtime records that
are not user-wiki source pages:

Agent mail V0 records are transport state, not wiki page source. Message
envelopes and bodies are immutable once accepted. Delivery and claim state is
append-only. Mailbox files are rebuildable indexes and may be regenerated from
messages, deliveries, and claims. Address keys used in paths must be encoded by
the Rust core; raw addresses are never trusted as path fragments.

Agent directory records map durable agent ids to live transport pointers such
as Codex `thread_id`, requested/granted roles, capabilities, leases, and
retirement state. A thread id is a transport locator, not the durable identity.

Proposals are immutable suggested changes. New versions create new files rather
than overwriting old versions. Decisions record whether a proposal is accepted,
rejected, deferred, withdrawn, or superseded.

Runs are replay/debug records. Artifacts are outputs, not conversation.
Observations are source material for memory work. Ledgers are append-only JSONL.

Detailed write protocol, proposal promotion, route plans, render requests,
lists, and governance storage belong to their own contracts. Agent mail and
notification wakeups are owned by the Agent Mail Protocol; do not add ad hoc
wiki mail state under `context-engine` without first updating that protocol and
this user-data spec.

## Indexes And Search

Indexes are rebuildable acceleration surfaces, not canonical memory.

Canonical records remain ordinary files and append-only ledgers:

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

Every important source file, talk entry, attachment, proposal, decision,
render result, and operational event should be readable from `user-wiki` or
`context-engine` without a hidden database.

Heavy derived indexes belong under Application Support:

```text
~/Library/Application Support/1Context/indexes/
```

User-owned index manifests and rebuild state live under:

```text
~/1Context/context-engine/indexes/
  index-manifest.toml
  rebuilds.jsonl
```

The app must be able to delete and rebuild Application Support indexes from
`user-wiki`, `context-engine`, and index manifests. A missing or corrupt index
may degrade retrieval, routing, inbox speed, or semantic search, but it must
not erase memory or block Open Wiki.

## Application Support

Application Support is app-owned machinery.

```text
~/Library/Application Support/1Context/
  setup/
  staging/
  wiki-site/
    current/
    previous/
    next/
  indexes/
  local-web/
    caddy/
  notifications/
  sockets/
  run/
  state/
```

Rules:

- `staging/` is disposable.
- `wiki-site/current/` is a mirror of the last-good export for local serving.
- `wiki-site/current/` is not the canonical export.
- `indexes/` are rebuildable.
- `notifications/` may hold transient transport state, but durable mail and
  notification evidence lives under `~/1Context/context-engine/`.
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
created or copied into `~/1Context`, the user owns it.

The shipped defaults live in the app bundle under
`Contents/Resources/RuntimeDefaults/1Context`. The build also writes
`Contents/Resources/RuntimeDefaults/1Context/.1context/runtime-defaults-manifest.json`
with the release version, git commit/dirty bit, defaults source hash,
pre-rendered site hash, bundled wiki-engine hash, signed wiki-core helper hash,
renderer hash, manifest-writer hash, and sanitized render counts. This manifest
is package evidence and setup ledger input; it is not an editable user wiki
page.

Application Support records setup state:

```text
~/Library/Application Support/1Context/setup/runtime-defaults-install.json
~/Library/Application Support/1Context/setup/page-lifecycle.jsonl
```

Historical dev fixtures may still contain the older setup receipt name. The
target architecture records page lifecycle evidence through the page ledger and
setup receipts, not through a consumer-facing helper API.

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

Installed dev builds use a separate app identity and do not write into the
official user-data tree:

```text
/Applications/1Context Dev.app
~/1Context-Dev/
~/Library/Application Support/1Context Dev/
~/Library/Logs/1Context Dev/
~/Library/Caches/1Context Dev/
~/Library/Preferences/com.haptica.1context.dev.plist
```

The installed dev identity is for side-by-side machine testing. It is not the
repo `runtime-test/` fixture, and it must not overwrite `~/1Context` or the
official app support folders.

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
- agent directory records
- mail delivery records
- mailbox view records
- notification outbox and attempt records
- proposal records
- decision records
- run manifests
- artifact manifests
- observation records
- append-only ledgers
- index manifests
- setup state TOML
- `page-ledger.jsonl`
- `current-render.json`
- `render-events.jsonl`
- `route-manifest.json`
- `content-index.json`

Schema drift between defaults, page lifecycle helpers, talk/mail helpers,
renderers, core validators, Swift host adapters, and tests is a release
blocker.

## Minimal V0 Data Contract

The first wiki integration only needs to prove the data shape:

- initialize `user-wiki` and `context-engine`
- create configured source-backed pages from templates through page lifecycle
- register agents and route explicitly delivered talk entries to mailboxes
- create notification wakeups through Agent Mail Protocol outbox, attempt,
  poll, ack, and Codex steering semantics
- preserve edited user files
- respect tombstones
- render accepted source into `user-wiki/site`
- write `site/.1context/current-render.json`
- append `site/.1context/render-events.jsonl`
- mirror the last-good export into Application Support for local web serving
- prove `runtime/` is public-safe and `runtime-test/` is ignored

The first slice should not require a bundled `memory-core` source checkout,
long-running Python web server, vector indexes, embeddings, broad memory jobs, runtime
`npm install`, runtime `uv run`, or host Python/Node to open the wiki.

Those are implementation constraints for V0, but they exist to protect the data
contract: user-owned files must be enough to understand and rebuild the wiki.
