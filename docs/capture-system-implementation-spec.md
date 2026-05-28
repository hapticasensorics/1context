---
title: 1Context Capture System Implementation Spec
slug: capture-system-implementation-spec
section: architecture
access: private
summary: "Implementation plan for the native capture runtime from macOS sensors through short-lived READY capture bundles, stopping before attention filtering and Timescale memory writes."
status: draft
last_updated: 2026-05-25
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Capture System Implementation Spec

## 0. Purpose

This spec defines the implementation of the capture system up to the point where
it emits a READY capture bundle.

It intentionally stops before:

```text
attention filtering
semantic observation selection
Timescale memory writes
agent-facing memory packets
wiki projections
```

The downstream contract is simple:

```text
1Context macOS capture runtime
  -> live capture spool
  -> normalized READY capture bundle
  -> attention-filter agent consumes later
  -> attention-filter output writes selected durable rows to Timescale later
```

Until the attention filter and DB population path are stabilized, the capture
runtime's success condition is not "memory was written." Its success condition
is:

```text
a bounded time window of local user activity becomes a truthful, time-aligned,
auditable, short-lived capture bundle with explicit source health and retention
metadata
```

## 1. Boundary

The capture system owns:

```text
TCC permission status and capability proof metadata
window graph indexing
Accessibility focused context and semantic deltas
UX event anchors
ScreenCaptureKit frame metadata and sparse visual evidence
browser/terminal/editor source lane ingestion where available
live JSONL/media spool
bundle export
bundle lifecycle and retention
debug dashboard and diagnostics
```

The capture system does not own:

```text
attention scoring
keep/drop decisions
semantic summary text
composite document construction
Timescale object writes
memory DB retention
agent packet generation
```

The capture bundle may contain cheap mechanical facts, such as dirty-rect area,
scroll totals, source health, frame status, and inferred source identity. It must
not contain downstream attention decisions or memory-write receipts.

## 2. Current Repo Anchors

The implementation lives mainly in the macOS package:

```text
macos/Sources/OneContextCapture/
  WindowIndexer.swift
  FocusedContextReader.swift
  AXSemanticEvents.swift
  OneContextUXEventTap.swift
  UXEventModels.swift
  ActiveWindowMetadataStream.swift
  SCStreamFrameMetadataParser.swift
  CaptureMotionClassifier.swift
  CaptureStore.swift
  CaptureStatusMetadata.swift
  CaptureDashboard.swift

macos/Sources/OneContextDaemon/main.swift
  capture.status
  capture.snapshot
  capture.ux.status
  capture.ux.probe
  active-window metadata sampling
  UX event tap startup and JSONL persistence

crates/onecontext-capture-core/
  Rust file-contract layer for bundle paths, V0 required lane files,
  manifest/schema types, atomic processing -> live promotion, validation,
  spool-window reads, and retention planning/audit helpers

crates/onecontext-capture-bundler/
  Rust operator CLI for describe/list/validate/sweep and an export command that
  refuses to act as the production READY writer

crates/onecontext-capture-dashboard/
  Rust/egui live capture dashboard plus bundle-contract tests

docs/capture-window-bundle-spec.md
  normalized bundle file contract
```

The capture runtime should continue to be Swift-first for macOS APIs. Rust is
appropriate for the debug dashboard, validators, and later bundle inspection
tools.

Current V0 ownership split:

```text
Swift daemon / OneContextCapture:
  owns sensors, permissions, live source truth, and the eventual production
  capture.bundle.export RPC

onecontext-capture-core:
  owns reusable file-level bundle mechanics that do not require macOS sensor
  access

onecontext-capture-bundler:
  owns local operator commands around existing bundle folders; its export
  command refuses to act as production bundle export

onecontext-capture-dashboard:
  owns GUI/debug visibility into live capture state and contract tests for
  bundle lifecycle expectations
```

## 3. Runtime Architecture

```text
1Context.app / menu bar / setup UI
  -> permission onboarding and user controls

1contextd
  -> owns long-lived capture runtime
  -> prepares private Application Support capture tree
  -> starts persistent UX event tap
  -> serves capture RPCs over the daemon API
  -> runs periodic flush/sweep timers

OneContextCapture Swift module
  -> builds window graph
  -> reads AX focused context
  -> aggregates AX semantic deltas
  -> aggregates UX anchors
  -> samples ScreenCaptureKit active-window metadata
  -> appends live JSONL events
  -> exports READY bundles

Debug dashboards
  -> read daemon status and live spool
  -> request fresh snapshots/samples
  -> display storage, source health, and bundle lifecycle
```

