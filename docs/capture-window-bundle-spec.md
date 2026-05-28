---
title: 1Context Capture Window Bundle Spec
slug: capture-window-bundle-spec
section: architecture
access: private
summary: "V0 capture-input bundle contract for handing maximal time-aligned evidence to the attention-filter agent, with V1 and future work marked explicitly."
status: draft
last_updated: 2026-05-25
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Capture Window Bundle Spec

## 0. Purpose

The capture window bundle is the short-lived input artifact produced by capture
infrastructure and consumed by the attention-filter agent.

It is not durable product memory. It is not a screenshot gallery. It is not an
attention-filter output folder. It is not the memory database.

The V0 contract is:

```text
capture lanes
  -> harvest every mandatory source as far as possible
  -> append normalized evidence into a capture bundle
  -> mark the bundle READY
  -> attention filter consumes the bundle
  -> attention filter writes selected durable output to memory DB
  -> bundle expires unless pinned for debugging, replay, or failure audit
```

The boundary is intentionally sharp:

```text
capture bundle:
  truthful, time-aligned, replayable input evidence

attention filter:
  judgment, selection, composites, explanations, memory-write decisions

memory DB:
  durable selected product memory
```

The bundle may contain cheap mechanical summaries, such as dirty-rect area,
scroll burst totals, source quality, or inferred browser/editor/terminal hints.
It must not contain attention-filter results, memory write receipts, summaries,
or final keep/drop judgments.

## 0.1 Existing Proven Inputs

The V0 bundle writer is still an integration layer, but the upstream capture
inputs are not speculative. Current repo surfaces already prove:

```text
capture.status and capture.snapshot:
  installed-app runtime status, capture paths, available methods, active app,
  displays, windows, focused context

capture store JSONL:
  canonical event envelope, durability class, privacy class/shape, source clock,
  lane/stream/source ids

AX capture:
  focused context, visible region graph, semantic deltas, read issues,
  redaction/value shape

UX capture:
  scroll/pointer/keyboard/shortcut/focus anchors, target process hints, tap
  health, bounded queues, no raw typed text

ScreenCaptureKit metadata:
  active-window frame status, dirty rect math, motion features, UX fusion,
  adaptive stream decisions

browser extension:
  URL/title/selection/visible text/DOM excerpt/viewport/scroll/screenshot
  artifacts plus signed app and extension proof

memory-source connectors:
  connector identity, read posture, source hashes, source record keys, optional
  perception/object ids
```

The bundle should package these proven inputs without pretending every direct
connector is already perfect.

## 1. Contract Levels

This file uses three labels:

```text
V0 required:
  the file or lane must exist in every READY bundle

V0 mandatory lane:
  capture must attempt the lane using the best available source stack; the lane
  may be degraded, but it cannot be omitted

V1:
  expected soon; reserve names and shape now, but do not block V0 READY

Future:
  useful later; do not implement or validate as part of V0
```

Mandatory does not mean perfect. It means we try hard and tell the truth.

For example, `browser.events.jsonl` is mandatory in V0. If the browser extension
is not installed, the file still exists and the lane still records what capture
can infer from window metadata, Accessibility, Automation, app names, titles,
and visible UI state. The source inventory then marks the lane as `degraded`,
not absent.

## 2. V0 Design Goals

V0 should make the attention filter information-rich without drowning it in
frames:

```text
harvest every granted permission
prefer structured semantics over pixels
use pixels as evidence, not the memory primitive
write mandatory lane files even when degraded
make missing capability explicit
preserve source capability, proof, and health metadata
keep all paths relative
make all records time-bounded
store media sparsely
discard bundles after consumption unless pinned
```

The first useful bundle should answer:

```text
What windows existed?
Which window/app was active?
What UX and AX events happened?
What ScreenCaptureKit frame metadata changed?
What capture capabilities, proofs, and source health states were active?
What browser/page context can be inferred or directly captured?
What terminal/session context can be inferred or directly captured?
What editor/file context can be inferred or directly captured?
What source-envelope or local connector records overlap this window?
What evidence files, if any, prove the interesting parts?
Which lanes were degraded, unavailable, or permission-limited?
```

V0 should avoid:

```text
continuous high-FPS video
hundreds of near-duplicate screenshots
raw keystroke text
pretending a missing connector means no useful context exists
blocking READY because a mandatory lane is degraded
mixing attention-filter output into the capture bundle
```

## 3. Storage Location

Bundles live in app-owned Application Support storage:

```text
~/Library/Application Support/1Context/capture/bundles/
~/Library/Application Support/1Context Dev/capture/bundles/
```

Recommended permissions:

```text
directories: 0700
files:       0600
```

All bundle-internal references must be relative to the bundle root.

Allowed:

```text
media/event-frames/frame-000042.jpg
events/browser.events.jsonl
```

Forbidden:

```text
/Users/paul/Desktop/frame-000042.jpg
../../secrets.txt
```

The bundle is files-first. SQLite or the memory DB may index bundle metadata,
but the handoff artifact must remain inspectable, copyable, hashable, and
replayable as a folder.

## 3.1 Rust Contract Status

The current Rust capture-bundle layer owns reusable V0 file-contract behavior;
the Swift daemon owns production export orchestration.

Crate boundaries:

```text
crates/onecontext-capture-core:
  owns reusable bundle file-contract code:
  - CaptureRootPaths for Application Support capture roots
  - V0 required file and mandatory lane constants
  - manifest/source/known-gap/validation structs
  - BundleRelativePath safety checks
  - AtomicBundleWriter processing/<id>.partial -> live/<id>
  - raw JSONL spool time-window reads
  - READY bundle validation
  - retention inventory, sweep planning, apply, and sweeps.jsonl audit append

crates/onecontext-capture-bundler:
  owns operator/debug CLI commands around bundle folders:
  - describe
  - list
  - validate
  - sweep
  - export command that refuses to act as the production READY writer

crates/onecontext-capture-dashboard:
  owns live GUI/debug visibility and bundle-contract tests. It should read the
  daemon and bundle folders; it must not become a high-rate capture source.
```

The Swift daemon remains the V0 production exporter owner because it owns live
sensor state, permission proof, capture.status, capture.snapshot, source health,
and native macOS APIs. Rust can validate and manipulate the file contract, but
it should not invent permission truth outside the daemon.

