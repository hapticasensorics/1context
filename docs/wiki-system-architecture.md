# 1Context Wiki System Architecture

- Status: current architecture plus near-term freeze target
- Last updated: 2026-05-24

This document explains the internal shape behind the
[Wiki Publishing System API](wiki-publishing-system-api.md).

The architecture target is deliberately simple:

```text
portable Rust core
  -> Swift/macOS host
  -> bundled JavaScript renderer
  -> Application Support last-good site
  -> Local Web reader surface
```

The removed scattered mail prototype is not part of this architecture. Agent
mail now lives as a separate Rust-backed transport kernel in
[Agent Mail Protocol](agent-mail-protocol.md), with its own storage and
notification contract beside page/talk/publish.

Agent-facing access is defined separately in
[Agent Tool Gateway](agent-tool-gateway.md): one consolidated backend may serve
all tools, but ordinary agents see only `toolset-mail`, `toolset-wiki`, or both.

## North Star

The wiki should feel like a tiny local database whose records are editable
files.

Consumers should think in:

```text
page id
route
page type
collection
source handle
talk handle
asset handle
published handle
publish receipt
```

They should not need to memorize:

```text
source/families/<group>/<family>/source/<slug>.md
page lifecycle setup receipts
RuntimeDefaults installer details
route-manifest internals
Application Support mirror layout
renderer staging directories
```

Those are core-owned implementation details.

## Language Ownership

```text
Rust   = portable wiki semantics and filesystem safety
Swift  = Apple app, setup, daemon bridge, menu UI, permissions, Local Web host
JS     = deterministic static renderer helper
Python = memory-side client and future memory proposals, not wiki semantics
```

Rust owns:

- wiki inventory compilation
- page lifecycle
- page body writes and stale-write protection
- page-local assets
- page talk append
- tombstones and restore
- validation
- publish preflight
- renderer invocation
- last-good promotion and evidence
- RuntimeDefaults install/backfill rules where portable

Swift owns:

- app UI and menu actions
- setup and permissions
- launchd/service lifecycle
- Apple-specific path discovery
- Caddy/Local Web supervision
- bridging app RPC calls to the Rust core
- app settings such as automatic publish cadence

JavaScript owns static HTML rendering only. It should not decide user-data
truth, page lifecycle, or publish eligibility.

Python owns memory-side orchestration and should call the Rust core through the
thin adapter in `memory-core/src/onectx/wiki_interface`.

## Component Map

Repo:

```text
runtime/1Context/
  user-wiki/                 blessed public RuntimeDefaults source
  context-engine/

runtime-test/                private ignored fixture lab

crates/
  onecontext-wiki-core/      portable wiki semantics
  onecontext-wiki-daemon/    CLI/JSON adapter

wiki-engine/                 renderer package

macos/Sources/
  OneContextPlatform/        paths and app identity
  OneContextWikiRuntime/     Swift bridge to Rust wiki commands
  OneContextLocalWeb/        local static/API serving
  OneContextDaemon/          app daemon host
  OneContextCLI/             installed `1context wiki` CLI

memory-core/src/onectx/wiki_interface/
  core_client.py             Python adapter over `onecontext-wiki`
```

Installed:

```text
1Context.app/
  Contents/MacOS/onecontext-wiki
  Contents/Resources/WikiEngine/
  Contents/Resources/RuntimeDefaults/1Context/

~/1Context/
  user-wiki/
  context-engine/

~/Library/Application Support/1Context/
  wiki-site/current/
  wiki-site/previous/
  wiki-site/next/
  setup/
  local-web/
```

## Inventory

`WikiInventory` is the resolved view of the wiki. Every public wiki operation
should read it or update inputs that are then recompiled into it.

Inputs:

```text
~/1Context/user-wiki/wiki.toml
~/1Context/user-wiki/templates/
~/1Context/user-wiki/assets/
~/1Context/user-wiki/source/
~/1Context/user-wiki/source/**/source/*.assets/
~/1Context/user-wiki/source/**/talk/**
~/1Context/user-wiki/.1context/page-ledger.jsonl
~/1Context/user-wiki/site/.1context/
~/Library/Application Support/1Context/wiki-site/current/.1context/
~/Library/Application Support/1Context/setup/
```

Outputs:

```text
pages[]
collections[]
generated_pages[]
aliases[]
tombstones[]
validation_summary
publish_status
route_manifest_summary
site_activity_feed
```

`wiki.list` and `wiki.page.status` should answer from this resolved view so
agents do not path-walk.

