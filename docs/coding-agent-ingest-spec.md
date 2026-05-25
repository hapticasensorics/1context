# Coding Agent Ingest Spec

Status: draft implementation contract

Primary implementation target: `crates/onecontext-memory-db`

Related docs:

- [Memory DB API And Protocol Spec](memory-db-api-protocol-spec.md)
- [Memory Source Connectors Spec](memory-source-connectors-spec.md)
- [Memory DB Infra And Viewer Spec](memory-db-infra-viewer-spec.md)

This document defines how 1Context should ingest coding-agent sessions such as
Codex and Claude Code into Perception DB.

The central rule:

```text
Do not ingest coding-agent logs as flat chat transcripts.
Ingest them as reduced sessions with typed projections.
```

Codex rollouts, Claude logs, and future 1Context-native agent logs should be
compiled into one shared agent-session intermediate representation, then
materialized as Perception DB timeline objects.

---

## 1. Thesis

Coding-agent logs are not one stream. They braid together:

```text
session metadata
turn boundaries
model-visible messages
compaction events
tool calls and tool results
runtime events
UI/audit messages
prompt construction facts
output items recorded back into history
```

Perception DB should preserve that structure.

The useful mental model:

```text
raw rollout/log = fossil record
agent session IR = normalized reduced session
prompt snapshot = what the model likely saw
perception objects = time-aligned evidence
memory = later selected meaning
```

The Codex resume path is especially important:

```text
rollout.jsonl
  -> parse RolloutItem
  -> reduce into ConversationHistory
  -> history.for_prompt(...)
  -> Prompt { input, tools, base_instructions, personality, schema }
  -> model request
```

Codex does not send rollout JSONL to the model. It reduces the rollout into
internal conversation history, then builds a fresh prompt. 1Context should mirror
that distinction when ingesting sessions.

---

## 2. Non-Goals

V0 should not:

```text
store every raw JSONL line as a hot timeline row
turn every tool stdout byte into display text
create one physical table per agent product
create one persisted lane per session/thread/project
pretend UI events are model-visible prompt input
rewrite raw history when compaction happens
require full forensic fidelity for default memory ingest
```

Raw evidence can be referenced through blob descriptors or raw source offsets.
The hot Perception DB path should contain compact, queryable, source-linked
objects.

---

## 3. Source Classes

### 3.1 Codex Existing Logs

Codex local logs are existing external evidence. They can be verbose because
they serve Codex's own resume/debug needs.

V0 source:

```text
~/.codex/sessions/**/rollout-*.jsonl
```

Important native shapes:

```text
SessionMeta
ResponseItem
Compacted
TurnContext
EventMsg
```

### 3.2 Claude Existing Logs

Claude Code local logs are also external evidence. Their exact shape differs,
but they should reduce into the same agent session IR.

V0 source:

```text
~/.claude/projects/**/*.jsonl
```

Important native shapes:

```text
message
tool_use
tool_result
progress/runtime rows
project/session metadata inferred from path and payload
```

### 3.3 1Context-Native Agent Logs

1Context's own future logs do not need to copy Codex's verbosity.

They should write a compact reduced format directly:

```text
session metadata
turn boundaries
user messages
assistant messages
compaction summaries
prompt snapshot metadata
selected tool summaries
evidence refs
```

Default native logs should skip full tool payloads. If raw tool input/output is
needed, store it as cold evidence behind a blob descriptor or source ref.

---

## 4. Ingest Profiles

Coding-agent ingest must be profile-driven.

### 4.1 `hot_memory`

Default product path.

Emits:

```text
agent_session
agent_turn
agent_message
agent_compaction
agent_prompt_snapshot
agent_tool_summary only when salient
```

Skips:

```text
full tool calls
full tool results
streaming deltas
progress spam
debug-only events
large stdout/stderr
```

### 4.2 `compact_audit`

Detailed viewer/debug path.

Emits everything in `hot_memory`, plus compact tool call/result summaries:

```text
tool name
command or request preview
exit code/status
duration
stdout/stderr byte counts
content hash
raw evidence ref if available
```

