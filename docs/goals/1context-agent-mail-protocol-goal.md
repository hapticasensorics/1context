# Milestone: Agent Mail Protocol Integration

## Goal

Implement agent mail as a clean transport kernel beside the wiki publishing
system, not inside the renderer and not as scattered page helper commands.

V0 lets agents identify themselves, receive routed work, inspect inbox headers,
read one message or thread, claim one delivery, mark one delivery, receive a
Codex steering wakeup for eligible mail, reconcile notification wakeups, and
prove that mail-only operations do not affect `wiki.publish.status`.

Later layers can add lists, role assignment management, page watches, and
governance. They should not be part of the first tranche.

## Design Shape

The implementation starts inside the portable Rust wiki runtime boundary as a
real module, not as more code poured into one large file.

```text
onecontext-wiki-core/
  pages/assets/talk/publish
  agent_mail/
    addresses
    agents
    messages
    deliveries
    inboxes
    storage
    errors
```

Adapters stay thin:

```text
onecontext-wiki-daemon = CLI/JSON adapter
Swift bridge           = app host and Local Web integration
Python wiki adapter    = memory-side client over Rust receipts
```

No Swift or Python layer should own routing, claim, mark, lease, mailbox, or
notification semantics.

The core boundary:

```text
wiki.talk.append = page-local source archive; optional explicit mail delivery
wiki.mail.*      = operational queue and delivery state
wiki.agent.*     = agent identity and lease state
wiki.publish     = reader content proof only
```

Talk and mail are connected, but not the same thing. A talk entry may create a
mail message only when delivery is explicitly requested. Mail read/claim/mark
state must never make the reader site need a publish.

## Compatibility And Boundary Guardrails

- Existing `wiki.talk.append` calls keep current behavior: `to` and `cc` are
  metadata labels unless the caller explicitly requests mail delivery.
- V0 uses explicit delivery, such as `delivery_mode = "mail"` or `deliver =
  true`; there is no silent `auto` delivery in V0.
- Talk delivery is talk-first, not all-or-nothing across talk, mail, and
  notification. If mail delivery fails after the talk file is written, the talk
  entry remains and the receipt returns `delivery_status` plus repair hints.
- No operational mail controls go under `wiki.page.*`. Page namespace stays
  for source/page lifecycle only.
- Role/capability requests are separate from granted roles/capabilities.
  Agents cannot self-assign curator/reviewer power by calling identify.
- Multi-recipient state changes are explicit and scoped. `mark_all` is exposed
  only as a scoped adapter convenience over per-delivery marks, not as hidden
  routing logic.
- Mailbox indexes are rebuildable caches; ledgers and source files are truth.
- Every implementation slice must include proof that mail-only changes leave
  `wiki.publish.status` unchanged.

## Storage Plan

Current wiki source truth stays where it is:

```text
~/1Context/user-wiki/
  wiki.toml
  source/
  templates/
  assets/
  .1context/page-ledger.jsonl
```

V0 agent mail state lives under `context-engine`:

```text
~/1Context/context-engine/
  agents/
    directory/agents.jsonl
    directory/current.json
    directory/leases.jsonl

  mail/
    messages/YYYY/MM/<message-id>.json
    bodies/YYYY/MM/<message-id>.md
    deliveries.jsonl
    mailboxes/<address-key>/inbox.jsonl
    claims.jsonl
    idempotency.jsonl
    dead-letter.jsonl
```

Deferred layers may add:

```text
~/1Context/context-engine/
  mail/
    subscriptions.jsonl
    lists.jsonl
  notifications/
    outbox.jsonl
    attempts.jsonl
    cursors/<agent-id>.json
  governance/
    proposals.jsonl
    reviews.jsonl
    decisions.jsonl
    commits.jsonl
```

Page talk entries and page talk attachments remain under the page talk folder:

```text
user-wiki/source/families/<group>/<family>/talk/<slug>.talk/
  <timestamp>.<kind>.<subject>.md
  attachments/<message-id>/<safe-filename>
```

Before Rust implementation, storage paths must be reconciled with
`docs/user-data-spec.md` so there is one authoritative folder contract.

## V0 API Plan

Add back only the API that the first tranche can defend and test.

### Agent Identity

```text
wiki.agent.identify(thread_id, requested_roles?, requested_capabilities?, ttl_seconds?)
wiki.agent.heartbeat(agent_id, ttl_seconds?)
wiki.agent.status(agent_id)
wiki.agent.retire(agent_id, reason?)
```

