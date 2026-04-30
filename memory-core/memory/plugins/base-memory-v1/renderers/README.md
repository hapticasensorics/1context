# Renderers

Renderers prepare deterministic context artifacts before a hire.

The first renderer is `render_hour_experience`. The implementation currently
lives in `src/onectx/memory/hour_experience.py` so the CLI and lab runner can
import it directly. This folder records the plugin-owned renderer contract.

`render_hour_experience`:

```text
input
  date
  hour
  source_harnesses
  experience_mode

scribe default
  braided_lived_messages

forensic/full default
  braided_lived_transcript

output
  memory/runtime/experiences/<experience_id>/experience.md
  memory/runtime/experiences/<experience_id>/streams/*.md
  memory/runtime/experiences/<experience_id>/agent-context.md
  memory/runtime/experiences/<experience_id>/meta.yaml
  lakestore.artifacts row kind=runtime_experience_packet
```

The rendered packet is an artifact. `experience.md` is the braided control
surface; `streams/*.md` are durable per-stream transcripts.
`agent-context.md` is the birth-loaded lived experience: it concatenates the
control surface and all stream transcripts so the hired agent starts with the
hour already in context.

## Durability Model

The lakestore is the source of truth, not the rendered prompt.

```text
lakestore events
  raw durable observations, including tool calls and tool results

runtime experience packet
  deterministic projection of a bounded window for one hire mode
  materialized as runtime/experiences files and an artifacts table row
  stable artifact id per experience_id/mode so rerenders update the row
  cache-reused only when mode, source window, and source-event hash match

ledger event
  source window, renderer version, mode, paths, hashes, and hire/output links
```

`braided_lived_messages` is not a separate store and not a destructive view. It
is a renderer projection over the raw event log:

```text
read raw events from LanceDB/lakestore
drop tool_use/tool_result from the agent-facing birth context
keep all user and assistant messages
write runtime/experiences/<experience_id>/*
record hashes and source metadata in lakestore.artifacts and the ledger
```

Tool traces remain available in the lakestore for retry, audit, and forensic
jobs. Scribes default to messages-only because the comparison test showed the
tool trace was mostly context tax for journal memory. Jobs that turn on exact
command output, diffs, test failures, or terminal logs should use
`braided_lived_transcript` or request a retry with tool detail.
