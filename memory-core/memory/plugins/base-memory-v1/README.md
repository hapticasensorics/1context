# Base Memory V1

This is the seed memory plugin for 1Context.

It is intentionally sparse. It names the folders and contracts where a memory system grows without freezing default agents, jobs, or state machines too early.

Start with [memory/README.md](../../README.md) for the larger framing.

## Folder Map

```text
plugin.toml        plugin identity
linking.toml       versioned policy plus the linker implementation it requires
native-memory.toml native memory surfaces
providers.toml     provider/model routing
dependencies/      plugin-declared needs, not a full key catalog
agents/            agent profiles when we define them
harnesses/         agent loop backends such as codex-harness and claude-code
prompts/           harness, agent, job, and state-machine prompt files
jobs/              reusable work contracts
state_machines/    versioned scoped control definitions
lived-experiences/ curated static experience owned by this plugin
custom-tools/      plugin-defined tool contracts and optional implementations
diagrams/          Mermaid diagrams plus Markdown notes
docs/              architecture notes and experiment-to-system translations
```

## Identity

Use human ids and versions for shared definitions.

Use one runtime UUID:

```text
hired_agent_uuid
```

The linker mints it when an agent is hired, then writes a verbose `hired_agent.created` birth certificate. Runtime experience folder names are derived storage addresses, not identity.

Job ids and state-machine ids are user language. They should stay readable.

## Runtime Experience

Runtime experience lives outside the plugin:

```text
memory/runtime/experiences/
```

That lets people hot-swap plugins while the local ledger records which hired agent touched each experience.

This seed leaves `scope.plugin = false` in `linking.toml`, so runtime experience can follow a job id across plugin experiments. A plugin that needs stricter isolation can set `scope.plugin = true`.

Runtime experience writes the selected native memory surface only. A harness's `primary_memory_format` wins when a harness is selected:

```text
Codex        real CODEX_HOME
Claude       Claude-native project memory
raw API      OpenAI-compatible chat messages when that is the natural shape
```

Translation between native memory formats should be an explicit state machine, not hidden background magic.

## Accounts And Dependencies

Plugin dependencies live in:

```text
dependencies/dependencies.toml
```

They say what this plugin may need: model-provider accounts, specific model families, and local resources such as a raw-data database.

Root `accounts.toml` explains concrete auth families such as ChatGPT subscription auth, Claude account auth, API keys, Cloudflare tokens, and future 1Context subscription auth. Neither file stores secret values.

The global account linker regenerates `accounts.toml` from dependency declarations while preserving user choices such as `selected_mode`, env var names, and notes.

## Storage

This plugin expects the host runtime to provide a local LanceDB/Lance lakestore:

```text
storage/lakestore/
```

The dependency is declared as:

```text
lancedb-lakestore   storage_engine   >=0.30,<0.31
```

Use the lakestore for broad runtime truth: imported sessions, normalized
events, state-machine ticks, artifact rows, evidence rows, and readable
documents. Files are still welcome when they are the best artifact for humans
to inspect; the lakestore records where they live and what validated them.

## Harnesses

Harnesses are part of the agent identity, not the orchestrator.

They own execution mechanics:

```text
command invocation
endpoint protocol
input preparation
sandbox and approval mode
native memory surface
captured artifacts
```

Use `codex-harness` for Responses-native Codex/OpenAI work. Use `claude-code` for Opus/Sonnet work. Trying Opus inside `codex-harness` belongs behind an explicit Responses-compatible gateway and should be treated as experimental until it passes harness compatibility checks.

Codex defaults to the best-of-both-worlds auth path: fresh runtime `CODEX_HOME`, plus a symlink to the user's existing `~/.codex/auth.json`. That avoids an extra login while keeping old sessions, config, rules, and project memory out of the hired agent's home.

Do not call raw `codex exec resume` and assume it is isolated. Use the harness template: set `CODEX_HOME` to the runtime experience home, pass `--ignore-user-config`, pass `--ignore-rules`, and disable project docs with `project_doc_max_bytes = 0`.

## Prompts

Prompts live in `prompts/`.

TOML names prompt files and explains how they are used. Markdown carries the actual instruction text. Harness orientation prompts belong here too, because they are prompts, not executable harness code.

Prompts are not lived-experience. A prompt says how to behave. Lived-experience says what was previously done.

## Jobs

Jobs are reusable work contracts. They choose an agent, optional custom tools, inputs, outputs, prompt fragments, permissions, and completion states.

