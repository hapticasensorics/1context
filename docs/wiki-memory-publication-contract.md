# Wiki Memory Publication Contract

- Status: supporting contract
- Last updated: 2026-05-19

Canonical API surface:
[Wiki Publishing System API](wiki-publishing-system-api.md). This document
keeps the memory-publication behavior model and governance rules.

Target architecture:
[Wiki System Architecture](wiki-system-architecture.md). The wiki path should
converge on one inventory compiler, one page ledger, one lifecycle service, one
agent directory, one talk/mail router, one notification dispatcher, one
renderer boundary, and one publisher.

This document defines the behavior contract for turning memory work into
published wiki output.

It answers:

- who may write which classes of wiki memory
- how source, talk, mail, proposals, decisions, notifications, and renders move
  through the system
- how the portable wiki core publishes without becoming the memory engine
- how Swift hosts Apple-specific behavior without owning core wiki semantics
- how the bundled renderer stays deterministic
- how render scheduling avoids CPU storms
- what must be true before the V0 wiki path is shippable

It does not define every folder in `~/1Context`. That storage contract lives in
[User Data Spec](user-data-spec.md).

## Core Rule

```text
Agents author and govern memory work through files plus the wiki API.
The portable wiki core owns lifecycle, mail, validation, and publication.
Swift hosts Apple-specific app behavior.
The bundled renderer renders deterministic static output.
```

The wiki engine displays memory. It is not the memory system.

## Actors

### Memory Authoring

Memory agents own memory work:

- observe activity and source material
- plan routes and ownership
- write talk entries
- register in the agent directory
- read and mark inbox deliveries
- respond to notification wakeups
- write proposals and decisions
- run agents and state machines
- create artifacts, previews, and validation receipts
- promote accepted source, template, prompt, and site-map changes
- request renders through the daemon

Agents may write durable user data under `user-wiki` and `context-engine` only
through governed write classes. It must not write the app-served Application
Support mirror directly.

Canonical publication can see only accepted files in:

```text
~/1Context/user-wiki/wiki.toml
~/1Context/user-wiki/templates/
~/1Context/user-wiki/assets/
~/1Context/user-wiki/source/
```

Proposal overlays may render as previews under `context-engine/artifacts`, but
they are not canonical wiki output.

### Portable Wiki Core Publication

The portable wiki core owns wiki publication and collaboration mechanics:

- initialize first-run defaults
- compile the wiki inventory and call page lifecycle operations for configured
  missing pages when explicitly requested
- maintain the agent directory, mail delivery ledgers, and notification outbox
- validate path confinement and registry shape
- queue and coalesce render requests
- invoke the bundled renderer helper
- validate staged output
- atomically promote `user-wiki/site`
- mirror last-good output to Application Support
- serve local web status and redacted diagnostics

The core publishes or diagnoses. It does not decide semantic memory truth,
rewrite page meaning without a curator decision, or run open-ended memory
agents.

### Swift Host

Swift owns the installed macOS boundary:

- resolve production and development runtime paths
- locate app bundle resources
- present menu/UI/status
- handle Apple permissions and privacy UX
- launch and supervise the Rust-backed wiki API
- supervise Caddy/local-web
- bridge menu commands into the wiki API

Swift should not become the permanent home for wiki lifecycle, inbox routing,
notification semantics, or render governance.

### Bundled JS Rendering

The JS wiki engine is a pure render helper:

- read explicit source, template, asset, and output roots
- render Markdown/frontmatter and talk folders
- generate static assets, route manifests, content indexes, and markdown twins
- write only to the staging directory the publisher provides
- return structured JSON results

The renderer must not mutate source, fetch network resources, discover user
paths by itself, install dependencies at runtime, or become the long-running app
server.

## Component Locations

Target repo shape:

```text
1context-public-launch/
  macos/
    Sources/
      OneContextPlatform/       typed paths and permissions
      OneContextDaemon/         macOS host for wiki API process
      OneContextLocalWeb/       static serving and redacted local APIs
      OneContextWikiRuntime/    transitional Swift render/defaults bridge

  crates/
    onecontext-wiki-core/       target portable Rust wiki core
    onecontext-wiki-daemon/     target JSON-RPC/CLI daemon surface

  wiki-engine/
    package.json
    src/
      renderer/
    theme/
    schemas/
    tools/
      render-site.mjs
    tests/

  memory-core/
    src/onectx/memory/          memory jobs, scheduling, state, and invariants
    src/onectx/wiki_interface/  Python wiki-control boundary for memory
    src/onectx/ports/           capture/import/storage boundaries
    src/onectx/state_machines/  governance over stochastic agent work
```