## 3.2 Application Support Capture Root

Production and dev capture roots:

```text
~/Library/Application Support/1Context/capture/
~/Library/Application Support/1Context Dev/capture/
```

The Rust `CaptureRootPaths` contract expects and creates:

```text
capture/
  events/
  windows/
  displays/
  media/
  bundles/
    processing/
    live/
    failed/
    pinned/
  retention/
    sweeps.jsonl
```

The bundle writer writes only under `bundles/processing/<capture_id>.partial/`
until promotion. The live spool remains separate under `events/`, `windows/`,
`displays/`, and `media/`.

Production permissions remain:

```text
directories: 0700
files:       0600
```

All bundle paths in manifests and refs must be relative to the bundle root.

## 4. Live Spool And Bundle Export

The bundle is the normalized attention-filter input. It is not necessarily the
native live write layout.

The current capture system already writes a live spool under Application Support:

```text
capture/
  events/
    <day>.events.jsonl
  windows/
    <day>.windows.jsonl
  displays/
  media/
    display/
    windows/
    keyframes/
```

The bundle exporter reads the live spool plus current status/proof APIs and
produces the normalized bundle layout:

```text
live spool daily JSONL + status/proof APIs + browser/native-host artifacts
  -> time-window query
  -> split by lane
  -> attach source/proof/capability metadata
  -> write capture/bundles/processing/<capture_id>.partial/
  -> promote to capture/bundles/live/<capture_id>/
```

The spec therefore defines the **export contract** consumed by the attention
filter. It does not require the live capture writer to use this exact directory
layout internally.

`manifest.json` should preserve source-spool provenance so a bundle can be
audited back to the raw capture logs:

```json
{
  "source_spool": {
    "kind": "onecontext_capture_spool",
    "events": ["capture/events/2026-05-25.events.jsonl"],
    "windows": ["capture/windows/2026-05-25.windows.jsonl"],
    "browser_extension_captures": [
      "browser-extension-captures/20260525-191212"
    ]
  }
}
```

Existing proof surfaces before bundle assembly:

```text
capture.status:
  daemon/runtime capabilities, paths, permissions, sampler state, AX/UX health

capture.snapshot:
  current window graph, active app, displays, focused context

capture.active_window_metadata_sample:
  active-window ScreenCaptureKit metadata sample with adaptive capture facts

capture.ux.status / capture.ux.probe:
  input event tap lifecycle, event mask, queue/drop/coalesce counters

browser-extension native host:
  tab/page context, DOM/visible-text artifacts, screenshot evidence, signed
  app and extension proof

capture dashboard:
  tails live JSONL and merges window/SCK/UX/AX events into a time-aligned debug
  timeline
```

V0 bundle assembly is the integration layer that packages these proven inputs
for the attention filter. The presence of this spec does not imply that every
required bundle file is already written by the live capture spool.

## 4.1 Export Mapping

The exporter should map current raw sources into normalized bundle files:

| Raw source / event | Normalized bundle destination |
| ------------------ | ----------------------------- |
| `capture/windows/<day>.windows.jsonl` `capture.window_snapshot` | `events/windows.jsonl` |
| `CaptureSnapshot.displays` from window snapshots | `events/displays.jsonl` |
| `capture/events/<day>.events.jsonl` mixed events | `events/capture.events.jsonl` |
| `capture.ax_focused_context` | `events/ax.events.jsonl` |
| `capture.ax_semantic.*.v1` | `events/ax.events.jsonl` |
| `capture.ux.*.v1` | `events/ux.events.jsonl` |
| `capture.active_window_frame_metadata` | `events/sck-frame-metadata.events.jsonl` |
| `capture.status` RPC output | `capabilities/capture.status.json` |
| permission-derived metadata from `capture.status` / diagnostics | `capabilities/permissions.json` |
| `capture.ux.status` / event-tap status | `capabilities/ux-event-tap.json` |
| active-window metadata sample/session summaries | `capabilities/samplers.json` |
| browser-extension native-host proof and artifacts | `capabilities/browser-extension-proof.json`, `events/browser.events.jsonl`, `media/media.index.jsonl` |
| memory-source connector records overlapping the bundle window | `external_refs/source-envelopes.jsonl` |

The exporter may duplicate an event into a lane-specific file and the aggregate
`events/capture.events.jsonl` index when that helps replay/debugging.

Every normalized event that came from a raw spool record should preserve raw
record provenance when available:

```text
raw_source_uri
raw_line_number
raw_byte_offset
raw_record_hash
exporter_mapping
exporter_version
```

Those fields make replay and bundle bugs debuggable without making the bundle
the durable memory store.

## 5. Directory Classes And Manifest State

Directory class and processing state are separate.

Directory classes:

```text
capture/bundles/
  processing/   bundles currently being written
  live/         recent READY bundles, auto-expired after consumption
  failed/       bundles preserved because capture/export failed
  pinned/       developer/user-kept replay fixtures
```

Manifest states:

```text
partial:
  writer is still producing files

ready:
  bundle is complete and safe for attention-filter reads

failed:
  capture bundle production failed

expired:
  bundle was deleted by retention policy
```

Attention-filter claim/consume state is external to the bundle. Do not mutate a
capture bundle into `claimed`, `filtered`, or `memory_written`. Those are
downstream processing states and belong in the attention/runtime queue or memory
DB audit trail.

A READY bundle normally lives under `live/<capture_id>/` with
`manifest.state = "ready"`. The directory name `live` is not itself a manifest
state.

Recommended retention:

| Directory class | Retention                                           |
| ---------------- | --------------------------------------------------- |
| `processing`     | until completed, failed, or swept as stale          |
| `live`           | 10 to 60 minutes, or last N bundles                 |
| `failed`         | 24 to 72 hours                                      |
| `pinned`         | until explicit deletion                             |

Pinned bundles are fixtures. They are not durable product memory.

## 6. Atomic Write Protocol

Writers must never expose partial bundles as READY.

V0 write flow:

```text
1. create capture/bundles/processing/<capture_id>.partial/
2. write manifest.json with state = "partial"
3. write required V0 files
4. fsync or durable-close files where practical
5. write READY sentinel
6. update manifest.json to state = "ready"
7. atomically rename directory to capture/bundles/live/<capture_id>/
```

