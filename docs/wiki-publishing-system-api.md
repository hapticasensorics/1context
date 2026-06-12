# 1Context Wiki Publishing System API

- Status: canonical current wiki API
- Last updated: 2026-05-21

This is the consumer contract for the 1Context wiki publishing system. It is
written for agents, the Swift app, Local Web, and memory-side code that need to
create, edit, inspect, validate, and publish the user wiki.

The implemented V0 surface is intentionally small:

```text
page lifecycle + page assets + page talk + validate + publish
agent mail lives in its own protocol namespace
```

The previous scattered mail prototype has been replaced with a clean V0 agent
mail surface in the Rust core. Page lifecycle remains documented here; agent
directory, delivery, and notification semantics belong in
[Agent Mail Protocol](agent-mail-protocol.md).

For generic agent consumption, this API is exported through the consolidated
[Agent Tool Gateway](agent-tool-gateway.md) as `toolset-wiki`. Mail is exported
separately as `toolset-mail`; do not mix mail controls into the wiki toolset.

## Ownership Rule

```text
User data is live truth.
RuntimeDefaults are seed and backfill material.
Rust owns portable wiki semantics.
Swift hosts the macOS app, daemon bridge, and Local Web.
JavaScript renders static HTML behind the publisher.
Python is a thin memory-side client, not a second wiki engine.
```

No component may silently overwrite user-owned wiki source, talk files,
templates, prompts, assets, `_curator.md`, or `wiki.toml`.

## Data Roots

Repo source:

```text
runtime/1Context/              public-safe shipped defaults source
runtime-test/                  ignored local/private scenario fixtures
crates/onecontext-wiki-core/   portable Rust wiki semantics
crates/onecontext-wiki-daemon/ CLI/JSON adapter for the Rust core
crates/onecontext-context-engine/
                               native wiki-company orchestration
wiki-engine/                   deterministic static renderer
macos/Sources/                 app host, daemon bridge, Local Web, setup
```

Installed user data:

```text
~/1Context/
  user-wiki/
    wiki.toml
    templates/
    assets/
    source/
    .1context/page-ledger.jsonl
    site/
  context-engine/

~/Library/Application Support/1Context/
  setup/
  wiki-site/current/
  wiki-site/previous/
  wiki-site/next/
  local-web/
```

The reader-facing site is served from Application Support, but it is derived
from `~/1Context/user-wiki`. The source tree is the editable truth.

## Implemented Operations

The Rust CLI operation names are the canonical low-level names. The installed
Swift CLI and daemon bridge expose the app-facing subset shown below.

These operations are the public contents of `toolset-wiki` when the agent is
allowed to edit and publish. A host may filter individual tools for read-only
or supervisor sessions, but the toolset name stays `toolset-wiki`.

| Operation | Rust CLI | Swift CLI/RPC | Purpose |
| --- | --- | --- | --- |
| `wiki.ensure` | `ensure` | not exposed | Create missing runtime directories and safe defaults. |
| `wiki.status` | `status` | not exposed | Compact whole-system status from the Rust core. |
| `wiki.list` | `list` | `list` | List configured, source-backed, generated, missing, and tombstoned pages. |
| `wiki.validate` | `validate` | `validate` | Validate wiki structure without publishing. |
| `wiki.page.status` | `page-status` | `page-status` | Inspect one page's state, flags, hashes, and next action. |
| `wiki.page.open` | `page-open` | `page-open` | Return editable handles, source body, talk handles, and hashes. |
| `wiki.page.create` | `page-create` | `page-create` | Add a configured page, backing source, talk folder, and ledger event. |
| `wiki.page.create_all` | `page-create-all` | not exposed | Backfill all configured missing source pages from templates. |
| `wiki.page.write_body` | `page-write-body` | `page-write-body` | Replace a page body with an optional source hash precondition. |
| `wiki.page.patch_body` | `page-patch-body` | `page-patch-body` | Replace one body fragment with an optional source hash precondition. |
| `wiki.asset.add` | `asset-add` | `asset-add` | Copy a file into a page-local asset folder and return markdown. |
| `wiki.asset.list` | `asset-list` | `asset-list` | List page-local assets. |
| `wiki.reference.list` | `reference-list` | `reference-list` | List published citeable assets, links, and code blocks, optionally scoped to one page. |
| `wiki.page.delete` | `page-delete` | `page-delete` | Tombstone a source page and reserve the route. |
| `wiki.page.restore` | `page-restore` | `page-restore` | Restore a tombstoned source page. |
| `wiki.talk.append` | `talk-append` | `talk-append` | Append a durable page-talk entry. |
| `wiki.publish.status` | `publish-status` | `publish-status` | Explain whether publishing is needed and why. |
| `wiki.publish` | `publish` | `publish` | Validate, render, promote, mirror, and return publish evidence. |

