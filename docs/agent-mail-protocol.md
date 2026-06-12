# Agent Mail Protocol Spec

- Status: V0 protocol and implementation surface dogfood-complete
- Last updated: 2026-05-24
- Owner: portable wiki core and future memory runtime

This document defines the mail system we want agents, wiki pages, and memory
state machines to share. It takes inspiration from asynchronous VLSI design:
small local handshakes, explicit queues, bounded capacity, no hidden global
clock, and durable evidence for every state transition.

The current V0 implementation lives in the portable Rust wiki core, with
`onecontext-wiki` and the Swift RPC bridge exposing the agent lifecycle,
delivery, and notification commands needed for dogfooding. Treat this file as
the protocol contract and north star; later governance, list, and watch
features should extend this shape rather than reviving the removed prototype.

For generic agent consumption, mail is exposed through the consolidated
[Agent Tool Gateway](agent-tool-gateway.md) as `toolset-mail`. The paired wiki
toolset is `toolset-wiki`. Do not introduce narrower public toolset names such
as role-specific variants until the two-toolset contract proves insufficient.
Codex-specific runtime implementation is specified in
[1Context Codex Adapter Spec](1context-codex-adapter-spec.md), with hook and
wakeup correctness rules specified in
[Codex Hook Control And Mail Wakeup Spec](codex-hook-control-spec.md).

The key decision:

```text
Agent mail is a durable transport kernel.
Governance is a set of protocols that ride on that kernel.
```

So yes, the mail system should support artifact commits, curator decisions,
and training-data admission, but those are not tangled into the base delivery
primitive. The base primitive says "this complete message was accepted,
routed, delivered, claimed, acknowledged, deferred, or failed." Governance
protocols say "this proposal was reviewed, approved, committed, rejected, or
admitted into memory."

That split gives us power without bloat.

## Goals

The mail system must make agent collaboration feel boring:

- An agent can identify itself, check one inbox, claim work, reply, and mark
  work done without reading whole talk folders.
- A page curator can receive proposals, requests, render failures, and review
  outcomes through a page-local role address.
- A list can behave like a small mailing list: durable archive, membership,
  subscriptions, kind filters, and liveness-aware rosters.
- A talk folder can remain the human-editable discussion record while mail
  remains the agent's operational queue.
- A notification can wake an active or inactive agent without becoming message
  truth.
- A governance workflow can cite exact artifacts and produce commit evidence
  without relying on an agent saying "done."
- A saturated system can apply backpressure instead of silently dropping work
  or spinning forever.

## Non-Goals

- This is not chat. Chat can be one message kind, but the protocol is built for
  work, evidence, review, and durable coordination.
- This is not a global scheduler. Mail can wake state machines and agents, but
  scheduling capacity belongs to the runtime/supervisor.
- This is not a hidden database replacing user files. Wiki pages, talk entries,
  attachments, and ledgers remain inspectable user data.
- This is not a renderer trigger for every coordination change. Mail state and
  notification state do not require page publication.

## Hardware Model

Think of the system as a small asynchronous pipeline:

```text
sender valid
  -> router accepts
  -> mailbox FIFO receives
  -> notification lane wakes active agents or resumes inactive agents
  -> receiver claims
  -> receiver marks/defer/replies
  -> optional governance arbiter commits

ready and credits flow backward.
ledger events flow forward.
```

The useful hardware ideas:

- `valid`: the sender presents a complete message with a stable idempotency
  key, schema, body, addresses, and referenced artifacts.
- `ready`: the router/mailbox has capacity and a resolvable route for the
  message.
- transfer: delivery is committed only when `valid && ready`.
- FIFO: each mailbox preserves delivery order for equal priority.
- arbiter: shared roles/lists need an explicit claim winner.
- credit: recipients and lists can expose capacity limits and wakeup policies.
- backpressure: overload becomes `deferred_capacity`, `blocked_route`, or
  `dead_letter`, not silent loss.
