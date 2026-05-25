---
title: 1Context Semantic Observation System
slug: semantic-observation-system
section: architecture
access: private
summary: "Downstream reconstruction layer that turns raw screen, AX, browser, and input capture into selected screenshot states, optional composites, and per-minute attention captures."
status: draft
last_updated: 2026-05-24
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Semantic Observation System

## 0. Purpose

This is the layer downstream of the macOS capture spine.

The capture system records the window graph, frame/keyframe evidence,
Accessibility metadata, browser metadata, input events, and timing anchors. The
semantic observation system turns those raw streams into seen-surface artifacts:

```text
capture streams
  -> semantic observation
  -> memory-db captured objects
  -> viewer, agents, and wiki projections
```

The unit we want is not a pile of screenshots, a stitched mega-image, or a
forensic metadata record. It is a per-minute attention capture: up to three
semantically linked screenshots of what the user actually paid attention to,
with time as the join key for everything else.

## 1. Boundary

The semantic layer assumes upstream capture has already supplied:

```text
window/app identity
per-window frame/keyframe refs
dirty rects or changed-region metadata
Accessibility tree snapshots/deltas
input monitoring events
browser URL/title/DOM/scroll metadata when available
OCR/text-layer candidates when needed
raw evidence refs
```

It does not own TCC prompts, ScreenCaptureKit sessions, browser extension UX, or
the raw media store. It owns reconstruction and compression.

## 2. V0 Product Objects

The first repo-native contract lives in
`crates/onecontext-memory-db/src/semantic_observation.rs`. The concrete output
shape is defined in
[Semantic Observation Output Contract](semantic-observation-output-contract.md).

It introduces three implementation concepts, but the product output should stay
visual-first:

```text
ScrollDocument
  implementation-side reconstruction object for scrollable surfaces

SemanticChangeFeatures + SalienceDecision
  deterministic first-pass scoring for likely attention and importance

AgentObservationPacket
  compact agent-facing pointer to a seen-surface composite
```

These objects serialize into normal `CaptureEnvelope` rows:

```text
kind = scroll_document
schema_name = onecontext.scroll_document
lane = memory.observations

kind = agent_observation_packet
schema_name = onecontext.agent_observation_packet
lane = memory.observations
```

The source connector registry includes `desktop.semantic_observation` as a
`live_observation` connector. It declares Accessibility, Input Monitoring, and
browser extension inputs, while keeping raw pixels as evidence references rather
than embedding them in memory packets.

## 3. Reconstruction Pipeline

V0 should stay deterministic before any model touches it. The first
compositing decision is which screenshot states to save; stitching happens only
when one saved state cannot cover the visible surface.

```text
1. maintain a rolling 30s frame/event hindsight buffer
2. group frames/events into candidate sessions
3. render a filter overlay across all candidate screenshots
4. choose saved screenshot states: pre-scroll maximums, final stable states, and surface transitions
5. detect scroll sessions that need more than one saved state
6. build full development composites only where a single state is insufficient
7. score attention from scroll, dwell, hover, click, selection, typing, and return signals
8. apply local attention bands to saved screenshots
9. choose up to three attended items for the minute
10. emit an `AttentionCapture` with the filter overlay, selected screenshots, highlighted composites, and development refs
```

Scroll detection uses plain features:

```text
same window
same content pane
dominant vertical translation
stable chrome/header/sidebar
new exposed strip
input scroll events
```

Text dedupe starts deliberately boring:

```text
normalize whitespace/case
merge matching text across different frames
require compatible bounding-box width
do not merge duplicate copies from the same frame
preserve source span/frame ids
```

That gives us the first important property: a scroll burst can shrink from many
frames into one recognizable visual composite while still pointing back to exact
frames by time.

## 4. Salience Rule

The semantic layer must separate content that merely passed through view from
content the user probably cared about.

```text
fast scroll with no pause -> keep visual context, low attention
slow scroll or pause near content -> mark attention region
typing still in progress -> keep in buffer, usually do not save yet
typing completion or send/selection/pause -> save final stable state
content about to scroll out of view -> save pre-scroll maximum state
hover/click/selection/typing/return -> stronger attention region
modal/error -> preserve as important visible event
```

Inputs:

```text
Input Monitoring:
  scroll velocity, scroll bursts, pauses, clicks, hover, selections, typing

Accessibility:
  focused scroll view, visible element bounds, selected text, element under pointer

Screen capture:
  actual visible pixels, overlap, scroll translation, stable chrome, visual gaps

Browser metadata:
  URL/title, scroll offsets, DOM bounds, visible text, post/article containers
```

V0 scoring is a deterministic, tunable heuristic. Later model-assisted
summarization can sit on top of the same visual artifact, but attention should
first come from observable behavior: dwell, slow scroll, pointer hover, click,
selection, typing, and returning to a region.

## 5. Agent Packet Shape

Agents should receive compact pointers to seen surfaces, not raw sensory exhaust:

```yaml
time_range: "2026-05-24T17:12:00Z/2026-05-24T17:13:00Z"
attended_items:
  - label: "Codex chat correction about visual-first output"
    highlighted_composite_ref: "blob://sha256/ab/cd/example"
    attention_summary: "User paused on the correction that output should focus on what was actually seen."
askable_evidence:
  - "show final attention composite"
  - "show full development composite"
  - "show attention mask"
  - "show original frames"
```

The packet is allowed to be brief because the time range can recover the raw
proof and metadata when the viewer or agent asks for it.

## 6. Near-Term Build Steps

1. Wire the macOS capture log and browser extension outputs into a local
   `desktop.semantic_observation` adapter.
2. Add a small fixture generator with window frames, AX spans, browser metadata,
   and input events.
3. Build the first one-minute `AttentionCapture` compositor for Codex chat
   scrollback, including the 30s hindsight screenshot selector.
4. Keep selected screenshot states first, then build a full development
   composite only when selected states do not cover the visible surface.
5. Keep the attention filter overlay over all candidates so save/skim/drop
   decisions are explainable.
6. Keep final attention-highlighted screenshots with natural layout.
7. Add viewer support for opening final composites, full development
   composites, attention masks, original frames, and time-correlated metadata.
