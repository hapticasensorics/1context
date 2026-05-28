# Screencap Attention Algorithm Notes

Status: working algorithm notes
Version: attention-ledger.v3
Last updated: 2026-05-25

These notes define the downstream system that turns dense screen capture,
accessibility, input monitoring, and app metadata into a compact attention
ledger.

The system should not be a screenshot classifier. It should be a state-change
accountant.

Core rule:

```text
Pixels are evidence.
Metadata is judgment.
Composites are compression.
Overlays are explanation.
```

The output should answer one question:

```text
What changed in the user's world, and what is the smallest visual or semantic
receipt that proves it?
```

## 1. Product Target

For every 30 to 60 second capture window, produce:

```text
raw capture buffer
  -> normalized observation timeline
  -> candidate states
  -> attention and memory signals
  -> delayed outcome links
  -> fused save/drop decisions
  -> saved visual receipts
  -> optional composites
  -> agent packet
  -> raw audit trail for development
```

The user-facing and agent-facing surface should stay compact:

```text
activity summary
saved visual receipts
attention overlay regions
semantic excerpts
reason codes
askable proof links
```

Raw frames and low-level metadata remain available in development mode, but
agents should consume the agent packet by default.

## 2. Product Non-Goals

Do not build:

```text
a forensic metadata dump
a keylogger output
a mouse trace output
a screenshot thumbnail gallery
a frame-diff classifier
a perfect gaze tracker
a fragile OCR-first memory system
```

The system may use all available metadata internally, but the final artifact
should feel like what a person saw, acted on, and cared about.

## 3. Skip The Toy V0

Do not ship a screenshot-only v0.

The first useful implementation should already include:

```text
candidate screenshot states
keyboard and pointer attention lanes
focus/selection target resolution
selected text hard keeps
scroll burst grouping
delayed stable outcome selection
near-duplicate rejection
raw-buffer audit explanations
data-driven mockup rendering
```

This is the minimum viable ledger slice. A simpler frame-reducer would teach the
wrong lessons and overfit to pixel changes.

Deferred after the first slice:

```text
terminal shell integration
editor/LSP adapters
filesystem/git events
browser network/download events
notification/toast source
full object lineage graph
true scroll mosaics
human-feedback ranker
```

## 4. Current Fixture

Generated capture samples now live outside the active docs tree. Use an
external sample pack when replaying this design note:

```text
<sample-pack>/
```

Important files:

```text
screen-recording.mov
manifest.json
contact-1fps.jpg
contact-2fps.jpg
frames-1fps/
frames-2fps/
snapshot-index.tsv
snapshots/
capture-events/run-window.events.jsonl
capture-events/run-window-event-counts.txt
```

Fixture summary:

```text
duration: 59.978333s
video: 2940 x 1912
raw video frames: 3220
candidate frames: 60 at 1fps, 120 at 2fps
capture snapshots: 61 attempted, 58 succeeded
run-window metadata events: 155
```

Event counts:

```text
60 capture.ax_focused_context
36 capture.ux.pointer.v1
19 capture.ux.scroll_burst.v1
13 capture.ux.keyboard_activity.v1
10 capture.active_window_frame_metadata
4  capture.ux.focus_transition.v1
4  capture.ax_semantic.focused_window_changed.v1
2  capture.ax_semantic.value_changed.v1
2  capture.ax_semantic.selected_text_changed.v1
2  capture.ax_semantic.focused_element_changed.v1
1  capture.ux.shortcut.v1
```

This fixture is good for:

```text
app/window transitions
selected-text hard keeps
keyboard bursts
pointer bursts
scroll bursts
near-duplicate rejection
raw-buffer audit explanations
saved-state budgeting
```

This fixture is not enough for:

```text
terminal command outcomes
permission matrix outcomes
true long-page scroll mosaics
browser DOM quality
editor/LSP diagnostic outcomes
```

## 5. Source Priority

Use semantic truth before pixels whenever possible.