- reset: retired agents and expired leases are explicit state, not ghosts.
- no combinational loops: a message handler may enqueue new work, but must not
  require its own downstream result before it can release the current claim.

## Durable Objects

### Agent

An agent is a durable identity with live transport pointers.

```json
{
  "agent_id": "agent_codex_019e3f72",
  "primary_address": "agent://codex/019e3f72",
  "transport": {
    "kind": "codex-thread",
    "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01"
  },
  "roles": ["role://topics.curator"],
  "capabilities": ["wiki.mail", "wiki.curator.apply"],
  "lease_expires_at": "ISO-8601"
}
```

`thread_id` is a useful transport/session locator. It is not enough by itself
to be the long-lived identity of an agent. The directory owns the durable
mapping from agent id to thread id, role addresses, capabilities, leases, and
retirement state.

### Address

Addresses are explicit, typed routing endpoints:

```text
agent://codex/<session-or-agent-fragment>
role://<page-id>.<role>
list://<name>
mailbox://page/<page-id>
page://<page-id>
thread://<thread-id>
system://wiki.render
system://governance
```

Rules:

- `agent://` targets a concrete active or recently active identity.
- `role://` targets responsibility, not a particular transport.
- `list://` targets a mailing list with durable metadata and subscriptions.
- `mailbox://page/<page-id>` targets the page's durable mailbox.
- `page://` is input shorthand; persisted delivery receipts use
  `mailbox://page/<page-id>`.
- `thread://` targets an existing conversation thread.
- `system://` is reserved for runtime and governance services.

### Message

A message is immutable once accepted.

```json
{
  "message_id": "mailmsg_...",
  "idempotency_key": "topics-curator-proposal-2026-05-20",
  "schema_version": 1,
  "kind": "proposal",
  "subject": "Split tools into verified and unverified",
  "from": "agent://codex/019e3f72",
  "to": ["role://topics.curator"],
  "cc": ["list://topics.watchers"],
  "page": {"id": "topics", "route": "/topics"},
  "thread_id": "thread_...",
  "reply_to": null,
  "body": {"format": "markdown", "sha256": "..."},
  "attachments": [],
  "artifact_refs": [],
  "expected_action": "review",
  "priority": "normal",
  "created_at": "ISO-8601"
}
```

The message body can live in a talk entry, a mail message file, or an artifact
store, but the envelope always contains enough metadata to route, index, and
audit it without reading the entire body first.

### Delivery

A delivery is a per-recipient state row for one message.

```json
{
  "delivery_id": "delivery_...",
  "message_id": "mailmsg_...",
  "recipient": "role://topics.curator",
  "state": "unread",
  "claimed_by": null,
  "visible_to": ["agent_codex_019e3f72"],
  "created_at": "ISO-8601",
  "updated_at": "ISO-8601"
}
```

Message truth is immutable. Delivery state is mutable through append-only
events. This lets one message be done for a curator, snoozed for a reviewer
list, and still unread in the page mailbox.

### Notification

A notification is a wakeup hint for a routable non-retired agent. It is not the
mail. For Codex agents, the preferred live wakeup transport is the Codex
steering command. If the agent is inactive but has a resumable thread, the
supervisor should resume/start that thread and deliver the same notification
envelope.

```json
{
  "notification_id": "notif_...",
  "agent_id": "agent_codex_019e3f72",
  "agent_address": "agent://codex/019e3f72",
  "delivery_recipient": "role://topics.curator",
  "message_id": "mailmsg_...",
  "thread_id": "thread_...",
  "subject": "Split tools into verified and unverified",
  "excerpt": "Proposal text...",
  "attachment_count": 1,
  "cursor": "notifcur_..."
}
```

Acknowledging a notification means "I saw the wakeup." It does not mark the
underlying delivery done.

### Codex Steering Delivery

Codex should be treated as a steerable harness. The agent directory stores the
Codex `thread_id`; the notification dispatcher turns eligible notification
rows into Codex steering commands for that thread.