The daemon is the owner of long-lived sensors. CLI requests and dashboards may
ask for snapshots, but they should not be the only reason the core lanes run.

## 4. Storage Layout

All capture data is under app-owned Application Support:

```text
~/Library/Application Support/1Context/capture/
~/Library/Application Support/1Context Dev/capture/
```

Required tree:

```text
capture/
  events/
    <YYYY-MM-DD>.events.jsonl
  windows/
    <YYYY-MM-DD>.windows.jsonl
  displays/
    <YYYY-MM-DD>.displays.jsonl
  media/
    display/
    windows/
    keyframes/
  bundles/
    processing/
    live/
    failed/
    pinned/
  retention/
    sweeps.jsonl
```

The Rust `CaptureRootPaths` contract currently creates:

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

Production Swift setup should keep the stricter permissions above. Future Rust
callers that create app-owned capture roots should match those permissions on
macOS rather than relying on platform defaults.

Permissions:

```text
directories: 0700
files:       0600
```

The live spool is allowed to be optimized for append speed and debugging. The
bundle exporter is responsible for producing the stable file layout described in
[Capture Window Bundle Spec](capture-window-bundle-spec.md).

## 5. Event Envelope

Every live JSONL event should use the canonical capture envelope shape already
introduced by `CaptureEventEnvelope`:

```text
schema_version
event_type
durability
recorded_at
event_time_start
event_time_end
ingested_at
lane_id
stream_id
source_record_id
source_hash
source_span_id
capture_bundle_id when present
privacy_class
privacy_shape
source_clock
payload
```

Implementation rule:

```text
source_span_id:
  groups raw source records before bundle export

capture_id:
  exported bundle id

capture_bundle_id:
  existing Swift envelope field; new exporter code should write source_span_id
  and reserve capture_id for bundle identity
```

Durability classes:

```text
lossless:
  app/window identity
  focused app/window changes
  focused AX context snapshots
  source connector records
  browser navigation
  terminal command/output checkpoints
  bundle manifests

best_effort:
  dirty-rect telemetry
  UX aggregates
  frame samples
  thumbnails
  low-confidence OCR candidates
```

Under pressure, the runtime drops or coalesces best-effort records before
lossless records.

## 6. Source Lanes

### 6.1 Window Graph Lane

Implementation:

```text
OneContextWindowIndexer
  -> SCShareableContent
  -> CGWindowListCopyWindowInfo
  -> NSWorkspace frontmost app
  -> NSScreen display geometry
  -> AX focused-window matching
```

Writes:

```text
windows/<day>.windows.jsonl
event_type = capture.window_snapshot
lane_id = capture.windows
durability = lossless
```

V0 cadence:

```text
on capture.snapshot request
on status/debug sample request when needed
periodic daemon tick, target 0.5s to 2s once enabled
immediate tick on app activation/focus/UX burst where practical
```

Required payload:

```text
generatedAt
activeApplication
displays
windows with id, pid, app, bundle, title, frame, z-rank, layer, alpha,
on-screen/minimized/focused/captureEligible/source/focusMetadata
focusedContext when available
```

### 6.2 Accessibility Focused Context Lane

Implementation:

```text
AXFocusedContextReader
AXSemanticEventAggregator
```

Writes:

```text
events/<day>.events.jsonl
event_type = capture.ax_focused_context
event_type = capture.ax_semantic.<kind>.v1
lane_id = capture.ax_focused_context or capture.ax_semantic
durability = lossless for focused context, best_effort for deltas
```

Required V0 fields:

```text
process trust status
active/focused app
focused window role/subrole/title/bounds
focused element role/subrole/title/label/value shape
selection shape and capped selected text when safe
visible region graph when available
scrollbar/visible range state when available
transient UI state
read issues
redaction reasons
matched window id
```

Raw password values and raw typed keystrokes are forbidden.

### 6.3 UX Event Lane

Implementation:

```text
OneContextUXEventTap
daemon persistent startup
daemon persistence timer
UXEventAnchor aggregation
```

Writes:

```text
events/<day>.events.jsonl
event_type = capture.ux.scroll_burst.v1
event_type = capture.ux.pointer.v1
event_type = capture.ux.modifiers.v1
event_type = capture.ux.keyboard_activity.v1
event_type = capture.ux.shortcut.v1
event_type = capture.ux.focus_transition.v1
lane_id = capture.ux
durability = best_effort
```

The lane stores behavioral anchors, not surveillance logs.

Allowed:

```text
scroll burst direction/distance/duration
pointer action type/count/duration/distance
modifier state
keyboard activity cadence
known shortcut classes
target process hints when available
tap health, queue depth, drops, coalescing
```

Forbidden by default:

```text
raw typed text
raw key codes for ordinary text entry
unbounded mouse move streams
high-rate coordinates without an explicit debug pin
```

### 6.4 ScreenCaptureKit Metadata Lane

Implementation:

```text
ActiveWindowMetadataStream
SCStreamFrameMetadataParser
CaptureMotionClassifier
ActiveWindowMetadataAdaptiveController
```

Writes:

```text
events/<day>.events.jsonl
event_type = capture.active_window_frame_metadata
lane_id = capture.active_window_frames
durability = best_effort
source_clock = screen_capture_kit
```

V0 policy:

```text
active-window metadata only
max dimension 480 for metadata sampling
queueDepth = 1
cursor hidden
audio off
video_motion classified but not recorded
dirty rects and frame status retained
keyframes sparse and only when exporter policy asks for evidence
```

The callback must parse metadata and enqueue/append quickly. OCR, model calls,
and heavy media work are forbidden on the ScreenCaptureKit callback path.

### 6.5 Browser Lane

V0 status:

```text
mandatory lane, may be degraded
```

Preferred direct source:

```text
Chrome extension
native messaging bridge
browser capture artifacts
```

Secondary source:

```text
window graph titles
Accessibility focused element
Automation where configured
visible UI text
```

Writes during bundle export:

```text
events/browser.events.jsonl
capabilities/browser-extension-proof.json
external_refs/source-envelopes.jsonl when artifacts live outside the bundle
```

Required V0 fields when available:

```text
URL
tab title
tab id
browser window id
selection
scroll position
visible DOM/text excerpt
viewport
screenshot artifact refs
extension/native-host proof
degraded reason
```

### 6.6 Terminal Lane

V0 status:

```text
mandatory lane, may be degraded
```

Preferred direct source:

```text
shell hooks
PTY/session recorder
terminal integration
```

Secondary source:

```text
AX text/value/selection
window title and cwd hints
OCR/text strip candidates later
```

Writes during bundle export:

```text
events/terminal.events.jsonl
```

Required V0 fields when available:

```text
shell/session id
command start
stdout/stderr chunks or hashes
cwd
exit code
window/app mapping
degraded reason
```

### 6.7 Editor Lane

V0 status:

```text
mandatory lane, may be degraded
```

Preferred direct source:

```text
VS Code extension or editor plugin
source connector
Automation where configured
```

Secondary source:

```text
AX focused element
window title
file path hints
selection/value shape
visible UI text
```

Writes during bundle export:

```text
events/editor.events.jsonl
```

Required V0 fields when available:

```text
file path
workspace/repo path
selection shape
diagnostics
document symbols
save/change anchors
degraded reason
```

## 7. Live Capture Lifecycle

### 7.1 Daemon Startup

On startup:

```text
1. resolve RuntimePaths for current app identity
2. create capture directories with private permissions
3. load capture retention config
4. start persistent UX event tap if Input Monitoring proof allows it
5. start UX JSONL persistence timer
6. prepare AX semantic aggregator
7. expose capture RPC methods
8. run startup capture.status health sample
9. run retention sweep opportunistically
```

Startup must not crash if one lane is unavailable. It should mark the lane
`degraded`, record a proof/health reason, and keep the rest of capture alive.

### 7.2 Periodic Capture Tick

A normal tick:

```text
1. build window snapshot
2. read focused AX context
3. feed AX semantic aggregator
4. flush UX anchors since last tick
5. optionally sample active-window SCK metadata
6. append live JSONL
7. update status counters
```