`identify` is create-or-renew for a live Codex thread. It returns durable
agent id, primary address, lease state, requested roles/capabilities, granted
roles/capabilities, and pending inbox summary.

### Mail

```text
wiki.mail.send(envelope, body, attachments?)
wiki.mail.inbox(address, filters?)
wiki.agent.inbox(agent_id, filters?)
wiki.mail.read(message_id | thread_id)
wiki.mail.claim(message_id, recipient, agent_id)
wiki.mail.mark(message_id, recipient, state)
wiki.mail.snooze(message_id, recipient, until)
```

Inbox calls return headers and thread summaries first. `read` hydrates bodies
and attachments only when needed.

Claims are same-agent idempotent and competing-agent conflicting. `done` and
`archived` are terminal for open-work counts. `snoozed` requires a future time.

### Talk Bridge

```text
wiki.talk.append(page, kind, subject, from, to?, cc?, body, attachments?, delivery_mode?)
```

V0 delivery modes:

- `labels_only`: write talk; store `to`/`cc` as metadata labels; no mail.
- `mail`: write talk; create a mail message; create delivery rows.

The default is `labels_only` to preserve current behavior. Mail delivery is an
explicit act.

Receipts must expose:

```text
talk_status = written | failed
delivery_status = skipped | delivered | failed_retryable | dead_letter
publish_required = false
```

### Deferred API Namespaces

Do not implement these in V0:

```text
wiki.mail.subscribe / unsubscribe
wiki.list.create / status / members
wiki.role.assign / unassign / status
wiki.mail.watch_page / unwatch_page
wiki.governance.*
```

These remain design targets after base delivery is boring.

Implemented V0 notification controls:

```text
wiki.notify.poll(agent_id, cursor?)
wiki.notify.ack(agent_id, notification_id)
```

The daemon also owns host-side `notify-dispatch`; agents use that only through
the supervisor, not as an ordinary mail action.

## V0 Done When

- Agents can identify with a Codex thread id and receive a durable agent id,
  address, lease, requested/granted role summary, and capability summary.
- Direct and role addresses can receive mail through `wiki.mail.send`.
- Active non-retired agents matching a delivery receive durable notification
  rows, and inactive/stale non-retired agents can still be seen by supervisor
  dispatch while their own self-poll/claim/mark calls remain lease-gated.
- A page talk entry with default delivery mode keeps `to`/`cc` as labels and
  creates no mail.
- A page talk entry with explicit mail delivery creates a talk file and mail
  delivery rows without requiring `wiki.publish`.
- A delivery failure after talk write preserves the talk file and returns
  retryable/dead-letter evidence.
- An agent inbox returns relevant headers and summaries before hydration.
- A specific message/thread can be read with body, attachments, page context,
  and source handles.
- Claims are atomic, same-agent idempotent, and competitor-safe.
- A notification ack records wakeup handling only; it does not claim, read, or
  complete the underlying delivery.
- Failed steering dispatch records a failed attempt, preserves the pending
  notification, and leaves the delivery unread.
- Steering, notification polls, dispatch attempts, and acks carry only the
  control-plane envelope; `wiki.mail.open` authorizes the full body and the
  Codex host delivers it through `thread/inject_items`.
- Marking one delivery updates inbox summaries without mutating wiki source or
  publish state.
- Mailbox indexes can be rebuilt from message and delivery truth.
- Bad addresses, duplicate idempotency keys, stale agents, retired agents,
  path traversal in address keys, full mailboxes, and corrupted mailbox indexes
  produce structured repair hints.
- Role and list deliveries can be proven through agent-visible notifications
  when the agent has the matching route grant. Page-mailbox delivery is durable
  and canonicalizes `page://topics` to `mailbox://page/topics`; page-watch
  notification routing remains deferred until page watch grants are implemented.
- Mail-only operations leave publish status unchanged; browser-visible route
  checks remain covered by the wiki dogfood harness.

## Current Completion Proof Checklist

The current dogfood acceptance surface is:

- [x] Active role delivery creates durable notifications for two live curators.
  Evidence: `retired mail dogfood harness`;
  `test-results/agent-mail-dogfood-20260525T023826Z/summary.json`.
- [x] Steering and notification control-plane outputs omit full message bodies.
  Evidence: captured steering files plus
  `test-results/agent-mail-dogfood-20260525T023826Z/notification-ledger/attempts.jsonl`.
