# Attention Source Arbitration Scaffold

This scaffold defines the first implementation target for window-change visibility and raw-source disagreement handling in the attention dashboard and runner.

## Goal

The system should show what the user actually saw and attended to, while keeping enough raw-source reasoning to debug why a frame or transition was saved.

The current failure mode is that app/window changes are present in the raw capture stream, but the dashboard collapses them into a noisy `window-focus` lane and the runner treats nearby source events as unranked context.

## Source Roles

Treat each source as a claim about a different part of reality:

| Source | Wins For | Does Not Win For | Notes |
| --- | --- | --- | --- |
| AX focused context | current semantic app/window/focused element | visual keyframe/motion | Primary focus truth when status is usable and app/window fields exist. |
| AX `focused_window_changed` | semantic window-change edge | input-cause timing | Best product lane for real app/window changes. |
| UX `focus_transition` | input/OS transition cause timing | app/window title truth | Can lag AX by hundreds of ms to more than a second. Use as corroboration. |
| SCK active-window frame metadata | visual receipt, geometry, dirty/motion evidence | focus truth | Sparse/cooldown-bound. May miss short visits. |
| Snapshot `focusedContext` | snapshot-local semantic receipt | primary timing truth | Preferred over top-level `activeApplication`. |
| Snapshot top-level `activeApplication` | fallback only | transitions | Known to be stale during transitions. |

## Dashboard Lane Contract

Split the existing `window-focus` lane into focused lanes:

| Lane ID | Title | Event Types | Default |
| --- | --- | --- | --- |
| `window-changes` | Window Changes | `capture.ax_semantic.focused_window_changed.v1` | visible |
| `focus-transitions` | Focus Transitions | `capture.ux.focus_transition.v1` | visible |
| `focused-elements` | Focused Elements | `capture.ax_semantic.focused_element_changed.v1` | visible |
| `focus-samples` | Focus Samples | `capture.ax_focused_context` | debug/noisy |
| `visual-active-window` | Visual Active Window | `capture.active_window_frame_metadata` | debug/noisy |

Dashboard labels should make transitions readable without hovering:

```text
window changed: Code -> Chrome
focus transition: pid 28730 -> 19780
visual frame: Code
focused element: AXTextArea
focus sample: Chrome
```

If a source cannot provide a previous app/window name, the title should still name the current app/window and the tooltip should explain which fields were unavailable.

## Runner Source Claim Contract

The runner should normalize source events into claims before scoring.

```ts
SourceClaim {
  id: string
  source: "ax_context" | "ax_window_changed" | "ux_focus_transition" | "sck_active_window" | "snapshot_focused_context" | "snapshot_active_application"
  field: "app_window_focus" | "focused_element" | "visual_active_window" | "transition_edge" | "visual_motion"
  t_ms: number
  confidence: number
  app_name?: string
  bundle_id?: string
  window_title?: string
  previous_app_name?: string
  previous_window_title?: string
  event_ref: string
  explanation: string
}
```

Then resolve claims into candidate context:

```ts
ResolvedField {
  field: string
  value: string
  winning_source: string
  confidence: number
  supporting_sources: string[]
  conflicting_sources: string[]
  explanation: string
}
```

For v0, it is acceptable to implement this as Rust structs that serialize into `score_components`, `source_conflicts`, and state explanations rather than exposing a full public schema.

## Conflict Contract

Populate top-level `source_conflicts` when sources disagree within the same candidate window and both claims are strong enough to matter.

```json
{
  "id": "conflict-000",
  "t_ms": 12000,
  "candidate_id": "candidate-025",
  "source_a": "ax_window_changed",
  "source_b": "sck_active_window",
  "conflict": "app_window_focus",
  "resolution": "AX focused window wins focus identity; SCK retained as visual receipt only.",
  "severity": "info",
  "explanation": "AX reported Chrome while active-window metadata was still Code."
}
```

Severity guide:

| Severity | Meaning |
| --- | --- |
| `info` | Expected source lag or sparse sampling; winner is clear. |
| `warning` | Winner exists but confidence should be reduced. |
| `error` | No reliable winner; candidate should be marked attention debt or fallback visual-only. |

## Canonical Signal Classes

Runner signal `kind` values can stay detailed, but every detailed signal should map to one canonical class for decisions:

| Canonical Class | Examples |
| --- | --- |
| `selection` | selected text changed, copied text |
| `command` | shortcut, submit, save, run, paste |
| `transition` | AX window changed, UX focus transition, app switch |
| `visual_motion` | dirty rects, scroll exposure, visual keyframe |
| `semantic_focus` | AX focused context, focused element |
| `source_conflict` | unresolved or degraded source disagreement |

Decision logic should use canonical classes, not raw string equality. In particular, `ax_focused_window_changed` and `ux_focus_transition` must both count as `transition`.

## Transition Save Rule

Meaningful app/window transitions should be eligible for saved states even without keyboard or pointer hard keeps.

For v0:

- If AX reports a focused-window change and a nearby candidate frame exists, the candidate receives `transition` class.
- If UX focus transition corroborates AX within a small window, boost confidence.
- If SCK metadata disagrees, preserve it as a conflict or supporting visual receipt depending on timing.
- Avoid saving repeated AX focus samples unless they are paired with dwell, keyboard, pointer, selection, or visual novelty.

## Frame-Time Rule

Use the video recording start/duration as the frame timeline base. Do not stretch a 60 second video across a longer manifest event range. Event timestamps can extend beyond the video and should be clamped or marked out-of-frame when needed.

## Acceptance Checks

Dashboard:

- The VS Code -> Chrome -> Codex segment appears as readable transitions in `window-changes`.
- The noisy sample stream no longer hides window transitions.
- Hover details still expose raw source refs.

Runner:

- `save_transition` is reachable for AX and UX transition evidence.
- Candidate context uses source precedence instead of first-nearest non-empty event.
- `source_conflicts` is non-empty when strong sources disagree, and empty/low-noise when they merely lag in expected ways.
- Saved-state explanations say which source won and why.