### 4.3 `forensic`

Explicit investigation path.

Emits compact objects and preserves raw payloads through blob descriptors or
raw source references. Forensic mode is not the default always-on ingest path.

---

## 5. Unified Agent Session IR

All coding-agent sources should reduce into this shared representation before
writing Perception DB objects.

```ts
type AgentSource = "codex" | "claude" | "onecontext_agent";

type AgentSessionIr = {
  source: AgentSource;
  source_id: string;
  session_id: string;
  session_key: string;
  source_uri: string;
  cwd?: string;
  project_key?: string;
  model?: string;
  started_at?: string;
  ended_at?: string;
  metadata: object;
  turns: AgentTurnIr[];
  session_items: AgentItemIr[];
  compactions: AgentCompactionIr[];
  prompt_snapshots: AgentPromptSnapshotIr[];
  runtime_events: AgentRuntimeEventIr[];
};

type AgentTurnIr = {
  turn_id: string;
  turn_index: number;
  event_start: string;
  event_end: string;
  user_goal?: string;
  status?: "completed" | "interrupted" | "failed" | "unknown";
  item_ids: string[];
  prompt_snapshot_id?: string;
  metadata: object;
};

type AgentItemIr = {
  item_id: string;
  source_record_key: string;
  event_start: string;
  event_end: string;
  role: "user" | "assistant" | "system" | "tool" | "runtime";
  kind:
    | "message"
    | "tool_call"
    | "tool_result"
    | "reasoning"
    | "patch"
    | "file_change"
    | "runtime_event";
  projections: AgentProjection[];
  text?: string;
  compact_text?: string;
  payload: object;
  raw_ref?: RawEvidenceRef;
  metadata: object;
};

type AgentProjection =
  | "model_visible"
  | "ui_timeline"
  | "audit"
  | "prompt_snapshot"
  | "memory_candidate"
  | "forensic";

type AgentCompactionIr = {
  compaction_id: string;
  source_record_key: string;
  event_start: string;
  event_end: string;
  compaction_epoch: number;
  replacement_item_ids: string[];
  replaced_item_ids?: string[];
  summary_text?: string;
  replacement_history_hash: string;
  raw_ref?: RawEvidenceRef;
  metadata: object;
};

type AgentPromptSnapshotIr = {
  prompt_snapshot_id: string;
  source_record_key: string;
  turn_id: string;
  event_start: string;
  event_end: string;
  compaction_epoch: number;
  input_item_ids: string[];
  input_item_count: number;
  tool_count: number;
  base_instructions_hash?: string;
  dynamic_tools_hash?: string;
  prompt_hash?: string;
  token_estimate?: number;
  metadata: object;
};

type AgentRuntimeEventIr = {
  event_id: string;
  source_record_key: string;
  event_start: string;
  event_end: string;
  event_kind: string;
  severity?: "debug" | "info" | "warning" | "error";
  compact_text?: string;
  payload: object;
  raw_ref?: RawEvidenceRef;
};

type RawEvidenceRef = {
  source_uri: string;
  byte_offset?: number;
  byte_len?: number;
  line_number?: number;
  sha256?: string;
};
```

IR rules:

```text
1. Every IR object must have stable source identity.
2. Every emitted timeline object must have event_start and event_end.
3. Every object must declare projections.
4. Tool payloads are compacted unless the ingest profile is forensic.
5. Compaction mutates projections, not raw history.
6. Prompt snapshots store hashes, counts, and item refs before full prompt text.
```

---

## 6. Reducer Pipeline

The coding-agent adapter should have these phases:

```text
discover sources
  -> parse raw records
  -> group by session
  -> reduce to AgentSessionIr
  -> build projections
  -> emit PerceptionObjectInput records
  -> memory.writeObjects
  -> advance source cursor only after durable success
```

### 6.1 Parse

Parsing only turns native rows into typed raw events. It should not decide final
memory value.

Codex raw kinds map roughly as:

```text
SessionMeta  -> raw_session_meta
ResponseItem -> raw_response_item
Compacted    -> raw_compaction
TurnContext  -> raw_turn_context
EventMsg     -> raw_event_msg
```