- [x] `wiki.notify.ack` clears only the wakeup hint; the delivery remains
  unread until opened/claimed/marked. Evidence: `ack_only_*` entries in
  `summary.json` and `mail-ledger/deliveries.jsonl`.
- [x] Failed dispatch preserves pending notification and unread delivery.
  Evidence: `failed_steering_*` entries in `summary.json`,
  `notification-ledger/attempts.jsonl`, and `mail-ledger/deliveries.jsonl`.
- [x] Inactive/stale self-poll and claim fail, while supervisor dispatch can
  still see the pending notification and heartbeat restores agent visibility.
  Evidence: `inactive_*` entries in `summary.json` and
  `notification-ledger/outbox.jsonl`.
- [x] Claiming shared role mail suppresses the competing notification.
  Evidence: `active_curator_b_notification_id` in `summary.json` and
  suppressed state in `notification-ledger/outbox.jsonl`.
- [x] Role and list notification routing are proven; page-mailbox delivery is
  proven as durable delivery only. Evidence: `route_coverage` and
  `page_mailbox_*` in `summary.json`.
- [x] Mail, notification, ack, dispatch, and ledger operations leave
  `wiki.publish.status` boundary fields unchanged. Evidence:
  `publish_boundary` in `summary.json`.
- [x] `wiki.mail.open` returns a bodyless message summary plus
  `content_delivery` for `thread/inject_items`. Evidence:
  `test-results/agent-mail-dogfood-20260525T033337Z/summary.json` and
  `commands.jsonl`.
- [x] `wiki.agent.status_by_thread` resolves a Codex transport thread to the
  durable agent identity, lease state, active work hint, and pending
  notification digest. Evidence:
  `test-results/agent-mail-dogfood-20260525T063840Z/summary.json`.
- [x] Host-facing injection recording writes `MailInjectionReceipt` and
  `MailControlEvent` rows after an authorized bodyless `wiki.mail.open`.
  Evidence:
  `test-results/agent-mail-dogfood-20260525T063840Z/mail-ledger/injection-receipts.jsonl`
  and
  `test-results/agent-mail-dogfood-20260525T063840Z/mail-ledger/control-events.jsonl`.

Post-V0 hardening checks:

- [x] Keep browser-visible route checks in the wiki dogfood harness, while this
  mail harness proves `wiki.publish.status` is unchanged by mail state.
  Evidence: `retired wiki dogfood harness` owns route checks;
  `retired mail dogfood harness` owns mail/publish-boundary checks.
- [x] Host-facing adapter API records a `MailInjectionReceipt` /
  `MailControlEvent` for `wiki.mail.open` content delivery.
- [ ] Codex host adapter executes the actual app-server `thread/inject_items`
  call for `wiki.mail.open` before recording the receipt.
- [ ] `SessionStart` on resume heartbeats active agent and adds a pending
  notification digest without injecting a body.
- [ ] `UserPromptSubmit` blocks retired/stale/unauthorized mail mutations.
- [ ] `PreToolUse` denies `wiki.mail.mark(done)` when a delivery is unclaimed
  and denies `wiki.mail.open` when the delivery is not visible to the agent.
- [ ] `Stop`/`SubagentStop` claimed-but-unmarked gates have a live hook proof
  and do not loop forever.
- [ ] `PreCompact` snapshots active mail context and `PostCompact` restores it
  without bypassing `wiki.mail.open`.
- [ ] App-server item completions are mirrored as evidence refs for
  `wiki.mail.mark(done)`.

## Later Done When

- Page/list/role subscriptions can route future mail and can be removed cleanly.
- Lists preserve durable mail even when all subscribers are stale.
- Role mail exposes staffing gaps when no active assignee exists.
- Governance messages can propose, review, decide, and commit wiki changes
  using exact artifacts and existing page write/publish APIs.

## Checklist

### 0. Compatibility And Boundary Guardrails

- [x] Current wiki API has been trimmed back to pages, assets, talk, validate,
  and publish. Evidence: `docs/wiki-publishing-system-api.md`.
- [x] Mail was initially documented as not implemented before the Rust boundary
  existed. Evidence:
  `docs/agent-mail-protocol.md`.
- [x] Update `docs/agent-mail-protocol.md` so V0 uses explicit delivery mode,
  talk-first partial failure, no page-namespace mail controls, scoped
  `mark_all`, and notification wakeup reconciliation.
  Evidence: `docs/agent-mail-protocol.md`.