The tick scheduler should avoid redundant full snapshots when nothing changed.
Focus, app activation, scroll bursts, and keyboard cadence can pull the next tick
forward.

### 7.3 Active-Window Metadata Sampling

Sampling is event-driven and low-budget:

```text
normal idle:
  no stream, or short metadata sample only for dashboard/proof

focused interaction:
  short active-window SCStream metadata sample

scroll/text flow:
  elevate metadata FPS briefly

video_motion:
  classify and suppress expensive media/OCR
```

Capture FPS, analysis FPS, and storage FPS are independent budgets. Metadata may
be sampled more often than media is retained.

## 8. Bundle Exporter

The bundle exporter is the bridge from live spool to the normalized bundle
contract.

Inputs:

```text
time window
capture target hint, optional
live JSONL spool
current capture.status
current capture.snapshot
browser/native-host artifacts
terminal/editor/source connector artifacts
media refs
2fps screen-recording-derived screenshots
retention config
```

Outputs:

```text
capture/bundles/live/<capture_id>/
  manifest.json
  READY
  events/*.jsonl
  capabilities/*.json
  media/media.index.jsonl
  media/frames-2fps/
  external_refs/source-envelopes.jsonl
```

Export flow:

```text
1. allocate capture_id
2. create processing/<capture_id>.partial/
3. write manifest.json state=partial
4. query raw spool by time window
5. normalize events into required lane files
6. copy the Swift decoder's 2fps frame cache into media/frames-2fps/
7. attach capability/proof/status snapshots
7. attach sparse media descriptors and copy/pin small evidence blobs if needed
8. write lane inventories and known gaps
9. write READY
10. update manifest.json state=ready with byte counts, file counts, expires_at
11. atomically rename to live/<capture_id>/
12. append retention/sweep audit event
```

The exporter must tolerate missing or degraded mandatory lanes by writing empty
or degraded lane files plus explicit capability metadata. It must not silently
omit a lane.

### 8.1 Rust Capture-Bundle Contract Runbook

Use the Rust crate as the contract harness while the Swift daemon exporter is
being wired.

Crate boundaries:

```text
onecontext-capture-core:
  stable-ish V0 library surface for paths, required files, lane inventory,
  manifest/schema structs, bundle validation, atomic writer helpers, raw spool
  time-window reads, and retention sweep planning

onecontext-capture-bundler:
  standalone operator/debug binary. It can describe the contract, list bundle
  directories, validate READY bundles, and dry-run/apply a simple folder sweep.
  Its export command refuses production export and points callers back to the
  daemon-owned capture.bundle.export method.

onecontext-capture-dashboard:
  GUI/debug binary. It reads daemon `capture status` and `capture snapshot`,
  tails live spool events, and has tests that encode the partial -> READY ->
  live lifecycle and V0 required file set.
```

Important implementation note:

```text
onecontext_capture_core::export_ready_bundle(...)
  is a useful test/fixture exporter over existing JSONL spool files.

onecontext-capture-bundler export
  is not the production exporter; production export must run through the Swift
  daemon's current status, permission, sampler, and source-health truth.
```

Build and test:

```bash
cargo test -p onecontext-capture-core
cargo test -p onecontext-capture-dashboard --test capture_bundle_contract
cargo run -p onecontext-capture-bundler -- describe
```

Operator inspection against a capture root:

```bash
CAPTURE_ROOT="$HOME/Library/Application Support/1Context Dev/capture"
cargo run -p onecontext-capture-bundler -- list --capture-root "$CAPTURE_ROOT" --class all
cargo run -p onecontext-capture-bundler -- validate --capture-root "$CAPTURE_ROOT" --capture-id <capture_id> --strict
cargo run -p onecontext-capture-bundler -- sweep --capture-root "$CAPTURE_ROOT"
```

Only use `--apply` for sweep after inspecting the dry-run candidate list:

```bash
cargo run -p onecontext-capture-bundler -- sweep --capture-root "$CAPTURE_ROOT" --apply
```

V0 fixture generation status:

```text
No standalone fixture-generator command is present in the capture-bundle crates.
The current fixture path is the `onecontext-capture-core` test helper that seeds
a window snapshot into a temp capture root, calls `export_ready_bundle`, and
validates the resulting READY bundle.
```

