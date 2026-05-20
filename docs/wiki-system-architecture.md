# 1Context Wiki System Architecture

- Status: greenfield implementation target
- Last updated: 2026-05-20

This document defines the architecture we want to build toward for the 1Context
wiki system. The core should be portable infrastructure, preferably Rust, with
Swift limited to Apple app integration, permissions, menu/UI, service launch,
and platform path discovery. The API contract is in
[Wiki Publishing System API](wiki-publishing-system-api.md); this document
explains the internal shape that makes that API simple.

The goal is aggressive simplification:

```text
one inventory compiler
one page ledger
one lifecycle service
one agent directory
one talk/mail router
one notification dispatcher
one renderer
one publisher
```

Everything else is a consumer.

## North Star

The wiki should feel like a tiny local database whose records happen to be
editable files.

Agents and humans should think in:

```text
page id
route
type
collection
source handle
talk handle
mailbox handle
agent address
published handle
```

They should not need to understand:

```text
source/families/<group>/<family>/source/<slug>.md
page lifecycle setup receipts
runtime defaults install details
route-manifest internals
Application Support mirror layout
transport-specific notification mechanics
```

Those are implementation details.

## Core Objects

### Portable Core Boundary

The wiki core should be implemented as a portable library and daemon surface.
Rust is the preferred greenfield implementation language because the core is
mostly filesystem, schema, validation, diff, routing, queue, and publication
logic. Swift should remain the macOS host.

The language split is an ownership split:

```text
Rust  = portable wiki product semantics
Swift = Apple app, permissions, lifecycle, and local-web host integration
JS    = deterministic static renderer helper, until rendering is moved or kept
Agents/memory = authoring, reasoning, curation, and proposal work
```

Swift and the memory system should both call the Rust core. Neither should
carry a second implementation of wiki placement, template fallback, talk
routing, inbox state, notification semantics, or publish eligibility.

Rust owns:

- `WikiInventory`
- page ledger reads/writes
- page lifecycle operations
- agent directory and leases
- talk append, mail delivery, inbox reads, and notification outbox
- validation and handle/path confinement
- publish preflight, render orchestration, last-good promotion, and evidence

Swift owns:

- app UI and menu actions
- app sandbox and permissions
- launchd/service lifecycle
- Apple-specific path discovery and bundle resource lookup
- Caddy/local-web process supervision
- app Settings, including automatic wiki publish cadence
- calling the Rust core through FFI, subprocess JSON, or a local daemon socket

The Rust core must not depend on a source checkout, system Node, system Python,
or Swift-only types. The macOS app can bundle the Rust binary/library, the
renderer artifact, RuntimeDefaults, and local-web support files.

Target crate shape:

```text
crates/
  onecontext-wiki-core/
    src/
      inventory/
      handles/
      page_lifecycle/
      page_ledger/
      agents/
      talk_mail/
      notifications/
      validation/
      publish/
      runtime_defaults/
      renderer_bridge/
      schemas/
  onecontext-wiki-daemon/
    src/
      json_rpc/
      cli/
      local_api/
```

The daemon and CLI are adapters. The business logic belongs in
`onecontext-wiki-core` so tests can call it without launching the macOS app.

### `WikiInventory`

`WikiInventory` is the resolved view of the wiki. Every public wiki operation
reads it or updates inputs that are then recompiled into it.

Inputs:

```text
~/1Context/user-wiki/wiki.toml
~/1Context/user-wiki/templates/
~/1Context/user-wiki/assets/
~/1Context/user-wiki/source/
~/1Context/user-wiki/source/**/source/*.assets/
~/1Context/user-wiki/source/**/talk/**/attachments/
~/1Context/user-wiki/site/.1context/
~/1Context/user-wiki/.1context/page-ledger.jsonl
~/1Context/context-engine/agents/
~/1Context/context-engine/mail/
~/1Context/context-engine/notifications/
~/Library/Application Support/1Context/wiki-site/current/.1context/
~/Library/Application Support/1Context/setup/
```