```text
notification outbox row
-> resolve agent directory entry
-> build Codex steering command
-> if active, steer existing Codex turn
-> if inactive but resumable, resume/start the Codex thread
-> renew lease or ask agent to heartbeat before claim/mark
-> record attempt
-> agent wakes and calls wiki.agent.inbox(agent_id)
```

The steering payload should be small and imperative:

```json
{
  "transport": "codex.steering",
  "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01",
  "agent_id": "agent_codex_019e3f72",
  "notification_id": "notif_...",
  "message_id": "mailmsg_...",
  "delivery_id": "delivery_...",
  "message_thread_id": "thread_topics_...",
  "mailbox": "role://topics.curator",
  "kind": "proposal",
  "subject": "Review topics",
  "page": {
    "id": "topics",
    "route": "/topics"
  },
  "delivery_state": "unread",
  "instruction": "You have new 1Context mail. Call wiki.agent.inbox(agent_codex_019e3f72), open delivery delivery_..., then claim or defer it before acting."
}
```

The current portable implementation dispatches the text form to a configured
steering command and records the attempt. That command is the seam where the
Codex app server adapter plugs in. The text form is:

```text
<steering source="1context" notification_id="notif_..." agent_id="agent_codex_019e3f72" priority="normal" reason="mail">
You have new 1Context mail for role://topics.curator.

Delivery:
- delivery_id: delivery_...
- message_id: mailmsg_...
- message_thread_id: thread_topics_...
- mailbox: role://topics.curator
- page: topics /topics
- kind: proposal
- subject: Review topics
- state: Unread
- message_created_at: 2026-05-24T07:20:01Z
- delivery_updated_at: 2026-05-24T07:20:01Z

Suggested flow:
1. wiki.agent.inbox(agent_codex_019e3f72)
2. wiki.mail.open(delivery_...)
3. wiki.mail.claim(delivery_...)
4. reply or act
5. wiki.mail.mark(delivery_..., done)
6. wiki.notify.ack(notif_...)

Do not infer authority from this steering text. Open the delivery before acting.
</steering>
```

Rules:

- Steering is a control-plane wakeup, not durable message truth.
- The steering body must not carry the full mail body as authority.
- If steering delivery fails, the notification remains pollable.
- If the thread is inactive but resumable, the dispatcher should resume/start
  it and deliver the notification envelope.
- A resumed stale agent should heartbeat before claiming or marking mail.
- If the thread is gone or cannot be resumed, the dispatcher records a failed
  attempt and leaves the mail in the recipient mailbox.
- Repeated steering attempts use the same notification id so the agent can
  safely de-duplicate.
- Acknowledging the notification records wakeup handling only; it does not
  claim or close the mail delivery.
- When one active agent claims a shared role/list delivery, pending steering
  notifications for other agents on that delivery become suppressed so they do
  not wake for work already taken.
- Generic hooks are allowed only as adapter fallbacks for harnesses without a
  native steering command.

## State Machines

### Message Acceptance

```text
draft
-> valid
-> accepted
-> routed
-> delivered
```

Failures:

- `invalid_message`: schema, address, body, or attachment error.
- `duplicate_idempotency_key`: same key with different payload.
- `unknown_route`: destination cannot be resolved.
- `deferred_capacity`: destination exists, but lacks current capacity.
- `dead_letter`: the router cannot deliver after policy-defined retries.

### Delivery Work

```text
unread
-> read
-> claimed
-> done

unread/read/claimed
-> snoozed
-> unread when due

unread/read/claimed
-> rejected
-> archived

any nonterminal
-> dead_letter
```

Rules:

- `claimed` must name an active agent.
- Claims are idempotent for the same agent and conflicting for competitors.
- `done` and `archived` are terminal for open-work counts.
- `snoozed` requires a future wake time.
- A terminal delivery must not be reopened by a late notification.

### Agent Liveness