Removed from current V0: the former agent identity, inbox, claim,
subscription, notification, page observing, page-role, and list-management
prototype operations. Do not build new consumers on those namespaces until the
mail protocol is deliberately exposed through the Rust core API.

## Agent Loop

The intended low-overhead loop for a wiki-editing agent is:

1. Call `wiki.list` or `wiki.page.status`.
2. Call `wiki.page.open` before editing a page.
3. Use the returned `hashes.source_sha256` as the write precondition.
4. Call `wiki.page.patch_body` for small edits or `wiki.page.write_body` for a
   prepared replacement body.
5. Use `wiki.asset.add` when the page needs an embedded file or image.
6. Use `wiki.talk.append` for coordination notes, review comments, or evidence
   that should not change reader content.
7. Call `wiki.publish.status`.
8. Call `wiki.publish` when source, assets, tombstones, or `wiki.toml` changed.
9. Use `wiki.reference.list` after publish when an agent needs citeable image,
   file, hyperlink, or code-block handles.
10. Recheck `wiki.page.status`, `wiki.list`, or the browser-visible route.

Agents should not path-walk the wiki to infer placement, template state, or
publish eligibility. Those answers belong in receipts.

## Page Metadata

`wiki.list` and `wiki.page.status` should return enough metadata for a consumer
to decide what to do without opening files manually:

- `id`, `title`, `route`, `slug`, `type`, `collection`, and navigation fields
- page kind: source-backed, generated site page, missing source, or tombstoned
- origin: runtime default, created from template, custom, or generated
- content state: template baseline, edited, missing, stale, or rendered
- flags: configured, enabled, source-backed, rendered, stale, tombstoned,
  talk-ready, template-derived, user-edited
- hashes: source, rendered, template baseline, and publish fingerprint when
  available
- handles: source, talk, curator, conventions, published route, and assets
- validation summary and `next_action`

Generated site pages are reader routes, not editable source pages. Page-editing
operations must reject them with a typed failure and repair hints.

## Page Creation

`wiki.page.create` owns the full placement problem. Callers may configure:

- page id, title, slug, route, summary, and type
- navigation section/order
- family group and family id
- source template and talk conventions template
- generated/custom/default classification

The core writes:

```text
user-wiki/wiki.toml
user-wiki/source/families/<group>/<family>/source/<slug>.md
user-wiki/source/families/<group>/<family>/talk/<slug>.talk/_meta.yaml
user-wiki/source/families/<group>/<family>/talk/<slug>.talk/_conventions.md
user-wiki/source/families/<group>/<family>/talk/<slug>.talk/_curator.md
user-wiki/.1context/page-ledger.jsonl
```

If a destination file already exists, page creation preserves it and reports
`skipped_existing`. If the page or route is tombstoned, creation refuses the
operation until the caller restores the page or chooses a different route.

## Template Fallback

Templates are fallback material, not live truth. A configured source page may be
created from `user-wiki/templates`, but once a user-owned file exists, that file
wins.

RuntimeDefaults follow the same rule:

1. On first run or upgrade, bundled defaults copy only missing files.
2. Existing user files are preserved.
3. Changed packaged defaults create proposals or conflict evidence instead of
   overwrites.
4. Publishing always renders from user data, not directly from bundled
   defaults.

## Body Edits

Use `wiki.page.write_body` when replacing the full markdown body. Use
`wiki.page.patch_body` when replacing one exact fragment. Both support inline
input and file-backed input through the CLI/Python adapter.

Receipts should include:

- old and new source hashes
- page status after the write
- whether publish is required
- validation hints or stale-write errors

A stale `expected_source_sha256` must fail rather than overwrite a newer edit.

## Assets And Images

`wiki.asset.add` copies files into the page-local source asset folder:

```text
user-wiki/source/families/<group>/<family>/source/<slug>.assets/
```

The receipt returns sanitized filename, hash, media metadata, source path,
route-relative output path, a stable `user-wiki://` citation URI, and a
markdown snippet such as:

```markdown
![Caption](./topics.assets/image.png)
```

The caller then inserts that markdown with `wiki.page.patch_body` or
`wiki.page.write_body`. Asset changes affect publish fingerprints.

Fenced Markdown code blocks are rendered inline and indexed as citeable code
resources during publish. Source-code files that need to remain downloadable or
hash-addressable should be added through `wiki.asset.add` with a purpose such as
`source_file` and then linked from the page body.

Wikipedia-style Markdown footnotes render as superscript citation numbers and a
bottom References section:

```markdown
This claim has a source.[^source]

[^source]: Source details or a Markdown link.
```

Footnote citation records are indexed during publish and exposed through
`wiki.reference.list` alongside assets, links, and code blocks.

Successful publishes write:

```text
user-wiki/site/.1context/reference-index.json
```

The reference index records page-local assets, Markdown links, image links,
footnote citations, and fenced code blocks. Agents should cite these records by
their `citation_uri` instead of citing raw local paths.