Output:

```text
WikiInventory
  pages[]
  collections[]
  page_types[]
  aliases[]
  generated_pages[]
  site_activity_feed[]
  tombstones[]
  agents_summary
  mail_summary
  validation_summary
  publish_fingerprint
```

Every page row should be normalized enough for `wiki.list` and
`wiki.page.status` to answer without path walking.

Required page row fields:

```json
{
  "id": "topics",
  "title": "Topics",
  "route": "/topics",
  "type": "index",
  "collection": "topics",
  "kind": "source_page",
  "state": "needs_publish",
  "content_state": "edited",
  "origin": "created_from_template",
  "template_state": "edited_from_template",
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
  "handles": {
    "source": "user-wiki://page/topics/source",
    "talk": "user-wiki://page/topics/talk",
    "curator": "user-wiki://page/topics/curator",
    "conventions": "user-wiki://page/topics/conventions",
    "mailbox": "mailbox://page/topics",
    "published": "app-support://wiki/topics"
  },
  "mail": {
    "message_count": 3,
    "actionable_count": 3,
    "open_delivery_count": 3,
    "open_thread_count": 1,
    "unread_count": 3,
    "watcher_count": 1
  },
  "validation": {
    "status": "warning",
    "issue_count": 1,
    "blocking_count": 0,
    "warning_count": 1,
    "highest_severity": "warning"
  },
  "next_action": "publish"
}
```

The inventory compiler owns the mapping from handles to paths. Consumers should
not invent storage paths.

The app-facing `wiki.list` and `wiki.page.status` APIs should read from a
compiled inventory snapshot when one is current. If the snapshot is missing or
stale, the daemon may rebuild it from files and ledgers, then answer from that
resolved view.

### Page Ledger

The page ledger is the provenance spine:

```text
~/1Context/user-wiki/.1context/page-ledger.jsonl
```

It records durable facts that cannot be safely inferred later.

Events:

```json
{"event":"page.created","page":"topics","origin":"created_from_template","actor":{"kind":"system","name":"runtime-defaults"}}
{"event":"template.baseline","page":"topics","source_sha256":"...","template_sha256":"..."}
{"event":"page.observed_edit","page":"topics","actor":{"kind":"unknown_manual"},"source_sha256":"..."}
{"event":"page.published","page":"topics","publish_fingerprint":"...","source_sha256":"..."}
{"event":"page.tombstoned","page":"old-page","actor":{"kind":"operator"}}
{"event":"page.restored","page":"old-page","actor":{"kind":"operator"}}
```

Rules:

- The ledger is append-only.
- It is user data, not app support state.
- It is safe to rebuild derived indexes and inventory snapshots from it.
- It records provenance for source, `_conventions.md`, `_curator.md`, and
  family-local templates, not only article markdown.
- Older pages may have `origin = "unknown"` and `template_state = "unknown"`.
  Unknown is a real state, not a failure.

### Agent Directory

The agent directory is the current address book for live and recently-live
agents:

```text
~/1Context/context-engine/agents/directory/
  agents.jsonl
  current.json
  leases.jsonl
```

Directory records bind a durable `agent_id` to transport pointers such as a
Codex `thread_id`, role subscriptions, capabilities, and a lease:

```json
{
  "event": "agent.registered",
  "agent_id": "agent_codex_019e3f72",
  "transport": {"kind": "codex-thread", "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01"},
  "addresses": ["agent://codex/019e3f72", "role://topics.curator"],
  "capabilities": ["wiki.mail", "wiki.curator.apply"],
  "lease_expires_at": "ISO-8601"
}
```

Rules:

- Agents register when they are born.
- Agents heartbeat while active.
- Agents retire when they exit cleanly.
- `agents.jsonl` is the source of truth. `current.json` is a rebuilt cache for
  quick inspection, not the authority.