READY sentinel:

```text
READY
```

The READY file contains no schema. Its presence means the bundle directory has
been promoted after required files were written.

If capture fails before READY, the writer should move the partial bundle to
`failed/<capture_id>/` and write `quality/failure.json`.

## 7. Consumer Claim Protocol

The attention filter may claim a READY bundle, but the claim is not part of the
capture bundle.

Recommended external claim path:

```text
~/Library/Application Support/1Context/capture/attention-claims/<capture_id>.json
~/Library/Application Support/1Context Dev/capture/attention-claims/<capture_id>.json
```

Claim shape:

```json
{
  "schema_version": 1,
  "capture_id": "cap_20260525T191212Z_000042",
  "claimed_at": "2026-05-25T19:12:17.123Z",
  "worker_id": "attention-filter-dev-01",
  "algorithm_version": "attention-ledger.v0"
}
```

The claim file should be created with exclusive-create semantics. If it already
exists, another worker owns the bundle.

Attention output should be written to the memory DB and its own audit/log
surface, not into the capture bundle.

## 7.1 What Is Explicitly Not In The Bundle

The capture bundle is input evidence. It must not contain:

```text
attention-filter output folders
attention scores
keep/drop decisions
semantic summaries
composite documents
agent-facing memory packets
Timescale write receipts
memory DB row ids as proof of write success
claim/ack state
post-filter overlays
```

The Rust validator currently rejects obvious attention-output names such as:

```text
attention-filter-output
memory-write
memory_written
keep-drop
composites/
decisions.jsonl
```

Future validators should expand that denylist as the downstream attention
runtime gets concrete names. The rule is semantic, not just filename-based:
capture may preserve source provenance and cheap mechanical facts, but judgment
and memory-write results belong downstream.

## 8. V0 Bundle Layout

V0 required layout:

```text
<capture_id>/
  manifest.json
  READY
  sources.json
  time_alignment.json
  capabilities/
    capture.status.json
    permissions.json
    ux-event-tap.json
    samplers.json
    browser-extension-proof.json
  quality/
    known_gaps.jsonl
  events/
    windows.jsonl
    displays.jsonl
    capture.events.jsonl
    ax.events.jsonl
    ux.events.jsonl
    sck-frame-metadata.events.jsonl
    browser.events.jsonl
    terminal.events.jsonl
    editor.events.jsonl
  media/
    media.index.jsonl
    frames-2fps/
  external_refs/
    source-envelopes.jsonl
```

V0 optional layout:

```text
<capture_id>/
  media/
    event-frames/
    debug/
    thumbs/
  replay/
    replay-manifest.json
```

V1 reserved layout:

```text
<capture_id>/
  events/
    ocr.events.jsonl
  derived/
    text-flow/
    scroll-mosaics/
  media/
    strips/
  redaction/
    policy.json
    regions.jsonl
```

Future reserved layout:

```text
<capture_id>/
  events/
    filesystem.events.jsonl
    git.events.jsonl
    lsp.events.jsonl
    network.events.jsonl
    notifications.events.jsonl
    audio.events.jsonl
```

Future lanes are intentionally not V0 requirements.

## 8.1 Lifecycle Runbook

V0 production lifecycle:

```text
1. Resolve the app identity's Application Support capture root.
2. Query live spool records and current status/proof surfaces for the requested
   time window.
3. Create bundles/processing/<capture_id>.partial/.
4. Write manifest.json with state="partial".
5. Write every V0 required JSON/JSONL file, even for degraded lanes.
6. Write sources.json entries for every mandatory V0 lane.
7. Write quality/known_gaps.jsonl for degraded or unavailable mandatory lanes.
8. Write capabilities/*.json from daemon proof/status surfaces.
9. Write READY only after all required files parse and relative paths validate.
10. Update manifest.json to state="ready" with ready_at, expires_at,
    byte_count, file_count, lane_count, and known_gap_count.
11. Atomically rename to bundles/live/<capture_id>/.
12. Make the READY bundle visible to the attention-filter queue.
```

Reader rule:

```text
Ignore processing/*.partial directories.
Read only live/<capture_id>/ directories where READY exists and
manifest.state == "ready".
```

Failure rule:

```text
If export fails before READY, move the partial directory to failed/<capture_id>/
when it contains useful audit material, or delete it when empty. Record the
reason in quality/failure.json if the bundle directory is preserved.
```

Pinning rule:

```text
Pinned bundles are replay/debug fixtures. Set manifest.pinned=true and
retention_class="pinned_debug", or move the bundle under pinned/. Pinned bundles
are exempt from TTL sweeps until explicit deletion.
```

The current Rust constants in `required_bundle_files()` use exactly the V0
required file list above, with `manifest.json` and `READY` validated as required
bundle top-level files. Empty JSONL files are allowed for degraded lanes except
`events/windows.jsonl`, which must contain at least one window snapshot before a
bundle can be considered valid READY input.

## 8.2 Dashboard And CLI Usage

Dashboard:

```bash
1context capture dashboard
```

The Rust dashboard is for live debugging. It reads daemon status/snapshot output,
tails recent capture spool files, shows lane health and recent events, and can
request low-rate previews. It should not be used as the production exporter or
as a high-rate capture source.

Rust bundle CLI:

```bash
cargo run -p onecontext-capture-bundler -- describe

CAPTURE_ROOT="$HOME/Library/Application Support/1Context Dev/capture"
cargo run -p onecontext-capture-bundler -- list --capture-root "$CAPTURE_ROOT" --class all
cargo run -p onecontext-capture-bundler -- validate --capture-root "$CAPTURE_ROOT" --capture-id <capture_id> --strict
cargo run -p onecontext-capture-bundler -- sweep --capture-root "$CAPTURE_ROOT"
```

The `sweep` command is dry-run by default. Add `--apply` only after reviewing
the candidate list.

Current export surfaces:

```text
onecontext_capture_core::export_ready_bundle:
  test/fixture helper over existing JSONL spool files

onecontext-capture-bundler export:
  refuses to write production READY bundles

future 1contextd capture.bundle.export:
  production V0 export owner
```