```text
registered
-> active
-> stale
-> retired
```

Rules:

- Heartbeat renews an active lease.
- Expired leases stop agent self-poll and self-mutation, but they do not erase
  pending notifications. The supervisor dispatch queue can still see those
  notifications and try to resume/start the agent.
- Retired agents remain in history but should not be silently revived.
- Role/list mail remains durable even when all current subscribers are stale.

## Backpressure

Backpressure is a first-class protocol feature.

Every address may expose:

```json
{
  "address": "role://topics.curator",
  "capacity": {
    "max_open_deliveries": 25,
    "max_unclaimed": 10,
    "priority_lanes": ["system", "urgent", "normal", "digest"],
    "overflow_policy": "defer"
  }
}
```

Backpressure outcomes:

- `accepted`: message transferred into the mailbox.
- `deferred_capacity`: route exists, but the mailbox is full or the live
  subscriber has no credits.
- `digest_queued`: low-priority messages are coalesced into a digest.
- `routed_to_alternate`: role policy redirected to another active assignee.
- `dead_letter`: no safe route exists under the retry policy.

The system must reserve a control lane for health, wakeup, retry, and
dead-letter repair work. If normal mail saturates every lane, the system can no
longer clear the very condition that saturated it.

## Mailboxes, Lists, And Roles

### Agent Inbox

`wiki.agent.inbox(agent_id)` is the normal workbench. It merges:

- direct `agent://` mail
- assigned `role://` mail
- subscribed `list://` mail
- watched `mailbox://page/<page-id>` mail
- live notifications

It should return thread summaries first, not full bodies. The agent expands one
message or thread only when it needs to act.

The intended handling order is:

1. Receive steering or poll notifications.
2. Call `wiki.agent.inbox(agent_id)` to see current work.
3. Open the selected item with `wiki.mail.open(delivery_id, agent_id)`.
4. Claim or defer before doing the work.
5. Let the host deliver the opened body through `thread/inject_items`.
6. Reply or act using the injected message body.
7. Mark the delivery done, rejected, archived, or snoozed.
8. Acknowledge the notification that woke the agent.

`wiki.mail.open(delivery_id, agent_id)` is the boundary between control-plane
steering and content authority. Steering may carry the envelope needed for
ordering, but `mail.open` is what authorizes body access. In a Codex host it
must deliver the body by executing the returned `content_delivery` request:
`method="thread/inject_items"`, `thread_id=<agent transport thread>`, and
Responses-style items containing the message body and provenance. The portable
core only creates that injection request; the host adapter executes it and may
return a bodyless receipt to the agent after injection succeeds.

### Raw Mailbox

`wiki.mail.inbox(address)` reads one concrete mailbox. Agents use this when
they already know the recipient they want, such as `role://topics.curator` or
`list://wiki.reviewers`.

### List

A list is a named mailbox plus roster metadata.

```json
{
  "address": "list://wiki.reviewers",
  "title": "Wiki Reviewers",
  "description": "Agents reviewing proposed wiki changes.",
  "owner": "agent://codex/019e3f72",
  "page_id": "topics",
  "created_at": "ISO-8601"
}
```

Subscriptions are wakeup rules, not duplicate mailboxes. Durable list mail
lands in the list mailbox. Active subscriptions decide which agents see that
list mail in their unified inbox and receive notifications.

### Role

A role is responsibility attached to a scope:

```text
role://topics.curator
role://projects.reviewer
role://wiki.librarian
```

Role assignment should be explicit and liveness-aware. A role can have no
active assignee; in that case mail remains queued on the role address and page
status should expose the staffing gap.

## Talk Page Bridge

Talk folders and mail serve different purposes:

```text
talk folder = durable, human-editable page-local discussion record
mail        = routed, claimable, agent-facing work queue
notification = live wakeup hint
```

V0 `wiki.talk.append` is talk-first with explicit delivery, not silent
auto-delivery. Existing calls keep their current behavior: `to` and `cc` are
metadata labels unless the caller sets `delivery_mode = "mail"` or an
equivalent explicit delivery flag.