Current transition rule:

- `wiki-engine/` is the first-class renderer package and bundle source.
- The durable destination is this first-class package or bundled renderer
  helper outside `memory-core`.
- Memory code may call the wiki API to write preview artifacts, append talk,
  read inboxes, and request `wiki.publish`; the portable core owns renderer
  invocation.
- Memory code must not own renderer internals or require renderer imports for
  ordinary memory planning.
- Swift bundles the Rust core and renderer artifact, not a `memory-core` source
  checkout.

Installed app bundle shape:

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

The installed app must not need the development `memory-core` tree in order to
open the wiki. It also must not ship the retired `memory-runtime` fallback
artifact; freshness is proven by the RuntimeDefaults manifest and the packaged
WikiEngine source/artifact identity.

## Logical Ids

Runtime protocols should use logical ids before local paths:

```text
page://<page-id>
family://<family-group>/<family-id>
talk://<page-id>
artifact://<run-id>/<artifact-id>
evidence://<run-id>/<evidence-id>
user-wiki://source/families/...
context-engine://proposals/...
render://<render-id>
```

Browser-visible APIs and static exports must not expose absolute local paths.
Debug logs may contain paths only behind explicit developer/debug surfaces.

## Site Map Behavior

`wiki.toml` is the site map. It is defined by the user-data spec. This contract
defines how it behaves.

Rules:

- `[[pages]]` declare source-backed pages. Page creation creates source under
  `source/families/**` through the lifecycle service.
- `[[site_pages]]` define generated pages, aliases, and diagnostics.
- Navigation order lives in `wiki.toml`, never in folder-name prefixes.
- Missing configured source is a typed inventory problem. It is fixed by
  `wiki.page.create` or diagnosed by validation.
- Tombstoned source is not recreated.
- Unconfigured routes diagnose; they do not fall back to `/your-context` or
  hidden bundled content.
- Aliases are explicit. `/for-you` may point to the latest accepted orientation
  page, but unrelated missing routes must not silently redirect there.

Initial page behavior:

| Page id | Route | Kind | Behavior |
| --- | --- | --- | --- |
| `home` | `/` | generated | Build from site map, render state, and accepted summaries. |
| `for-you` | `/for-you` | alias/generated | Point to latest accepted orientation page when enabled. |
| `your-context` | `/your-context` | source | Durable collaboration and operator context. |
| `projects` | `/projects` | source | Project index and work state. |
| `project:<slug>` | `/projects/<slug>` | source | Memory/operator-created project page. |
| `topics` | `/topics` | source | Topic index. |
| `topic:<slug>` | `/topics/<slug>` | source | Memory/operator-created topic page. |
| `this-week` | `/this-week` | generated | Recent changes and decisions digest. |
| `open-questions` | `/open-questions` | generated | Worklist from talk, proposals, diagnostics, and ledgers. |

## Write Classes

Different writes have different risk.

| Write class | Normal writer | Gate |
| --- | --- | --- |
| Talk entry append | memory agent or operator | schema, page identity, provenance |
| Proposal artifact | memory agent | route plan and expected outputs |
| Decision record | curator, policy, or operator | explicit outcome |
| Source edit | operator or approved promotion | old hash, ownership scope, backup |
| `wiki.toml` edit | operator or approved promotion | route validation and migration proof |
| Template/prompt edit | operator or approved promotion | preserve user edits |
| Static site render | portable publisher | accepted source only |
| App Support mirror | portable publisher | last-good export only |

Safe first-run page creation is the exception: the page lifecycle service may
create missing configured source and talk from user-owned templates when no
tombstone exists and no user file would be overwritten.

## Route Plan Contract

Memory agents need a route plan before mutating wiki memory. A mutating route
row must name:

- target page, family, talk folder, or registry entry
- owned sections or files
- input source hashes
- operator-touched scan result
- expected outputs
- validators
- idempotency key
- freshness and staleness policy
- approval requirement
- preview artifact location
- promotion preconditions

