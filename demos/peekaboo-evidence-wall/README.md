# Peekaboo Evidence Wall

Local web demo for assessing Peekaboo as a 1Context screen-capture and agent-evidence layer.

The current version is deliberately file-backed and low-overhead: long-running `peekaboo capture live` segments write immutable frame bundles to `.evidence/`, while the dashboard streams small metadata events and loads images directly from disk-backed HTTP URLs.

## What It Exercises

- `peekaboo capture live` for focused terminal/log evidence in showcase/maximal profiles
- `peekaboo image --mode screen` for low-power full-screen work-display keyframes
- `peekaboo image --mode window` for low-power Codex terminal/log keyframes
- `peekaboo image --retina` for sparse high-fidelity Codex keyframes
- `peekaboo image --mode multi` for multi-display still capture
- `peekaboo image --mode area --region ...` for region capture
- `peekaboo see --annotate` for screenshot plus Accessibility/UI map evidence
- `peekaboo see --menubar` for the built-in menu-bar OCR path
- `peekaboo list screens/apps/windows/menubar`
- `peekaboo menu list --app Codex`
- `peekaboo window list --app Codex`
- `peekaboo space list`
- `peekaboo daemon status`
- `peekaboo bridge status`
- `peekaboo tools --json`

Automation and control tools are inventoried but not invoked. The point is capture, evidence, and memory-readiness, not remote control.

## OCR Note

Peekaboo 3.2.1 has OCR-adjacent features, but not a general continuous local OCR stream in the CLI:

- `peekaboo see --menubar` captures menu bar popovers via a window-list plus OCR path.
- `peekaboo image --analyze` and `peekaboo see --analyze` can ask a configured AI provider to extract text from pixels, but that is opt-in and not local-only.
- MCP has an accessibility text inspection path, but this CLI build does not expose `inspect_ui`.

For the memory-system proof, OCR should be a derived queue:

1. Keep every sampled frame and segment metadata first.
2. Run Apple Vision or Tesseract over Codex-window frames asynchronously.
3. Store raw OCR text, normalized visible-line spans, confidence, source frame IDs, and timing.
4. If OCR falls behind, mark it pending or deferred. Do not drop capture frames to let OCR catch up.

This keeps the capture layer faithful and keeps expensive text extraction from driving CPU or blocking evidence writes.

## Run

```bash
uv run python demos/peekaboo-evidence-wall/peekaboo_evidence_wall.py \
  --profile low-power \
  --work-screens 1,2 \
  --screen-labels "1=Studio Display,2=MacBook Internal" \
  --codex-app Codex \
  --port 8765
```

Open:

```text
http://127.0.0.1:8765
```

Put the browser on the third screen and use `--work-screens` for the two screens you want captured. Discover screen indexes with:

```bash
peekaboo list screens --json
```

If `peekaboo list screens --json` returns confusing or empty display names, pin the labels yourself with `--screen-labels`. The panel titles and agent feed use those labels for the active work lanes, so the evidence wall does not depend on macOS display-name guesses.

On Paul's current desk, the current low-power run uses `--work-screens 1,2` with `1=Studio Display` and `2=MacBook Internal`. The DELL/evidence-wall display should stay out of `--work-screens`. Use one-shot screen captures after granting Screen Recording to confirm the current index map before treating a display label as ground truth.

Use mock mode when presenting without permissions or without Peekaboo:

```bash
uv run python demos/peekaboo-evidence-wall/peekaboo_evidence_wall.py --mode mock
```

## Profiles

- `low-power`: default. Full-screen work-display keyframes plus Codex window keyframes, target CPU <= 5%.
- `showcase`: more aggressive Codex-window sampling for demos.
- `maximal`: uses Peekaboo's higher capture-live settings and should be treated as a stress test, not an always-on background mode.

Every profile keeps the sampled frames as artifacts. Lower-power profiles reduce temporal sampling rate; they do not throw away frames after capture.

Use `--stream-work-screens` or `--stream-codex` with `low-power` if you want to force continuous `capture live` for those lanes. On the current three-screen setup, empirical testing showed even one continuous `capture live` process can sit far above the 5% target after settling, so the default low-power mode uses keyframes and reserves `capture live` for showcase/stress profiles.

Use `--sample-see` to run `peekaboo see --annotate` and `peekaboo see --menubar` probes. They are listed in the dashboard by default but not sampled automatically because this local Peekaboo build can hang those calls.

## Agent Feed

The canonical agent-visible evidence is available at:

```text
http://127.0.0.1:8765/agent-feed/latest
http://127.0.0.1:8765/agent-feed/stream
```

The feed includes frame URLs, segment paths, contact sheets, metadata URLs, frame hashes, feature status, capability inventories, process metrics, and the OCR plan. Raw pixels stay in the repo-local `.evidence/` folder and are not uploaded by this demo.

Each run also writes:

```text
demos/peekaboo-evidence-wall/.evidence/<run-id>/events.ndjson
demos/peekaboo-evidence-wall/.evidence/<run-id>/events.sqlite3
```

For takeover details, including the current desk mapping and verified run command, see `HANDOFF.md`.

## Performance Shape

The key architectural choice is to let `peekaboo capture live` own sampling and file emission, then have the dashboard watch the artifact directory. SSE carries compact events only; the browser fetches images by URL. Expensive analysis such as OCR or AI image analysis stays out of the capture loop.

Peekaboo 3.2.1 limits `capture live` duration to 180 seconds. This demo runs rolling segments and preserves each bundle.