- Active directory reads replay `agents.jsonl` and filter expired leases so
  stale agents do not receive live notifications.
- Subscription rosters replay the same directory ledger and surface liveness
  per subscription. Total member counts remain durable roster counts; active
  counts are the live coverage signal.
- Expired agents stop receiving live pushes, but their role/list mail remains
  queued.
- Durable mail should usually target roles, lists, pages, or threads rather
  than a single live transport pointer.

### Talk Messages, Deliveries, And Notifications

Talk entries are durable page-local messages. Mailboxes are recipient views
over those messages. Notifications are wakeups that tell a live recipient to
check mail.

Canonical storage:

```text
~/1Context/user-wiki/source/families/<group>/<family>/talk/<slug>.talk/
  <timestamp>.<kind>.<short-title>.md
  attachments/<message-id>/

~/1Context/context-engine/mail/
  subscriptions.jsonl
  deliveries.jsonl
  mailboxes/<address-key>/inbox.jsonl
  claims.jsonl

~/1Context/context-engine/notifications/
  outbox.jsonl
  attempts.jsonl
  cursors/<agent-id>.json
```

Rules:

- Talk message truth is the talk entry file.
- Delivery truth is the delivery ledger.
- Inbox files are rebuildable recipient views from talk entries,
  subscriptions, and deliveries.
- Subscription truth is a JSONL event ledger. Page/list/role rosters can be
  rebuilt from subscription events plus the agent directory liveness ledger.
- Snooze is delivery metadata with a due timestamp. Future snoozes suppress
  default inbox, notification, and page open-mail pressure; due or legacy
  untimed snoozes are actionable again.
- Notification outbox entries are not message truth; they are wakeup attempts.
- A failed push must not lose mail.
  The recipient can still poll `wiki.mail.inbox`.

### Derived Indexes

Derived indexes are optional acceleration surfaces. They may summarize pages,
routes, links, talk headers, search chunks, validation issues, or publish
history, but they are not the source of truth.

Rules:

- Source files, talk files, `wiki.toml`, and ledgers remain canonical.
- Missing or corrupt indexes must degrade search or speed, not erase memory.
- Index schemas belong to the implementation that owns the index.
- Consumer APIs should return handles and typed status, not raw index rows.

### Handles

Handles are the consumer-visible address space:

```text
user-wiki://page/<page-id>/source
user-wiki://page/<page-id>/talk
user-wiki://page/<page-id>/curator
user-wiki://page/<page-id>/conventions
user-wiki://page/<page-id>/template
user-wiki://page/<page-id>/tombstone
app-support://wiki/<route>
render://<render-id>
```

The handle resolver maps these to local paths only inside trusted app, daemon,
CLI, or dev surfaces. Browser-visible output must not expose absolute local
paths.

## Services

### Inventory Compiler

Responsibilities:

- parse `wiki.toml`
- scan source files, talk folders, templates, tombstones, and generated pages
- read page ledger events
- read last render and served-site manifests
- compute page states and flags
- compute stale status from publish input fingerprints
- build the page handle map
- produce validation headers and compact page issue summaries
- refresh derived inventory/search indexes when asked by the daemon

Non-responsibilities:

- rendering HTML
- mutating source
- writing talk entries
- deciding semantic memory truth

### Index Builder

Responsibilities:

- decide whether derived indexes are current
- rebuild indexes from files, ledgers, render manifests, and setup ledgers
- write rebuild evidence and freshness fingerprints
- keep common app reads off repeated filesystem scans where useful

Non-responsibilities:

- mutating canonical source
- deciding curator judgments
- pushing mail to live agents

Index rule:

```text
if index current -> use it as an acceleration surface
if index stale   -> rebuild from canonical inputs, then answer
if rebuild fails -> return typed stale/degraded status, not guessed truth
```

### Page Lifecycle Service