Route plans prevent agents from writing into the wrong page, erasing user edits,
or claiming completion without evidence.

## Change Lifecycle

All durable memory changes follow this lifecycle:

```text
observe
  -> plan route
  -> write talk/proposal/artifact
  -> validate artifact/evidence
  -> decide
  -> apply accepted change to sandbox when needed
  -> promote accepted source/registry/template change
  -> request wiki.publish
  -> publisher snapshots and renders
  -> publisher validates staged site
  -> publisher promotes last-good site
  -> publisher records render event
  -> memory system reacts to result
```

Append-only talk entries can be lower-friction than source edits, but they are
still governed writes. They need page identity, provenance, timestamp, access,
and schema-valid metadata.

Source, template, prompt, and site-map edits are higher-risk. They require a
decision artifact or explicit operator action unless the edit is safe
first-run page creation.

## Proposal And Promotion

Proposal previews live under `context-engine/artifacts`. They are not the public
site and are not canonical wiki memory.

Accepted changes must land in `user-wiki` before canonical render.

A source promotion records:

- proposal id
- decision id
- target logical id
- changed files
- old source hashes
- new source hashes
- operator-touched result
- validators run
- backup or snapshot location
- approval token or policy reason

Promotion failure is a typed state: `failed`, `blocked`, `needs_approval`,
`needs_repair`, `deferred`, or `no_change`. It is never silent success.

## Talk Behavior

Talk belongs beside the page it discusses. It is durable page history, not
hidden agent state. It is also the durable message source for inboxes.

Behavior rules:

- Talk access defaults to the page access.
- A private page must not produce public talk unless an explicit downgrade
  policy allows it.
- Entries are append-oriented.
- Corrections, objections, replies, and closures create new entries.
- Closures are explicit: accepted, rejected, resolved, withdrawn, or superseded.
- Rendered talk pages are part of the wiki export unless disabled by policy.
- Agents register before consuming mail.
- Mailboxes are recipient views over talk entries; agents should not need to
  read a whole talk folder to know what needs their attention.
- Notification pushes are wakeups over mailbox state, not a second copy of the
  message.
- Failed notification delivery never loses mail; the inbox remains pollable.

## Render Request

Agents, UI actions, or setup flows request publication through the daemon:

```json
{
  "method": "wiki.publish",
  "params": {
    "reason": "source-promoted",
    "requested_by": "context-engine://runs/...",
    "page_ids": ["your-context"],
    "expected_input_hashes": {
      "user-wiki://wiki.toml": "sha256:..."
    }
  }
}
```

The wiki core responds with typed render state:

```json
{
  "render_id": "2026-05-14T18-20-00Z-a1b2c3",
  "status": "succeeded",
  "last_success": "2026-05-14T18-20-00Z-a1b2c3",
  "latest_attempt": "2026-05-14T18-20-00Z-a1b2c3",
  "site_revision": "sha256:...",
  "diagnostics": []
}
```

If inputs change during snapshot or before promotion, the publisher aborts or
retries.
It must not publish output whose input hashes no longer match the render event.

## Render Scheduling And Backpressure

Agents may talk frequently. The publisher must not render on every file write.

`wiki.publish` is a queued publication intent. It means "render accepted wiki
changes soon," not "spawn a renderer immediately for every request."

Scheduling rules:

- single-flight: only one canonical render may run at a time
- coalescing: pending requests merge into one queued attempt
- dirty sets: requests carry page ids, family ids, or global flags
- debounce: ordinary agent writes wait briefly so bursts become one render
- hash-aware no-op: if accepted inputs did not change, record `no_change`
- manual priority: explicit refresh or Open Wiki on stale content may run
  immediately when idle
- backoff: repeated failures delay automatic retries while preserving last-good
- previews: proposal previews do not trigger canonical publication

Suggested V0 policy:

```text
talk/proposal write          mark dirty, debounce 10-30 seconds
accepted source promotion    mark dirty, debounce 5-15 seconds
manual refresh/open stale    render immediately if idle
active edit storm            publish at most every 30-60 seconds
renderer failure             exponential backoff, keep last-good
no accepted input change     record no_change, skip renderer
```