If a future agent adds a fixture generator, keep it in the Rust layer unless it
needs macOS permissions. It should generate a disposable Application Support
capture-root shape, seed at least one `capture.window_snapshot`, export a READY
bundle, run validation, and print the bundle path plus validation report.

### 8.2 Export API

V0 daemon RPC:

```text
capture.bundle.export
```

Request:

```json
{
  "time_start": "2026-05-25T20:00:00.000Z",
  "time_end": "2026-05-25T20:01:00.000Z",
  "target": {
    "mode": "active_window"
  },
  "debug_pin": false
}
```

Response:

```json
{
  "capture_id": "cap_20260525_200000_...",
  "state": "ready",
  "bundle_path": "capture/bundles/live/cap_20260525_200000_...",
  "expires_at": "2026-05-25T21:00:00.000Z",
  "byte_count": 123456,
  "lane_count": 9,
  "known_gap_count": 2
}
```

V0 CLI:

```bash
1context capture bundle export --since-seconds 60
1context capture bundle export --start <iso> --end <iso>
1context capture bundle list
1context capture bundle sweep
```

The CLI is a debug/operator surface. The attention filter should use daemon RPC
or a local queue, not shell parsing.

## 9. Retention And Storage Control

Bundles and live spool are temporary capture infrastructure. They are not the
product memory store.

### 9.1 Bundle Retention

Default policy:

```text
processing:
  stale after 15 minutes; move to failed or delete if empty

live:
  delete after attention-filter success ack
  otherwise expire after 60 minutes
  keep last 20 READY bundles even if TTL would delete them, unless over byte cap

failed:
  keep 24 to 72 hours
  cap total failed bytes

pinned:
  keep until explicit deletion
  show warning when pinned bytes exceed budget
```

Every READY manifest must include:

```text
created_at
ready_at
expires_at
retention_class
byte_count
file_count
pinned
pin_reason
source_spool
```

Deletion must be auditable:

```text
capture/retention/sweeps.jsonl
  sweep_id
  started_at
  completed_at
  policy_version
  deleted_paths
  deleted_bytes
  preserved_paths
  errors
```

### 9.2 Live Spool Retention

The live spool needs its own cap because it grows before bundles exist.

Default policy:

```text
events/windows/displays JSONL:
  retain 24 hours in production
  retain 72 hours in dev unless byte cap is hit
  cap metadata spool bytes per app identity

media:
  keep sparse evidence only
  cap media bytes separately
  delete unreferenced media before referenced media
```

When over budget:

```text
1. delete expired bundles
2. delete stale failed bundles
3. delete oldest unreferenced media
4. compact or delete old best-effort JSONL
5. preserve newest lossless identity/focus records if possible
6. report degraded retention status in capture.status
```

Do not let capture silently fill the disk. `capture.status` must report:

```text
capture_root_bytes
spool_bytes
bundle_bytes
media_bytes
pinned_bytes
oldest_spool_event_at
newest_spool_event_at
last_sweep_at
last_sweep_error
over_budget
```

## 10. Backpressure

Capture should degrade predictably under pressure.

Priority order:

```text
1. lossless window/focus/source identity
2. AX focused context
3. browser/terminal/editor direct structured events
4. UX anchors
5. SCK dirty-rect/frame metadata
6. keyframe evidence
7. thumbnails/debug-only previews
```

If queues fill:

```text
coalesce UX anchors
drop best-effort frame metadata
skip keyframes
extend capture tick interval
record dropped_count and degraded reason
```

The dashboard should make degradation visible.

## 11. Observability

Required status surfaces:

```text
capture.status:
  permissions, methods, paths, active app, sampler state, AX/UX health,
  retention health, storage bytes, bundle counts

capture.snapshot:
  current window graph and focused context

capture.ux.status:
  event tap health, queue depth, drops, persistence counters

capture.bundle.list:
  processing/live/failed/pinned counts, byte totals, newest/oldest, errors
```

Dashboard requirements:

```text
show current target and preview
show active lanes and degraded lanes
show recent UX anchors
show latest SCK frame metadata
show bundle export status
show storage/retention budget
show last sweep result
```

The dashboard must not become a high-rate capture source. It reads low-rate
status and requests debug snapshots explicitly.

