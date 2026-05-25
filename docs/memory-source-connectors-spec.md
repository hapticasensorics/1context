---
title: 1Context Memory Source Connectors Spec
slug: memory-source-connectors-spec
section: architecture
access: private
summary: "Source connector contract for discovering, reading, and importing app data into the 1Context Perception DB."
status: draft
last_updated: 2026-05-24
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Memory Source Connectors Spec

## 0. Purpose

Source connectors are how real-world app data enters Perception DB.

The core rule:

```text
Every source connector is read-only at the source and append-only at the memory DB.
```

Connectors discover sources, prove what they can read, normalize source records
into Perception DB object inputs, and send those objects to
`onecontext-memory-service`. They do not write directly to TimescaleDB, mutate
source app data, or create source-specific durable storage models.

## 1. Why Connectors Exist

Full Disk Access can make many local app stores readable, but it does not make
them canonical, stable, complete, or safe to scrape blindly.

Source connectors give each integration a visible contract:

```text
what source was found
where it was found
how canonical it is
what permission made it readable
what records were imported
what confidence the import deserves
what source hashes prove the import
```

This lets 1Context add iMessage, Slack, Linear, Discord, browsers, terminal
sessions, and agent logs without turning the product into an invisible disk
crawler.

## 2. Connector Tiers

Connectors should declare their read posture.

```text
stable_local_record
  Local database/log is a durable-ish source of truth.
  Examples: iMessage chat.db, Codex JSONL, Claude JSONL, file watcher journal.

app_cache
  Local files are useful but partial or implementation-specific.
  Examples: Slack/Discord Electron caches, browser IndexedDB caches.

official_api
  Authorized remote API is the preferred source.
  Examples: Linear API, Slack API, calendar APIs.

live_observation
  Source is observed as it happens through screen, AX, browser extension, or shell hook.
  Examples: terminal scrollback capture, browser page context, active window metadata.

manual_import
  User-selected files or exports.
  Examples: exported chat archive, CSV, downloaded project log.
```

The viewer should expose this posture so the user understands whether a lane is
canonical, partial, observed, or API-backed.

## 3. Connector Registry

The product registry is the Perception DB source layer:

```text
perception.sources
perception.source_records
perception.source_cursors
```

These are not timeline tables. They are the setup, dedupe, and cursor layer for
timeline imports. Older `capture.source_connectors`, `capture.source_locations`,
and `capture.source_connector_probes` migrations may remain for existing dev DB
compatibility, but new product reads and writes should target `perception.*`.

`perception.sources` records:

```text
source_key
display_name
source_type
read_posture
access_modes
status
metadata
```

`perception.source_records` records source identity and dedupe:

```text
source_id
source_record_key
source_record_hash
object_id
object_event_start
first_seen_at / last_seen_at
```

`perception.source_cursors` records durable adapter progress:

```text
source_id
cursor_key
cursor_value
advanced_at
metadata
```

The current Rust source-probe structs in
`crates/onecontext-memory-db/src/source_connector.rs` are a compatibility
surface until the Perception DB source registry is fully wired.

## 4. Standard Connector Lifecycle

Every connector follows the same lifecycle:

```text
registered
probed
permission_checked
connected_or_blocked
backfilled
tailed_or_polled
summarized_for_viewer
```

Probe answers:

```text
Is the app/source present?
Which local files/databases/caches exist?
Are they readable from the signed app/service?
What schema/version do they appear to use?
Is the local source canonical, partial, cache-only, or observed?
What permissions or auth are missing?
What stable source ids and timestamps can be extracted?
```

Backfill imports historical records inside an explicit time range. Tail/poll
imports new records after the cursor.

Codex and Claude session ingest is governed by
[Coding Agent Ingest Spec](coding-agent-ingest-spec.md). They reduce into a
shared agent-session IR before writing Perception DB objects.

```text
hot_memory
  default product path
  emits agent_session, agent_turn, agent_message, agent_compaction,
  agent_prompt_snapshot, and salient agent_tool_summary objects

compact_audit
  detail path
  emits the hot path plus compact tool summaries with source refs

forensic
  explicit investigation path
  preserves raw payloads through blobs or source refs
```

The prototype lesson is important: tool traces must remain recoverable, but raw
tool calls and results should not dominate the default lived-memory feed. The
daemon should advance cursors through skipped tool rows, preserve source offsets
and hashes, and let a separate evidence profile emit compact tool summaries
when the viewer or a memory job needs them.

Always-on local connectors should use a cheap loop:

```text
1. watch source roots with filesystem/database-specific notifications where possible
2. periodically reconcile the source root so missed notifications do not matter
3. skip files/databases whose cursor already matches size/rowid
4. read only new JSONL bytes or SQLite rows after the cursor
5. stop each tick at max_events, max_lines, or wall-clock budget
6. persist cursors before sleeping
```

Directory walking thousands of session files is acceptable for a first reconcile
loop, but it is not the final hot path. The Rust daemon should wake from
notifications and use the reconcile pass as a correctness backstop.

## 5. Access Modes

Connectors declare which access mode they need:

```text
full_disk_access
app_login
api_token
browser_extension
accessibility
user_selected_files
```

A connector may support several modes. For example Slack may prefer `api_token`,
fall back to `full_disk_access` cache reads, and be supplemented by
`browser_extension` observation.

## 6. Connector Output

Connectors emit normal Perception DB timeline objects:

```text
imessage_message
imessage_attachment
slack_message
slack_thread_event
discord_message
linear_issue_event
linear_comment
browser_page
browser_event
terminal_command
terminal_output
agent_session
agent_turn
agent_message
agent_tool_summary
agent_compaction
agent_prompt_snapshot
file_version
```