Responsibilities:

- implement `wiki.page.create`
- implement `wiki.page.delete`
- implement `wiki.page.restore`
- allocate ids, slugs, routes, collections, and storage placement
- write `wiki.toml` changes safely
- copy templates into source/talk/family-local template files
- write page ledger events and template baselines
- preserve existing files
- reject collisions and ambiguous requests

Consumer-facing rule: template fill may exist internally, but the product
operation is page creation.

### Agent Directory Service

Responsibilities:

- register newly born agents with stable `agent_id` and transport-specific
  pointers such as Codex `thread_id`
- refresh leases through `wiki.agent.heartbeat`
- record roles, capabilities, addresses, leases, and last-seen time
- mark agents stale when leases expire
- retire agents on clean exit
- resolve durable addresses such as `role://topics.curator` to live agents when
  possible

Agent registration should write an inspectable receipt and update the current
routing view.

### Talk And Mail Router

Responsibilities:

- validate and append typed talk entries
- attach files, images, screenshots, patches, logs, and other artifacts
- resolve `to`, `cc`, `list_id`, role subscriptions, and page subscriptions
- create durable delivery records per recipient
- enqueue notification wakeups for active transports when available
- keep role/list mail queued when no live agent is present
- expose inbox headers without forcing agents to scan whole talk folders
- hydrate one message or thread on demand

Talk folders remain the durable page-local archive. Mailboxes are recipient
views derived from talk entries and delivery records. Attachments remain
inspectable files in the talk folder, with safe relative handles in the talk
entry.

### Notification Dispatcher

Responsibilities:

- read notification outbox entries created by talk/mail routing
- resolve active transport endpoints from the agent directory
- push wakeups to supported transports when possible
- record delivery attempts, failures, retries, and acknowledgements
- expose `wiki.notify.poll` for transports that cannot receive direct pushes

Notifications are deliberately small. They should contain a notification id,
recipient, message id, thread id, page id, kind, urgency, and inbox cursor.
They should not contain the full talk body or attachments. The recipient uses
`wiki.mail.read` to hydrate content.

### Validator

Responsibilities:

- validate inventory consistency
- validate route uniqueness
- validate handle/path confinement
- validate source frontmatter
- validate talk readiness
- validate template paths and new-page template renderability before registry
  mutation
- validate tombstones
- validate stale served output
- return typed issues with suggested next actions

Validation headers:

```json
{
  "scope": {"kind": "page", "id": "topics"},
  "checked_at": "ISO-8601",
  "input_fingerprint": "...",
  "can_publish": true,
  "status": "warning",
  "issue_count": 1,
  "blocking_count": 0,
  "warning_count": 1,
  "highest_severity": "warning"
}
```

Validation is not an explanation engine and not an auto-repair system.

### Renderer

Responsibilities:

- render deterministic static output from accepted source
- write only to a staging directory
- produce route manifest, content index, markdown twins, and render result JSON
- render talk folders as reader surfaces

Non-responsibilities:

- mutating source
- deciding page creation
- resolving RuntimeDefaults
- serving HTTP
- running as a daemon

The renderer may stay JS. The important thing is that it is called through the
publisher with explicit roots and explicit staging output.

### Publisher

Responsibilities:

- compile inventory
- validate
- respect the configured automatic publish cadence for daemon-initiated source
  changes
- decide skip vs render from publish fingerprints
- render to staging
- validate staged output
- promote `~/1Context/user-wiki/site`
- mirror last-good output to Application Support
- preserve last-good on failure
- write render ledgers
- write page ledger publish events

Publish rule:

```text
source is edited
user-wiki/site is exported last render
Application Support current is served last-good
```

V0 has two publish adapters while the Swift loop closes:

- `onecontext-wiki publish` renders `user-wiki/site`, writes the publish
  fingerprint, and returns structured evidence for agents and tests.
- Swift still performs the Apple-hosted last-good mirror promotion to
  Application Support.