## 12. Implementation Phases

### Phase 0: Keep Current Capture Stable

Already present:

```text
window graph snapshots
AX focused context
AX semantic event aggregation
UX event tap and persistence
SCK active-window metadata sampling
capture.status and capture.snapshot
terminal and Rust GUI dashboards
private capture tree
```

Exit criteria:

```text
existing capture tests pass
dev app can run with granted permissions
dashboard shows live window graph and preview
capture/status reports capability proofs and UX health
```

### Phase 1: Retention Manager

Implement:

```text
OneContextCaptureRetentionPolicy
OneContextCaptureRetentionSweeper
capture/retention/sweeps.jsonl
capture.status storage fields
unit tests with fake files and timestamps
```

Exit criteria:

```text
expired live bundles are removed
stale processing bundles are moved/deleted
failed bundles obey age and byte caps
pinned bundles are preserved
old live spool files obey max age and byte caps
dashboard reports storage budgets
```

### Phase 2: Bundle Exporter

Implement:

```text
OneContextCaptureBundleExporter
processing -> live atomic write flow
manifest writer
required lane files
capabilities/proof files
media index writer
2fps frame cache writer/copier
raw spool provenance
known gap inventory
```

Exit criteria:

```text
exporting a 60s window creates a READY bundle
all V0 required files exist
media/frames-2fps contains available frame_2fps screenshots
mandatory degraded lanes are explicit, not missing
paths are relative
JSON validates
partial bundles are never visible as READY
```

### Phase 3: Source Lane Fill-In

Implement or connect:

```text
browser extension/native-host artifacts into browser.events.jsonl
terminal hooks or degraded terminal inference into terminal.events.jsonl
editor/plugin/degraded editor inference into editor.events.jsonl
display events JSONL
external source envelopes
```

Exit criteria:

```text
browser/terminal/editor files are present in every READY bundle
direct source proof appears when installed/configured
degraded reason appears when direct source is missing
attention filter can replay a bundle without talking to live sensors
```

### Phase 4: Handoff Queue Stub

Implement only the capture-side handoff, not attention filtering:

```text
bundle READY queue
claim/ack metadata owned outside bundle
success ack deletes eligible live bundle
failure keeps bundle under failed or live depending on error
```

Exit criteria:

```text
capture can produce and list READY bundles
a fake attention consumer can ack success
ack success makes bundle eligible for retention deletion
ack failure preserves debug evidence
```

## 13. Verification

Unit tests:

```text
capture paths permissions
event envelope conformance
retention policy decisions
retention sweeper file deletion/preserve logic
bundle manifest required fields
bundle atomic promotion
relative path validation
mandatory lane file creation
known gap inventory
```

Installed-app tests:

```bash
1context capture status
1context capture snapshot
1context capture dashboard --snapshot
1context capture bundle export --since-seconds 60
1context capture bundle list
1context capture bundle sweep
```

A successful proof should save:

```text
capture-status-before.json
capture-snapshot-before.json
bundle-export-response.json
bundle-tree.txt
manifest.json
validation-report.json
capture-status-after.json
```

Validation rules:

```text
READY sentinel exists
manifest.state == ready
all V0 required files exist
all JSON/JSONL parses
bundle paths are relative
no attention-filter output files are present
bundle byte_count matches filesystem walk
expires_at is present unless pinned
storage sweep preserves pinned bundle
```

## 14. Known V0 Gaps

These are acceptable before attention filtering stabilizes, but must be surfaced
as known gaps in bundle capability files:

```text
browser extension not installed or not connected
terminal direct source unavailable
editor direct source unavailable
microphone/system-audio permission granted but audio lane not harvested
OCR strip stitching not implemented
full display video intentionally suppressed
Timescale write not part of capture proof
```

## 15. Future Work

After the attention filter and Timescale population path are stable:

```text
replace fake handoff consumer with real attention-filter claim/ack
write selected bundle provenance to capture.capture_bundles
write selected memory objects to Timescale
add retention based on durable DB write receipts
add OCR strip/text-flow reconstruction
add richer media blob lifecycle
add cross-device clock alignment
add user-facing storage controls
```

The capture system should still stop at READY bundles. The downstream memory
pipeline may become sophisticated, but capture remains the truthful sensor and
handoff layer.
