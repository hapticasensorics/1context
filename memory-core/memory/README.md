# Memory System

This folder is the 1Context memory subsystem.

The central metaphor is simple:

```text
a configured agent is hired into a job,
and that concrete hire receives a birth certificate
```

## Shared Design And Local Life

```text
plugins/   shared definitions people can copy, edit, publish, and swap
runtime/   local state created by this machine while the system runs
```

A plugin describes what could exist. Runtime records what did exist.

## Plugin Definitions

```text
agents/             who can be hired
harnesses/          the body or agent loop they inhabit
native-memory.toml  native memory surfaces
providers.toml      model providers and fallback memory routing
prompts/            instruction text
jobs/               reusable work contracts
state_machines/     versioned scoped control definitions
lived-experiences/  curated prior doing
custom-tools/       extra plugin-defined capabilities
dependencies/       what the host must provide
linking.toml        memory-link policy and ledger schema
```

These files are shareable. They should use human ids and versions.

## Runtime Records

```text
../storage/lakestore/           LanceDB/Lance tables for runtime storage
runtime/ledger/events.jsonl     readable append-only journal mirrored to lakestore
runtime/experiences/            harness-native memory homes
runtime/runs/                   future run artifacts
runtime/proposals/              future promoted-memory proposals
```

These files are local. They should record what happened without becoming the plugin.

## Lakestore

The runtime lakestore is the broad storage substrate:

```text
events      normalized session events, state-machine ticks, job outcomes
sessions    source session summaries
artifacts   durable outputs such as talk entries, rendered pages, transcripts
evidence    validation rows that let state machines advance
documents   readable text surfaces and future embedding inputs
```

Earlier prototype used SQLite/FTS5 for the raw session archive. 1Context keeps that
queryable-storage lesson but uses LanceDB/Lance so raw records, artifact
metadata, evidence, and future vectors can live in one local lake-style store.
The session rows still keep the earlier prototype event contract directly:
`hash`, `session_id`, `kind`, `cwd`, and `char_count`.

Ports are named separately in `../ports/`. The memory system consumes
observations and produces surfaces, but many surfaces loop: a wiki page is
output when published and input when read later. Storage is the shared durable
substrate underneath all ports.

## Birth Certificates

Runtime identity uses one UUID:

```text
hired_agent_uuid
```

When the system hires an agent, the linker writes a `hired_agent.created` event. That event records:

```text
who       hired_agent_uuid
why       job ids plus concrete job params
body      harness
mind      provider and model
memory    runtime experience id plus native harness homes
prompts   prompt files and copied text
powers    harness tools, custom tools, host grants, permissions
history   selected lived-experiences
versions  plugin, linker, ledger schema
hashes    exact config and policy fingerprints
```

The `experience_id` is a storage address. It is not another identity.

## Jobs

A job is a reusable work contract and a human-readable reason to hire an agent:

```text
article-draft
raw-memory-search
daily-summary
memory.hourly
```

A hire may carry several job ids when that helps humans understand the work.

Concrete job params are recorded at hire time. The job definition can say "summarize one hour"; the birth certificate says which hour. Params stay flat in v1. If a multi-job hire needs namespacing, use explicit dotted keys such as `memory.hourly.hour`.

## State Machines

A state machine describes continuation across hired agents. It is control logic for async work:

```text
observe stable events and artifacts
hire or resume a job
watch artifacts and outcomes
continue, wait, branch, stop, or promote
```

Readiness facts are derived from async events: artifact ready, job done, job failed, approval acknowledged, plugin changed, reset requested. State-machine provenance should live in its own future event family and link multiple `hired_agent_uuid`s together. It should not be baked into the birth certificate for a single hire.

The language/runtime is reusable 1Context code. Plugin definitions are authored in `state_machines/` and compiled into inspectable IR:

```text
ai_state_machine 0.1.0
```

Use many scoped local states instead of one global state enum:

```text
one event ledger
many scoped states
explicit signals
evidence-based completion
```

## Experience

Lived-experience is curated prior doing owned by a plugin. It is static, shareable, and intentionally chosen.

Runtime experience is local doing owned by a machine. It is where native harness memory lives.

When a runtime experience is created, selected lived-experience is copied into `seeds/lived-experience.md` as prompt seed material. The harness-native store remains the source of truth for resume. For Codex, that means a real isolated `CODEX_HOME`. For Claude, it should mean Claude's native project memory.

We do not fake one universal transcript until a real state machine requires translation.

## Product Boundary

The core action is:

```python
hire_agent(...)
```

The CLI is a developer inspection door. It should stay thin. The daemon, menu-bar app, and future UI should call the loader, account linker, memory linker, and harness runner directly.

The line to protect:

```text
hire this agent for this job
```

not:

```text
assemble a command with every internal knob
```
