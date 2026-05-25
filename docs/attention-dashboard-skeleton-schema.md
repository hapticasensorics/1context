# Attention Dashboard Skeleton Schema

Status: implementation contract
Version: attention-dashboard.v1
Last updated: 2026-05-25

This dashboard is the human judgment surface for the attention ledger. It should
let a reviewer watch the original activity recording, inspect the attention
filter output, label mistakes, and rerun algorithms against the same fixture.

The app should be native Rust, not a browser app. The first implementation
should use `eframe`/`egui`, either by extending the existing
`crates/onecontext-capture-dashboard` patterns or by creating a sibling crate:

```text
crates/onecontext-attention-dashboard
```

The dashboard is not the algorithm. It is the judge bench. Algorithms can be
implemented independently, but they all have to emit the same JSON contract so
the dashboard can judge the combined result.

## 1. Design Goal

The dashboard answers:

```text
After watching the footage, does the attention output preserve what mattered?
```

Primary layout:

```text
left:   video / frame-cache playback
right:  attention filter output for current time
bottom: synchronized event + candidate + decision timeline
```

The reviewer should be able to:

```text
scrub video
click a candidate frame
see saved/dropped/merged reasoning
toggle algorithm lanes
inspect attention overlays
apply human labels
export review labels
compare runs
```

## 2. Native Video Policy

The Rust app should own playback timing and rendering. Do not put a webview
between the reviewer and the video.

First pass:

```text
screen-recording.mov
  -> ffmpeg/frame-cache decode step
  -> timestamped JPEG/PNG frames
  -> egui texture playback and scrubber
```

The fixture already has `frames-1fps/` and `frames-2fps/`. Those are enough for
initial candidate inspection, but real review needs a denser playback frame
cache.

Expected media modes:

```text
frame_cache:
  required for first implementation
  deterministic
  fast scrubbing
  easy screenshots

native_decoder:
  optional later
  uses ffmpeg/gstreamer/avfoundation binding
  avoids large frame cache

external_player:
  debug fallback only
  cannot be the main review surface
```

## 3. Files

Per fixture:

```text
attention-dashboard-session.json
attention-filter-output.json
review-labels.jsonl
frame-cache/
  frame-000000.jpg
  frame-000001.jpg
  index.jsonl
```

Existing fixture root:

```text
docs/assets/attention-capture-mockup/attention-debug-20260524-215739/
```

Initial dashboard session path:

```text
docs/assets/attention-capture-mockup/attention-debug-20260524-215739/attention-dashboard-session.json
```

Machine-readable schema:

```text
docs/schemas/attention-dashboard-session.schema.json
```

## 4. Dashboard Session Schema

The dashboard loads one session manifest. The manifest points to the video,
frame cache, candidate frames, filter output, event files, and label output.

```ts
export type AttentionDashboardSession = {
  schema_version: "attention-dashboard.v1"
  session_id: string
  title: string
  created_at: string

  fixture: DashboardFixture
  media: DashboardMedia
  inputs: DashboardInputs
  filter_output: FilterOutputRef
  review: ReviewConfig
  ui: DashboardUiConfig
  agent_work_packages: AgentWorkPackage[]
}
```

### DashboardFixture

```ts
export type DashboardFixture = {
  run_id: string
  root: string
  duration_ms: number
  timezone?: string
  notes?: string

  source_manifest_ref?: string
  source_readme_ref?: string
}
```

### DashboardMedia

```ts
export type DashboardMedia = {
  video_ref: string
  video_width: number
  video_height: number
  video_duration_ms: number
  video_fps?: number

  playback_mode: "frame_cache" | "native_decoder" | "external_player"

  frame_cache?: {
    root: string
    index_ref: string
    frame_width: number
    frame_height: number
    fps: number
    format: "jpg" | "png"
  }

  candidate_frame_sets: Array<{
    id: string
    root: string
    fps: number
    count: number
    naming: string
  }>
}
```

### DashboardInputs

```ts
export type DashboardInputs = {
  candidate_index_ref?: string
  snapshots_root?: string

  event_refs: Array<{
    id: string
    kind:
      | "capture_events"
      | "snapshot_index"
      | "window_metadata"
      | "ax_snapshots"
      | "browser_events"
      | "terminal_events"
      | "review_labels"
      | "algorithm_output"
    ref: string
    format: "json" | "jsonl" | "tsv"
    required: boolean
  }>

  timeline_lanes: TimelineLaneConfig[]
}
```

### FilterOutputRef

```ts
export type FilterOutputRef = {
  ref: string
  schema_version: "attention-ledger.v3"
  generated_by?: string
  generated_at?: string
}
```

### ReviewConfig

```ts
export type ReviewConfig = {
  labels_ref: string
  autosave: boolean

  allowed_labels: ReviewLabelKind[]

  required_metrics: Array<
    | "must_save_recall"
    | "bad_save_rate"
    | "compression_ratio"
    | "stable_outcome_accuracy"
    | "region_quality"
    | "reason_quality"
    | "sensitivity_quality"
  >
}
```