The target is one Rust publisher path with Swift acting as host/supervisor.
Talk-message, mailbox, and notification writes are collaboration state; they do
not force a page-content publish. Page source, source tombstones, and
`wiki.toml` navigation changes do.

Automatic publish policy is a host setting, not source truth. Swift/App Settings
owns the user's cadence choice in
`~/Library/Preferences/com.haptica.1context.plist` under
`WikiAutomaticPublishCadence`, and the daemon enforces it:

```text
no_limit  -> publish as soon as source changes are accepted and the queue is free
1_minute  -> coalesce automatic publishes to at most one start per minute
30_minute -> coalesce automatic publishes to at most one start per thirty minutes
```

Manual `wiki.publish` always remains available and bypasses the automatic
cadence. The publisher still serializes work, skips unchanged inputs, and
preserves last-good output on failure. `wiki.status` reports the active cadence,
queue state, and earliest next automatic publish time so agents can choose
whether to wait or issue an explicit publish.

### Site Activity Feed

The home page should be a generated site page with a rolling "what changed"
feed. The feed is a projection, not memory truth.

Inputs:

- page ledger events: created, observed edit, tombstoned, restored, published
- render events and link diagnostics
- accepted decisions and curator apply receipts
- optional talk summaries or decisions, never raw every-message inbox churn by
  default

Rules:

- The feed is configured in `wiki.toml` as part of the site map.
- The feed renders into the home page and optional markdown twin.
- The feed is safe to rebuild from ledgers and manifests.
- The feed must not leak absolute local paths, private run transcripts, or raw
  inbox state.
- Talk/mail-only changes may appear in the feed only when the configured source
  says they should, such as `include_talk = "decisions_only"`.

## API Mapping

All API calls should be thin wrappers over the services above.

