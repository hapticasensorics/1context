# Attention Runner Bundle Migration Notes

This note compares the READY capture-bundle contract with the current attention dashboard and algorithm-development assumptions.

## Short Version

The bundle mostly matches the raw-signal assumptions we want for the attention algorithm. It gives us time-bounded AX, UX, SCK, browser, terminal, editor, window, display, source-health, capability, and known-gap lanes.

The original mismatch was visual review: the dashboard was built around a one-minute debug recording with fixed-rate screenshots, while the first capture-bundle scaffold allowed media to be empty.

That was an oversight. READY capture bundles now require the Swift screen-recording decoder's 2fps screenshots under `media/frames-2fps/`, indexed as `frame_2fps` records in `media/media.index.jsonl`. Full source video remains debug-only and is copied under `media/debug/` only when explicitly requested.

## What Matches

The bundle already gives the attention filter useful raw material:

| Attention Need | Bundle Surface | Status |
| --- | --- | --- |
| Time-bounded evidence | `manifest.json`, `time_alignment.json`, per-event timestamps | Good |
| Window/app context | `events/windows.jsonl`, `events/ax.events.jsonl`, `events/sck-frame-metadata.events.jsonl` | Good |
| Keyboard/mouse/scroll | `events/ux.events.jsonl` | Good |
| Accessibility semantics | `events/ax.events.jsonl` | Good |
| Visual receipts and motion/keyframe hints | `media/frames-2fps/`, `media/media.index.jsonl`, `events/sck-frame-metadata.events.jsonl` | Good |
| Browser/editor/terminal lanes | `events/browser.events.jsonl`, `events/editor.events.jsonl`, `events/terminal.events.jsonl` | Present or explicitly degraded |
| Source health | `sources.json`, `quality/known_gaps.jsonl`, `capabilities/*.json` | Good |
| Provenance safety | relative-path validation and source-envelope refs | Good |
| Debug retention | `live`, `processing`, `failed`, `pinned`, TTL sweep | Good |

This lines up with the direction we settled on: use metadata to infer what mattered, but output what a person would recognize as attended state.

## What Does Not Match Yet

### 1. The dashboard assumes a visual fixture

Current dashboard sessions assume:

```text
frames-2fps/frame-001.jpg ... frame-120.jpg
screen-recording.mov, when debugging
snapshot-index.tsv
attention-filter-output.json
```

READY bundles require:

```text
events/*.jsonl
media/media.index.jsonl
media/frames-2fps/
sources.json
quality/known_gaps.jsonl
capabilities/*.json
```

So the dashboard should treat the 2fps media records as normal bundle evidence. It should only expect source video when the bundle contains a `debug_screen_recording` media record.

### 2. The runner assumed fixed-rate screenshot candidates

The first runner selected candidates from a `CandidateFrameSet`, usually `2fps`.

Bundle reality:

```text
2fps screenshots are required for READY
SCK may only say "candidate omitted"
event streams may be richer than visual frames
```

The runner now has a first fallback: if no frame set exists, it creates event-time candidates and scores AX/UX/SCK evidence without pretending a screenshot exists.

### 3. Attention output must not be written into the bundle

The capture bundle is input evidence. It must not contain:

```text
attention-filter-output.json
attention scores
saved/drop decisions
memory write receipts
```

The new bundle runner path writes those to an external work directory under `target/attention-bundle-runs/<capture_id>/` by default.

### 4. Media index is not a frame-cache index

The dashboard frame loader expects sequential files:

```text
frame-001.jpg
frame-002.jpg
...
```

The bundle media index is record-based:

```text
media_id
media_ref/path/uri
status
source event
dimensions
```

We need a dashboard media adapter that can render sparse media records directly instead of requiring fixed FPS naming.

### 5. Bundle lanes are split files

The mock fixture used one combined `run-window.events.jsonl`.

The bundle uses:

```text
events/ax.events.jsonl
events/ux.events.jsonl
events/sck-frame-metadata.events.jsonl
events/browser.events.jsonl
...
```

That is better for source arbitration, but the dashboard and runner need to treat the bundle as a multi-lane source instead of a single raw-event file.

## First Implemented Bridge

`onecontext-attention-runner` now supports bundle input:

```bash
cargo run -p onecontext-attention-runner -- \
  --bundle /path/to/capture/bundles/live/<capture_id>
```

It writes:

```text
target/attention-bundle-runs/<capture_id>/
  attention-filter-output.json
  attention-dashboard-session.json
  bundle-compatibility-report.json
```

The generated dashboard session points at bundle event files and external attention output. If the bundle has 2fps media, the dashboard can show the visual input directly from the bundle. If the bundle has no 2fps media, it should be treated as a failed or non-READY capture artifact rather than a valid attention-development input.

## Problems To Fix Next

1. Add dashboard support for `frame_2fps` records in `media/media.index.jsonl`.
2. Wire the Swift screen-recording decoder into the capture export path so `media/frames-2fps/` is produced before bundling.
3. Add an attention-claim file before bundle processing:

```text
capture/attention-claims/<capture_id>.json
```

4. Write memory-output receipts to the memory DB/audit surface, not the capture bundle.
5. Add bundle-backed dashboard fixtures to tests so the visual judge loop uses the real handoff contract.

The key product stance stays the same: the bundle is the dense reality buffer; the attention runner creates selected, explainable memory receipts outside it.
