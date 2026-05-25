---
title: 1Context Seen Surface Output Contract
slug: semantic-observation-output-contract
section: architecture
access: private
summary: "Output contract for semantically linked screenshots: selected screenshot states from a hindsight buffer, optional composites, and attention hints over what the user actually looked at."
status: draft
last_updated: 2026-05-24
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Seen Surface Output Contract

## 0. Purpose

The output of this system should be what a person would recognize as what they
actually saw.

For a scrolling Codex chat, the output should look like a clean long screenshot
of the chat that the user scrolled through. For a webpage, it should look like
the portion of the page the user actually viewed. For a feed, it should preserve
the posts that passed through the user's visual field and highlight the ones
that likely mattered.

The system is time-correlated, so the artifact does not need to carry every
piece of source metadata. App identity, URL, AX tree, browser state, input
events, and raw frame history can be recovered by querying the same time range.

The core output is:

```text
what the user saw
  + when they saw it
  + where attention probably concentrated
  + enough evidence to replay or inspect
```

## 1. Design Turn

Do not make the output a forensic metadata object.

Metadata is still valuable, but mostly as internal scaffolding:

```text
metadata helps group frames
metadata helps align scroll overlap
metadata helps infer attention
metadata helps recover URL/app/window later
metadata should not dominate the output
```

The durable artifact should be visual-first:

```text
AttentionCapture
  time_range
  hindsight buffer selection
  attention filter overlay
  saved screenshot states
  up to 3 attended composites
  development full composites
  final attention-highlighted composites
  small human labels
```

The time range is the join key. If a viewer or agent needs app name, URL, DOM,
Accessibility tree, input events, or exact frames, it asks the timeline for
objects inside that range.

## 2. Hindsight Buffer: Save States, Then Composite If Needed

The compositor should not begin by weaving every screenshot into one artifact.
Its first job is to decide which screenshots are worth saving.

For typing, drafting, and chat surfaces, most intermediate frames are
transient. A partial sentence, blinking cursor, or half-rendered reply is
usually not the memory object. The useful screenshot is often the final stable
state, plus a previous high-information state if wrapping or scrolling is about
to hide content that was visible.

For scrolling, the selector should keep the smallest set of screenshot states
that cover what the user actually saw:

```text
rolling 30s raw frame buffer
  -> identify cohesive visible surface
  -> find maximal visible states before scroll/wrap loss
  -> find final stable states after pause/selection/surface switch
  -> save selected screenshots
  -> build composite only when a single screenshot cannot cover the surface
```

This means a "composite screenshot" is often a selected state, not a stitched
image. Stitching is still useful for long scrollback, feeds, documents, and web
pages, but only after state selection says one screenshot is insufficient.

The filter should be visible as an overlay over all candidate screenshots, not
hidden as a scoring table. A developer or user should be able to inspect the
buffer and see which frames were saved, skimmed, or dropped.

```ts
AttentionFilterOverlay {
  overlay_id: string
  time_range: {
    start: string
    end: string
  }
  candidate_frame_count: number
  candidate_still_count: number
  cells: AttentionFilterCell[]
  saved_state_ids: string[]
}

AttentionFilterCell {
  candidate_index: number
  screenshot_ref: EvidenceRef
  decision:
    | "save_high_attention"
    | "save_coverage"
    | "skim_context"
    | "drop_transient"
  explanation: string
  reasons: AttentionReason[]
}
```

```ts
SavedScreenshotState {
  state_id: string
  time_range: {
    start: string
    end: string
  }
  screenshot_ref: EvidenceRef
  surface_kind: string
  coverage_role:
    | "pre_scroll_maximum"
    | "final_stable_state"
    | "surface_transition"
    | "selected_region"
    | "manual_pin"
  attention_score: number
  reasons: AttentionReason[]
}
```

Selection rules:

```text
drop partial typing frames until text stabilizes
drop repeated frames with no new visual information
save the last high-information frame before content scrolls out of view
save the final stable state after a pause, click, selection, or surface switch
promote only states with attention evidence into final attended items
retain raw video/frame history only as development evidence
```

## 3. Primary Cadence: AttentionCapture

The system should emit one combined attention capture per minute of active use.

That minute-level object answers:

```text
During this minute, what did the user actually pay attention to?
```

The output should contain up to three attended things. Each thing can be a
composite screenshot: a Codex chat segment, a web page section, a feed post
cluster, a terminal region, a document passage, or a native app panel.

```ts
AttentionCapture {
  schema_version: 1
  attention_capture_id: string
  time_range: {
    start: string
    end: string
  }
  hindsight_buffer: {
    lookback_ms: number
    observed_frame_count: number
    candidate_still_count: number
    attention_filter_overlay_ref?: EvidenceRef
    saved_state_refs: EvidenceRef[]
  }
  attended_items: AttentionItem[] // max 3 by default
  skim_context_ref?: EvidenceRef
  development_refs: {
    full_seen_surface_refs: EvidenceRef[]
    attention_mask_refs: EvidenceRef[]
    original_frame_refs: EvidenceRef[]
  }
  quality: {
    confidence: number
    warnings: string[]
  }
}
```

Rules:

```text
emit at most 3 attended_items by default
allow 0 items for a minute of pure idle or low-confidence activity
rank items by dwell, interaction, return, center weight, and slow-scroll evidence
prefer selected stable screenshot states before building stitched composites
keep a development/full composite before filtering
final output keeps natural layout and highlights attended rows/regions
never claim attention with no behavioral support
```

The development refs are important. During development and debugging, we need to
see the full pre-filtered capture and the attention mask. The final product view
can be brutally compact.

## 4. AttentionItem

An `AttentionItem` is one thing inside the minute that likely mattered.

```ts
AttentionItem {
  item_id: string
  label: string
  surface_kind:
    | "codex_chat"
    | "web_page"
    | "feed"
    | "document"
    | "terminal"
    | "native_scroll_view"
    | "unknown"
  time_range: {
    start: string
    end: string
  }
  attention_score: number
  estimated_attention_ms: number
  highlighted_composite_ref: EvidenceRef
  full_composite_ref?: EvidenceRef
  attention_overlay_ref?: EvidenceRef
  text_layer_ref?: EvidenceRef
  source_frame_refs: EvidenceRef[]
  attention_bands: AttentionBand[]
  reasons: AttentionReason[]
}
```

The highlighted composite should preserve natural visual size and layout. The
attention overlay/tint explains where time and interaction concentrated; it does
not stretch large items or delete low-attention regions.

```ts
AttentionBand {
  band_id: string
  y_start: number
  y_end: number
  attention_score: number
  estimated_attention_ms: number
  tint:
    | "none"
    | "low"
    | "medium"
    | "high"
  explanation: string
  reasons: AttentionReason[]
}
```

```ts
AttentionReason {
  reason:
    | "dwell"
    | "slow_scroll"
    | "scroll_pause"
    | "centered_in_view"
    | "hover"
    | "click"
    | "selection"
    | "typing"
    | "returned_to"
    | "manual_pin"
  score_delta: number
  time_range?: {
    start: string
    end: string
  }
}
```

## 5. SeenSurface

`SeenSurface` is the visual building block used by `AttentionCapture`.

```ts
SeenSurface {
  schema_version: 1
  seen_surface_id: string
  time_range: {
    start: string
    end: string
  }
  surface_kind:
    | "codex_chat"
    | "web_page"
    | "feed"
    | "document"
    | "terminal"
    | "native_scroll_view"
    | "unknown"
  label: string
  composite_image_ref: EvidenceRef
  text_layer_ref?: EvidenceRef
  attention_regions: AttentionRegion[]
  source_frame_refs: EvidenceRef[]
  quality: {
    confidence: number
    warnings: string[]
  }
}
```

