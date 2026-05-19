# Peekaboo Evidence Wall Handoff

## Current Run Command

```bash
uv run python demos/peekaboo-evidence-wall/peekaboo_evidence_wall.py \
  --profile low-power \
  --work-screens 1,2 \
  --screen-labels "1=Studio Display,2=MacBook Internal" \
  --codex-app Codex \
  --port 8765
```

Run it in `tmux` as `peekaboo-evidence-wall` when leaving the demo live for another agent.

## Verified State

- Local dashboard: `http://127.0.0.1:8765/`
- Agent feed: `http://127.0.0.1:8765/agent-feed/latest`
- Permissions sampled through Peekaboo: Screen Recording granted, Accessibility granted, Event Synthesizing granted.
- Active work lanes use manual labels from `--screen-labels`; dashboard panel titles are rendered from lane `source_label`, not hardcoded HTML.
- Screen `1` is the large Studio/Codex work display for the current desk arrangement.
- Screen `2` is the MacBook internal display for the current desk arrangement.
- Keep the DELL/evidence-wall display out of `--work-screens`; it is for watching the dashboard.

## Evidence Contract

The agent-visible surface is the feed plus repo-local artifacts under `.evidence/`:

- latest frame URLs per lane
- absolute artifact paths
- dimensions, byte sizes, and sha256 hashes
- exact Peekaboo commands and command-output JSON
- inventory files for permissions, screens, apps, windows, menus, menubar, spaces, tools, bridge, and daemon
- CPU/process metrics
- OCR and Accessibility probe status

## Known Caveats

- `peekaboo list screens --json` can report ambiguous display names, so trust manual `--screen-labels` plus one-shot pixel checks for the demo.
- Low-power mode uses screen/window keyframes, not continuous full-screen `capture live`, to stay near the 5% CPU target.
- `peekaboo see --annotate` and `peekaboo see --menubar` are visible in the feature grid but are opt-in with `--sample-see` because this local build has shown hangs.
- OCR is a planned derived layer over kept frames; it should not block capture or force frame drops.