Fixture generator status:

```text
No standalone capture-bundle fixture-generator command exists yet.
Current fixtures are Rust tests that seed temp spool JSONL and call
onecontext_capture_core::export_ready_bundle.
```

## 9. Manifest

`manifest.json` is required.

V0 manifest shape:

```json
{
  "schema_version": 1,
  "contract_version": "capture-window-bundle.v0",
  "capture_id": "cap_20260525T191212Z_000042",
  "state": "ready",
  "created_at": "2026-05-25T19:12:12.000Z",
  "ready_at": "2026-05-25T19:13:12.000Z",
  "time_range": {
    "start": "2026-05-25T19:12:12.000Z",
    "end": "2026-05-25T19:13:12.000Z"
  },
  "duration_ms": 60000,
  "app_identity": {
    "name": "1Context Dev",
    "bundle_id": "com.haptica.1context.dev",
    "build_channel": "dev",
    "build_id": "20260525-191000"
  },
  "producer": {
    "name": "1contextd",
    "version": "0.0.0-dev"
  },
  "source_spool": {
    "kind": "onecontext_capture_spool",
    "events": ["capture/events/2026-05-25.events.jsonl"],
    "windows": ["capture/windows/2026-05-25.windows.jsonl"],
    "browser_extension_captures": [
      "browser-extension-captures/20260525-191212"
    ]
  },
  "v0_required_files": [
    "sources.json",
    "time_alignment.json",
    "capabilities/capture.status.json",
    "capabilities/permissions.json",
    "capabilities/ux-event-tap.json",
    "capabilities/samplers.json",
    "capabilities/browser-extension-proof.json",
    "quality/known_gaps.jsonl",
    "events/windows.jsonl",
    "events/displays.jsonl",
    "events/capture.events.jsonl",
    "events/ax.events.jsonl",
    "events/ux.events.jsonl",
    "events/sck-frame-metadata.events.jsonl",
    "events/browser.events.jsonl",
    "events/terminal.events.jsonl",
    "events/editor.events.jsonl",
    "media/media.index.jsonl",
    "media/frames-2fps/",
    "external_refs/source-envelopes.jsonl"
  ],
  "optional_files": [
    "media/event-frames/",
    "media/debug/",
    "media/thumbs/",
    "replay/replay-manifest.json"
  ]
}
```

The manifest is the bundle table of contents. It should not duplicate every
event payload.

## 10. Source Inventory

`sources.json` is required.

Each mandatory V0 lane must have a source inventory entry. Missing or degraded
capability does not remove the lane; it changes the lane status.

Example:

```json
{
  "schema_version": 1,
  "sources": [
    {
      "source_id": "windows",
      "lane_id": "capture.windows",
      "status": "present",
      "required_for_v0": true,
      "permission": "screen_recording",
      "source_stack": ["coregraphics", "screencapturekit", "appkit"],
      "record_count": 60
    },
    {
      "source_id": "browser",
      "lane_id": "capture.browser",
      "status": "degraded",
      "required_for_v0": true,
      "permission": "browser_extension",
      "source_stack": ["accessibility", "window_metadata", "automation"],
      "record_count": 8,
      "degraded_reason": "browser_extension_not_installed"
    },
    {
      "source_id": "terminal",
      "lane_id": "capture.terminal",
      "status": "degraded",
      "required_for_v0": true,
      "permission": "accessibility",
      "source_stack": ["accessibility", "window_metadata"],
      "record_count": 3,
      "degraded_reason": "pty_integration_unavailable"
    },
    {
      "source_id": "editor",
      "lane_id": "capture.editor",
      "status": "degraded",
      "required_for_v0": true,
      "permission": "accessibility",
      "source_stack": ["accessibility", "window_metadata"],
      "record_count": 5,
      "degraded_reason": "editor_extension_unavailable"
    }
  ]
}
```

Allowed V0 source statuses:

```text
present
degraded
permission_denied
source_unavailable
disabled_by_policy
```

`not_implemented` is not a valid status for a V0 mandatory lane. If a direct
connector is not implemented, capture must still write a degraded lane using the
secondary source stack.

Any non-present source should also emit a known-gap record.

## 11. Capabilities And Proofs

`capabilities/` is required.

This namespace records the capture system's operational truth for the bundle
time window. These files are capture input, not attention output.

Required files:

```text
capabilities/capture.status.json
capabilities/permissions.json
capabilities/ux-event-tap.json
capabilities/samplers.json
capabilities/browser-extension-proof.json
```

`capabilities/capture.status.json` should preserve the daemon status surface:

```text
available capture RPC methods
capture root/events/windows/media paths
continuous sampler status
motion-hint fusion status
permission-derived metadata
AX semantic status
UX event tap status
latest active-window metadata status
```

`capabilities/permissions.json` should preserve:

```text
privacy guarantees:
  raw_keystrokes_included
  raw_text_included
  coordinates_included
  aggregates_and_counts_only

process identities:
  daemon/app/helper roles
  pid
  executable path
  bundle identifier
  app version
  designated requirement hash when available

permission signals:
  screen/system audio recording
  accessibility
  input monitoring
  browser extension
microphone
automation
full disk access
system audio

proof summaries:
  proof key
  recorded
  matches current subject
  method
  proved_at
  details
```

`capabilities/ux-event-tap.json` should preserve:

```text
tap lifecycle state
tap owner/process identity
tap options
event mask
observed event count
queue depth
dropped count
coalesced count
last event time
recent target process id
recent scroll/keyboard/focus hints
```

`capabilities/samplers.json` should preserve:

```text
active-window metadata stream status
stream configuration snapshots
frame counts by status
persisted event count
persist errors
UX-hint fusion count
adaptive decision count
configuration update count
configuration update errors
```

`capabilities/browser-extension-proof.json` should exist even when degraded. If
the Chrome extension/native host proof is present, it should preserve:

```text
extension id
extension version
extension name
granted extension permissions
granted host permissions
native messaging host name
signed app subject
designated requirement hash
proved_at
browser-extension capture artifact paths
```

If browser proof is unavailable, the file should record a degraded status and a
known-gap code.