The composite image is the star. It should be easy to open, inspect, skim, and
recognize.

The text layer is secondary. It exists for search, selection, and agent access.
If the visual composite and text disagree, the visual composite remains the
human evidence.

## 6. EvidenceRef

The evidence shape should stay tiny.

```ts
EvidenceRef {
  evidence_id: string
  kind:
    | "composite_image"
    | "source_frame"
    | "text_layer"
    | "attention_overlay"
    | "replay_manifest"
  uri: string
  content_type?: string
  sha256?: string
  time_range?: {
    start: string
    end: string
  }
}
```

The object does not need to embed app names, URLs, DOM IDs, AX node IDs, or
input metadata. Those belong in the time-correlated timeline.

## 6. AttentionRegion

Attention is not a claim of literal eye tracking unless we have eye tracking.
It is a useful proxy for what likely mattered to the user.

```ts
AttentionRegion {
  region_id: string
  bounds_in_composite: {
    x: number
    y: number
    width: number
    height: number
  }
  score: number
  estimated_attention_ms?: number
  reason:
    | "dwell"
    | "scroll_pause"
    | "centered_in_view"
    | "hover"
    | "click"
    | "selection"
    | "typing"
    | "returned_to"
    | "slow_scroll"
    | "manual_pin"
  label?: string
  explanation?: string
}
```

For a feed, the system may see hundreds of posts. The output should not pretend
all posts mattered equally. A long composite can retain the skimmed context,
while attention regions mark the few posts the user paused on, clicked, hovered,
selected, or returned to.

## 7. Permission Signals For Attention

Permissions should make the composite more human, not more complicated.

The output remains a `SeenSurface`. Internally, the system uses permission-backed
signals to decide:

```text
which frames belong to the same scrolling surface
which regions were skimmed
which regions were probably read
which regions deserve attention marks
where to split a long scroll into chapters
```

Input Monitoring gives the behavioral rhythm:

```text
scroll wheel / trackpad deltas
scroll velocity and acceleration
bursts versus pauses
keyboard navigation such as PageUp, PageDown, arrows, space, home, end
mouse position and hover dwell
clicks, drags, selections, copy shortcuts, typing
```

Accessibility gives the semantic target:

```text
focused app/window and focused scroll view
visible text fields, rows, cells, buttons, links, and messages
element bounds in screen coordinates
selected text and caret/focus changes
scroll bar/value changes when exposed by the app
which visible element is under or near the pointer
```

Screen capture gives visual truth:

```text
what actually entered the viewport
frame overlap and scroll translation
stable chrome versus moving content
duplicate bands to remove from the composite
visual holes or low-confidence joins
```

Browser metadata is an alignment accelerator when available:

```text
URL/title and navigation boundaries
scroll offsets
DOM element bounds
post/article/message containers
link/media hit targets
visible text from DOM rather than OCR
```

None of these need to be repeated in the primary `SeenSurface` object. They are
time-correlated inputs used to build a better visual artifact.

## 8. Tunable Human-Behavior Heuristics

The attention model should be tunable and humble. It should say "probably
attended," not "the user definitely read this."

Useful first-pass heuristics:

```text
fast continuous scroll
  keep the region in the composite, mark low attention

scroll pause over content
  mark content near the viewport center as possible attention

slow scroll through text
  mark broader reading attention than a fast flick

pointer hover over a region
  raise attention, especially over links, posts, buttons, or messages

click, selection, copy, typing, expand, reply, like, open
  mark strong attention and preserve the before/after visual state

return to a region after scrolling away
  raise attention; the user probably cared enough to re-find it

content visible only at the viewport edge
  keep as context, lower attention

modal, error, permission prompt, or blocking UI
  preserve strongly even with short dwell
```

Suggested default knobs:

```text
fast_scroll_threshold_px_per_s
slow_scroll_threshold_px_per_s
minimum_pause_ms_for_attention
minimum_hover_ms_for_attention
center_weight
edge_penalty
click_boost
selection_boost
return_boost
chapter_pause_ms
max_composite_height_px
```