### DashboardUiConfig

```ts
export type DashboardUiConfig = {
  default_left_panel: "video"
  default_right_panel:
    | "current_decision"
    | "saved_states"
    | "agent_packet"
    | "raw_audit"
  default_bottom_panel: "timeline"

  enabled_tabs: Array<
    | "final_output"
    | "agent_packet"
    | "saved_states"
    | "composites"
    | "object_lineage"
    | "raw_buffer"
    | "source_conflicts"
    | "algorithm_votes"
    | "json"
  >

  feature_flags: {
    algorithm_toggles: boolean
    ablation_compare: boolean
    label_export: boolean
    overlay_editor: boolean
    side_by_side_runs: boolean
  }
}
```

## 5. Timeline Schema

The bottom timeline is the spine of the review experience. Every lane maps
events to the same `t_ms` coordinate.

```ts
export type TimelineLaneConfig = {
  id: string
  title: string
  kind:
    | "video"
    | "candidate_frames"
    | "saved_states"
    | "keyboard"
    | "pointer"
    | "scroll"
    | "selection"
    | "window_focus"
    | "source_conflict"
    | "attention_debt"
    | "algorithm"
    | "review_labels"
  visible: boolean
  color: string
  source_ref?: string
}
```

Runtime event shape:

```ts
export type TimelineEvent = {
  id: string
  lane_id: string
  t_ms: number
  duration_ms?: number

  title: string
  kind: string
  severity?: "info" | "attention" | "warning" | "error"

  candidate_id?: string
  saved_state_id?: string
  raw_event_ref?: string
  tooltip?: string
}
```

## 6. Attention Output Subset

The dashboard does not need every algorithm-internal field, but it needs enough
to judge save/drop quality.

```ts
export type DashboardAttentionOutput = {
  version: "attention-ledger.v3"
  capture_id: string
  time_range_ms: [number, number]

  summary: {
    activity_label: string
    activity_summary: string
    confidence: number
  }

  saved_states: DashboardSavedState[]
  raw_buffer_audit: DashboardRawBufferItem[]
  composites: DashboardComposite[]
  agent_packet: DashboardAgentPacket
  source_conflicts: DashboardSourceConflict[]
  attention_debt: DashboardAttentionDebt[]
  algorithms: DashboardAlgorithmSummary[]
}
```

### DashboardSavedState

```ts
export type DashboardSavedState = {
  id: string
  candidate_id: string
  decision:
    | "save_high_attention"
    | "save_high_memory"
    | "save_coverage"
    | "save_transition"
    | "save_outcome"
    | "save_error"
    | "save_sensitive_redacted"

  title: string
  time_ms: number
  duration_ms?: number

  app_name: string
  window_title: string
  url?: string
  active_file?: string
  terminal_command?: string

  base_screenshot_ref?: string
  thumbnail_ref?: string
  overlay_regions: AttentionRegion[]

  semantic_excerpt?: string
  redaction_summary?: string

  explanation: DecisionExplanation
  proof_bundle: ProofBundleSummary
  related_composite_ids: string[]
  related_object_lineage_ids: string[]
}
```

### DashboardRawBufferItem

```ts
export type DashboardRawBufferItem = {
  candidate_id: string
  frame_id: string
  t_ms: number
  thumbnail_ref: string

  decision:
    | "saved"
    | "merged"
    | "rejected_duplicate"
    | "rejected_low_information"
    | "rejected_ui_chrome_noise"
    | "rejected_sensitive"

  nearest_saved_state_id?: string
  top_signals: DashboardSignal[]
  score_components: Record<string, number>
  explanation: string
}
```

### Shared Decision Types

```ts
export type AttentionRegion = {
  bbox: Rect
  score: number
  tint: "orange" | "green" | "blue" | "purple" | "red" | "gray" | "yellow"
  label: string
  reasons: string[]
  evidence: string[]
  sensitive?: boolean
}

export type DashboardSignal = {
  algorithm: string
  kind: string
  strength: number
  hard_keep?: boolean
  region?: AttentionRegion
  explanation: string
}

export type DecisionExplanation = {
  primary_reason: string
  reasons: string[]
  attention_score: number
  memory_value_score: number
  confidence: number
  score_components: Record<string, number>
  algorithm_votes: AlgorithmVote[]
  source_conflicts?: string[]
}

export type AlgorithmVote = {
  algorithm: string
  vote:
    | "save"
    | "merge"
    | "drop"
    | "redact"
    | "uncertain"
  reason: string
  strength: number
}

export type Rect = {
  x: number
  y: number
  width: number
  height: number
  coordinate_space: "screen_px" | "image_px" | "normalized"
}
```

## 7. Review Label Schema

Human labels are append-only JSONL. This lets agents compare algorithm runs
against human judgment without mutating source outputs.