Claude raw kinds map roughly as:

```text
message      -> raw_message
tool_use     -> raw_tool_call
tool_result  -> raw_tool_result
progress     -> raw_runtime_event
```

### 6.2 Group

Group by stable session identity:

```text
Codex: first SessionMeta id, else rollout UUID tail, else path fingerprint
Claude: explicit session id when present, else project path + file fingerprint
1Context: native session id
```

The session key must survive file moves whenever the source has stable IDs.

### 6.3 Reduce

Reducer output should distinguish:

```text
model-visible history
UI timeline
audit/runtime events
turn contexts
compaction epochs
prompt snapshot candidates
```

Codex-style reducer shape:

```ts
for (const item of rolloutItems) {
  switch (item.kind) {
    case "SessionMeta":
      seedSessionConfig(item);
      break;
    case "ResponseItem":
      appendModelVisibleItem(item);
      appendUiTimelineItem(item);
      break;
    case "Compacted":
      replaceModelVisibleHistory(item.replacementHistory);
      recordCompactionMarker(item);
      break;
    case "TurnContext":
      appendTurnContext(item);
      break;
    case "EventMsg":
      appendAuditEvent(item);
      break;
  }
}
```

### 6.4 Build Projections

The same raw event can appear in different projections:

```text
model_visible: item can contribute to future prompt input
ui_timeline: item should render in a transcript-like viewer
audit: item explains runtime behavior but is not prompt input
prompt_snapshot: item was part of a specific prompt snapshot
memory_candidate: item is eligible for later memory formation
forensic: item is available only in explicit detail views
```

Projection membership belongs in `payload.projections` and in edges. It should
not create hundreds of persisted lanes.

---

## 7. Perception DB Object Mapping

Use coarse persisted lanes:

```text
agents.sessions
agents.turns
agents.messages
agents.compactions
agents.prompts
agents.tools
agents.events
```

Source-specific viewer labels can be virtual:

```text
Codex messages
Claude tools
Codex prompt snapshots
```

Recommended object kinds:

```text
agent_session
agent_turn
agent_message
agent_tool_summary
agent_tool_call
agent_tool_result
agent_compaction
agent_prompt_snapshot
agent_runtime_event
agent_file_change
agent_patch
```

For V0 source-specific kinds may remain:

```text
codex_message
claude_message
codex_tool_call
claude_tool_call
```

but the payload should include the unified fields so the viewer and memory
compiler can treat them generically.

### 7.1 `agent_session`

One per coding-agent session.

Payload:

```json
{
  "agent_source": "codex",
  "session_id": "019e...",
  "session_key": "codex:019e...",
  "cwd": "/repo",
  "project_key": "repo-or-project",
  "model": "gpt-5",
  "source_uri": "/Users/.../rollout.jsonl",
  "schema": "agent_session_ir.v1"
}
```

### 7.2 `agent_turn`

One per user turn when turn boundaries can be inferred.

Payload:

```json
{
  "agent_source": "codex",
  "session_id": "019e...",
  "turn_id": "019e.../turn/12",
  "turn_index": 12,
  "status": "completed",
  "item_count": 9,
  "prompt_snapshot_id": "..."
}
```

### 7.3 `agent_message`

User and assistant messages.

Payload:

```json
{
  "agent_source": "codex",
  "session_id": "019e...",
  "turn_id": "019e.../turn/12",
  "role": "assistant",
  "item_id": "response-item-id",
  "projections": ["model_visible", "ui_timeline", "memory_candidate"],
  "compaction_epoch": 2
}
```

`display_text` should contain the compact message text when safe. Very large
message bodies should be truncated in display text and referenced through
payload/raw evidence.

### 7.4 `agent_tool_summary`

Default representation for tools.

Payload:

```json
{
  "agent_source": "codex",
  "session_id": "019e...",
  "turn_id": "019e.../turn/12",
  "tool_name": "shell",
  "action_preview": "cargo test -p onecontext-memory-db",
  "exit_code": 0,
  "duration_ms": 18322,
  "stdout_bytes": 120934,
  "stderr_bytes": 91,
  "content_sha256": "hex",
  "raw_blob_id": null,
  "projections": ["audit"],
  "salience": "verification"
}
```

Full tool request/result rows are only emitted in `forensic` mode or when a
specific policy marks the tool use as memory-relevant.

### 7.5 `agent_compaction`

Compaction is a first-class object, not just another assistant message.

Payload:

```json
{
  "agent_source": "codex",
  "session_id": "019e...",
  "compaction_epoch": 3,
  "replacement_item_count": 14,
  "replaced_item_count": 420,
  "replacement_history_hash": "hex",
  "summary_text": "optional compact summary"
}
```

Edges:

```text
agent_compaction derived_from replaced agent_message objects
agent_compaction contains replacement agent_message objects
agent_prompt_snapshot references agent_compaction
```

Raw message objects remain valid. Compaction changes the model-visible
projection, not historical truth.

### 7.6 `agent_prompt_snapshot`

Prompt snapshots answer:

```text
What did the agent likely know when it acted?
```

Payload:

```json
{
  "agent_source": "codex",
  "session_id": "019e...",
  "turn_id": "019e.../turn/12",
  "compaction_epoch": 3,
  "input_item_count": 38,
  "tool_count": 12,
  "base_instructions_hash": "hex",
  "dynamic_tools_hash": "hex",
  "prompt_hash": "hex",
  "token_estimate": 18300,
  "input_object_ids": ["uuid", "uuid"]
}
```

Edges:

```text
agent_prompt_snapshot references model-visible input objects
agent_turn references agent_prompt_snapshot
```

Do not store full prompt text by default.

---

## 8. Source Identity

The write key remains:

```text
(source_id, source_record_key)
```

Recommended keys:

```text
session:
  agent/{source}/{session_id}

turn:
  agent/{source}/{session_id}/turn/{turn_index}

message:
  agent/{source}/{session_id}/item/{item_id}
  agent/{source}/{session_id}/line/{line_number}/{source_hash}

tool summary:
  agent/{source}/{session_id}/tool/{tool_call_id}/summary

compaction:
  agent/{source}/{session_id}/compaction/{compaction_epoch}/{hash}

prompt snapshot:
  agent/{source}/{session_id}/turn/{turn_index}/prompt/{hash}
```

Avoid source keys that depend only on byte offsets when stable item IDs exist.
Byte offsets are good evidence references, not always good semantic identity.

---

## 9. Raw Evidence And Blobs

Every emitted object should have enough raw evidence to support drill-down:

```json
{
  "raw_ref": {
    "source_uri": "/Users/.../rollout.jsonl",
    "byte_offset": 123456,
    "byte_len": 982,
    "line_number": 991,
    "sha256": "hex"
  }
}
```

Default policy:

```text
hot row:
  compact semantic event

cold blob:
  raw large payloads and tool outputs when retained

discard:
  low-value progress/debug noise after cursor-safe accounting
```

The adapter must advance cursors across skipped rows. Skipping a tool result in
`hot_memory` mode is not a parse failure.

---

## 10. Cursor And Backfill Rules

Coding-agent ingest must support:

```text
probe
sample
backfill
tail
```

### 10.1 Probe

Report available source roots, readability, file counts, likely schemas, and
permission failures.

### 10.2 Sample

Read a bounded number of sessions/objects without advancing durable cursors.

### 10.3 Backfill

Historical import over a bounded horizon. Backfill may ignore per-tick event
caps when explicitly requested, but should honor a wall-clock stop budget.

### 10.4 Tail

Always-on incremental ingest.

Rules:

```text
1. Track file fingerprints, sizes, mtimes, byte offsets, and parser state.
2. Track SQLite rowids or source-native sequence numbers where applicable.
3. Read only new bytes/rows after the cursor.
4. Stop each tick at max_events, max_lines, or wall-clock budget.
5. Persist cursor only after DB success or explicit durable audit fallback.
6. Reconcile source roots periodically so missed file notifications do not matter.
```