| API | Service | Notes |
| --- | --- | --- |
| `wiki.list` | Inventory compiler | Compact page rows and createable page types. |
| `wiki.page.open` | Inventory compiler + handle resolver | Paths/handles for editing. No mutation. |
| `wiki.page.status` | Inventory compiler | Expanded one-page status card. |
| `wiki.page.create` | Page lifecycle service | Writes files, registry, and ledger. |
| `wiki.page.write_body` | Page lifecycle service | Replaces only markdown body; preserves frontmatter and uses source hash guards. |
| `wiki.page.patch_body` | Page lifecycle service | Exact body find/replace; refuses ambiguous or stale edits. |
| `wiki.page.delete` | Page lifecycle service | Tombstone-first; preserves history. |
| `wiki.page.restore` | Page lifecycle service | Removes tombstone, re-enables registry/navigation, preserves history. |
| `wiki.validate` | Validator | Detailed diagnostics, no publishing. |
| `wiki.publish` | Publisher | Whole-site render internally, scoped evidence externally. |
| `wiki.status` | Publisher/daemon | Tiny system status only. |
| `wiki.agent.register` | Agent directory service | Adds active agent presence and lease. |
| `wiki.agent.identify` | Agent directory service | Waking-agent entrypoint: registers missing sessions, refreshes active/stale sessions, and refuses retired sessions. |
| `wiki.agent.heartbeat` | Agent directory service | Extends an active lease. |
| `wiki.agent.retire` | Agent directory service | Retires a live agent instance. |
| `wiki.agent.whoami` | Agent directory service | Resolves a transport/thread pointer or agent id into the current directory identity and next action. |
| `wiki.agent.list` | Agent directory service | Lists active agents by default, with explicit stale/retired audit modes. |
| `wiki.agent.status` | Agent directory service | Detailed liveness, mailbox, and subscription card for one agent. |
| `wiki.agent.inbox` | Agent directory + talk/mail router | One-call union of direct mail, subscription mail, mailboxes, and pending notifications. |
| `wiki.agent.claim` | Agent directory + talk/mail router | Claims one actionable message from the unified agent inbox without exposing role/list/page mailbox bookkeeping. |
| `wiki.mail.inbox` | Talk/mail router | Header/excerpt-first inbox for agent, role, or list. |
| `wiki.mail.read` | Talk/mail router | Hydrates one message or thread. |
| `wiki.mail.claim` | Talk/mail router | Atomic work claim with same-agent idempotency and competing-agent conflict. |
| `wiki.mail.mark` | Talk/mail router | Seen, claimed, done, timed snoozed, archived. |
| `wiki.mail.mark_all` | Talk/mail router | Resolver action that marks every delivery for one message. |
| `wiki.mail.subscribe` | Talk/mail router | Adds live-agent wakeup rules for role/list/mailbox deliveries without duplicating mail. |
| `wiki.mail.unsubscribe` | Talk/mail router | Cancels matching live-agent subscription rules while preserving durable mailbox history. |
| `wiki.page.watch` | Talk/mail router | Page-aware helper that creates/reuses the page watchers list and subscribes the agent. |
| `wiki.page.assign_role` | Talk/mail router | Page-aware helper that expands role shorthand and subscribes the agent as assignee. |
| `wiki.list.create` | Talk/mail router | Adds durable list metadata such as title, page affinity, owner, and description. |
| `wiki.lists` | Talk/mail router | Reads the list directory with active member counts. |
| `wiki.list.status` | Talk/mail router | One-call list workbench with metadata, mailbox counts, members, and messages. |
| `wiki.list.members` | Talk/mail router | Ergonomic roster alias for list-address subscriptions. |
| `wiki.talk.append` | Talk/mail router | Appends one validated entry and delivers mail. |
| `wiki.talk.thread` | Talk/mail router | Reads one thread without scanning the whole talk page. |
| `wiki.notify.poll` | Notification dispatcher | Reads pending wakeups for a live agent. |
| `wiki.notify.ack` | Notification dispatcher | Records delivered/seen wakeups. |
| `wiki.curator.apply` | Curator apply service | Bounded source patch from an accepted decision. |
| `wiki.asset.add` | Asset service | Copies an image/file into a page-local asset folder and returns markdown/link handles. |
| `wiki.asset.list` | Asset service | Lists page-local embedded assets and publication status. |

List addresses are explicit collaboration objects. Agents may subscribe to role
and page mailboxes by address, but `list://` addresses must exist in the list
directory first so page participants and routing state stay inspectable.
Page status exposes the default page mailbox, curator role, and watchers list
so agents can discover the right collaboration addresses before subscribing or
writing talk.

Publish writes machine diagnostics and reader-visible warnings for broken
internal links after render. This is generated output under the rendered site,
not a source markdown mutation.
Generated `[[site_pages]]` remain renderer-owned. They can publish, appear in
route manifests, and carry `source_kind=generated_site_page`, but source-page
lifecycle commands should reject their ids/routes with `generated_site_page`
instead of creating shadow source-backed pages.

## RuntimeDefaults

RuntimeDefaults are seed and backfill material. They are never the live truth.

First run or upgrade:

1. Install missing safe defaults.
2. Preserve existing user files.
3. Write conflict proposals for changed existing files.
4. Record setup ledger.
5. Compile inventory from user data.
6. Publish from user data.

RuntimeDefaults may create initial ledger entries when they seed pages. They
must not overwrite user-owned source, talk, templates, prompts, `_curator.md`,
or `wiki.toml`.

## Talk, Mail, And Notification Boundary

Talk, mail, directory, and notifications are part of the V0 spine, not a later
bolt-on. Pages without a collaboration surface are not enough for memory
agents. The system should be born with a way to create pages, discuss them,
route work, wake agents, and publish accepted changes.

The right split:

```text
talk folder   = durable page-local mailing-list archive
delivery log  = durable per-recipient mail facts
mailbox       = filtered recipient view for an agent, role, page, or list
directory     = active agent addresses, roles, capabilities, and leases
subscription  = routing rule from talk entries to recipients
notification  = wakeup that tells a live transport to check mail
```

Talk folders are durable workflow source. Rendered talk pages are reader
output. Agents write source/workflow talk files, not rendered talk output.
Talk folders may include an `attachments/` subtree for images, files, logs,
patches, and screenshots. Attachment records keep safe source-relative paths;
rendered talk entries link to route-local published copies under
`/<page-route>/talk/attachments/...`.

Mailboxes hide archived messages by default because inboxes should stay
actionable. The same API must support `include_archived` for audit and delete
proofs so an agent can prove mail survived a tombstone without path-walking the
mailbox files.
Counters distinguish durable history from work pressure. `message_count` and
`total_count` can stay non-zero forever; `actionable_count` and
`open_delivery_count` should fall to zero when the agent or page has nothing
left to do. `open_thread_count` is conversation pressure, so one message fanned
out to a role, list, and page mailbox is still one open thread. Agent inboxes
also keep `pages_with_open_mail_count` separate from
`pages_requiring_action`, because a page can need publish or link repair after
the agent has no mail left.
Notifications are wakeups over delivery state, not independent work items.
Polling is also filtered through the agent's active owned addresses and active
subscriptions, so expired/cancelled subscriptions and mismatched kind filters do
not keep stale wakeups visible.
Polling hides unacked notifications once the corresponding delivery becomes
terminal, while the durable outbox/attempt logs remain available for audit.

Tombstone delete must remove the page from active navigation and disable its
registry row, then publish. Route removal alone is not enough because every
remaining rendered page may contain a brand-menu link to the deleted route.
The delete receipt should include a source-level inbound-link preview so agents
can repair obvious links before publish; the post-render link diagnostics are
still the final reader-surface proof.
Until that publish happens, the tombstoned page should remain a warning with
`next_action = publish`; once rendered output no longer contains the route, it
becomes clean terminal state.
Publish status keeps two freshness layers: a full-site fingerprint for skip
decisions and page fingerprints for agent-facing status. Page fingerprints let
`pages_needing_publish` stay page-scoped after a normal body edit, while
`site_needs_publish` reports that the rendered site is stale because source,
tombstones, or site-map inputs changed. If those changes mean the previous link
report is no longer authoritative, `link_health.fresh = false` tells agents to
publish before trusting link health as post-render evidence.
By default, tombstoned and disabled pages reject new talk append operations;
archive-maintenance writes must be explicit so normal agents do not keep
working a retired page by accident.

Addresses:

```text
role://topics.curator
list://topics.watchers
agent://codex/<thread-id>
page://topics
thread://topics/infrastructure-taxonomy
```

Durable mail should usually target roles or lists. Live agent addresses are
transport pointers and may disappear. If no live curator exists, the mail stays
queued for `role://topics.curator` until a new curator registers.
`page://<page-id-or-route>` is an authoring alias for the page mailbox; talk and
mail recipient APIs normalize it to `mailbox://page/<page-id>` in stored
delivery state and receipts. Agents can use the page alias when thinking in
site terms, then rely on the returned mailbox address for durable follow-up
commands and audit paths.

## Implementation Phases

### Phase 1: Portable Rust Core And Collaboration Spine

Initial Rust slice:

- `WikiInventoryCompiler` resolves `wiki.toml`, source paths, talk readiness,
  rendered route presence, publish staleness, template provenance, and per-page
  handles.
- `WikiPageLedger` reads and appends private JSONL provenance at
  `~/1Context/user-wiki/.1context/page-ledger.jsonl`.
- `WikiPageLifecycleService` implements create, open, status, delete, and
  tombstone-first restore groundwork.
- `WikiAgentDirectory` implements register, heartbeat, retire, lease expiry,
  and role/list address resolution.
