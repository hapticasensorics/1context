# Memory System Replay IR

## How to read it

This diagram is generated from the `replay` scope inside
`memory_system_fabric`.

Replay is the bridge from batch experiments to live real-time operation:

- load real historic Codex/Claude events in timestamp order
- derive an event-stream clock
- schedule hourly/daily/weekly/monthly fires
- dry-run or live-fire the same route planner and hired-agent runners
- capture `events.jsonl`, `fires.jsonl`, snapshots, timings, failures, costs,
  and summary evidence
- tune daemon cadence and failure policy from evidence before live rollout

The key discipline is that replay should use the same system surfaces as live
operation. It is not a separate toy simulator.