```ts
export type ReviewLabelEvent = {
  schema_version: "attention-review-label.v1"
  label_id: string
  session_id: string
  created_at: string

  target:
    | { kind: "candidate"; candidate_id: string; t_ms: number }
    | { kind: "saved_state"; saved_state_id: string; candidate_id?: string }
    | { kind: "region"; saved_state_id: string; region_index: number }
    | { kind: "time_range"; start_ms: number; end_ms: number }

  label: ReviewLabelKind
  note?: string
  replacement_decision?: string
  expected_region?: Rect
}

export type ReviewLabelKind =
  | "must_save"
  | "good_save"
  | "acceptable_drop"
  | "bad_save"
  | "missed_save"
  | "wrong_region"
  | "wrong_reason"
  | "too_sensitive"
  | "not_sensitive"
```

## 8. App Screens

### Video Judge

Left panel:

```text
video/frame-cache playback
play/pause
scrubber
step previous/next candidate
step previous/next saved state
current timestamp
current overlay toggle
```

Right panel:

```text
current candidate or saved state
decision
reason codes
attention/memory scores
top signals
source confidence
proof tier
human label buttons
```

Bottom panel:

```text
candidate strip
saved/drop/merge bands
keyboard lane
pointer lane
scroll lane
selection lane
focus transition lane
attention debt lane
source conflict lane
review labels lane
```

### Final Output

Shows only the compact ledger:

```text
summary
saved states
composites
agent packet
askable evidence
```

### Raw Audit

Shows every candidate:

```text
thumbnail
timestamp
decision
nearest saved state
top signals
score components
reason for save/drop
review labels
```

## 9. Four-Agent Implementation Split

Each agent should work against the same session manifest and attention-output
schema. Do not let agents invent separate JSON shapes.

### Agent 1: Native App Shell And Media

Owns:

```text
new Rust crate or capture-dashboard mode
CLI args
session manifest loading
frame-cache decoding/index loading
video playback controls
texture cache
timestamp synchronization
```

Deliverables:

```text
cargo run -p onecontext-attention-dashboard -- --session <path>
left video panel
scrubber
candidate timestamp seeking
basic error UI for missing assets
```

### Agent 2: Timeline And Lanes

Owns:

```text
timeline model
candidate strip
saved/drop/merge bands
keyboard/pointer/scroll/selection/focus lanes
lane visibility toggles
click-to-seek
hover tooltips
```

Deliverables:

```text
bottom timeline panel
TimelineEvent parser
lane rendering from manifest + filter output
current-time marker synchronized to video
```

### Agent 3: Attention Output Inspector

Owns:

```text
right-side decision panel
saved states tab
raw buffer audit tab
agent packet tab
source conflicts tab
algorithm votes tab
overlay rendering on video/screenshot
```

Deliverables:

```text
current candidate detail view
saved state cards
score and reason views
attention overlay drawing
JSON inspector for selected item
```

### Agent 4: Human Review And Metrics

Owns:

```text
review label UI
append-only review-labels.jsonl writer
label lane
metrics panel
run comparison hooks
schema validation CLI
sample session generation for existing fixture
```

Deliverables:

```text
label buttons
autosave labels
metrics summary
review export
fixture bootstrap command
schema validation command
```

## 10. Shared Rust Module Shape

Suggested crate layout:

```text
crates/onecontext-attention-dashboard/
  Cargo.toml
  src/
    main.rs
    app.rs
    schema.rs
    fixture.rs
    media/
      mod.rs
      frame_cache.rs
    timeline/
      mod.rs
      lanes.rs
      events.rs
    panels/
      video.rs
      decision.rs
      saved_states.rs
      raw_audit.rs
      agent_packet.rs
      metrics.rs
    review/
      labels.rs
      metrics.rs
      writer.rs
```

Minimum dependencies:

```toml
anyhow = "1"
eframe = { version = "0.34.2", default-features = false, features = ["default_fonts", "glow"] }
egui_extras = { version = "0.34.2", features = ["image"] }
image = "0.25"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
time = { version = "0.3", features = ["formatting", "parsing", "serde"] }
```

Optional later:

```toml
ffmpeg-next = "8"
rfd = "0.15"
schemars = "0.8"
jsonschema = "0.26"
```

First pass can shell out to `ffmpeg` for frame-cache generation instead of
embedding a video decoder. That keeps the Rust UI simple and deterministic.

## 11. Definition Of Done

The skeleton is done when:

```text
1. It opens the current fixture from attention-dashboard-session.json.
2. It shows playback on the left.
3. It shows saved/drop/merge decisions on the right.
4. It renders a synchronized bottom timeline.
5. It can jump from timeline item to video timestamp.
6. It can apply and persist review labels.
7. It can load a new attention-filter-output.json without code changes.
8. It has at least one generated sample session for the existing fixture.
```

The first benchmark is not an automated score. It is the reviewer saying:

```text
I watched the footage, then inspected the ledger, and the misses are visible
enough for us to improve the algorithms.
```
