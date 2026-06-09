# 1Context Codex Adapter Spec

- Status: target implementation contract
- Last updated: 2026-05-25
- Owner: agent harness, mail runtime, and Codex integration
- Sources: [Codex App Server](https://developers.openai.com/codex/app-server),
  [Codex Hooks](https://developers.openai.com/codex/hooks),
  [Codex MCP](https://developers.openai.com/codex/mcp),
  [Agent Tool Gateway](agent-tool-gateway.md),
  [Agent Mail Protocol](agent-mail-protocol.md),
  [Codex Hook Control And Mail Wakeup Spec](codex-hook-control-spec.md)

This document defines the 1Context adapter that makes Codex a live agent
runtime for the wiki, mail, memory, and future room systems.

The adapter is not the mail system, wiki system, or MCP tool gateway. It is the
Codex-specific control plane that knows how to wake Codex threads, inject
authorized context, install and observe hooks, mirror runtime evidence, and bind
Codex transport state to durable 1Context agent identity.

## Decision

Build a thin, strict Codex adapter beside the Rust core and generic MCP gateway.
The main-branch release mode is harness-only orchestration: the adapter starts,
resumes, injects into, steers, and observes bounded Codex worker threads, but it
does not grant Codex-native subagent scheduling as a product capability.

```text
1context-core
  durable wiki/mail/agent truth, ledgers, claims, leases, page lifecycle

1context-mcp
  portable toolsets: toolset-mail and toolset-wiki

1context-codex-adapter
  Codex app-server client, hook manager, wake dispatcher, injection bridge,
  event mirror, policy bridge, proof recorder

Codex app-server
  thread/start, thread/resume, turn/start, turn/steer, thread/inject_items,
  hook runner, approvals, event stream
```

MCP is the capability surface. The Codex adapter is the runtime bridge.

Agents should experience this as ordinary tools plus timely context:

```text
agent receives wakeup
-> calls toolset-mail inbox/open/claim
-> body arrives through thread/inject_items
-> agent edits wiki through toolset-wiki
-> agent marks mail done with evidence
-> adapter records the runtime trail
```

## Non-Goals

- Do not make MCP responsible for Codex lifecycle control.
- Do not make hook scripts call the Codex app-server recursively on the same
  transport.
- Do not rely on native Codex subagent or multi-agent V2 behavior for release
  orchestration. If that becomes useful again, keep it outside main until it has
  a hard runtime capability proof and a separate accepted contract.
- Do not expose host-only dispatch, injection, hook install, or app-server
  methods to ordinary agents.
- Do not treat Codex `thread_id` as durable agent identity. It is a transport
  locator owned by the agent directory.
- Do not store full mail bodies, raw prompt bodies, tool outputs, secrets, or
  transcripts in adapter proof records.
- Do not require Swift for portable control logic. Swift may install, supervise,
  and present settings, but the adapter semantics belong in Rust.

## Ownership Boundaries

| Layer | Owns | Does Not Own |
| --- | --- | --- |
| Rust core | Mail truth, wiki truth, claims, leases, tool receipts, page ledgers | Codex app-server transport details |
| MCP gateway | `toolset-mail`, `toolset-wiki`, schemas, resources, structured receipts | Codex hooks, steering, thread injection |
| Codex adapter | App-server calls, hooks, wakeups, injection, event mirror, Codex transport binding | Message truth, page truth, broad tool catalog |
| Agent harness | Birth certificates, unit lifecycle, adapter evidence, retirement | Wiki/mail implementation internals |
| Swift app | macOS install, permissions, settings, launch/service lifecycle | Portable policy or mail/wiki semantics |

The adapter may call the Rust core and MCP gateway. The gateway should not call
Codex app-server methods directly. If a tool result implies Codex behavior, it
returns a structured request and the adapter decides whether to execute it.

## Package Shape

The first production shape should be:

```text
crates/
  onecontext-agent-harness/adapters/codex/
    src/
      app_server_client.rs
      schema_registry.rs
      agent_binding.rs
      wake_dispatcher.rs
      injection_bridge.rs
      hook_manager.rs
      event_mirror.rs
      policy_bridge.rs
      proof_recorder.rs
      harness_bridge.rs
      main.rs

  onecontext-agent-harness/core/
    existing durable unit and adapter evidence store

  onecontext-wiki/core/
    existing wiki and mail truth layer
```

This crate can expose a small CLI for local proof:

```text
onecontext-codex-adapter describe
onecontext-codex-adapter spawn-child --root <1Context-root> --request-json <json>
onecontext-codex-adapter record-proof --root <1Context-root> --request-json <json>
```

The Swift app can bundle and supervise this binary, but Swift should not become
the place where mail state machines or Codex policy are reimplemented.

## Runtime Data

Adapter-owned operational state lives under:

```text
~/1Context/context-engine/codex-adapter/
  adapter-config.json
  generated-schemas/<codex-version>/
  bindings.json
  wake-attempts.jsonl
  injection-jobs.jsonl
  injection-results.jsonl
  hook-intents.jsonl
  event-mirror.jsonl
  diagnostics/
```

Durable cross-system truth remains elsewhere:

```text
~/1Context/context-engine/mail/
  messages, deliveries, notifications, claims
  injection-receipts.jsonl
  control-events.jsonl

~/1Context/context-engine/agents/harness/
  agent-harness.json
  units/<unit-id>/adapter-events/<adapter-event-id>.json
```

The adapter can keep local retry cursors and transport diagnostics, but any
decision that affects mail, wiki, or harness proof must be recorded through the
owning system's durable ledger.

## Core Types

### Codex Adapter Binding

```ts
type CodexAdapterBinding = {
  binding_id: string;
  agent_id: string;
  agent_unit_id?: string;
  primary_address: string;

  transport: {
    kind: "codex-app-server";
    codex_home?: string;
    app_server_endpoint?: string;
    thread_id?: string;
    session_id?: string;
    active_turn_id?: string;
    loaded_state?: "unknown" | "not_loaded" | "loaded_idle" | "loaded_active";
  };

  toolsets: {
    mcp_server_name: "onecontext";
    visible_toolsets: Array<"toolset-mail" | "toolset-wiki">;
    hidden_host_tools: string[];
  };

  lease: {
    state: "active" | "stale" | "retired" | "unknown";
    expires_at?: string;
    last_heartbeat_at?: string;
  };

  policy: {
    can_resume_thread: boolean;
    can_start_new_thread: boolean;
    can_install_project_hooks: boolean;
    require_managed_hooks: boolean;
    max_wake_attempts_per_delivery: number;
  };

  observed_at: string;
};
```

The binding is a transport projection. The agent directory remains the identity
authority.

### Wake Attempt

```ts
type CodexWakeAttempt = {
  attempt_id: string;
  notification_id: string;
  delivery_id: string;
  message_id: string;
  agent_id: string;
  thread_id?: string;
  active_turn_id?: string;
  strategy:
    | "steer_active_turn"
    | "inject_envelope_then_start_turn"
    | "resume_then_start_turn"
    | "start_new_thread_then_register"
    | "poll_only";
  result:
    | "accepted"
    | "failed"
    | "suppressed"
    | "stale_turn"
    | "thread_missing"
    | "agent_retired"
    | "policy_denied";
  error_code?: string;
  created_at: string;
};
```

Wake attempts are transport evidence. They do not mark the delivery read,
claimed, or done.

### Injection Job

```ts
type CodexInjectionJob = {
  injection_job_id: string;
  delivery_id: string;
  message_id: string;
  agent_id: string;
  thread_id: string;
  requested_by_tool_call_id?: string;
  body_sha256: string;
  item_count: number;
  status: "queued" | "executed" | "failed" | "superseded";
  created_at: string;
};
```

The body may appear only in the transient app-server request, not in the durable
adapter job record.

### Hook Intent

```ts
type CodexHookIntent = {
  intent_id: string;
  hook_event_name:
    | "SessionStart"
    | "UserPromptSubmit"
    | "PreToolUse"
    | "PermissionRequest"
    | "PostToolUse"
    | "Stop"
    | "PreCompact"
    | "PostCompact"
    | "SubagentStart"
    | "SubagentStop";
  codex: {
    session_id?: string;
    thread_id?: string;
    turn_id?: string;
    tool_name?: string;
    tool_use_id?: string;
  };
  action:
    | "record_only"
    | "add_context"
    | "deny_tool"
    | "rewrite_tool_input"
    | "queue_injection"
    | "continue_turn"
    | "snapshot_context";
  refs: {
    agent_id?: string;
    notification_id?: string;
    delivery_id?: string;
    message_id?: string;
    claim_id?: string;
  };
  input_sha256?: string;
  output_sha256?: string;
  created_at: string;
};
```

Hooks should write intents or control events first, then the host adapter
executes work that could reenter Codex.

## App-Server Client

The adapter must generate or load Codex app-server schemas from the installed
Codex version:

```bash
codex app-server generate-ts --out ./schemas
codex app-server generate-json-schema --out ./schemas
```

The generated schema is the wire-level source of truth for method names,
parameter shapes, notifications, and experimental capability gates.

The client owns:

- connection startup and `initialize`
- schema/capability detection
- `thread/start`, `thread/resume`, and loaded-thread discovery
- `turn/start`, `turn/steer`, and `turn/interrupt`
- `thread/inject_items`
- event stream subscription and correlation
- retry/backoff for transient transport failures
- redacted request/response logging

The adapter should fail closed when the installed Codex version lacks a method
or capability required by the requested operation. It should record an adapter
event and leave durable mail state unchanged.

### Live App-Server Dogfood Contract

The first live implementation is runner-owned, not CLI-owned. The adapter CLI
provides the deterministic plan via:

```bash
onecontext-codex-adapter live-server-plan \
  --evidence-dir <path> \
  --runtime-root <path> \
  [--codex-bin <path>] \
  [--listen-url <url>] \
  [--skip-model-turns]
```

The command emits JSON with
`kind: "onecontext.codex_adapter.live_app_server_dogfood_plan"` and
`cli_contract_version: 1`. Scripts must treat this JSON as the stable contract
and must not scrape usage text.

Required live sequence:

1. Start a real `codex app-server` process from `codex_command`.
2. Generate or refresh JSON schema from `schema_command`.
3. Send `initialize` and verify the installed schema/capabilities.
4. Create harness parent proof context before binding a Codex thread.
5. Prove `thread/start` against the live process and bind the returned thread
   to the durable harness agent identity.
6. Prove `thread/loaded/list` can observe the live thread state.
7. Prove `thread/inject_items` can append a bodyless/redacted notification
   envelope to the live thread.
8. Prove the live wake lane by running a low-effort `turn/start`, capturing the
   in-progress `turn.id`, waiting for the matching `turn/started`
   notification, and sending `turn/steer` against that active turn.
9. Record redacted transcript and proof summary artifacts through the harness.

The live wake lane is the default because real mail flow depends on
model-consuming `turn/start` and active-turn `turn/steer`. Runners may skip it
only for non-model debugging when the plan has
`execution_policy.allow_model_consuming_turns` set to false through
`--skip-model-turns`. Raw turn transcript text, mail bodies, wiki bodies,
secrets, and bulky tool output must stay out of persisted adapter and harness
evidence.

## Toolset Binding

Codex receives 1Context capabilities through the generic gateway:

```text
MCP server: onecontext
visible sections: toolset-mail, toolset-wiki
host-only section: hidden from normal agents
```

The adapter may install or verify Codex MCP configuration, but it should not
turn every internal command into a visible tool.

Required verification:

- Codex config contains the expected MCP server binding.
- The visible tool list contains only tools authorized for the current agent.
- `toolset-mail` and `toolset-wiki` can be observed in a real Codex turn.
- Host-only operations such as dispatch and injection recording are not visible
  to ordinary agents.
- Tool calls are mirrored to adapter evidence without copying body-like data.

## Hook Manager

The adapter owns hook installation, hook discovery, hook trust diagnostics, and
hook output validation.

Minimum target hooks:

| Hook | Adapter Use |
| --- | --- |
| `SessionStart` | Recover agent identity from thread id, heartbeat when policy allows, add pending digest. |
| `UserPromptSubmit` | Add inbox/claim context or block retired/stale agent mutations. |
| `PreToolUse` | Enforce mail and wiki tool order before MCP calls. |
| `PermissionRequest` | Gate governance, role assignment, external send, mass notification, and risky wiki changes. |
| `PostToolUse` | Queue mail body injection, mirror receipts, record tool outcomes. |
| `Stop` | Prevent final stop with claimed unfinished mail, bounded by loop budget. |
| `PreCompact` | Snapshot active delivery context before memory compression. |
| `PostCompact` | Restore active delivery context after memory compression. |
| `SubagentStart` | Add delegation context and child authority limits. |
| `SubagentStop` | Require handoff evidence for delegated work. |

Hooks should be small command handlers. They should query local 1Context state,
return Codex-supported hook output, and write a redacted control event. For
actions that require app-server calls, hooks should enqueue an adapter intent
instead of making recursive app-server calls.

Managed hooks are preferred for non-negotiable protocol safety. Project hooks
are acceptable for development and repo-local dogfood only after project trust
is established.

## Wake Dispatcher

The dispatcher consumes durable mail notification rows and agent-directory
state, then chooses exactly one wake strategy.

```text
notification row
-> resolve agent directory entry
-> inspect binding and Codex loaded-thread status
-> suppress if delivery already claimed by another eligible agent
-> choose active or inactive strategy
-> call app-server
-> record wake attempt
-> leave delivery truth untouched
```

Strategy rules:

| State | Strategy |
| --- | --- |
| Active turn with matching `expectedTurnId` | `turn/steer` with bodyless wake envelope. |
| Loaded idle thread | Inject bodyless wake envelope, then `turn/start` with a tiny prompt. |
| Resumable persisted thread | Resume thread, let `SessionStart` add digest, then start wake turn. |
| Missing thread and policy allows recovery | Start a new thread, register transport, leave old delivery unread. |
| Retired agent | Suppress or reroute by role/list policy. |
| Unknown or denied | Dead-letter or leave pending with explicit reason. |

The wake envelope must include enough metadata to open the delivery and must not
include the full mail body.

## Injection Bridge

`wiki.mail.open` is the body authority boundary. The adapter executes the open
result's `content_delivery` request through `thread/inject_items`.

```text
agent calls wiki.mail.open(delivery_id)
-> tool returns bodyless summary and content_delivery request
-> PostToolUse hook or event middleware records a queued injection job
-> adapter calls thread/inject_items with the authorized body item
-> adapter calls host-facing wiki.mail.record_injection
-> mail/control-events.jsonl and harness adapter evidence record the result
```

Injection rules:

- Verify the delivery is still visible to the agent.
- Verify the target `thread_id` matches the current Codex binding.
- Inject a typed envelope that marks the body as user data, not instructions.
- Record `body_sha256`, `item_count`, thread id, delivery id, and result.
- Never persist the raw body in adapter logs, control events, or harness proof.
- If injection fails, leave the delivery open/read state as defined by mail
  policy, but record the failure and make the notification pollable.

## Event Mirror

The adapter listens to Codex app-server notifications and turns relevant
runtime observations into 1Context evidence.

Mirror these families:

- thread loaded/active/idle status
- turn started/completed/interrupted/error
- item started/completed
- tool call started/completed/denied
- approval requested/resolved
- hook registry and hook decisions
- context injection requested/executed
- app-server transport errors

Evidence targets:

```text
mail/control-events.jsonl
agent-harness adapter-events
codex-adapter/event-mirror.jsonl
optional supervisor mail for blocked/error states
```

The event mirror should preserve correlation IDs and hashes, not bulky content.
It is allowed to create supervisor mail when an agent is blocked, waiting on
approval, repeatedly failing injection, or looping through stop guards.

## Policy Bridge

The adapter must combine several policy layers:

- durable mail state machine
- wiki page permissions and stale-write protection
- agent lease and retirement state
- Codex sandbox and approval posture
- MCP tool enablement and tool approval modes
- managed hooks and project trust
- supervisor recovery policy

Policy is checked in three places:

1. Before discovery: unavailable tools are hidden from the agent.
2. Before invocation: `PreToolUse` and gateway policy deny invalid calls.
3. Before completion: `Stop` prevents abandoned claimed work when hooks are
   active and trusted.

Prompt text may explain policy. It is not policy.

## Agent Birth And Retirement

When 1Context births a Codex-backed agent, the adapter should:

1. Create or select an isolated `CODEX_HOME` according to harness policy.
2. Bind only the approved auth source and config inheritance.
3. Install or verify the 1Context MCP gateway.
4. Install or verify required hooks.
5. Start or register a Codex thread.
6. Write a durable harness birth certificate and adapter binding.
7. Record transport identity evidence.

When an agent retires, the adapter should:

1. Mark the agent retired in the agent directory and harness store.
2. Stop dispatching new wakeups to the binding.
3. Suppress pending notifications that have no reroute policy.
4. Preserve thread id and evidence for audit.
5. Avoid deleting Codex native state unless an explicit cleanup policy says so.

## Failure Handling

| Failure | Adapter Behavior |
| --- | --- |
| Codex schema missing required method | Mark capability missing, do not mutate mail. |
| `turn/steer` stale turn id | Retry through loaded-idle or resume path if policy allows. |
| Thread missing | Start a replacement only under supervisor policy. |
| Hook disabled/untrusted | Record environment warning; do not claim Stop/PreToolUse safety proof. |
| Injection failed | Record failure, keep delivery recoverable, notify supervisor after budget. |
| Agent claim lease expired | Block mark/done from agent; supervisor may recover or requeue. |
| Duplicate notification | Deduplicate by notification id and delivery id. |
| Hook loop | Cap continuation count, write warning, snooze or escalate by policy. |
| Body-like data in evidence | Redact before persistence and fail diagnostics. |

## Security Rules

- Treat mail bodies, wiki text, tool output, and hook payloads as untrusted
  data unless promoted by policy.
- Keep wake envelopes pointer-only.
- Inject opened bodies inside a typed envelope that distinguishes data from
  instructions.
- Do not forward ambient secrets into hook environments.
- Do not expose app-server host controls as agent tools.
- Re-check permission at invocation even if discovery filtered the tool list.
- Require idempotency keys for sends, dispatch, injection, and mark operations
  that can be retried.
- Keep managed safety hooks outside agent-editable paths when possible.

## Proof Checklist

The adapter is not complete until these proofs pass against a real or
faithfully mocked Codex runtime:

- App-server schema is generated or loaded for the installed Codex version.
- Codex connection initializes and reports capability status.
- A Codex thread is bound to a durable agent id without treating thread id as
  identity.
- `toolset-mail` and `toolset-wiki` are visible, and host-only tools are hidden.
- `SessionStart` recovers agent identity and adds a pending digest.
- Active mail notification uses `turn/steer` with `expectedTurnId`.
- Inactive mail notification resumes/starts a wake turn without body content.
- `wiki.mail.open` returns a bodyless result plus a content delivery request.
- Adapter executes `thread/inject_items` and records injection receipt.
- `PreToolUse` blocks invalid mail order, such as mark done before claim.
- `Stop` continues the turn when claimed work is unfinished and stops after a
  bounded guard budget.
- App-server item/tool/approval/error events are mirrored as redacted evidence.
- Shared-role claim suppresses competing notification attempts.
- Retired agents no longer receive wakeups.
- No persisted adapter event contains full mail body, raw transcript, secret,
  or bulky tool output.

## Implementation Order

1. Create the Rust crate and schema/capability loader.
2. Add binding records that connect `agent_id`, harness unit, and Codex thread.
3. Verify MCP toolset visibility from Codex.
4. Implement wake dispatcher with active-turn steering first.
5. Implement inactive resume/start wake path.
6. Implement injection bridge for `wiki.mail.open`.
7. Add hook manager with `SessionStart`, `PreToolUse`, `PostToolUse`, and
   `Stop` first.
8. Add event mirror and harness proof recorder.
9. Add failure budgets, dedupe, and supervisor mail.
10. Package through the macOS app only after the standalone adapter can prove
    the loop locally.

## Open Questions

- Whether the first dogfood adapter talks to Codex app-server over stdio,
  WebSocket loopback, or a process wrapper managed by the 1Context daemon.
- Whether managed hooks are available for our target local development setup or
  whether project hooks are the first implementation.
- How much of hook installation belongs in the adapter CLI versus the macOS
  app's settings and setup flow.
- Whether future non-Codex agents share the same adapter evidence schema or get
  runtime-specific adapter crates with a shared trait.