```text
Tier 0: direct app-native truth
  browser DOM, URL, title, selection, input value
  editor buffer, diff, cursor, diagnostics
  terminal command, transcript, exit code
  filesystem save/modify events
  git state
  LSP diagnostics

Tier 1: OS semantic truth
  Accessibility tree
  focused element
  selected text
  window metadata
  app metadata
  input events
  clipboard metadata
  notification metadata

Tier 2: visual truth
  ScreenCaptureKit frames
  dirty rects
  perceptual hashes
  visual diffs
  layout regions
  OCR fallback

Tier 3: derived truth
  task episodes
  summaries
  inferred intent
  memory value estimates
```

Rules:

```text
Use pixels as receipts.
Use semantic sources as the ledger.
Use OCR only when better truth is missing.
Record source conflicts instead of hiding them.
```

## 6. Input Attention Lanes

Keyboard and pointer are first-class attention lanes.

Pointer tells us where the hand went. Keyboard tells us what operation the mind
committed to. Screen, DOM, AX, editor, and terminal state tell us what changed.

Normalize raw input into these lanes:

```text
pointer lane
  cursor sample
  hover dwell
  click / double click / right click
  drag
  scroll / trackpad gesture

keyboard lane
  typing burst
  shortcut command
  navigation key
  submit / dismiss
  selection expansion
  copy / paste / cut
  app/window switching
  command execution

target resolver
  focused AX element
  focused DOM node
  editor caret / selection
  terminal prompt / command line
  active text field
```

Mouse and cursor are related but not identical:

```text
mouse / trackpad = physical input events
cursor = current visual pointer location
```

Pointer evidence strength:

```text
click target > drag path > scroll gesture > hover dwell > parked cursor
```

Cursor dwell is useful, but weak by itself. Many people park the cursor while
reading. Clicks, selections, typing focus, copy/paste, and command execution are
stronger signals.

Keyboard events usually do not have coordinates. Resolve them through focus,
selection, caret, terminal, editor, DOM, and AX state.

```ts
export type InputEvent =
  | PointerAttentionEvent
  | KeyboardAttentionEvent
  | FocusAttentionEvent
  | ClipboardAttentionEvent

export type PointerAttentionEvent = {
  t_ms: number
  kind:
    | "cursor_sample"
    | "hover_dwell"
    | "click"
    | "double_click"
    | "right_click"
    | "drag"
    | "scroll"
  position?: Point
  target_region?: Rect
  target_node_id?: string
  attention_strength: number
  explanation: string
}

export type KeyboardAttentionEvent = {
  t_ms: number
  kind:
    | "typing_burst"
    | "shortcut"
    | "navigation"
    | "submit"
    | "dismiss"
    | "selection"
    | "copy"
    | "paste"
    | "cut"
    | "app_switch"
    | "command_execution"

  key?: string
  shortcut?: string

  focused_node_id?: string
  focused_element?: SceneNode
  caret_region?: Rect
  selection_region?: Rect
  selected_text_ref?: string
  inferred_region?: Rect

  attention_strength: number
  sensitive?: boolean
  explanation: string
}
```

Privacy rule:

```text
If the focused element is secure/password/credential-like:
  record event type, timing, app/window, and coarse region
  do not store typed text
  mark region as sensitive
  blur or redact screenshot regions according to policy
```

Prefer semantic text deltas over raw key streams:

```text
better: focused text field changed from value A to value B
worse: raw key stream h e l l o backspace o
```

Raw keys are useful for timing, shortcuts, and causality. They should not become
the final memory artifact.

## 7. Core Data Model

### AttentionFilterOutput

```ts
export type AttentionFilterOutput = {
  version: "attention-ledger.v3"
  capture_id: string
  time_range_ms: [number, number]

  summary: {
    activity_label: string
    activity_summary: string
    apps_seen: string[]
    windows_seen: string[]
    urls_seen?: string[]
    files_seen?: string[]
    commands_seen?: string[]
    confidence: number
  }

  agent_packet: AgentAttentionPacket
  saved_states: SavedAttentionState[]
  composites: AttentionComposite[]
  raw_buffer_audit: RawBufferItem[]
  attention_debt: AttentionDebtItem[]
  source_conflicts: SourceConflict[]
  algorithms: AlgorithmRunSummary[]
  policy: AttentionPolicy
  provenance_refs: ProvenanceRef[]
}
```