- [x] Update `docs/user-data-spec.md` with final V0 mail storage paths, file
  ownership, rebuildable indexes, and migration rules.
- [x] Update `docs/wiki-publishing-system-api.md` as the current publishing
  contract; keep mail operations in the mail protocol doc, with talk append as
  the explicit bridge.
  Evidence: `docs/user-data-spec.md`, `docs/wiki-publishing-system-api.md`.

### 1. Rust Module Boundary

- [x] Create a real `agent_mail` module boundary in `onecontext-wiki-core`
  before adding behavior. Evidence:
  `crates/onecontext-wiki-core/src/agent_mail.rs`.
- [x] Keep the initial module private to Rust tests; do not expose CLI, Swift,
  or Python calls yet.
- [x] Add schema/error types for addresses, agents, messages, deliveries,
  claims, inbox rows, and repair hints.
- [x] Add safe address grammar and mailbox-key encoding before any path writes.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`.

### 2. Storage And Replay

- [x] Add append-only storage helpers for messages, bodies, deliveries, and
  claims.
- [x] Add append-only storage helpers for agents and leases.
- [x] Add mailbox index rebuild from message/delivery truth.
- [x] Add idempotency-key handling for message acceptance.
- [x] Add dead-letter storage for invalid or exhausted delivery attempts.
- [x] Prove corrupted mailbox indexes rebuild correctly.
- [x] Prove path traversal in address keys is rejected.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`.

### 3. Agent Directory

- [x] Define role/capability grant policy before implementing identify.
- [x] Implement `wiki.agent.identify` as create-or-renew for Codex thread ids.
  Evidence: internal Rust `AgentGrantPolicy` and
  `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`.
- [x] Implement heartbeat, status, and retire.
- [x] Prove stale leases stop live push eligibility but do not lose mail.
- [x] Prove retired agents remain historical and are not silently revived.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`.

### 4. Mail Send And Delivery

- [x] Implement `wiki.mail.send` with schema validation and address resolution.
- [x] Implement direct-agent, role, and page-mailbox delivery fanout.
- [x] Implement open-count summaries by recipient and thread.
- [x] Implement visible backpressure outcomes for full mailboxes.
- [ ] Implement blocked-route/dead-letter outcomes once directory route rules
  exist.
- [x] Prove duplicate idempotency keys with identical payloads are safe.
- [x] Prove duplicate idempotency keys with different payloads are rejected.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`.

### 5. Inbox, Read, Claim, Mark

- [x] Implement `wiki.mail.inbox` header-first reads.
- [x] Implement `wiki.agent.inbox` over direct and granted-role deliveries.
- [x] Implement `wiki.mail.read` for message or thread hydration.
- [x] Implement `wiki.mail.claim`.
- [x] Implement single-delivery mark states: read, done, archived, rejected.
- [x] Implement snooze as its own due-time-required operation.
- [x] Prove snooze due behavior.
- [x] Prove unknown/retired agents cannot claim, mark, or snooze deliveries.
- [x] Prove generic mark rejects snooze without a due time.
- [x] Prove claim conflict, same-agent idempotency, and
  terminal-state invariants.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`.
- [x] Prove mail marks do not change `wiki.publish.status`.
  Evidence: `cargo test -p onecontext-wiki-core -- --nocapture`.

### 6. Talk Bridge

- [x] Extend `wiki.talk.append` with explicit `delivery_mode`.
- [x] Prove default talk calls stay labels-only and create no mail.
- [x] Prove explicit mail delivery creates a talk file plus message/delivery
  rows.
- [x] Prove crash/replay after talk write but before delivery is retryable by
  stable operation id.
- [x] Prove delivery failure does not delete the talk file and returns repair
  hints.
- [x] Prove talk/mail-only changes do not require `wiki.publish`.
  Evidence: `cargo test -p onecontext-wiki-core -- --nocapture`;
  `cargo test -p onecontext-wiki-daemon -- --nocapture`.
- [ ] Prove talk attachments remain source truth and render safely when the
  static talk page is later published.

### 7. Deferred Lists, Roles, Watches, Notifications

- [ ] Implement list create/status/members after V0 delivery is stable.
- [ ] Implement subscribe/unsubscribe as wakeup and inbox-visibility rules.
- [ ] Implement `wiki.mail.watch_page` / `wiki.mail.unwatch_page` as page
  mailbox subscription convenience, not `wiki.page.*`.
- [ ] Implement `wiki.role.assign` as explicit role routing, not `wiki.page.*`.
- [x] Implement notification outbox generation, poll, ack, and attempt ledgers.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`;
  `test-results/agent-mail-notify-hook-20260521T083900Z/outbox.jsonl` and
  `test-results/agent-mail-notify-hook-20260521T083900Z/attempts.jsonl`.