Audio permissions are V0 capability inputs even though `audio.events.jsonl` is
Future. If microphone or system-audio permission is granted but no audio lane is
harvested for the bundle, write a known gap:

```text
source_id: audio
code: audio_lane_not_harvested
blocks_ready: false
```

## 12. Time Alignment

`time_alignment.json` is required.

V0 uses RFC3339 timestamps as the canonical join surface because current capture
lanes already emit them. Monotonic nanoseconds are preferred when available, but
they are optional in V0.

Example:

```json
{
  "schema_version": 1,
  "canonical_clock": "system_utc",
  "time_range": {
    "start": "2026-05-25T19:12:12.000Z",
    "end": "2026-05-25T19:13:12.000Z"
  },
  "join_keys": [
    "event_time_start",
    "event_time_end",
    "recordedAt",
    "monotonic_ns"
  ],
  "clock_sources": [
    {
      "source_clock": "system_utc",
      "status": "primary"
    },
    {
      "source_clock": "screen_capture_kit",
      "status": "present",
      "native_fields": ["displayTime"]
    },
    {
      "source_clock": "cg_event_tap",
      "status": "present"
    },
    {
      "source_clock": "accessibility_api",
      "status": "present"
    }
  ],
  "monotonic_ns": {
    "status": "optional_v0",
    "note": "Required in V1 once all capture lanes share a monotonic clock base."
  }
}
```

V1 should promote `monotonic_ns` to required for sources that can provide it.

When records come from source connectors, browser fallbacks, AX inference, or
spool export, V0 should preserve time quality metadata when available:

```text
time_semantics:
  instant
  interval
  observed_at
  inferred_range

temporal_level:
  event
  snapshot
  sample
  aggregate

time_resolution_ns
time_uncertainty_ns
alignment_method
alignment_confidence
```

High-confidence ScreenCaptureKit frame times, CGEventTap times, AX snapshot
times, browser extension timestamps, and connector-derived times should remain
distinguishable. The attention filter should not have to guess which timestamp
was precise and which was inferred.

The bundle should also preserve the current source/group IDs emitted by the live
spool:

```text
capture_id:
  exported bundle id, for example cap_20260525T191212Z_000042

source_span_id:
  source-group id already emitted by current events. In current Swift envelopes
  this may still appear as capture_bundle_id. Examples:
  window-capture:<timestamp>
  active-window:<bundle>:<window>:<stream>
  ux-anchor-batch:<start>:<end>:<count>
  ax-semantic-batch:<start>:<end>:<count>
```

The exported `capture_id` groups the handoff bundle. Existing source-group IDs
remain useful provenance and should not be discarded. New bundle writers should
prefer the name `source_span_id` for these values and reserve
`capture_bundle_id` for the exported bundle id.

## 13. Event Envelope

All V0 JSONL event files should use the same envelope shape.

Envelope:

```json
{
  "schemaVersion": 1,
  "eventType": "capture.ux.scroll_burst.v1",
  "durability": "best_effort",
  "recordedAt": "2026-05-25T19:12:14.200Z",
  "event_time_start": "2026-05-25T19:12:14.000Z",
  "event_time_end": "2026-05-25T19:12:14.200Z",
  "ingested_at": "2026-05-25T19:12:14.230Z",
  "lane_id": "capture.ux",
  "stream_id": "ux.scroll_burst",
  "source_record_id": "ux:scroll_burst:2026-05-25T19:12:14.000Z:2026-05-25T19:12:14.200Z:0",
  "source_hash": "sha256:optional",
  "capture_bundle_id": "cap_20260525T191212Z_000042",
  "source_span_id": "ux-anchor-batch:2026-05-25T19:12:14.000Z:2026-05-25T19:12:14.200Z:1",
  "privacy_class": "interaction_metadata",
  "privacy_shape": "ux_anchor",
  "source_clock": "cg_event_tap",
  "payload": {}
}
```

Required envelope fields for V0:

```text
schemaVersion
eventType
durability
recordedAt
lane_id
payload
```

Strongly recommended fields:

```text
event_time_start
event_time_end
ingested_at
stream_id
source_record_id
capture_bundle_id
privacy_class
privacy_shape
source_clock
```

Optional V0 fields:

```text
source_hash
monotonic_ns
```

Durability classes are current capture semantics and must be preserved:

```text
lossless:
  identity, focus, source-state, and other events the filter must not silently
  lose

best_effort:
  high-rate frame metadata, UX anchors, thumbnails, and hints that can be
  dropped or sampled under pressure
```

## 14. V0 Required Event Files

### 14.1 Window Graph

`events/windows.jsonl` is required.

It should contain one or more `capture.window_snapshot` events covering the
bundle time range.

Each window record should include:

```text
windowID
appPID
appName
bundleID
title
framePoints
framePixels, if known
displayID, if known
zRank
layer
alpha, if known
isFocused
isOnScreen
isMinimized
captureEligible
source
focusMetadata, if known
```

Each `capture.window_snapshot` payload should also preserve snapshot-level
fields:

```text
generatedAt
activeApplication
displays
windows
focusedContext, when available
```

Focus provenance is first-class capture input. `focusMetadata` should preserve:

```text
source
status
confidence
matchedWindowID
matchSignals
```

### 14.2 Display Graph

`events/displays.jsonl` is required.

It should contain display state extracted from window snapshots or direct display
sampling:

```text
displayID
framePoints
scaleFactor
isMain
source snapshot id
```

### 14.3 Unified Capture Events

`events/capture.events.jsonl` is required.

This file may be an aggregate index stream. It should not be the only place
rich lane data lives; lane-specific files are required below.

Allowed event types include all V0 lane event types.

### 14.4 Accessibility Events

`events/ax.events.jsonl` is required.

Minimum V0 source stack:

```text
Accessibility focused window
Accessibility focused element
Accessibility selected text/range when available
Accessibility value shape
Accessibility visible region summaries when available
Accessibility transient UI state
AX read issues
```

AX focused-context snapshots should preserve:

```text
status
isProcessTrusted
activeApplication
focusedApplicationProcessID
focusedWindow
focusedElement
visibleContext
matchedWindowID
issues
```

AX visible-region summaries should preserve:

```text
focusedWindowRegionID
stable region IDs
depth
role/subrole
title shape
frame
value shape
visible range
selected text range
insertion point state
scroll context
child count
captured child count
children truncated
sensitivity flag
redaction reasons
element-under-pointer hint
```

AX redaction/value-shape fields are source facts. They should preserve:

```text
selectedTextCharacterCount
selectedTextTruncated
selectedTextRedacted
valueShape.kind
valueShape.characterCount
valueShape.sourceAttribute
valueShape.redacted
title/value visible text shape
```

Expected event types:

```text
capture.ax_focused_context
capture.ax_semantic.focused_window_changed.v1
capture.ax_semantic.focused_element_changed.v1
capture.ax_semantic.value_changed.v1
capture.ax_semantic.selected_text_changed.v1
capture.ax_semantic.transient_ui_state_changed.v1
```

AX semantic deltas should preserve bounded-buffer and dedupe provenance when
available:

```text
buffer capacity
event count
dropped count
emitted count
deduped unchanged state
transient UI close events with visible=false
```

### 14.5 UX/Input Events

`events/ux.events.jsonl` is required.

Minimum V0 source stack:

```text
CGEventTap or NSEvent global monitor
scroll bursts
pointer actions
keyboard activity cadence
shortcuts
modifier changes
focus transitions
target process hints
motion hints
event-tap health
```

Raw typed text must not be stored. Keyboard events are cadence, modifier, and
shortcut summaries.

Expected event types:

```text
capture.ux.scroll_burst.v1
capture.ux.pointer.v1
capture.ux.modifiers.v1
capture.ux.keyboard_activity.v1
capture.ux.shortcut.v1
capture.ux.focus_transition.v1
```

UX event and health records should preserve:

```text
scroll burst:
  event count
  total dx/dy
  max abs dy
  momentum event count
  duration

pointer:
  action
  button
  event count
  duration
  distance
  dominant axis
  click count

keyboard activity:
  event count
  key down/up counts
  auto-repeat count
  modified-key event count
  duration

shortcut:
  modifier combinations
  action categories

focus transition:
  trigger
  previous/current target hints

event tap:
  active/lifecycle state
  event mask
  queue depth
  dropped/coalesced counts
  last event time
```

### 14.6 ScreenCaptureKit Frame Metadata

`events/sck-frame-metadata.events.jsonl` is required.

This file normalizes the current `capture.active_window_frame_metadata` event
type and `capture.active_window_frames` lane. Preserve the original event type,
lane id, stream id, source record id, source clock, and source/group
`capture_bundle_id`.

Minimum V0 source stack:

```text
active-window ScreenCaptureKit stream metadata
frame status
display time
content rect
content scale
scale factor
dirty rect summary
motion features
adaptive decision hints
parse warnings
```

The lane records metadata first. It does not require retaining the underlying
frame image.

The dirty-rect summary should preserve:

```text
dirtyRectCount
dirtyAreaRatio
changedTileRatio
unionRect
cappedRects
cappedRectLimit
malformedRectCount
weightedCenterY
estimatedDY
```

Frame status should preserve:

```text
frameStatus
frameStatusRawValue
attachmentsPresent
feedsMotionClassifier
parseWarnings
```

Adaptive decisions should preserve:

```text
classifierMode
controllerMode
proposedTargetFPS
targetFPS
previousTargetFPS
targetAnalysisFPS
minimumFrameIntervalSeconds
shouldUpdateStreamConfiguration
updateReason
shouldStoreKeyframe
shouldOCRDirtyRegions
shouldEncodeVideoSegment
dirtyRectCount
dirtyAreaRatio
changedTileRatio
estimatedDY
scrollEventRecently
keyboardEventRecently
uxMotionHintsFused
```

Stream configuration snapshots should preserve:

```text
filter kind, such as desktop-independent active window
configured width/height
minimumFrameInterval
queueDepth
showsCursor
capturesAudio
excludesCurrentProcessAudio
```

Sampler summaries should preserve:

```text
requested duration
requested max frames
frame count
complete frame count
idle frame count
non-complete frame count
classifier-feed frame count
persisted event count
persist errors
UX-hint fusion count
adaptive decision count
configuration update decision count
configuration update errors
latest adaptive decision
latest UX motion hints
latest frame
```

### 14.7 Browser Events

`events/browser.events.jsonl` is required.

V0 captures the best available browser context:

```text
URL
tab title
tab id/window id
browser app/window identity
selection
scroll position
visible DOM or visible text
DOM excerpt path or inline excerpt reference
active element/input shape
viewport
device pixel ratio
visible-tab screenshot media id, when retained
extension id/version/name
extension permissions
host permissions
native messaging proof metadata
signed app subject/proof match
source confidence
source degradation reason
```

Source priority:

```text
browser extension or browser-native API
browser-extension native-host artifact import
Automation / Apple Events where available
Accessibility tree and selected text
window title / app metadata
ScreenCaptureKit/OCR-derived capture in V1
```

If the browser extension is not installed, this file still exists and records
degraded browser observations from Accessibility and window metadata.

The Chrome extension/native host is a current direct source. Its existing
artifacts should be normalized into this lane when present:

```text
meta.json
dom.html
visible-text.txt
screenshot.png
preferences proof record
```

### 14.8 Terminal Events

`events/terminal.events.jsonl` is required.

V0 captures the best available terminal/session context:

```text
command starts
stdout/stderr chunks or visible text shape
cwd
exit code
shell/session ids
terminal app/window identity
selection
scroll position when available
source confidence
source degradation reason
```

Source priority:

```text
PTY/session recorder or shell integration
terminal app integration
Accessibility visible text and selected text
window title / process metadata
ScreenCaptureKit/OCR-derived capture in V1
```

If PTY or shell integration is unavailable, this file still exists and records
degraded terminal observations from Accessibility and window metadata.

Direct PTY/shell truth should be marked `present` only when that integration is
actually active. Otherwise the lane remains mandatory but degraded.

### 14.9 Editor Events

`events/editor.events.jsonl` is required.

V0 captures the best available editor context:

```text
file path
workspace/project path
selection
cursor/caret shape when available
diagnostics
document symbols
save/change anchors
active editor tab/window identity
source confidence
source degradation reason
```

Source priority:

```text
editor extension or editor-native API
LSP/editor diagnostics export where available
Accessibility focused element, selected text, and value shape
window title / file path hints
filesystem/git/LSP connector lanes in Future
```

If an editor extension is unavailable, this file still exists and records
degraded editor observations from Accessibility and window metadata.

Direct editor/LSP diagnostics should be marked `present` only when an editor
connector is actually active. Otherwise the lane remains mandatory but degraded.

### 14.10 Source Envelope References

`external_refs/source-envelopes.jsonl` is required, but it may be empty.

This file links the capture window to other local source records that overlap
the same time range without copying or reclassifying them as attention output.

Reference record:

```json
{
  "schema_version": 1,
  "ref_id": "source_ref_000001",
  "source": "memory_source_connector",
  "lane_id": "codex.sessions",
  "kind": "terminal_output",
  "time_range": {
    "start": "2026-05-25T19:12:14.000Z",
    "end": "2026-05-25T19:12:16.000Z"
  },
  "source_hash": "sha256:...",
  "source_id": "src_codex_sessions",
  "source_record_key": "codex-session:2026-05-25:...",
  "source_record_id": "codex-session:...",
  "object_id": "obj_optional_perception_or_memory_object",
  "connector_key": "codex.sessions",
  "read_posture": "read_only",
  "access_mode": "local_file",
  "source_uri": "file://redacted-or-relative",
  "source_byte_offset": 128,
  "source_line_number": 42,
  "privacy_class": "private_metadata",
  "confidence": 0.92,
  "alignment_method": "source_timestamp",
  "alignment_confidence": 0.86,
  "display_text_shape": {
    "character_count": 240,
    "redacted": false
  }
}
```

Allowed V0 reference kinds:

```text
capture_envelope
source_connector_record
browser_extension_artifact
local_adapter_record
perception_object
```

External refs should preserve connector/source quality when available:

```text
connector_key
source_id
source_record_key
source_record_id
object_id, if already written to a perception/memory object store
read_posture
access_mode
source_location
probe_status
viewer_confidence
source_hash
source_uri
source byte/line offsets
alignment method/confidence
```

These are provenance refs. They are not memory decisions.

## 15. ScreenCaptureKit Metadata Policy

V0 should prefer ScreenCaptureKit metadata over retained media.

The active-window frame metadata event should include:

```text
streamID
sequence
capturedAt
target
frameStatus
attachmentsPresent
displayTime, if present
contentRect, if present
contentScale, if present
scaleFactor, if present
dirtyRectSummary
motionFeatures
uxMotionHints, if fused
adaptiveDecision, if available
parseWarnings
```

Dirty-rect summaries are enough for V0. Full dirty rect arrays may be capped.

For video-like motion, V0 policy is:

```text
classify or tag as video_motion
avoid OCR
avoid high-rate retained media by default
keep the 2fps screenshot receipts and sparse metadata
retain source video only when explicitly requested for debug
```

This matches the product rule: video is usually not useful memory until we have
a specific media-understanding feature.

## 16. Media Index

`media/media.index.jsonl` is required.

V0 requires a low-rate screenshot cache extracted from the screen recording by
the Swift decoder:

```text
media/frames-2fps/frame-000001.jpg
media/frames-2fps/frame-000002.jpg
...
```

Those 2fps screenshots are normal capture evidence, not debug-only artifacts.
They are the visual receipt stream the attention dashboard uses to compare raw
activity against attention-filter output. Full source video is different: it is
debug-only and is copied under `media/debug/` only when explicitly requested.

Media index record:

```json
{
  "schema_version": 1,
  "media_id": "capture_20260525_191230:frame_2fps:000042",
  "kind": "frame_2fps",
  "path": "media/frames-2fps/frame-000042.jpg",
  "created_at": "2026-05-25T19:12:30.000Z",
  "time_range": {
    "start": "2026-05-25T19:12:30.000Z",
    "end": "2026-05-25T19:12:30.000Z"
  },
  "source": "capture.screen_recording_frame_decoder",
  "sample_rate_fps": 2,
  "frame_index": 42,
  "window_id": 9710,
  "bundle_id": "com.google.Chrome",
  "frame_hash": "sha256:...",
  "storage_backend": "bundle_file",
  "uri": "media/frames-2fps/frame-000042.jpg",
  "content_type": "image/jpeg",
  "byte_count": 123456,
  "state": "available",
  "dimensions": {
    "width": 960,
    "height": 540
  },
  "codec": "jpeg",
  "duration_ms": null,
  "privacy_class": "visual_evidence",
  "retention": "ephemeral"
}
```

Optional debug video record:

```json
{
  "schema_version": 1,
  "media_id": "capture_20260525_191230:debug_video:000001",
  "kind": "debug_screen_recording",
  "path": "media/debug/screen-recording.mov",
  "uri": "media/debug/screen-recording.mov",
  "status": "available",
  "storage_backend": "bundle_file",
  "content_type": "video/quicktime",
  "debug": true,
  "privacy_class": "debug_visual_evidence"
}
```

Allowed V0 media kinds:

```text
frame_2fps
event_frame
thumbnail
debug_contact_sheet
debug_screen_recording
blob_descriptor
```

Media records should be compatible with blob promotion. Preserve these fields
when known:

```text
storage_backend
uri
safe/browser handle
content_type
byte_count
state
codec
duration
dimensions
hash
```

V1 media kinds:

```text
scroll_strip
scroll_mosaic
ocr_evidence_frame
short_video_segment
```

V0 media rules:

```text
Do not retain display video by default.
Do retain the 2fps screenshot cache as part of the capture bundle.
Do retain sparse event frames when they are needed to audit a source event.
Do allow debug exports to contain the full source video under `media/debug/`.
```

## 17. Quality And Known Gaps

`quality/known_gaps.jsonl` is required. It may be empty.

Known-gap record:

```json
{
  "schema_version": 1,
  "time": "2026-05-25T19:12:12.000Z",
  "source_id": "browser",
  "severity": "warning",
  "code": "browser_extension_unavailable",
  "message": "Browser lane used Accessibility and window metadata because the browser extension was unavailable.",
  "blocks_ready": false
}
```

Allowed severities:

```text
info
warning
error
fatal
```

Only fatal structural failures block READY.