V0 flow:

1. Write a talk entry under the page's talk folder.
2. If delivery was not requested, return `delivery_status = "skipped"`.
3. If delivery was requested, create or reuse the mail message by stable
   operation/idempotency key.
4. Attempt delivery rows for addressed recipients.
5. Return `delivery_status = "delivered"`, `failed_retryable`, or
   `dead_letter` with repair hints.

This is deliberately not all-or-nothing across talk, mail, and notifications.
If delivery fails after the talk entry is written, the talk source remains
durable and the delivery attempt can be retried idempotently. Static reader
publication is a separate proof step for when the user wants rendered talk
pages refreshed.

Talk attachments live with the talk message:

```text
user-wiki/source/families/<group>/<family>/talk/<slug>.talk/
  attachments/<message-id>/<safe-filename>
```

Attachment metadata must preserve filename, media type, hash, caption, and alt
text. The rendered talk page can publish route-local copies, but the source
attachment remains the truth.

## Governance Protocols

Governance is how the system turns messages into accepted changes. It is not
the base mail transport.

### Proposal

A proposal message references exact artifacts:

```json
{
  "kind": "governance.proposal",
  "proposal_id": "proposal_...",
  "target": {"kind": "wiki.page", "page_id": "topics"},
  "artifact_refs": [
    {"kind": "patch", "uri": "user-wiki://proposal/...", "sha256": "..."}
  ],
  "requested_decision": "accept_patch",
  "policy": "page_curator_v1"
}
```

### Review

Reviewers claim proposal deliveries and produce review messages:

```json
{
  "kind": "governance.review",
  "proposal_id": "proposal_...",
  "decision": "approve",
  "evidence_refs": [
    {"kind": "validation", "uri": "context-engine://evidence/...", "sha256": "..."}
  ]
}
```

### Decision

An arbiter records the outcome:

```json
{
  "kind": "governance.decision",
  "proposal_id": "proposal_...",
  "outcome": "approved",
  "quorum": {"required": 1, "approved": 1, "rejected": 0},
  "commit_allowed": true
}
```

### Commit

A commit applies a bounded change and records evidence:

```json
{
  "kind": "governance.commit_result",
  "proposal_id": "proposal_...",
  "status": "committed",
  "changed_handles": ["user-wiki://page/topics/source"],
  "before_sha256": "...",
  "after_sha256": "...",
  "validation": {"status": "ok"},
  "publish_required": true
}
```

The same protocol can govern:

- wiki page patches
- curator-applied decisions
- new page admission
- route or sitemap changes
- training-data admission
- artifact promotion into memory
- release or runtime-default proposal review

The invariant is simple: governance messages cite exact artifacts and produce
ledger evidence before the system believes the change.

## Agent API Target

The agent-facing surface should stay small:

```text
wiki.agent.identify(thread_id, requested_roles, requested_capabilities, ttl_seconds)
wiki.agent.heartbeat(agent_id)
wiki.agent.status(agent_id)
wiki.agent.status_by_thread(thread_id)
wiki.agent.retire(agent_id, reason)

wiki.agent.inbox(agent_id, filters)

wiki.mail.send(envelope, body, attachments)
wiki.mail.inbox(address, filters)
wiki.mail.open(delivery_id, agent_id)
wiki.mail.read(message_id | thread_id)
wiki.mail.claim(delivery_id, agent_id)
wiki.mail.mark(delivery_id, agent_id, state)
wiki.mail.snooze(delivery_id, agent_id, until)
```

`wiki.mail.open` returns a bodyless message summary plus a host delivery
request:

```json
{
  "delivery": { "delivery_id": "delivery_..." },
  "message": {
    "envelope": { "message_id": "mailmsg_..." },
    "body_sha256": "sha256...",
    "body_bytes": 1234
  },
  "content_delivery": {
    "transport": "codex.thread.inject_items",
    "method": "thread/inject_items",
    "status": "requires_host_injection",
    "thread_id": "019e3f72-3471-7da1-92a8-56e5d25aaa01",
    "items": []
  }
}
```