V0 may full-render after coalescing. Request and event schemas should preserve
dirty page/family ids so incremental rendering can arrive later without changing
the authoring contract.

CPU budget principles:

- static serving stays cheap and never depends on an active render
- file watching marks dirty state only
- hashing is scoped to known render inputs where possible
- derived indexes, embeddings, and search updates are separate jobs from static
  rendering
- renderer helpers are short-lived and observable
- long-running agent work may produce many artifacts before one accepted publish

Render events should record scheduling data:

```json
{
  "render_id": "2026-05-14T18-20-00Z-a1b2c3",
  "status": "succeeded",
  "trigger": "debounced",
  "coalesced_request_count": 14,
  "dirty_pages": ["your-context", "projects"],
  "dirty_scope": "pages",
  "skipped_reason": null,
  "duration_ms": 842,
  "renderer_duration_ms": 611
}
```

## Publish Coordinator

The portable publish coordinator performs canonical publication:

1. Receive production or dev runtime paths from the platform host.
2. Validate `wiki.toml` and user-wiki path confinement.
3. Create missing configured pages and talk folders through page lifecycle when
   policy allows.
4. Snapshot or hash render inputs.
5. Create fresh staging under Application Support.
6. Invoke the bundled renderer helper with explicit paths.
7. Parse structured render result JSON.
8. Validate routes, markdown twins, manifests, assets, access labels, and
   privacy/export allowlists.
9. Atomically replace `user-wiki/site` with the staged successful export.
10. Mirror the last-good export to Application Support for local web serving.
11. Write `site/.1context/current-render.json`.
12. Append `site/.1context/render-events.jsonl`.
13. Return typed status to the caller.

Failed render:

- updates `latest_attempt`
- appends a render event
- preserves `last_success`
- preserves served site
- leaves no partial site visible

Uninitialized render:

- reports an actionable diagnostic
- does not publish placeholder routes as if they were canonical

## Renderer Helper

The renderer helper accepts explicit arguments or equivalent JSON input:

```text
render-wiki
  --wiki-root <user-wiki>
  --source-root <user-wiki/source>
  --template-root <user-wiki/templates>
  --output <staging/site>
  --result-json <staging/render-result.json>
```

Successful result:

```json
{
  "status": "succeeded",
  "pages": [
    {
      "page_id": "your-context",
      "route": "/your-context",
      "html": "your-context/index.html",
      "markdown": "your-context/your-context.md",
      "talk_route": "/your-context/talk",
      "access": "private"
    }
  ],
  "warnings": [],
  "errors": []
}
```

Failure result:

```json
{
  "status": "failed",
  "errors": [
    {
      "code": "invalid_frontmatter",
      "page_id": "your-context",
      "message": "section must be one of ...",
      "path": "user-wiki://source/families/..."
    }
  ]
}
```

Renderer failures must be structured enough for the publisher to diagnose and
preserve the last-good site.

## Publication Privacy

Canonical static export may include:

- rendered HTML
- markdown twins
- public site assets
- route and content manifests
- redacted render metadata

Canonical static export must not include:

- `context-engine` internals
- raw prompts unless explicitly published by a user-owned page
- raw observations
- run transcripts
- local absolute paths
- usernames or home-directory fragments in browser-visible JSON
- private proposal previews
- private source outside the configured wiki export

Local APIs must use redacted paths or logical ids. Debug APIs may include local
paths only behind explicit developer/debug gates.

## Concurrency And Recovery

Rules:

- source writes record expected old hashes
- promotions are atomic per target set
- render events record input hashes
- the publisher promotes only the snapshot it validated
- if inputs change before promotion, the publisher aborts or retries
- source may be newer than site
- site must never be half-rendered
- served output is last-good or a clear uninitialized diagnostic

Recovery is append-only evidence plus last-good export. If the system crashes
mid-render, it can reconstruct status from source files, staging directories,
`current-render.json`, and `render-events.jsonl`.

## Required Behavioral Schemas

The implementation must define and test schemas for:

- route plan records
- proposal records
- decision records
- promotion receipts
- agent directory records
- mail delivery records
- notification outbox and attempt records
- render request JSON
- render result JSON
- render queue state
- dirty page/family records
- `/api/wiki/pages.json`
- `/api/wiki/state`
- `/api/wiki/health`