`wiki.reference.list` returns the same published reference records through the
core API. Without an argument it lists the whole published site; with a page id
or route it filters assets, links, citations, and code blocks to that page. If
the reference index is missing, it returns `status="missing"` and `next_action =
"publish"` rather than asking callers to path-walk.

## Talk

`wiki.talk.append` writes durable page talk. It supports:

- `page`
- `kind`
- `subject`
- `from`
- optional `to` and `cc` address labels
- optional `operation_id` for idempotent retries
- `delivery_mode`: `labels_only` by default, `mail` for explicit delivery
- body text or body file
- `reply_to` or `thread_id`
- attachments with captions and alt text

Current V0 talk is page-local discussion state by default. `to` and `cc` are
stored metadata labels and do not create inbox rows, notifications, claims, or
mail deliveries unless the caller explicitly sets `delivery_mode=mail`.

When `delivery_mode=mail`, `wiki.talk.append` writes the talk source first,
then asks the Rust mail transport to accept and deliver the message. Delivery
failure preserves the talk source and returns repair hints. A stable
`operation_id` lets the caller retry after a crash or partial delivery without
duplicating the talk file or delivery rows.

Talk append should return `render_required=false` unless another source, asset,
config, or lifecycle change requires publishing.

Talk attachments live under the page talk folder and must never expose local
absolute paths in rendered output.

## Publishing

`wiki.publish.status` answers whether a publish is needed. Typical reasons:

- source page body changed
- asset changed
- page created, deleted, or restored
- `wiki.toml` changed
- template fallback created missing configured pages
- existing site is missing or invalid

`wiki.publish` runs the proof step:

```text
validate inventory
  -> create any configured missing source pages safely
  -> compute publish fingerprint
  -> render complete static site into staging
  -> validate route manifest and markdown twins
  -> promote user-wiki/site
  -> mirror last-good site to Application Support
  -> return evidence
```

If rendering or validation fails, the last-good Application Support mirror must
remain served. The failure receipt should include operation, error code,
message, repair hints, and any renderer result available.

There is no page-scoped render API in V0. Consumers edit one page, then request
whole-site publish. The publisher may skip rendering when inputs are unchanged
and the existing site validates.

## Local Web

Local Web serves the published mirror from Application Support. Browser code
should use relative paths and app-owned `/api/wiki/*` routes. It must not read
directly from `~/1Context/user-wiki` or mutate `wiki-site/current`.

Current dynamic wiki routes are intentionally small: health/search/bookmarks
and local UI state. Inbox routes must be specified by the mail protocol before
they are exposed through Local Web.

## Failure Shape

Wiki API failures should be structured:

```json
{
  "schema_version": 1,
  "status": "error",
  "operation": "wiki.page.patch_body",
  "error": {
    "code": "stale_source",
    "message": "source changed since the caller opened the page"
  },
  "repair_hints": [
    "Run wiki.page.open again, read edit.expected_source_sha256, and retry."
  ]
}
```

Consumers should treat `repair_hints` as the next-step contract and should
preserve the full payload in logs or ledgers.

## Context Engine Caller

`onecontext-context-engine` is the native caller for wiki-company work. It
should use the Rust wiki core/daemon APIs and must not reimplement page
placement, fallback, tombstone, talk, asset, or publish rules.

Current operation surface:

```text
wiki_ensure
wiki_status
wiki_list
wiki_validate
wiki_page_status
wiki_page_open
wiki_page_create
wiki_page_create_all
wiki_page_write_body
wiki_page_patch_body
wiki_asset_add
wiki_asset_list
wiki_page_delete
wiki_page_restore
wiki_publish_status
wiki_publish
wiki_talk_append
```

## Freeze Boundary

This API is close to the wiki freeze boundary. Before adding new wiki
operations, ask whether the behavior can be expressed as:

- page create/open/status/write/patch/delete/restore
- asset add/list
- talk append
- validate
- publish status/publish

The mail system should stay a clean protocol with its own storage contract, not
as scattered wiki helper commands.

## Agent Mail Pointer

Agent mail is intentionally outside the page lifecycle API. Its V0 contract
belongs in [Agent Mail Protocol](agent-mail-protocol.md), with talk append as
the bridge from page-local discussion into explicit mail delivery.

Agent-facing mail tools are grouped by the
[Agent Tool Gateway](agent-tool-gateway.md) as `toolset-mail`. The wiki API
must continue to own reader content and publishing, while mail owns delivery,
claim, mark, notification, and future send/reply semantics.

Publishing rules remain unchanged when mail arrives:

- existing `wiki.talk.append` calls keep labels-only `to`/`cc` behavior unless
  explicit `delivery_mode=mail` is requested
- mail read, claim, mark, snooze, archive, inbox rebuild, and notification state
  do not require `wiki.publish`
- no mail operations should be added under `wiki.page.*`; page operations stay
  source/page lifecycle only