Concrete values for a particular run are job params recorded in the birth certificate, not baked into the job definition.

The first manifest-driven job path is `memory.hourly.scribe`. It renders one
hour into a braided lived-experience packet, builds a hashable prompt stack,
births a Claude Code hired agent with the packet loaded as inherited life, and
writes one hourly talk entry.

```bash
uv run 1context job run memory.hourly.scribe \
  --date 2026-04-06 \
  --hour 20 \
  --run-harness
```

The first state-machine bridge is the day hourly fanout. It discovers active
hours for a date, prepares one `memory.hourly.scribe` hire per hour, and runs
them through the capped batch runner:

```bash
uv run 1context job run-day-hourlies \
  --date 2026-04-06 \
  --max-concurrent 2
```

The shared outcome vocabulary is:

```text
done
skip
no_change
needs_approval
failure
```

The first reader-side wiki job is deterministic:

```bash
uv run 1context wiki build-inputs \
  --workspace /tmp/onecontext-wiki/wiki \
  --concept-dir /tmp/onecontext-wiki/concepts \
  --staging /tmp/onecontext-wiki/staged
```

It carries the e08 wiki lessons into the portable plugin: generated Topics,
Projects, Open Questions, Landing, and This Week pages; bracket resolution with
aliases and external fallbacks; a backlinks index; and staged concept pages with
Wikipedia-style "What links here". Agents still own judgment, forgetting,
skipping, and prose. This job keeps the reader surface navigable after they
write.

The wiki engine integration adds the visible renderer/browser side while
keeping the planner separate:

```bash
uv run 1context wiki list
uv run 1context wiki ensure
uv run 1context wiki render for-you
uv run 1context wiki routes
uv run 1context wiki serve --render
```

`src/onectx/memory/wiki.py` owns deterministic planner inputs and role routing.
`src/onectx/wiki/` owns page-family discovery, scaffolding, rendering, render
manifests, route tables, local serving, and render evidence. A successful render
records `wiki.render.succeeded`, `wiki.manifest.recorded`,
`wiki.generated.available`, and `wiki.render.completed` in the lakestore, which
is the evidence the state machines should wait on.

The e08 prototype should be translated into this plugin through
[docs/e08-to-system-translation.md](docs/e08-to-system-translation.md). Treat
the e08 prompts and observed behavior as valuable; treat its shell runners as
prototype mechanics to replace with route plans, hired-agent birth records,
validators, evidence, and state-machine reconciliation.

To inspect the dynamic route plan as hired-agent birth previews:

```bash
uv run 1context wiki route-dry-run \
  --workspace /tmp/onecontext-wiki/wiki \
  --concept-dir /tmp/onecontext-wiki/concepts
```

To persist that dry-run as a route execution artifact and freshness receipt:

```bash
uv run 1context wiki route-dry-run \
  --workspace /tmp/onecontext-wiki/wiki \
  --concept-dir /tmp/onecontext-wiki/concepts \
  --write-artifact
```

`--write-artifact` now persists both layers:

```text
memory/runtime/wiki/route-plans/*.json
memory/runtime/wiki/route-executions/*.json
```

The route plan is the scanner/planner contract: route rows, ownership, source
packet receipts, source hashes, budget hints, split pressure, freshness, and
role grouping. The execution preview is the birth surface: one planned hire per
`hire` row, prompt-stack part hashes, rendered source-packet estimates,
birth-certificate preview, and typed non-hire outcomes such as `no_change`.

Oversized wiki role packets are routed before they can become unsafe mutating
hires. When a route's source packet exceeds its token budget, the parent row
settles as `split_parent` and the planner emits bounded
`memory.wiki.source_packet_shard` rows plus a
`memory.wiki.source_packet_aggregate` row. The aggregate is the later relaunch
surface for the original curator/librarian/redactor-style role.

Use `--require-fresh` when a stale or missing Codex/Claude importer should
block the run instead of only appearing in the report.

To dry-run historic event replay without launching agents:

```bash
uv run 1context memory replay-dry-run \
  --start 2026-04-27T11:00:00Z \
  --end 2026-04-27T13:00:00Z
```

Replay writes `config.json`, `events.jsonl`, `fires.jsonl`, and `summary.json`
under `memory/runtime/replay-runs/` and records a lakestore artifact/evidence
receipt. It is the bridge from batch validation to real-time cadence tuning.

The first concrete state-machine runner bridge is a wiki-only memory tick:

```bash
uv run 1context memory tick --wiki-only
uv run 1context memory tick --wiki-only --execute-render --render-family for-you
uv run 1context memory tick --wiki-only --freshness-check always --require-fresh
```

It writes a cycle artifact under `memory/runtime/cycles/<cycle_id>/cycle.json`,
records `memory_cycle.artifact_written`, and, when rendering executes, records
`reader_surface.ready` after the wiki engine leaves routeable manifests. This is
not the full runner yet. It is the narrow executable bridge for the DSL path:

```text
route dry-run or skip -> wiki render or dry-run -> evidence -> cycle event
```

The cycle artifact now includes an `ir_contract` extracted from the compiled
`memory_system_fabric` state-machine IR. For the reader-surface tick, that
contract names the transition from `cycle.routing_wiki` on
`wiki.agent_layer.closed` to `cycle.building_reader_surface`, including the DSL
steps `run_wiki_reader_loop` and `render_wiki_engine_families` and the expected
evidence `reader_surface.ready`. Validation checks the cycle against that
compiled contract instead of relying only on a hand-maintained evidence list.

Future runner work should grow from this point rather than from another shell
script.

Every cycle has a `preflight.source_freshness` section. The default policy is
`auto`: check freshness when source-derived route planning is requested, skip it
for render-only ticks. Use `--freshness-check always --require-fresh` when stale
or missing source imports must block the cycle. A blocked cycle is still a valid
cycle: it writes an artifact, records failed `source_import.fresh` evidence, and
exits with code 2.

Ticks also carry a `recovery` section. Render failures do not vanish into a
traceback after the runner has enough context to write a cycle artifact:

```bash
uv run 1context memory tick --wiki-only --execute-render --render-family for-you --retry-budget 1
```

When a step fails and retry budget remains, the cycle status is `retryable`,
the terminal event is `memory.tick.retryable`, and the command exits with code
2. Without retry budget, the cycle status is `failed`, the terminal event is
`memory.tick.failed`, and the command exits with code 1. Both outcomes are
validatable cycle artifacts with `memory_tick.recovery_recorded` evidence.

Cycle artifacts are inspectable:

```bash
uv run 1context memory cycles list
uv run 1context memory cycles show <cycle_id>
uv run 1context memory cycles validate <cycle_id>
```

Validation checks the file, the lakestore artifact row, content hash, expected
evidence rows, source freshness preflight, terminal cycle event, the legacy DSL
evidence list, and the compiled `ir_contract`. Rerunning a cycle id replaces
the stable artifact row instead of accumulating stale hashes.

## State Machines

State machines are control logic for async agent work. They normalize schedules, file changes, artifact readiness, manual starts, approvals, and activity detection into durable events and readiness facts, then decide what happens next.

Production state-machine artifacts are generated from DSL source:

```bash
uv run 1context state-machines compile
uv run 1context state-machines verify
```

The output under `memory/runtime/state-machines/production/<run_id>/` includes
compiled IR, generated Mermaid diagrams, `checks.json`, and `summary.md`. This
is the verification loop for state-machine edits: source DSL -> compiled IR ->
generated diagrams -> checks against real jobs and runner evidence.

They should not become a giant DAG language or a hidden multi-agent chat script. Agents can reason freely inside jobs; the state machine should make the larger memory system understandable.

Borrow hardware-style timing ideas only where they make async behavior clearer. The files should still read like sober system design.

The language/runtime lives in `src/onectx/state_machines/`; this plugin owns definitions in `state_machines/`. The current language is `ai_state_machine 0.1.0`.

Because the language is executable host tooling, it is declared as a plugin dependency in `dependencies/dependencies.toml`:

```text
ai-state-machine-dsl   state_machine_language   >=0.1.0,<0.2.0
```

## Tools

Harnesses declare native default tools such as `workspace.read`, `workspace.write`, `patch.apply`, and `shell.exec`.

Do not mirror every Codex or Claude internal tool. Custom tool contracts live in `custom-tools/`. The seed custom tool is `raw_data.query`, a read-only SQLite command for querying raw data through an explicit contract.

There are no tool packs in this seed. A little repetition in agent and job definitions is better than adding another abstraction before the state machines are real.

## Promotion

Lived-experience is curated prior doing. Runtime experience is local doing.

When a runtime experience becomes useful, promote a distilled artifact into `lived-experiences/`. Do not copy a harness home blindly unless that harness is expected to resume from that exact native state.