Page rows should expose:

- stable identity: id, title, route, slug, type, collection
- origin: runtime default, custom, template-derived, generated, or unknown
- state: source only, rendered, stale, needs publish, missing, or tombstoned
- flags: configured, enabled, source-backed, rendered, stale, tombstoned,
  talk-ready, template-derived, user-edited
- handles: source, talk, curator, conventions, assets, published route
- hashes: source, rendered, template, and publish fingerprint when available
- validation summary and `next_action`

## Page Ledger

The page ledger records facts that should not be re-inferred later:

```text
~/1Context/user-wiki/.1context/page-ledger.jsonl
```

Typical events:

```json
{"event":"page.created","page":"topics","origin":"created_from_template"}
{"event":"template.baseline","page":"topics","source_sha256":"..."}
{"event":"page.body_written","page":"topics","source_sha256":"..."}
{"event":"asset.added","page":"topics","asset_sha256":"..."}
{"event":"talk.appended","page":"topics","message_id":"..."}
{"event":"page.tombstoned","page":"old-page"}
{"event":"page.restored","page":"old-page"}
{"event":"page.published","page":"topics","publish_fingerprint":"..."}
```

The ledger is evidence, not an alternate source of truth. The current source
file still owns current page content.

## RuntimeDefaults

RuntimeDefaults are packaged seed/backfill material. They are never live truth.

Install/update behavior:

1. Copy missing safe defaults into `~/1Context`.
2. Preserve any existing user file.
3. Write proposal/conflict evidence for packaged defaults that differ from
   existing user files.
4. Render from user data.
5. Record what happened in setup/defaults evidence.

The missing hardening still worth adding is a defaults freshness manifest with:

- app version
- git commit
- RuntimeDefaults source hash
- renderer version/hash
- materializer/core version/hash
- generated-at timestamp
- render result summary

## Page Lifecycle

`wiki.page.create` owns page placement. It writes the `wiki.toml` entry,
backing source, talk folder, conventions, curator prompt, and ledger event in
one operation.

`wiki.page.write_body` and `wiki.page.patch_body` own stale-write protection.

`wiki.page.delete` tombstones. It does not raw-delete user files.

`wiki.page.restore` clears the tombstone and makes the page publishable again.

Generated site pages are not editable source pages. They are rendered routes.

## Assets

Page-local source assets live beside the page source:

```text
user-wiki/source/families/<group>/<family>/source/<slug>.assets/
```

The renderer copies safe assets into route-visible output. Receipts should
return both source handles and markdown snippets so agents can insert assets
without guessing paths.

## Talk

Talk is page-local durable collaboration context:

```text
user-wiki/source/families/<group>/<family>/talk/<slug>.talk/
```

Current V0 talk append writes files and ledger evidence. It creates mailboxes,
claims, and notifications only when the caller explicitly requests
`delivery_mode = "mail"`.

This split is intentional. Talk remains the durable page-local source record;
mail is the operational queue layered beside it, not hidden inside page source
or static rendering.

## Publishing

Publishing is the proof step:

```text
compile inventory
  -> validate
  -> safely create configured missing source pages
  -> compute publish fingerprint
  -> render full static site into staging
  -> validate route manifest and markdown twins
  -> promote user-wiki/site
  -> mirror last-good site to Application Support
  -> return evidence
```

If publish fails, the last-good Application Support mirror remains served.

There is no page-scoped render API in V0. Whole-site publish is simpler and
safer while the site is small; fingerprinting and skip logic keep unchanged
publishes cheap.

## Local Web

Local Web serves the Application Support mirror and app-owned local API routes.
It must not become a second wiki engine. It should not write directly into
`wiki-site/current`.

The app may expose small local state APIs for browser UI. Source mutation goes
through the wiki API.

## Automatic Publish Cadence

The app can choose how often source changes automatically trigger publish:

```text
no_limit
1_minute
30_minute
```

Manual `wiki.publish` bypasses this cadence because the caller explicitly asks
for proof now. `wiki.publish.status` should explain whether automatic publish
is delayed.

## Mail Boundary

The mail system must not reappear as ad hoc wiki helpers. It is a separate
transport kernel with:

- agent directory and liveness model
- durable message schema
- delivery ledgers
- mailbox/index strategy
- notification/wakeup contract
- backpressure and claim rules
- migration story from page talk

It is exposed through `toolset-mail` in the
[Agent Tool Gateway](agent-tool-gateway.md), while pages, assets, talk,
validation, and publishing remain in `toolset-wiki`.