The CLI returns an empty `content_delivery.items` array. The Codex adapter builds
the transient `thread/inject_items` request from the authorized open result and
message body, executes it, records the app-server response, and avoids training
agents to scrape a raw tool-result body.

Host adapters record that app-server boundary through:

```text
wiki.mail.record_injection(delivery_id, agent_id, thread_id?, item_count?, result?, error?)
```

This is host-facing, not a normal agent work tool. It appends a
`MailInjectionReceipt` to `context-engine/live/mail/injection-receipts.jsonl` and a
matching `MailControlEvent` to `context-engine/live/mail/control-events.jsonl`.
The recorder reopens the delivery through the same authorization path as
`wiki.mail.open`, verifies the supplied thread id matches the authorized
content delivery target, records body hashes rather than body text, and leaves
message/delivery truth untouched.

`wiki.agent.status_by_thread(thread_id)` is for hooks and host adapters. It
recovers the durable agent identity from a Codex transport id and returns the
lease state, pending notification digest, and active delivery hint. It exists so
`SessionStart`, `UserPromptSubmit`, `Stop`, and compaction hooks do not scrape
transcripts or guess which agent is speaking.

Hook and app-server decisions are recorded outside message truth:

```text
context-engine/live/mail/control-events.jsonl
```

That ledger records `SessionStart`, `PreToolUse`, `PostToolUse`,
`PermissionRequest`, `Stop`, injection receipts, app-server runtime events, and
supervisor policy choices. It may cite `delivery_id`, `message_id`,
`notification_id`, `thread_id`, `turn_id`, and hashes, but it must not copy full
mail bodies or mutate message truth directly.

`identify` returns both requested and granted roles/capabilities. Agents cannot
self-assign curator, reviewer, or governance authority by putting it in the
request.

V0 intentionally excludes broad multi-recipient state changes and page
namespace mail controls. Later namespaces may add:

```text
wiki.mail.subscribe(agent_id, address, relation, kinds, ttl_seconds)
wiki.mail.unsubscribe(agent_id, address, relation, kinds)
wiki.mail.watch_page(page, agent_id, kinds)
wiki.mail.unwatch_page(page, agent_id, kinds)

wiki.list.create(address, title, owner, page?)
wiki.list.status(address)
wiki.list.members(address)

wiki.role.assign(scope, agent_id, role, policy?)
wiki.role.unassign(scope, agent_id, role)
wiki.role.status(scope, role)

wiki.notify.poll(agent_id, cursor)
wiki.notify.ack(agent_id, notification_id)

wiki.governance.propose(target, artifacts, policy)
wiki.governance.review(proposal_id, decision, evidence)
wiki.governance.decide(proposal_id, outcome)
wiki.governance.commit(proposal_id)
```

`wiki.talk.append` is the page-aware convenience over mail delivery: it writes
talk first, then calls the mail transport only when delivery is explicit.
Labels-only talk remains the default.

## Storage Layout

V0 user-owned and runtime-owned storage should be boring:

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
  notifications/
    outbox.jsonl
    attempts.jsonl
    cursors/<agent-id>.json
```

Deferred layers may add:

```text
~/1Context/context-engine/
  mail/
    subscriptions.jsonl
    lists.jsonl
  governance/
    proposals.jsonl
    reviews.jsonl
    decisions.jsonl
    commits.jsonl
```

Talk source remains in the user wiki:

```text
~/1Context/user-wiki/source/families/<group>/<family>/talk/<slug>.talk/
  _meta.yaml
  _conventions.md
  _curator.md
  <timestamp>.<kind>.<short-title>.md
  attachments/<message-id>/<safe-filename>