### ObservationFrame

Everything maps onto one shared time axis.

```ts
export type ObservationFrame = {
  id: string
  t_ms: number

  app: AppState
  window: WindowState
  screen: ScreenState

  browser?: BrowserState
  accessibility?: AccessibilityState
  editor?: EditorState
  terminal?: TerminalState
  filesystem?: FileSystemState
  git?: GitState
  lsp?: DiagnosticState
  network?: NetworkState
  notifications?: NotificationState
  clipboard?: ClipboardState

  semantic: {
    visible_text: TextSpan[]
    selected_text?: RedactableText
    focused_region?: Rect
    active_content_region?: Rect
    scene_nodes: SceneNode[]
    warnings: SemanticWarning[]
    errors: SemanticError[]
  }

  nearby_events: InputEvent[]
  source_confidence: Record<string, number>
}
```

### CandidateState

A candidate is a stable-enough visual state or tiny time slice that might become
a saved receipt.

```ts
export type CandidateState = {
  id: string
  frame_id: string
  t_ms: number

  app_name: string
  window_title: string
  url?: string
  active_file?: string
  terminal_command?: string

  image_ref: string
  thumb_ref: string
  content_region: Rect

  text_fingerprint: string
  visual_fingerprint: string
  ui_state_fingerprint: string
  object_lineage_ids: string[]

  semantic_text: string
  visible_text_spans: TextSpan[]
  scene_nodes: SceneNode[]
  nearby_events: InputEvent[]

  intent_primitives: IntentPrimitive[]
  signals: AttentionSignal[]

  attention_score?: number
  memory_value_score?: number
  preliminary_decision?: AttentionDecisionKind
}
```

## 8. Signal Contract

Algorithms emit signals. They do not directly decide final output.

```ts
export type AttentionSignal = {
  algorithm: AttentionAlgorithmName
  candidate_id: string

  kind:
    | "user_action"
    | "pointer_action"
    | "keyboard_action"
    | "command"
    | "semantic_novelty"
    | "visual_novelty"
    | "coverage"
    | "scroll_coverage"
    | "dwell"
    | "selection"
    | "clipboard_transfer"
    | "outcome"
    | "error"
    | "transition"
    | "task_relevance"
    | "memory_value"
    | "source_conflict"
    | "attention_debt"
    | "sensitive_surface"
    | "redundancy"
    | "animation_noise"
    | "ui_chrome_noise"
    | "low_information"

  strength: number
  hard_keep?: boolean

  suggested_decision?:
    | "save_high_attention"
    | "save_high_memory"
    | "save_coverage"
    | "save_transition"
    | "save_outcome"
    | "save_error"
    | "save_sensitive_redacted"
    | "merge_into_composite"
    | "reject_duplicate"
    | "reject_low_information"
    | "reject_animation"
    | "reject_ui_chrome_noise"

  region?: AttentionRegion
  explanation: string
  provenance_refs: ProvenanceRef[]
}
```

Region overlays should explain attention without altering the base screenshot.

```ts
export type AttentionRegion = {
  bbox: Rect
  score: number

  tint:
    | "orange" // high attention
    | "green"  // coverage
    | "blue"   // transition
    | "purple" // outcome
    | "red"    // error
    | "gray"   // rejected / low information
    | "yellow" // selection / copied text

  label: string
  reasons: string[]
  evidence: string[]
  sensitive?: boolean
}
```

## 9. Algorithm Roster

### 1. Candidate Builder And Redundancy Baseline

Purpose:

```text
Create candidate states and identify obvious duplicate or low-information
frames before reasoning gets expensive.
```

Inputs:

```text
frames-2fps/
snapshot-index.tsv
capture snapshots
run-window events
perceptual image hashes
simple visual diffs
app/window timeline
```

Outputs:

```text
CandidateState[]
visual_novelty signals
semantic_novelty signals when text is available
transition signals when app/window changes
redundancy signals for near-duplicates
low_information signals for repeated stable frames
```

### 2. Source Confidence Arbiter

Purpose:

```text
Assign confidence to semantic fields and detect conflicts across sources.
```

Example:

```text
DOM says page title changed.
Screenshot still shows old title.
Accessibility tree lagged.
Decision: delay final state until stable or mark source_conflict.
```

Default confidence hints:

```text
browser.url: 0.99
editor.diff: 0.97
terminal.exit_code: 0.97
ax.focused_node: 0.82
ocr.visible_text: 0.55
visual_inference: 0.40
```

### 3. Critical Event Keeper

Purpose:

```text
Never miss high-stakes semantic events.
```

Hard saves:

```text
selected text
copied text
cut/paste transfer
terminal command completion
terminal error
build/test failure
submit outcome
permission/security state change
modal confirmation
form submission
new warning/error
file save
new diagnostics
URL navigation
download/upload completion
visible error after action
```

Implementation shape:

```ts
for (const candidate of candidates) {
  if (hasSelectedText(candidate)) hardKeep("selection")
  if (hasCopiedOrPastedContent(candidate)) hardKeep("clipboard_transfer")
  if (hasKeyboardCommitOutcome(candidate)) hardKeep("outcome")
  if (hasTerminalFailure(candidate)) hardKeep("error")
  if (permissionStateChanged(candidate)) hardKeep("outcome")
  if (hasNewDiagnostics(candidate)) hardKeep("error")
}
```

### 4. Event-Causal Filter

Purpose:

```text
Connect action -> target -> visible or semantic result.
```

Questions:

```text
What did the user do?
What object did it target?
What changed afterward?
What receipt proves it?
```

Inputs:

```text
clicks
typing bursts resolved through focused target
keyboard shortcuts
copy/paste/cut
submit/dismiss/navigation keys
scroll bursts
focus transitions
AX focused element
DOM clicked node when available
DOM/AX value changes
editor caret and selection
terminal prompt and command line
semantic diff
visual diff
```

### 5. Delayed Outcome Linker

Purpose:

```text
Find the stable result frame after an action, not merely the next frame.
```

Sampling window:

```text
t + 150ms
t + 500ms
t + 1200ms
t + 2500ms
```

Long-running actions may stay pending longer:

```text
100ms to 5s for ordinary UI actions
100ms to 60s for terminal/build/test actions
```

Outputs:

```text
action_state
pending_state
stable_outcome_state
changed_region
semantic_delta
```

Rule:

```text
Save the stable outcome, not the twitchy middle frame.
```

### 6. Scroll Coverage Reconstructor

Purpose:

```text
Treat scrolling as document traversal, not as a pile of screenshots.
```

V1 session detection:

```text
same app/window
nearby scroll_burst events
keyboard scroll events such as Space, PageDown, PageUp, arrows, or app-specific keys
dominant vertical visual motion
stable chrome/header/sidebar
new content entering viewport
```

Anchor selection:

```text
first visible section
new heading
high-dwell paragraph
selected/copied text
major coverage jump
end/outcome state
```

V1 output can be a contact-sheet composite. True mosaic generation is deferred.

### 7. UI Chrome And Animation Noise Mask

Purpose:

```text
Prevent browser chrome, cursor blink, spinners, skeletons, ads, and layout twitch
from consuming budget.
```

Reject or downweight when:

```text
only cursor blink changed
only spinner changed
only progress shimmer changed
only scrollbar moved without new content
only tab hover changed
only menu bar/dock changed
only irrelevant chrome flickered
```

### 8. Region Attention Heatmap

Purpose:

```text
Draw honest overlays around meaningful regions.
```

Inputs:

```text
selection region
focused AX node
focused DOM node
click target
caret region
editor diff
terminal command/result
toast region
diagnostic region
permission row
changed DOM subtree
viewport center during scroll pause
```

Do not hallucinate gaze. Coarse is better than fake precision.