- [x] Define the Codex steering adapter interface without embedding it into
  message truth.
  Evidence: `test-results/agent-mail-notify-hook-20260521T083900Z/steering.txt`
  captured a `<steering source="1context" ...>` block with no mail body copied
  into the wakeup payload.

### 8. Swift And Python Adapters

- [x] Expose only the approved V0 methods through `WikiCoreRPCBridge`.
  2026-05-21 note: `wiki.talk.append` now forwards `operation_id` and
  `delivery_mode` to the Rust core instead of reinterpreting them in Swift.
  Notification methods now map to the Rust core as thin command translations.
- [x] Add installed `1context wiki` CLI commands for the approved V0 surface.
  2026-05-21 note: the installed Swift CLI help and parser now accept
  `talk-append --operation-id` and `--delivery-mode`; it also exposes
  `agent-identify`, `agent-inbox`, `mail-claim`, `mail-mark`, `mail-snooze`,
  `notify-poll`, `notify-ack`, and `notify-dispatch`.
- [x] Add memory-core Python wrappers that return raw Rust receipts.
  Evidence: `uv run --with pytest pytest tests/test_wiki_core_client.py -q`
  from `memory-core` passed 7 tests.
- [x] Keep adapters thin: no duplicate routing, claim, or notification logic.
  Evidence: `swift test --package-path macos --filter WikiCoreRPCBridgeTests`
  passed 9 tests; Python wrappers only translate arguments to CLI calls.

### 9. Closed-Loop Dogfood

- [x] Add a deterministic V0 mail dogfood harness that creates agents, role
  mail, list mail, page-mailbox delivery, page talk with labels, page talk
  with explicit delivery, notification dispatch, claims, marks, ack-only
  behavior, failed-dispatch preservation, inactive supervisor dispatch, and
  publish-status checks. Evidence: `retired mail dogfood harness`;
  `test-results/agent-mail-dogfood-20260525T023826Z/summary.json`.
- [x] Capture first manual dogfood evidence under `test-results/agent-mail-*`.
  Evidence: `test-results/agent-mail-dogfood-20260521T081802Z/`,
  `test-results/dogfood-probe-b-agent-mail-20260521T081749Z/`, and
  `test-results/agent-mail-notify-hook-20260521T083900Z/`.
- [ ] Browser-check that wiki routes remain correct while mail state changes.
- [x] Dogfood with at least three agents only after single-agent V0 is stable.
  Evidence: subagent probes A/B/C plus main-agent CLI dogfood on 2026-05-21.
- [x] Update this checklist with evidence after each proof-producing slice.
  Evidence: this document's "Current Completion Proof Checklist".

### 10. Later Governance Layer

- [ ] Add proposal/review/decision/commit message kinds after base mail is
  stable.
- [ ] Prove a proposal can cite exact artifacts and produce decision evidence.
- [ ] Prove a committed wiki change uses existing page write/publish APIs.
- [ ] Prove rejected/deferred proposals do not mutate source truth.

### 11. Exit

- [x] Remove or update stale mail docs and code paths created during the
  implementation.
  Evidence: stale "future/removed prototype" wording was replaced in the wiki
  architecture, use-story, runbook, README, and gateway docs; the old scattered
  prototype remains absent.
- [x] Run Rust, Swift, Python, and dogfood verification.
  Evidence: `cargo test -p onecontext-wiki-core --no-fail-fast`;
  `cargo test -p onecontext-wiki-daemon --no-fail-fast`;
  `swift test --package-path macos --filter WikiCoreRPCBridgeTests`;
  `uv run --with pytest pytest tests/test_wiki_core_client.py -q`;
  `archived mail dogfood evidence`.
- [ ] Commit the integrated protocol with evidence paths in the final summary.

## Notes

- Current baseline: wiki page/talk/publish is clean and the previous mail
  prototype has been removed from active code.
- Immediate next step: implement inbox, read, claim, and single-delivery mark
  behavior on top of the internal Rust `agent_mail` transport.
- Strong bias: build the small base transport first. Lists, roles,
  notifications, steering, and governance are easier once message acceptance,
  delivery, claim, mark, and talk bridge behavior are boring.