Examples that do not block READY:

```text
browser lane degraded to AX/window metadata
terminal lane degraded to AX/window metadata
editor lane degraded to AX/window metadata
AX source degraded for one app
Input Monitoring unavailable
ScreenCaptureKit dirty rects missing on one frame
media frame omitted by storage budget
UX event tap dropped/coalesced events under pressure
active-window stream configuration update failed once
browser extension proof missing but AX/window metadata exists
```

Examples that do block READY:

```text
manifest.json missing
sources.json missing
time_alignment.json missing
events/windows.jsonl missing
events/capture.events.jsonl missing
events/ax.events.jsonl missing
events/ux.events.jsonl missing
events/sck-frame-metadata.events.jsonl missing
events/browser.events.jsonl missing
events/terminal.events.jsonl missing
events/editor.events.jsonl missing
media/media.index.jsonl missing
media/frames-2fps/ missing
media/media.index.jsonl has no available frame_2fps record
invalid JSON in a required file
```

`quality/` should also preserve non-fatal source health records when available:

```text
parse warnings
malformed rect counts
dropped event counts
coalesced event counts
queue depths
persist errors
configuration update errors
degradation reasons
source confidence changes
```

## 18. Privacy And Redaction

V0 must record privacy shape metadata even when no redaction engine is active.

Required privacy labels:

```text
private_metadata
interaction_metadata
accessibility_semantic
browser_context
terminal_context
editor_context
visual_evidence
```

V0 redaction posture:

```text
raw keystroke text is not stored
keyboard events are cadence/shortcut summaries
selected text may be present when produced by AX/browser/editor/terminal lanes
sensitive values should be shape-only when detected
media should be sparse and event-linked
```

V1 should add a `redaction/` namespace:

```text
redaction/
  policy.json
  regions.jsonl
  source_policy_matches.jsonl
```

Future work may add app-specific and user-configured redaction policies.

## 19. Replay

V0 replay is optional but recommended.

If present:

```text
replay/replay-manifest.json
```

Replay manifest:

```json
{
  "schema_version": 1,
  "capture_id": "cap_20260525T191212Z_000042",
  "entrypoints": [
    "manifest.json",
    "capabilities/capture.status.json",
    "events/windows.jsonl",
    "events/displays.jsonl",
    "events/capture.events.jsonl",
    "events/ax.events.jsonl",
    "events/ux.events.jsonl",
    "events/sck-frame-metadata.events.jsonl",
    "events/browser.events.jsonl",
    "events/terminal.events.jsonl",
    "events/editor.events.jsonl",
    "media/media.index.jsonl",
    "media/frames-2fps/",
    "external_refs/source-envelopes.jsonl"
  ],
  "notes": "Can be loaded by the debug dashboard without live permissions."
}
```

Replay tooling should support bundles without source video, but READY capture
bundles include the 2fps screenshot cache. It should not assume full video
exists unless `media.index.jsonl` contains a debug video record.

## 20. V0 Acceptance Checklist

A V0 bundle is valid when:

```text
[ ] frontmatter-free JSON files parse
[ ] manifest.json exists and state is ready
[ ] READY exists
[ ] sources.json exists
[ ] time_alignment.json exists
[ ] capabilities/capture.status.json exists
[ ] capabilities/permissions.json exists
[ ] capabilities/ux-event-tap.json exists
[ ] capabilities/samplers.json exists
[ ] capabilities/browser-extension-proof.json exists
[ ] quality/known_gaps.jsonl exists, even if empty
[ ] events/windows.jsonl exists and has at least one window snapshot
[ ] events/displays.jsonl exists
[ ] events/capture.events.jsonl exists, even if empty
[ ] events/ax.events.jsonl exists
[ ] events/ux.events.jsonl exists
[ ] events/sck-frame-metadata.events.jsonl exists
[ ] events/browser.events.jsonl exists
[ ] events/terminal.events.jsonl exists
[ ] events/editor.events.jsonl exists
[ ] media/media.index.jsonl exists
[ ] media/frames-2fps/ exists
[ ] media/media.index.jsonl contains at least one available frame_2fps record
[ ] external_refs/source-envelopes.jsonl exists, even if empty
[ ] sources.json has an entry for every mandatory V0 lane
[ ] mandatory lanes are present or degraded, not omitted
[ ] all bundle paths are relative
[ ] all source failures are represented in sources.json or known_gaps.jsonl
[ ] no attention-output namespaces are inside the bundle
[ ] no saved/drop decisions, summaries, overlays, scores, agent packets, or
    memory-write receipts are inside the bundle
[ ] no fixed-FPS media directory is required for READY
```

## 21. V1 Work

V1 should enrich the mandatory lanes without changing the capture-bundle
boundary.

Expected V1 additions:

```text
ocr.events.jsonl:
  OCR strips and confidence, linked to event frames or scroll strips

derived/text-flow:
  reconstructed scrolling text timelines

media/strips:
  newly exposed scroll strips used as visual evidence

redaction:
  policy and source-region maps for sensitive content handling
```

V1 should promote monotonic time to a stronger requirement:

```text
events should include monotonic_ns when source APIs provide it
time_alignment.json should document clock offsets and confidence
attention filter should prefer monotonic joins when available
```

V1 should also add schemas and fixtures for:

```text
redaction/policy.json
redaction/regions.jsonl
```

## 22. Future Work

Future connector lanes:

```text
filesystem.events.jsonl
git.events.jsonl
lsp.events.jsonl
network.events.jsonl
notifications.events.jsonl
audio.events.jsonl
```

Future media/derived work:

```text
scroll mosaics
document reconstruction
chat transcript reconstruction
OCR confidence tuning
video metadata tagging
meeting/audio summaries
```

Future validation:

```text
JSON schemas for every required file
fixture replay harness
bundle hash manifest
content-addressed media promotion
dashboard bundle inspector
```

## 23. Product Rule

The bundle exists so the attention filter can make good decisions without
making capture magical or wasteful.

The rule:

```text
Harvest all granted sources.
Write mandatory lanes even when degraded.
Capture facts cheaply.
Store media sparsely.
Explain every missing capability.
Keep attention output outside the bundle.
Let the memory DB preserve only selected durable output.
```