### 9. Semantic Novelty Selector

Purpose:

```text
Select the smallest set of states that explains the most unique information.
```

Gain:

```ts
gain =
  unique_text_coverage
+ unique_ui_state_coverage
+ uncovered_event_coverage
+ unresolved_attention_debt
+ object_lineage_coverage
+ outcome_importance
+ error_importance
+ memory_value
- redundancy_penalty
- ui_chrome_noise_penalty
- sensitivity_penalty
```

### 10. Attention Debt Resolver

Purpose:

```text
Force the selector to cover unresolved meaningful events before spending budget
on attractive but low-value screenshots.
```

Debt examples:

```text
keyboard burst with no saved outcome
terminal command with no captured exit
copy event with no source or destination
app switch with no transition state
scroll session with no anchor
error appeared but no receipt
submit happened but no result was captured
```

### 11. Object Lineage Tracker

Purpose:

```text
Connect the same object across states and apps.
```

Examples:

```text
selected text -> copied text -> pasted text
terminal command -> output -> exit code
file edit -> save -> diagnostic change
browser URL -> loaded page -> selected paragraph
permission row -> granted state
```

This is useful in the first ledger slice for clipboard and selection, but the
general graph can deepen later.

### 12. Sensitive Surface Redactor

Purpose:

```text
Preserve useful event metadata while protecting content.
```

Behavior:

```text
save coarse event
redact sensitive text
blur sensitive region if screenshot retained
preserve app/window/time/action/proof type
```

Sensitive classes:

```text
passwords
credentials
API keys
tokens
2FA codes
SSH keys
payment forms
medical / legal / financial portals
private messages
secret env files
```

### 13. Proof Bundle Builder

Purpose:

```text
Attach the right amount of proof to each observation.
```

Bundle tiers:

```text
semantic_only
semantic_plus_screenshot
visual_required
raw_replay_required
redacted_sensitive
```

Not every saved state needs a screenshot. Visual receipts are required when the
proof is visual or when a human needs to audit what was on screen.

### 14. Agent Packet Compiler

Purpose:

```text
Give agents the smallest useful memory object.
```

Agents get:

```text
what happened
why it mattered
semantic excerpts
state IDs
composite IDs
confidence
askable proof refs
```

Agents do not get raw frames by default.

## 10. Decision Priority

Final decision priority:

```text
1. sensitive hard event -> save_sensitive_redacted
2. error/failure -> save_error
3. completed outcome -> save_outcome
4. terminal/build/test result -> save_outcome or save_error
5. clipboard transfer -> save_high_attention
6. selected text -> save_high_attention
7. permission/security change -> save_outcome
8. modal confirmation -> save_outcome
9. app/window/task transition -> save_transition
10. scroll/document coverage -> save_coverage or merge_into_composite
11. high memory value -> save_high_memory
12. high semantic novelty -> save_high_attention
13. visual-only novelty -> save only if visual task or no semantic source exists
14. animation/chrome/cursor noise -> reject_ui_chrome_noise
15. near duplicate -> reject_duplicate
16. low information -> reject_low_information
```

Expose reason codes, not score soup.

## 11. Fusion And Scoring

Use score components for ranking and debugging. Do not make the user read the
math as the product.

```ts
finalScore =
  0.18 * pointerActionScore
+ 0.18 * keyboardActionScore
+ 0.16 * semanticNoveltyScore
+ 0.14 * scrollCoverageScore
+ 0.12 * regionDwellScore
+ 0.12 * outcomeScore
+ 0.10 * taskRelevanceScore
+ 0.16 * errorScore
+ 0.08 * transitionScore
+ 0.08 * selectionScore
- 0.25 * redundancyPenalty
- 0.18 * animationNoisePenalty
- 0.12 * lowInformationPenalty
```

Memory value is separate from attention:

```text
attention_score = how much the user likely cared in the moment
memory_value_score = how useful this state is likely to be later
```

Examples:

```text
high attention, low memory:
  user wiggles pointer over same page

low attention, high memory:
  final stable permission matrix
  final terminal failure
  selected text before copy
  completed upload
```

