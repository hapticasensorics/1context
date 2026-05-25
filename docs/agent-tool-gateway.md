# 1Context Agent Tool Gateway

- Status: target contract for generic agent-facing tools
- Last updated: 2026-05-25

This document defines how 1Context exposes wiki and mail capabilities to
agents. The gateway may be implemented as an MCP server, direct dynamic tools,
daemon RPC, or a local SDK wrapper, but the agent-facing contract is the same:

```text
one consolidated backend
two visible toolsets
host-only hook dispatch
small typed tool lists
structured receipts
host-enforced permissions
```

The goal is a generic system any capable agent can use without being born into a
special room, reading repo internals, or memorizing filesystem layouts.

## Decision

Expose two first-class toolsets:

```text
toolset-mail
toolset-wiki
```

Do not add narrower public names or role-specific toolsets yet. Role and
permission policy can filter which tools a given agent sees, but the contract
vocabulary stays at these two toolset names until the surface proves it needs
another top-level split.

## Gateway Versus Toolset

The gateway is the host implementation. It can call Rust directly, call the
Swift daemon, wrap the installed CLI, or later expose MCP.

A toolset is the visible subset the agent receives for the current session:

- `toolset-mail` contains agent identity, inbox, delivery, claim, mark,
  notification, and mail-send/reply operations.
- `toolset-wiki` contains page lifecycle, assets, talk append, validation,
  publish status, publish, and reference listing.

The gateway must never expose every internal command just because it exists.
Large tool catalogs lower selection quality and make agents waste context on
tools they should not need.

## Hooks And Autonomy

Toolsets are the pull/action plane. Hooks are the push/wakeup plane.

```text
agent uses tools -> inspect, claim, edit, publish, mark
mail creates hooks -> wake or steer recipients
host manages hooks -> route, retry, suppress, retire, escalate
```

Mail notifications are hooks. They may become Codex steering commands, app-server
turn nudges, local supervisor wakeups, or another runtime-specific wake
transport. They are not message truth, and they should not be treated as normal
agent tools.

Any agent can use mail to talk to any other routable agent, role, list, page
mailbox, or system address. The agent-facing act is `wiki.mail.send` or
`wiki.mail.reply`; the notification hook is the delivery side effect that wakes
eligible active recipients and can resume/start inactive recipients when the
supervisor owns a valid transport handle. This keeps conversation durable while
still giving running agents an immediate nudge.

The autonomous system needs both:

- tools so an agent can act without path guessing
- hooks so mail can wake agents when work appears
- leases so agent self-service is bounded while supervisor wakeup stays
  explicit
- retries so transient hook failures do not lose work
- suppression so a shared-role delivery stops waking competitors after one
  agent claims it
- dead-letter/escalation so blocked work becomes explicit evidence rather than
  silent drift

Normal agents may see `wiki.notify.poll` and `wiki.notify.ack` inside
`toolset-mail` because those are mailbox-facing operations. Host-only hook
dispatch remains outside ordinary toolsets unless the caller is a supervisor.

For Codex, the canonical hook payload is a steering message that says which
delivery exists and which tools to call next. It must not include the full mail
body as authority; the agent must call `wiki.mail.open` before acting. The
Codex host then delivers the authorized body by executing the open result's
`content_delivery` request with `thread/inject_items`.
The implementation boundary for those Codex app-server calls, hook management,
and proof events lives in [1Context Codex Adapter Spec](1context-codex-adapter-spec.md).
The detailed hook mapping and mail correctness rules live in
[Codex Hook Control And Mail Wakeup Spec](codex-hook-control-spec.md).

The no-human loop is:

```text
agent sends mail or system accepts mail
-> delivery row created
-> notification row created for matching non-retired agents
-> hook dispatcher steers active agents or resumes/starts inactive agents
-> agent calls toolset-mail to open, claim, and act
-> agent optionally calls toolset-wiki
-> agent marks delivery and acknowledges notification
-> supervisor retries, suppresses, or escalates unfinished work
```

## Generic Agent Contract

An agent should be able to start with only:

```text
available_toolsets: ["toolset-mail", "toolset-wiki"]
```

From there it can list or receive the concrete tools in each set. Tool
descriptions, parameter schemas, and receipts must teach usage directly.

Every tool description should answer:

- what the tool does
- when to call it
- what state it mutates
- what the receipt means
- what to do after success
- what to do after a typed failure

The tool schema should not ask the agent to provide facts the host already
knows. For example, if a session is already bound to an `agent_id`, the host can
inject it or validate it instead of making the model reconstruct it from prose.

## Toolset Mail

`toolset-mail` is for operational coordination. It is not a wiki renderer
trigger and it is not the source of page truth.

Initial agent-visible operations:

| Tool | Purpose |
| --- | --- |
| `wiki.agent.identify` | Register or refresh an agent identity and lease. |
| `wiki.agent.heartbeat` | Extend a live lease. |
| `wiki.agent.status` | Inspect identity, granted roles, capabilities, and lease state. |
| `wiki.agent.retire` | Retire an agent identity. |
| `wiki.agent.inbox` | List actionable delivery envelopes for one agent. |
| `wiki.mail.open` | Authorize one delivery and deliver its body through host injection. |
| `wiki.mail.claim` | Claim one delivery before doing work. |
| `wiki.mail.mark` | Mark one claimed delivery read, done, archived, or rejected. |
| `wiki.mail.snooze` | Defer one delivery until a later time. |
| `wiki.notify.poll` | Read pending notification hints for this agent. |
| `wiki.notify.ack` | Acknowledge a notification hint after it has been handled. |

