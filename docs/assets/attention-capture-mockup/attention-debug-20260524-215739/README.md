# Attention Filter Debug Bundle

This bundle is a one-minute capture fixture for developing the screenshot attention filter.

## Primary files

- `screen-recording.mov`: raw 60 second screen recording with cursor/click visibility.
- `manifest.json`: machine-readable index for the bundle.
- `contact-1fps.jpg`: 60-frame contact sheet, one screenshot per second.
- `contact-2fps.jpg`: 120-frame contact sheet, two screenshots per second.
- `frames-1fps/`: extracted 1fps candidate screenshots.
- `frames-2fps/`: extracted 2fps candidate screenshots.
- `snapshots/`: once-per-second `1context capture snapshot` outputs.
- `snapshot-index.tsv`: wall-clock index for snapshot files.
- `capture-events/run-window.events.jsonl`: persisted UX/AX/frame metadata filtered to this run window.
- `capture-events/run-window-event-counts.txt`: count by event type.

## Capture notes

The simultaneous `capture metadata-sample` call timed out while recording, but per-second `capture snapshot` polling succeeded for 58 of 61 attempts, and the persisted event JSONL contains 155 run-window events. A post-run `metadata-sample` sanity check succeeded and is included as `metadata-sample-postcheck.json`.

## Intended use

Use this as the first real fixture for the filter algorithm:

1. Generate candidate screenshots from `frames-2fps/`.
2. Join candidates to nearby `capture-events/run-window.events.jsonl` records.
3. Label each candidate as save, skim, drop, or merge.
4. Render an overlay contact sheet for debugging.
5. Emit saved states with local attention bands and explanation reasons.