## 12. Budget Policy

```ts
export const DEFAULT_ATTENTION_POLICY_V3 = {
  window_ms: 30_000,

  max_saved_states: 8,
  max_saved_states_per_app: 4,
  max_composites: 3,

  hard_keep_overflow: 4,
  absolute_saved_state_cap: 12,

  always_save_errors: true,
  always_save_selections: true,
  always_save_clipboard_transfers: true,
  always_save_keyboard_commit_outcomes: true,
  always_save_permission_changes: true,
  always_save_terminal_outcomes: true,
  always_save_modal_confirmations: true,
  always_save_new_diagnostics: true,

  redact_sensitive_text_inputs: true,
  blur_sensitive_visual_regions: true,

  min_information_gain: 0.22,
  min_memory_value_gain: 0.18,
  duplicate_similarity_threshold: 0.88,
  ui_chrome_noise_threshold: 0.75,
  source_conflict_threshold: 0.30,

  max_raw_buffer_thumbnails: 80,
  raw_frame_retention_ms: 10 * 60 * 1000
}
```

Hard keeps may exceed the normal budget, but only inside the overflow cap.
Otherwise an error-spamming terminal or rapidly changing UI can still flood the
agent.

## 13. Output Artifacts

### SavedAttentionState

```ts
export type SavedAttentionState = {
  id: string

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

  app_name: string
  window_title: string
  url?: string
  active_file?: string
  terminal_command?: string

  base_screenshot_ref?: string
  thumbnail_ref?: string
  overlay_regions: AttentionRegion[]

  semantic_excerpt: string
  redaction_summary?: string

  explanation: {
    primary_reason: string
    reasons: string[]
    attention_score: number
    memory_value_score: number
    confidence: number
    score_components: Record<string, number>
    algorithm_votes: AlgorithmVote[]
    source_conflicts?: SourceConflict[]
  }

  proof_bundle: ProofBundle
  related_composite_ids: string[]
  related_object_lineage_ids: string[]
  provenance_refs: ProvenanceRef[]
}
```

### AgentAttentionPacket

```ts
export type AgentAttentionPacket = {
  time_range_ms: [number, number]
  activity_summary: string
  confidence: number

  important_observations: Array<{
    kind:
      | "high_attention"
      | "high_memory"
      | "coverage"
      | "transition"
      | "outcome"
      | "error"
      | "sensitive_redacted"

    summary: string
    evidence_state_id: string
    confidence: number
    proof_tier:
      | "semantic_only"
      | "semantic_plus_screenshot"
      | "visual_required"
      | "raw_replay_required"
      | "redacted_sensitive"
  }>

  extracted_text: Array<{
    source:
      | "browser_dom"
      | "accessibility"
      | "editor"
      | "terminal"
      | "ocr"
      | "clipboard"
      | "notification"
      | "lsp"

    text: string
    state_ids: string[]
    confidence: number
    sensitive?: boolean
  }>

  composites: Array<{
    id: string
    type:
      | "scroll_document"
      | "before_after_diff"
      | "contact_sheet"
      | "terminal_run"
      | "permission_matrix"
      | "object_lineage"

    summary: string
  }>

  askable_evidence: Array<{
    label: string
    ref: string
    proof_tier: string
  }>
}
```

### RawBufferAuditItem

```ts
export type RawBufferAuditItem = {
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

  top_signals: AttentionSignal[]
  score_components: Record<string, number>
  nearest_saved_state_id?: string
  explanation: string
}
```

## 14. Mockup Requirements

The mockup should become a pure renderer for a generated filter output:

```text
<sample-pack>/attention-filter-output.json
```

Required tabs:

```text
Final output
Agent packet
Saved states
Composites
Object lineage
Development raw buffer
Source conflicts
Algorithm votes
JSON
```

Final output cards should show:

```text
title
time
app/window
decision
primary reason
confidence
attention score
memory value score
overlay
semantic excerpt
proof bundle
askable raw proof
```