Persisted filesystem schemas are listed in the user-data spec. Schema drift
between defaults, page lifecycle output, talk/mail output, JS validation, core
validators, Swift host adapters, and tests is a release blocker.

## Non-Negotiable Invariants

- `wiki.toml` is the available-page registry.
- Configured missing pages are created by `wiki.page.create` or diagnose.
- Unconfigured routes diagnose; they do not fall back to `/your-context`.
- Tombstoned pages are not recreated.
- Talk access inherits page access by default.
- Talk, inboxes, agent directory, and notification wakeups are V0 primitives,
  not deferred add-ons.
- Mail delivery state is recipient-specific and does not rewrite message truth.
- Notification delivery is best-effort; failed pushes never drop inbox mail.
- Agent-view markdown URLs fetch valid markdown twins.
- Browser-visible APIs do not leak local absolute paths.
- Render staging is separate from served output.
- `wiki.publish` is queued, debounced, single-flight, and hash-aware.
- Failed render preserves last-good site.
- Proposal previews do not become canonical output.
- Operator-edited files are not overwritten by defaults.
- Wiki rendering is not a conceptual child of `memory-core`.
- No installed-app path requires source checkout, `uv run`, system Python,
  system Node, `npm install`, or `npm ci`.
- Packaged defaults include a portable `runtime-defaults-manifest.json` with
  release version, git commit/dirty bit, source hash, site hash, wiki-engine
  hash, lifecycle/helper hash, renderer hash, and render counts.
- `runtime-test` and generated state are never packaged as public defaults.

## V0 Blockers

| Blocker | Required fix |
| --- | --- |
| Runtime template frontmatter rejected by renderer | Align page schemas before package proof |
| Talk rendered public by default | Talk inherits page access |
| Agents must scan whole talk folders for work | Provide `wiki.mail.inbox` headers and `wiki.mail.read` hydration |
| No durable agent address book | Add agent register, heartbeat, retire, lease expiry, and role/list addressing |
| Notifications duplicate message truth | Keep notifications as wakeups over durable mailboxes |
| Bad markdown twin URLs | Validate every `md_url` or generated markdown route |
| Local API exposes absolute paths | Redact browser-visible API state |
| Host publishes placeholders | Replace placeholder publishing with publisher-owned render coordinator |
| `wiki.toml` and legacy `wiki/menu` both act canonical | Use `wiki.toml` for installed runtime |
| Page lifecycle accepts path escapes | Add confinement validation |
| Host Node/Python required to open wiki | Bundle renderer helper and invoke from the portable core |
| Renderer lives under `memory-core` as architecture | Extract to first-class `wiki-engine/` |
| Agent write bursts trigger render storms | Queue, debounce, coalesce, and skip no-op renders |

## Acceptance Tests

V0 is not done until these pass:

- initialize a clean dev runtime and production-shaped runtime
- create every configured `wiki.toml` source page idempotently
- preserve edited source, talk, templates, prompts, `_curator.md`, and
  `wiki.toml`
- respect tombstones
- reject duplicate routes and path escapes
- render every configured page and talk folder
- prove markdown twins fetch successfully
- prove private pages produce private talk pages unless explicitly downgraded
- prove failed render preserves last-good served site
- prove 100 rapid `wiki.publish` requests produce no overlapping renderer
  helpers and a bounded number of canonical renders
- prove no-op refresh skips renderer when accepted inputs are unchanged
- prove unconfigured routes return diagnostics, not hidden fallbacks
- prove `/api/wiki/health`, `/api/wiki/state`, and page APIs expose no home
  paths or absolute local file paths
- prove proposal previews render only under `context-engine/artifacts`
- package smoke forbids bundled source checkouts, generated runtime state, raw
  `runtime-test`, host interpreter dependency, runtime dependency install, and
  private/internal-only files in the static export

## Later Extensions

Once V0 is stable, this contract can carry larger memory behavior:

- derived search indexes as rebuildable Application Support machinery
- richer route planning and memory queue policy
- curator/librarian state machines
- preview renders for proposal review
- publication tiers and redaction policies
- cross-page refactors with multi-file promotion receipts
- agent-authored site-map changes with operator review

Extensions should add evidence and governance. They should not change the core
rule: memory work becomes public wiki output only after accepted user-owned files
render successfully.