These knobs should be repo-visible config, not hidden magic. Different surfaces
need different defaults: a Twitter feed, Codex chat, dense documentation page,
terminal, and PDF do not have the same reading rhythm.

## 9. Attention-Highlighted Screenshot

The final screenshot should preserve what the surface looked like while making
attention visible.

For a scroll surface, the compositor should first build a full linear screenshot
for development:

```text
full linear composite
  includes all observed scroll content
  dedupes overlap
  preserves order
  keeps skimmed sections
```

Then it applies an attention filter:

```text
attention-highlighted composite
  preserves natural item size and visual layout
  tints vertical rows/regions where attention concentrated
  leaves skimmed regions present but visually quiet
  attaches explanations for why each row was highlighted
```

For timeline-like surfaces such as Codex chat, the final composite should use
row or region tinting:

```text
more dwell time -> stronger tint on the natural row/region
fast skim -> no tint or low tint
click/selection/typing -> strong tint plus explanation metadata
return-to-region -> strong tint plus explanation metadata
```

This avoids a major flaw in attention-linear height: a large post, message, or
document panel should not become even larger just because it was visible for a
while. The screenshot stays faithful; the attention layer carries the meaning.

Suggested rendering rules:

```text
tint_low_threshold_ms
tint_medium_threshold_ms
tint_high_threshold_ms
center_weight
edge_penalty
interaction_boost
return_boost
show_attention_overlay_in_development
show_subtle_tint_in_final
```

## 10. Codex Chat Output

For this exact product surface, the target artifact is:

```text
one naturally stitched chat image
  + overlapping message regions deduped
  + scroll direction preserved
  + vertical rows/regions tinted by attention
  + skimmed regions retained but visually quiet
  + metadata explaining why highlighted rows were highlighted
  + optional text layer for search
  + attention marks for selected/copied/paused messages
  + time range covering the scroll session
```

Good output:

```yaml
seen_surface_id: "seen_codex_chat_20260524_171203"
surface_kind: "codex_chat"
label: "Codex chat about seen-surface screenshots"
time_range:
  start: "2026-05-24T17:12:03Z"
  end: "2026-05-24T17:13:10Z"
composite_image_ref:
  evidence_id: "composite_codex_chat_1"
  kind: "composite_image"
  uri: "blob://sha256/ab/cd/example"
  content_type: "image/png"
attention_regions:
  - region_id: "region_user_correction"
    reason: "scroll_pause"
    score: 0.86
    label: "User corrected the output contract toward what a person sees."
source_frame_refs:
  - evidence_id: "frame_001"
    kind: "source_frame"
    uri: "capture://frames/frame_001"
quality:
  confidence: 0.91
  warnings: []
```

The object does not need to say which app bundle or window ID produced the chat.
That is recoverable from the time range.

In a Codex chat, scrolling back often includes a lot of skimmed context. The
attention model should distinguish:

```text
messages that only flashed by during a fast scroll
messages centered during a pause
messages the user selected, copied, edited around, or responded to
messages the user returned to after scrolling away
```

The composite can show the whole scrolled region, but attention regions should
tell the viewer which parts were probably read closely.

Development output:

```text
full Codex scrollback composite
attention heat/mask overlay
final attention-highlighted chat composite
```

Final output:

```text
natural chat composite with subtle vertical attention bands and explanations
```

## 11. Web Page Output

For a webpage scroll, the output should be a seen-page composite:

```text
visual stitched page region
  + only the portions that actually entered view
  + browser chrome usually cropped away unless it mattered
  + optional text layer from DOM/PDF/AX/OCR
  + attention regions for pauses, clicks, selections, and slow reading
```

The system may use DOM, URL, Readability, and browser metadata to align the
composite. But it must not replace observed truth with page truth.

Bad output:

```text
"User read the whole article"
```