---

## 11. Viewer Requirements

The viewer should expose projections, not just lanes.

Suggested modes:

```text
Model Visible
UI Timeline
Prompt Snapshots
Tools
Audit
Forensic
```

Default session view should show:

```text
session header
turn list
user/assistant messages
compaction markers
selected tool summaries
prompt snapshot affordance
show raw evidence action
```

The viewer must make the difference clear between:

```text
what happened
what the UI showed
what the model likely saw
what later memory formation selected
```

---

## 12. Implementation Plan

### Phase 1: IR Types And Reducer

Add a Rust module:

```text
crates/onecontext-memory-db/src/agent_session_ir.rs
```

Responsibilities:

```text
define AgentSessionIr and child DTOs
define AgentProjection
define AgentIngestProfile
define validation
provide deterministic source key helpers
```

### Phase 2: Codex Compiler

Refactor Codex parsing in `local_adapters.rs` into:

```text
parse_codex_rollout_records
reduce_codex_session
emit_agent_session_objects
```

Keep the current message-only behavior as the V0 default profile, but route it
through the IR.

### Phase 3: Claude Compiler

Refactor Claude parsing into the same IR:

```text
parse_claude_records
reduce_claude_session
emit_agent_session_objects
```

Claude does not need to mimic Codex raw shapes. It only needs to produce the
same IR.

### Phase 4: Perception Object Emission

Map IR to `PerceptionObjectInput`:

```text
agent_session -> one object
agent_turn -> one object per turn
agent_message -> one object per message
agent_tool_summary -> compact audit object when enabled
agent_compaction -> one object per compaction
agent_prompt_snapshot -> one object per prompt epoch/turn when computable
```

Insert edges in the same `memory.writeObjects` call where possible.

### Phase 5: Viewer Projection Support

Teach the local viewer to filter by:

```text
payload.agent_source
payload.session_id
payload.turn_id
payload.projections
kind
```

Add a prompt snapshot detail view that shows hashes, counts, model-visible
object refs, and source evidence.

---

## 13. Acceptance Tests

### 13.1 Codex Compaction

Fixture:

```text
SessionMeta
ResponseItem x 5
Compacted with replacement history x 2
ResponseItem x 2
```

Expected:

```text
raw messages remain queryable
agent_compaction object exists
model-visible projection after compaction points at replacement history + suffix
prompt snapshot references replacement objects, not the entire old prefix
```

### 13.2 Tool Skipping

Fixture:

```text
user message
assistant tool call
large tool result
assistant final message
```

Expected in `hot_memory`:

```text
user and assistant messages emitted
full tool payload skipped
cursor advances through skipped tool rows
raw_ref/source hash retained for emitted rows
```

Expected in `compact_audit`:

```text
compact agent_tool_summary emitted
full tool stdout not in display_text
byte counts and hashes present
```

### 13.3 Claude Normalization

Fixture:

```text
Claude message with tool_use/tool_result content blocks
```

Expected:

```text
same AgentSessionIr shape as Codex
same generic agent_message and agent_tool_summary output kinds
source-specific fields preserved in payload.metadata
```

### 13.4 Source Identity Retry

Fixture:

```text
run same backfill twice
```

Expected:

```text
second run returns inserted=false receipts
no duplicate perception.objects rows
same object_id for same source_record_key
```

### 13.5 Prompt Snapshot

Fixture:

```text
session with two turns and one compaction
```

Expected:

```text
agent_prompt_snapshot exists for each computable prompt epoch
input_item_count matches reduced model-visible history
edges point to model-visible input objects
full prompt text is absent unless forensic mode enabled
```

---

## 14. Design Slogans

```text
Parse native logs.
Reduce into one agent session IR.
Emit compact timeline objects.
Keep raw evidence reachable.
Treat compaction as projection mutation.
Treat prompt snapshots as auditable context.
Skip tool noise by default.
```

That gives 1Context one shared ingest model for Codex, Claude, and our own
future agents without copying Codex's log size or flattening away Codex's useful
resume semantics.
