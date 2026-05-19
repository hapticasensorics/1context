# Peekaboo Evidence Wall

Local web demo for assessing Peekaboo as a 1Context screen-capture and agent-evidence layer.

The demo starts a local HTTP server, captures selected work screens, runs a high-frequency Codex window lens for terminal/log text, and streams the exact evidence feed that an agent would be allowed to consume.

## What It Exercises

- `peekaboo permissions status`
- `peekaboo list screens/apps/windows/menubar`
- `peekaboo tools --json`
- `peekaboo menu list --app Codex`
- `peekaboo image` for live screen and app/window frames
- `peekaboo see` for screenshot plus Accessibility/UI map evidence
- `peekaboo capture live` for diff-aware rolling bursts, kept frames, contact sheets, and metadata
- local `tesseract` OCR over the Codex window to infer terminal/log lines that were visibly available

The action tools are inventoried but not invoked. The point is capture, evidence, and memory-readiness, not remote control.

## Run

```bash
uv run python demos/peekaboo-evidence-wall/peekaboo_evidence_wall.py \
  --work-screens 0,1 \
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

On Paul's current three-display desk, the useful mapping is:

- `0`: Studio Display
- `1`: MacBook internal / Color LCD
- `2`: DELL U3219Q, good for the evidence wall itself

Use mock mode when presenting without permissions or without Peekaboo:

```bash
uv run python demos/peekaboo-evidence-wall/peekaboo_evidence_wall.py --mode mock
```

## Agent Feed

The canonical agent-visible evidence is available at:

```text
http://127.0.0.1:8765/agent-feed/latest
http://127.0.0.1:8765/agent-feed/stream
```

The feed includes screenshot URLs, UI-map summaries, OCR-visible line timelines, command provenance, frame hashes, and capability inventories. Raw pixels stay in the repo-local `.evidence/` folder and are not uploaded by this demo.

## Notes

Peekaboo 3.2.1 limits `capture live` duration to 180 seconds. This demo runs short rolling bursts and publishes each finished bundle into the evidence stream.