- `WikiTalkMailRouter` implements talk append, subscriptions, deliveries,
  inbox headers, thread reads, and mail marks.
- `WikiNotificationDispatcher` implements notification outbox, poll, ack, and
  best-effort push attempts.
- `WikiPublisher` consumes inventory preflight results before rendering.
- JSON-RPC exposes the whole V0 consumer surface, not only publish triggers.
- CLI wrappers call the same RPC/API surface as agents and the app.

Current V0 proof:

- `onecontext-wiki` exposes list, page status, page open, page create,
  page write body, page patch body, page delete, page restore, publish status, publish,
  agent directory, talk append, subscriptions, lists, mail, and notification
  commands.
- The Swift daemon forwards the same supported wiki RPC methods through
  `WikiCoreRPCBridge`; the installed `1context wiki` CLI exposes the common
  app-facing workbench for page lifecycle, publish, agent identity, lists,
  page watch/role assignment, subscriptions, talk, mail, inbox, and
  notification work.
- `wiki.page.create` can append a `wiki.toml` registry/navigation entry and
  create the source/talk/family files in one operation after preflighting that
  the rendered source template satisfies the renderer frontmatter contract.
- `wiki.page.write_body` and `wiki.page.patch_body` preserve page frontmatter,
  refuse tombstoned pages, support stale-source hash guards, and return
  publish receipts only when content changed.
- `render-site.mjs` reads `wiki.toml` during whole-site render so configured
  navigation affects the actual brand menu.
- Publish writes `.1context/link-diagnostics.json` and adds a compact
  `link_diagnostics` pointer/health summary to the route manifest.
- The publish fingerprint excludes talk-message/mail/notification churn and
  includes page source, tombstones, and `wiki.toml`.
- Rendered talk pages use structured attachment metadata for one attachment
  list per message and surface filenames, captions, and alt text in HTML and
  markdown twins.
- `wiki.talk.append` preserves attachment filename, caption, and alt metadata
  through the Rust core, Swift bridge, talk frontmatter, inbox/mail receipts,
  and rendered HTML/markdown talk twins.
- Talk/mail/notification-only work is inbox state and does not make page source
  dirty. A forced publish may refresh rendered talk reader output, but it is not
  a page-content freshness requirement.
- Source page images and files should use page-local asset folders. The target
  API is `wiki.asset.add` plus `wiki.page.patch_body`, not hand-copying files and
  guessing published URLs.

### Phase 2: macOS Host Integration

- bundle the Rust core binary/library in the app
- have Swift discover RuntimeDefaults, WikiEngine, Caddy, user-data, and App
  Support paths
- have Swift launch/supervise the Rust-backed daemon surface
- keep local-web serving and Apple permission UX in Swift/macOS-specific code
- route menu actions to `wiki.publish`, `wiki.status`, and inbox state

### Phase 3: Publish And Renderer Consolidation

- call the renderer only through the Rust publisher
- validate staging before last-good promotion
- record render and page ledger evidence
- keep JS renderer as a pure helper until/unless rendering moves into Rust

### Phase 4: Remove Historical Bloat

- retire public legacy page-fill language
- delete direct consumers of repo-only page-fill scripts
- hide family paths from consumer docs and APIs
- keep scripts only as dev harnesses around product APIs

### Phase 5: Curator Apply

- connect curator apply to sandbox, diff, operator-touched checks, promotion,
  ledger evidence, and publish

### Phase 6: Derived Search And Indexes

- add search indexes only after the file/ledger APIs are stable
- keep indexes rebuildable from user-owned data
- treat index freshness as evidence, not truth

## Non-Goals

- No page-specific incremental renderer promise in V0.
- No hidden database as the primary authoring surface.
- No browser-visible absolute local paths.
- No automatic semantic source rewrites during publish.
- No route fallback to `/your-context` for missing pages.
- No compatibility shim for old page-fill-as-public-API behavior.