Development raw buffer should show every candidate:

```text
thumbnail
decision tint
score
source confidence
top signals
attention debt paid
similar saved state
new text tokens
reason for save/drop
```

## 15. Validation Plan

The validation loop is human-in-the-loop by design:

```text
watch the footage
inspect contact sheet
inspect algorithm output
label saves and drops
adjust policy or algorithms
rerun deterministically
```

Review labels:

```text
must_save
good_save
acceptable_drop
bad_save
missed_save
wrong_region
wrong_reason
too_sensitive
not_sensitive
```

Metrics:

```text
must-save recall:
  Did the output keep the states a person says mattered?

noise rate:
  How many saved states feel redundant or irrelevant?

compression:
  120 candidates -> how many saved states and composites?

explanation quality:
  Does each save/drop reason match the visual evidence?

temporal alignment:
  Did the system save the stable outcome instead of a half-rendered state?

region quality:
  Do overlays point at what mattered instead of chrome, sidebars, dock, or menus?

sensitivity quality:
  Did the system redact sensitive surfaces without losing useful structure?
```

Main question:

```text
If I watched the footage and then looked at the output, would I agree that this
captures what mattered?
```

## 16. Acceptance Fixtures

### Current Fixture

Use an external sample pack with a representative one-minute capture:

```text
<sample-pack-id>
```

Expected saves:

```text
early Chrome / ChatGPT state
VS Code notes transition
Chrome return or selected-text state
Codex transition
Codex scroll / typing high-attention state
final stable Codex state
```

Expected rejects:

```text
near-identical Chrome frames
near-identical VS Code frames
most intermediate scroll frames
white-page / Codex repeats with no semantic signal
```

Expected hard keeps:

```text
selected_text_changed events
keyboard_activity events when they cause semantic or visible outcomes
meaningful focus/window transitions
shortcut event if it caused visible state change
```

Known caveats:

```text
metadata-sample timed out during simultaneous recording
snapshot samples 007 to 009 failed due to temporary daemon socket unavailability
pointer events do not expose raw coordinates, so v1 region heatmap must be coarse
```

### Keyboard-Heavy Session

Input:

```text
Cmd+L search or URL navigation
Cmd+F find inside page
typing into focused field
Enter submit
Cmd+C / Cmd+V transfer
Esc dismiss
```

Expected:

```text
keyboard actions resolved through focus/caret/selection
commit outcomes saved
raw key stream not exposed
secure fields redacted
parked cursor ignored unless paired with stronger evidence
```

### Scrolling Document

Input:

```text
80 screenshots over 30 seconds
continuous scroll
mostly text
```

Expected:

```text
1 scroll composite
2 to 4 anchor states
most intermediate scroll frames merged or dropped
text coverage preserved where DOM/AX/OCR is available
```

### Terminal Run Session

Input:

```text
command typed
command submitted
long output
failure exit code
fix
success exit code
```

Expected:

```text
command start saved only if useful
failure hard saved
success saved as outcome
stdout/stderr summarized
intermediate terminal spam dropped
```

### Editor Diagnostics Session

Input:

```text
edit file
diagnostic appears
save
diagnostic disappears
test passes
```

Expected:

```text
diff saved semantically
diagnostic delta captured
before/after saved around resolution
screenshots used as receipts
```

### Clipboard Transfer Session

Input:

```text
select text in browser
copy
switch app
paste into editor/chat
submit
```

Expected:

```text
source selection hard saved
destination paste hard saved
object lineage connects transfer
sensitive clipboard redacted
```

### Visual Design Session

Input:

```text
Figma/canvas edits
shape moves
color changes
visual layout changes
```

Expected:

```text
visual novelty allowed higher weight
screenshot is primary receipt
semantic metadata used where available
layout regions highlighted
```

### Permission Setup Session

Input:

```text
permission rows visible
one permission denied
user grants
all green
```

Expected:

```text
final matrix saved
changed row highlighted
duplicates dropped
agent packet says setup state
```

## 17. Build Order

### Pass 1: Data-Driven Mockup