Output rules:

```text
source ids go in payload
source path/URI hashes go in payload or perception.blobs metadata
large attachments become perception.blobs
message text goes in display_text when safe
threads and replies use perception.object_edges where useful
edits/deletes become new objects or correction edges
privacy_class must be set early
```

The service inserts into `perception.objects`; connector code only emits
Perception DB object inputs. Legacy `CaptureEnvelope` naming can remain in code
while the write path migrates, but it is not the product contract.

## 6.1 Lane Rule For Chat Apps

Chat connectors must not create one persisted lane per workspace, server,
channel, DM, group chat, or thread.

Use coarse lanes:

```text
slack.messages
discord.messages
messages.imessage
linear.events
```

Put high-cardinality details in payload and source metadata:

```text
payload.workspace_id
payload.channel_id
payload.guild_id
payload.chat_guid
payload.thread_ts
payload.issue_id
```

The viewer can then project virtual lanes and saved filters:

```text
Slack #launch
Discord design channel
iMessage with Jackie
Linear project Memory DB
```

Those virtual lanes are UI state over Perception DB objects, not a reason to create
hundreds of DB lanes.

## 7. Initial Connector Set

### 7.1 Codex Sessions

```text
connector_key: codex.local_sessions
posture: stable_local_record
access: full_disk_access
default lane: agents.messages
kinds: agent_session, agent_turn, agent_message, agent_tool_summary
```

Primary value: immediate proof path for our own work history. Raw Codex JSONL
rows remain source evidence; the hot path follows
[Coding Agent Ingest Spec](coding-agent-ingest-spec.md), not flat Codex-only
transcripts.

### 7.2 Claude Code Sessions

```text
connector_key: claude.local_sessions
posture: stable_local_record
access: full_disk_access
default lane: agents.messages
kinds: agent_session, agent_turn, agent_message, agent_tool_summary
```

Primary value: cross-agent development history using the same generic agent IR
and Perception DB kinds as Codex.

### 7.3 iMessage

```text
connector_key: imessage.chat_db
posture: stable_local_record
access: full_disk_access
default lane: messages.imessage
kinds: imessage_message, imessage_attachment
```

Primary value: local conversation history with relatively stable timestamps and
threading. It requires very careful consent, privacy labeling, and redaction UX.

### 7.4 Slack

```text
connector_key: slack.desktop_or_api
posture: official_api preferred, app_cache possible
access: api_token, full_disk_access, app_login
default lane: slack.messages
kinds: slack_message, slack_thread_event
```

Primary value: work communication. The official API should be preferred for
completeness when authorized. Local cache reads should be labeled partial unless
proven otherwise.

### 7.5 Linear

```text
connector_key: linear.api
posture: official_api
access: api_token, browser_extension
default lane: linear.events
kinds: linear_issue_event, linear_comment
```

Primary value: structured work state and issue history. API is the clean path;
browser observation is useful for "what was on screen now."

### 7.6 Discord

```text
connector_key: discord.desktop_or_observed
posture: app_cache, live_observation
access: full_disk_access, app_login, accessibility
default lane: discord.messages
kinds: discord_message, discord_channel_event
```

Primary value: conversation context. Local desktop data should be treated as
partial until an adapter proves otherwise.

### 7.7 Terminal

```text
connector_key: terminal.sessions
posture: live_observation
access: accessibility, full_disk_access
default lane: terminal.output
kinds: terminal_command, terminal_output
```

Primary value: command/output evidence. The best future version is shell
integration or terminal-session capture, not just shell history.

## 8. Viewer Surface

The viewer needs a "Sources" panel:

```text
Sources
  Codex        connected     stable local record    2,104 sessions
  Claude Code connected     stable local record    412 sessions
  iMessage    blocked       needs Full Disk Access
  Slack       partial       local cache found, API recommended
  Linear      not connected API recommended
  Discord     partial       desktop cache found
  Terminal    observing     live observation
```

Each source detail page should show:

```text
status
read posture
permissions/auth required
discovered locations
last probe
last import
records imported
oldest/newest event time
warnings
sample imported objects
privacy controls
```

This keeps source quality visible in the product instead of hiding it behind a
green checkmark.

## 9. Source Confidence In Timeline

Timeline objects should be visually distinguishable by source confidence:

```text
canonical/stable local records: normal
official API: normal
partial cache: subtle partial marker
live observation: observation marker
manual import: imported marker
redacted/secret: privacy marker
```

The object inspector should show:

```text
connector_key
read_posture
source_location_id
source_record_id
source_hash
imported_at
```

## 10. Safety Rules

```text
No connector mutates source app files.
No connector imports a protected source silently.
No connector marks app cache as canonical without proof.
No connector stores secrets in payload.
No connector bypasses privacy_class.
No connector writes directly to Timescale.
No connector hides probe diagnostics from the viewer.
```

For sources like iMessage, Slack, Discord, and Linear, the UI should require an
explicit enable action and explain what will be read.

## 11. First Implementation Slice

The first real connector proof should avoid API auth and fragile caches:

```text
Codex local sessions
terminal command/output fixture
file version fixture
```

Done when:

```text
source connector registry has rows
probe rows record what was found
agent-session IR and other connector inputs insert into perception.objects
viewer Sources panel shows connector status
timeline shows imported objects by lane
object inspector shows source connector metadata
```

Then add iMessage as the first Full Disk Access protected local DB connector.
Slack/Linear/Discord should come after the source panel and confidence model are
already visible.