Target operations that belong in this toolset when implemented:

| Tool | Purpose |
| --- | --- |
| `wiki.mail.send` | Send a durable message with idempotency, recipients, body, and artifacts. |
| `wiki.mail.reply` | Reply to an existing message or delivery thread. |

Host-only operations such as notification dispatch to Codex steering should
stay out of the normal agent-visible toolset unless the caller is a supervisor.

## Toolset Wiki

`toolset-wiki` is for user wiki source, rendered site publication, and durable
page-local context.

Initial agent-visible operations:

| Tool | Purpose |
| --- | --- |
| `wiki.status` | Inspect compact whole-system wiki status. |
| `wiki.list` | List configured, source-backed, generated, missing, and tombstoned pages. |
| `wiki.validate` | Validate wiki structure without publishing. |
| `wiki.page.status` | Inspect one page's state, hashes, flags, and next action. |
| `wiki.page.open` | Return editable source, handles, talk state, assets, and hashes. |
| `wiki.page.create` | Create a configured page, source file, talk folder, and ledger event. |
| `wiki.page.write_body` | Replace a page body with stale-write protection. |
| `wiki.page.patch_body` | Patch one exact body fragment with stale-write protection. |
| `wiki.asset.add` | Add a page-local file or image and return insertion markdown. |
| `wiki.asset.list` | List page-local assets. |
| `wiki.reference.list` | List citeable published resources and code blocks. |
| `wiki.page.delete` | Tombstone a page and reserve the route. |
| `wiki.page.restore` | Restore a tombstoned page. |
| `wiki.talk.append` | Append durable page-talk context. |
| `wiki.publish.status` | Explain whether publish is needed and why. |
| `wiki.publish` | Validate, render, promote, mirror, and return publish evidence. |

This is close to the maximum size a wiki editing agent should see up front. If
the surface grows, the gateway should use deferred loading or a list/search
operation rather than expanding the initially visible set.

## Teaching Without Prompt Bloat

The generic teaching stack is:

```text
toolset description
  -> tool names
  -> tool descriptions
  -> JSON schemas
  -> structured receipts with next actions
  -> optional recipe prompts
```

Recipes are allowed, but they are not authority. A recipe can say "process your
inbox" or "edit and publish a page." The host-enforced tool schema decides what
the agent can actually do.

Receipts should be self-orienting:

```json
{
  "status": "ok",
  "operation": "wiki.page.patch_body",
  "next_action": "wiki.publish",
  "allowed_actions": ["wiki.page.open", "wiki.publish.status", "wiki.publish"],
  "warnings": []
}
```

Typed failures should also teach recovery:

```json
{
  "status": "failed",
  "operation": "wiki.page.patch_body",
  "error": {
    "code": "source_hash_mismatch",
    "message": "The page changed after it was opened."
  },
  "repair_hints": [
    "Call wiki.page.open again and retry with the fresh source hash."
  ]
}
```

## Permission And Filtering

The backend may be consolidated, but authority is still filtered per caller.

Examples:

- A read-only agent can receive `toolset-wiki` with edit and publish tools
  filtered out.
- A live agent can receive `toolset-mail` without host-only notification
  dispatch.
- A supervisor can receive both toolsets plus host-only tools in a separate
  control surface.
- A future Weird Dreams room can pin the same two toolsets in its birth
  certificate without inventing room-specific tool names.

Filtering must be enforced by the host. Tool descriptions can explain policy,
but prose is not a security boundary.

## Implementation Notes

The first implementation should be boring:

1. Build one gateway inventory that can return `toolset-mail` and
   `toolset-wiki`.
2. Keep tool schemas strict and receipts structured.
3. Keep each toolset independently listable.
4. Make toolset selection explicit in the agent/session config.
5. Do not expose internal CLI-only helpers unless they are promoted into one of
   the two public toolsets.
6. Add dogfood cases where agents receive only one toolset, then both.

MCP is a strong candidate for the generic transport because it gives external
agents a normal discovery and call surface. Direct dynamic tools are also fine
for runtimes that support them. The 1Context contract is the two toolsets and
the receipts, not the transport.

## Gateway Proof Checklist

The current dogfood harness treats the gateway split as an acceptance boundary:

- `toolset-mail` operations prove identity, inbox, open, claim, mark,
  notification poll, notification ack, and supervisor dispatch without requiring
  wiki publication.
- `toolset-wiki` publication remains isolated: mail, notification, dispatch,
  and ack operations must leave `wiki.publish.status` boundary fields
  unchanged.
- Control-plane receipts from `wiki.notify.poll`, `wiki.notify.ack`, and
  `wiki.notify.dispatch` must not include full mail bodies; body delivery is
  authorized only by `wiki.mail.open` and performed through
  `thread/inject_items`.
- `wiki.mail.open` receipts expose a bodyless message summary plus
  `content_delivery`; the Codex host adapter, not the agent, executes that
  injection and records a receipt through the host-facing
  `wiki.mail.record_injection` boundary.
- Hook/control behavior writes `mail/control-events.jsonl` records, so
  app-server wakeups, policy denials, injection receipts, and stop guards are
  auditable without becoming message truth.
- Host-only dispatch is exercised through the harness, but it remains outside
  ordinary agent-facing toolsets.
- Role and list route grants are agent-visible; page-mailbox delivery is
  durable, while page-watch notification routing remains deferred.

Evidence: `scripts/test-agent-mail-dogfood.mjs` and
`test-results/agent-mail-dogfood-20260525T063840Z/summary.json`.