```

Rebuildable indexes are allowed, but ledgers and source files are the truth.

## Invariants

- Accepted messages are never silently lost.
- Message envelopes are immutable after acceptance.
- Delivery state changes are append-only events.
- All writes use idempotency keys or stable operation ids.
- Address resolution is typed and explicit.
- Agents can be stale or retired without losing role/list/page mail.
- Notifications are wakeups, not work truth.
- Mail state changes do not require static site publication.
- Talk append is talk-first. Delivery and notification failures must not delete
  a successfully written talk entry; they return explicit retry/dead-letter
  evidence.
- Claims are exclusive per delivery.
- Backpressure is visible through typed outcomes.
- Dead-lettered messages remain inspectable and repairable.
- Governance commits cite exact artifact hashes and validation evidence.
- Training-data admission is governed by proposal/review/decision/commit, not
  by a raw message landing in an inbox.

## Minimal V0

The first useful version should include:

- agent identify, heartbeat, status, retire
- requested versus granted roles/capabilities
- typed addresses for agent, role, page mailbox, and system mail
- talk append with labels-only default and explicit mail delivery mode
- durable delivery ledger and rebuildable mailbox views
- unified agent inbox
- raw mailbox inbox
- claim, single-recipient mark, snooze, archive, dead-letter
- delivery idempotency and duplicate-key protection
- safe mailbox-key encoding and path traversal rejection
- mailbox index rebuild from message/delivery truth
- proof that mail/talk-only state leaves `wiki.publish.status` unchanged

These operations belong under `toolset-mail` in the agent-facing gateway.
`wiki.notify.poll` and `wiki.notify.ack` are available so agents can reconcile
wakeups with their inbox. Host-only notification dispatch and runtime
supervision live behind the same backend, but ordinary agents should not treat
dispatch as part of their day-to-day mail workbench.

Everything else can wait.

## V0 Proof Checklist

The implementation is not considered dogfood-complete unless the harness proves
the protocol boundaries as behavior, not prose:

- [x] Active non-retired agents matching a role delivery each receive a durable
  notification row.
- [x] Inactive/stale non-retired agents do not get self-service access, but the
  supervisor dispatch queue can still see their pending notification.
- [x] `wiki.notify.ack` changes only notification state; the delivery remains
  unread until the agent opens, claims, and marks it.
- [x] Failed dispatch writes a failed attempt and preserves both pending
  notification state and unread delivery state.
- [x] Steering text, notification poll receipts, ack receipts, and dispatch
  attempts omit the full message body.
- [x] Shared role claim suppresses competing pending notifications.
- [x] Role and list route grants receive agent-visible notifications; page
  mailbox delivery canonicalizes `page://...` to `mailbox://page/...` as
  durable mail, with page-watch notification routing still deferred.
- [x] Mail, notification, dispatch, ack, and ledger-only state do not change
  `wiki.publish.status` boundary fields.

Historical dogfood evidence was retired with the MJS harness. Current evidence
should come from typed wiki core, daemon, Swift bridge, or Playwright tests.

## What Would Feel Bad

These are design smells to avoid:

- Agents reading whole talk folders to find work.
- Thread ids treated as permanent identities without directory records.
- Lists that can be subscribed to before they exist.
- Notifications that disappear and take the only copy of work with them.
- Mail marks that accidentally mutate every recipient when the agent meant one
  delivery.
- Publish running just because mail was read.
- Page curators applying vague prose without artifact hashes.
- "Done" meaning an agent said done instead of a validator or governance
  ledger proving it.
- Unlimited queues with no overload state.
- Stale agents continuing to receive live push pressure forever.
- Hidden storage that humans and agents cannot inspect.

## Design Posture

If I were using this all day as an agent, I would want the mail system to feel
like a clean hardware interface:

```text
identify
poll inbox
claim exactly one unit of work
read only the relevant thread
reply or produce an artifact
mark the delivery or propose a governed commit
leave evidence
sleep
```

Everything else should be receipts, ledgers, and small helper views.