Build the renderer first.

```text
<sample-pack>/attention-filter-output.json
<generated-output>/attention-output-mockup.html
```

Do not hand-maintain the mockup. The mockup should render from the same JSON
that the algorithm harness emits.

### Pass 2: First Ledger Slice

This intentionally skips a toy screenshot-only v0.

Implement:

```text
src/attention/normalizeRawCapture.ts
src/attention/normalizeInputEvents.ts
src/attention/resolveInputTargets.ts
src/attention/buildCandidateStates.ts
src/attention/sourceConfidenceArbiter.ts
src/attention/algorithms/redundancyNoiseDetector.ts
src/attention/algorithms/criticalEventKeeper.ts
src/attention/algorithms/eventCausalFilter.ts
src/attention/algorithms/delayedOutcomeLinker.ts
src/attention/algorithms/timeToStableSampler.ts
src/attention/fuseCandidateSignals.ts
src/attention/semanticNoveltySelector.ts
src/attention/buildAttentionFilterOutput.ts
src/attention/renderAttentionFilterOutput.ts
```

Use:

```text
frames-2fps/
snapshot-index.tsv
capture-events/run-window.events.jsonl
manifest.json
```

Pass 2 must produce a data-driven `attention-filter-output.json` that can be
loaded by the mockup and judged against the footage.

### Pass 3: Ledger Mechanics

Implement:

```text
src/attention/attentionDebtResolver.ts
src/attention/objectLineageTracker.ts
src/attention/proofBundleBuilder.ts
src/attention/sensitiveSurfaceClassifier.ts
src/attention/redactionPolicy.ts
```

This is where the output becomes more than a frame reducer.

### Pass 4: App-Native Sources

Implement adapters:

```text
src/attention/adapters/browserAdapter.ts
src/attention/adapters/editorAdapter.ts
src/attention/adapters/terminalAdapter.ts
src/attention/adapters/clipboardAdapter.ts
src/attention/adapters/filesystemAdapter.ts
src/attention/adapters/lspAdapter.ts
src/attention/adapters/notificationAdapter.ts
```

### Pass 5: Composites

Implement:

```text
src/attention/composites/scrollCompositeBuilder.ts
src/attention/composites/terminalRunCompositeBuilder.ts
src/attention/composites/beforeAfterDiffCompositeBuilder.ts
src/attention/composites/permissionMatrixCompositeBuilder.ts
src/attention/composites/objectLineageCompositeBuilder.ts
```

### Pass 6: Feedback Loop

Implement:

```text
src/attention/review/humanReviewLabels.ts
src/attention/review/attentionRankerCalibration.ts
src/attention/review/perAppPolicyOverrides.ts
```

## 18. Revised Rulebook

```text
1. Never save because pixels changed alone.
2. Always save high-stakes semantic events.
3. Prefer app-native truth over OCR and visual inference.
4. Use screenshots as receipts, not primary memory, unless the task is visual.
5. Resolve keyboard events through focus, caret, selection, editor, terminal, DOM, and AX.
6. Treat clipboard, terminal, editor diagnostics, downloads, uploads, and notifications as first-class sources.
7. Save stable outcomes, not twitchy intermediate frames.
8. Merge scroll frames into documents.
9. Collapse setup and permission flows into final outcome states.
10. Preserve screenshots and render attention as overlay metadata.
11. Track object lineage across apps and time.
12. Maintain source confidence and expose uncertainty when evidence conflicts.
13. Redact sensitive text while preserving useful event structure.
14. Every saved or rejected frame needs a believable explanation.
15. Agents receive packets, not raw buffers.
16. Dev UI can inspect raw buffer decisions and algorithm votes.
17. Human labels should tune the ranker.
18. Attention and memory value are related but not identical.
19. Budget is sacred, except capped hard-keep overflow.
20. The final output should feel like a proof ledger, not a thumbnail gallery.
```

## 19. North Star

Turn messy human-computer activity into a compact, auditable memory ledger:

```text
semantic truth first
visual receipts second
everything explainable
nothing important lost
```