unless the whole article actually passed through the visible surface with enough
time or interaction evidence.

Better output:

```text
"User viewed the visible portions of a ScreenCaptureKit article, with strongest
attention around the dirty-rect and DOM snapshot sections."
```

## 12. Feed Output

Feeds are the hardest and probably the most important proof case.

The user may scroll through hundreds of posts, but only a handful matter. The
system should create:

```text
one or more feed composites
  + context posts retained visually
  + attention regions over the posts that likely mattered
  + no heavy per-post metadata in the primary object
```

Attention signals:

```text
scroll slowed or stopped
post was centered in the viewport
cursor hovered over post/media/link
user clicked, expanded, liked, replied, copied, selected, or opened
user returned to the same post after scrolling away
post remained visible for a long dwell interval
```

The default memory should say:

```text
"User skimmed a feed and lingered on these regions."
```

not:

```text
"Here are 143 extracted post records."
```

The extracted post records can exist elsewhere as time-correlated data. They
should not be the primary seen-surface output.

## 13. Composite Construction Rules

The compositor should optimize for human visual continuity:

```text
dedupe overlapping scroll regions
preserve reading order
crop stable browser/app chrome by default
retain chrome only when it carries meaning
prefer visual alignment over inferred DOM completeness
mark holes or low-confidence joins
split very long scrolls into chapters instead of making unusable giant images
```

Suggested chaptering rules:

```text
new chapter after a long pause
new chapter after navigation
new chapter after a major surface change
new chapter when the composite exceeds a practical image height
new chapter when attention shifts to a different task
```

## 14. Time Correlation Contract

The seen-surface object deliberately keeps metadata sparse because time is the
index.

Given:

```text
seen_surface.time_range = [t0, t1]
```

the system can recover:

```text
active app/window
browser URL/title/DOM snapshots
Accessibility snapshots
input events
source frames
OCR/text spans
agent/chat logs
terminal output
file changes
```

This means the object does not need to duplicate all of that. It only needs to
be a reliable visual memory anchor.

## 15. Agent Output

Agents should usually receive the minute-level attention capture:

```ts
AttentionCapturePacket {
  time_range: { start: string, end: string }
  attended_items: {
    label: string
    highlighted_composite_ref: EvidenceRef
    attention_summary: string
  }[]
  askable_evidence: string[]
}
```

Example:

```yaml
label: "Codex chat about seen-surface screenshots"
time_range:
  start: "2026-05-24T17:12:03Z"
  end: "2026-05-24T17:13:10Z"
attended_items:
  - label: "Codex chat correction about visual-first output"
    highlighted_composite_ref:
      evidence_id: "attention_composite_codex_1"
      kind: "composite_image"
      uri: "blob://sha256/ab/cd/example"
    attention_summary: "User paused on the correction that output should focus on what was actually seen."
askable_evidence:
  - "show final attention composite"
  - "show full development composite"
  - "show attention mask"
  - "show original frames"
```

The packet should not be a dense forensic report. It should point to up to three
visual artifacts that represent the minute.

## 16. Quality Bar

A good output passes these checks:

```text
Can a person open each final composite and recognize what mattered?
Does the composite include the scrolled content without obvious duplicate bands?
Does it avoid claiming unseen DOM/feed content was viewed?
Does it mark attention as a proxy, not as certain eye gaze?
Can the timeline recover source metadata from the time range?
Can the viewer open original frames if the composite looks suspicious?
Is the agent packet short enough to be useful in context?
Does development mode preserve the full pre-filtered composite?
Does final mode preserve natural layout while tinting high-attention rows?
```

Reject outputs that:

```text
lead with app/browser metadata instead of the visual surface
emit hundreds of records where one composite would do
summarize posts/pages the user did not actually see
make attention claims without dwell, pause, center, hover, click, selection, or return evidence
distort item height as a proxy for attention
lose the time range
hide stitching gaps or low-confidence joins
```

The product should feel like memory with eyes, not a compliance log.
