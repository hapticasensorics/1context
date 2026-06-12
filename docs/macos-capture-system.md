# macOS Capture System

This is the native capture spine for the maximalist 1Context memory system.

The durable product model is not "one big screen movie." The unit of truth is a
timestamped window graph. Pixels, browser data, terminal streams, Accessibility
metadata, and UX events attach evidence to that graph.

```text
1Context app / 1contextd
  ├─ window graph: ScreenCaptureKit + CoreGraphics + AppKit
  ├─ semantic overlay: Accessibility
  ├─ visual evidence: display and per-window ScreenCaptureKit frames
  ├─ event anchors: Input Monitoring / AX / browser / terminal events
  └─ durable store: JSONL + media folders, then memory-db indexes
```

## Current V0

The first repo-native slice is the `OneContextCapture` Swift package target.

It provides:

- `OneContextWindowIndexer`: builds a merged window snapshot from
  `SCShareableContent`, `CGWindowListCopyWindowInfo`, `NSWorkspace`, `NSScreen`,
  and best-effort AX focused-window matching.
- `AXFocusedContextReader`: reads only the focused app/window/UI element through
  Accessibility, then attaches a redacted semantic overlay to the window graph.
  It records role, subrole, metadata, bounds, selection shape, value shape, and
  capped selected text while skipping password-looking fields.
- `CaptureWindowState`: stable JSON model for window id, PID, bundle id, title,
  frame, layer, z-rank, focus, on-screen state, capture eligibility, and source
  provenance.
- `OneContextCaptureLogStore`: private Application Support capture tree with
  lossless JSONL window-snapshot and `capture.ax_focused_context` events, plus
  best-effort aggregate UX anchors under `capture/events/*.events.jsonl`.
- `CaptureMotionClassifier`: deterministic dynamic-capture policy for idle,
  watch, active-text, scrolling-text, and video-motion modes.
- `capture.status` and `capture.snapshot` daemon RPC methods, exposed from the
  CLI as `1context capture status` and `1context capture snapshot`.
- `1context capture dashboard`, a terminal debug dashboard that reads persisted
  capture JSONL and can request a fresh snapshot with `--snapshot`.
- A daemon-owned UX event tap starts with `1contextd` after capture directories
  are prepared. It runs on a dedicated retained thread/runloop, not the request
  queues or main actor, and reports lifecycle health through `capture.status`
  and `capture.ux.status`.
- A daemon-owned UX persistence timer drains the event tap into bounded semantic
  UX anchors every 0.5 seconds and appends them to
  `capture/events/*.events.jsonl`. Status requests only peek at motion hints;
  they do not consume anchors or turn the lane into a probe.

Capture data is app-owned runtime evidence:

```text
~/Library/Application Support/1Context*/capture/
  events/
  windows/
  displays/
  bundles/
  media/
    display/
    windows/
    keyframes/
```

The directories and JSONL files are private (`0700` directories, `0600` files).

`capture/bundles/` is the short-lived handoff surface for downstream attention
filtering. The capture daemon writes normalized, time-aligned evidence bundles
there; the attention-filter agent reads those bundles and writes selected
durable output to memory DB. See the
[Capture System Implementation Spec](capture-system-implementation-spec.md) for
the runtime implementation plan up to READY bundle production, and the
[Capture Window Bundle Spec](capture-window-bundle-spec.md) for the file
contract, lifecycle, retention policy, and replay/debug shape.

Debug dashboard:

```bash
1context capture dashboard
1context capture dashboard --snapshot
1context capture dashboard --snapshot --watch --interval-seconds 2
```

The terminal dashboard is the durable-log debug view: it makes it obvious what
the window graph and persisted capture evidence contain before browser or
Gemini decoding enters the loop.

UX event tap status includes `startup_wired`, `tap_active`, tap owner process
identity, queue depth, dropped/coalesced counts, disabled/re-enable counters,
callback timings, `last_event_at`, and `jsonl_persistence` counters. If
CGEventTap creation fails, daemon startup continues and the lane reports
degraded status rather than crashing.

## Event Policy

The capture system follows the Codex-style lossless/best-effort split.

Lossless:

- app/window identity changes
- focus changes
- browser navigation
- terminal output deltas
- completed text-flow records
- memory compaction records

Best effort:

- aggregate UX anchors: `capture.ux.scroll_burst.v1`,
  `capture.ux.pointer.v1`, `capture.ux.keyboard_activity.v1`, and
  `capture.ux.modifiers.v1`
- raw frame deltas
- mouse movement
- dirty-rect telemetry
- low-confidence OCR candidates
- thumbnails

UX anchors are dashboard-friendly summaries. They intentionally omit typed text,
characters, key codes, and raw mouse coordinates; pointer anchors keep only
button/action/count/duration/distance/axis, and keyboard anchors keep activity
counts.

The daemon must stay responsive under load. Durable semantic records win over
high-volume sensory residue. The Rust GUI dashboard polls the live JSONL event
tail at low rate and shows recent UX anchors next to active-window frame
metadata so we can see whether user behavior is actually reaching the capture
spine.

## Dynamic Capture

Static FPS is not the policy. The scheduler classifies observed change:

```text
dirty rects + pixel diff + UX events + AX/browser/terminal signals
  → CaptureMotionClassifier
  → idle | watch | active_text | scrolling_text | video_motion
```

Active-window ScreenCaptureKit metadata samples fuse the daemon's persistent UX
motion hints into `MotionFeatures` only. Scroll and keyboard hints can mark
recent input, supply a scroll-DY fallback when compatible dirty rects lack a
previous center, and keep focus truth warm for classifier policy; pixels and
media writes stay untouched.

Policy defaults:

| Mode | Capture FPS | Purpose |
| --- | ---: | --- |
| `idle` | 1 | heartbeat only |
| `watch` | 3 | focused/static or low motion |
| `active_text` | 10 | typing or OCR novelty |
| `scrolling_text` | 30 | terminal/chat/page text flow |
| `video_motion` | 1 | broad motion; skip video recording for now |

The controller escalates immediately and decays slowly so sudden terminal dumps
and scrolls keep their prebuffer/evidence.

For the current debug build, `video_motion` is a classification and suppression
mode, not a recording mode. It avoids OCR and video segment encoding so a
YouTube/game window does not create hundreds of low-value frames for Gemini.

## Next Build Steps

1. Add capture-window bundle export under `capture/bundles/` so downstream
   attention filtering can consume normalized observation windows without owning
   capture.
2. Add frame metadata and selected keyframe media writes under `capture/media/`
   and reference them from bundle `media/index.jsonl`.
3. Connect browser-extension tab/DOM events to the same event envelope.
4. Add terminal/shell hooks so terminal truth is PTY/session text first and OCR
   is only fallback evidence.
5. Feed lossless capture records into the memory-db `captured_objects` spine.
6. Feed frame/AX/browser/input refs into the downstream
   [Semantic Observation System](semantic-observation-system.md) so scroll
   sessions become `scroll_document` and `agent_observation_packet` objects
   instead of screenshot piles.
