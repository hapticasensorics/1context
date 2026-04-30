# State Machines

State machines are the plugin-owned control definitions for async work.

They exist because 1Context memory is not just a chain of agent calls. It is a
durable knowledge workspace. Hourly agents, editors, critics, scouts,
librarians, and redactors can think freely inside their jobs, but their work
must enter the system through explicit events, permissions, artifacts, evidence,
and ledger records.

The short theory:

```text
hardware governs state
agents govern meaning
wiki-like artifacts govern memory
```

Or, more plainly:

```text
deterministic rails around non-deterministic cognition
```

The state machine is the rail. The agent is the cognition. The talk folder,
concept page, rendered article, lab record, transcript, and ledger are the
memory substrate.

If you are another agent picking up this plugin, keep this frame in your head:

```text
do not hide memory in chat
do not advance state on vibes
do not make one giant state enum
do let agents write append-only artifacts
do let evidence, not confidence, advance the machine
```

The point is not to make a rigid robot. The point is to let agents do subtle
work while the system around them stays boring enough to replay, inspect, and
share.

The language/runtime lives outside the plugin at:

```text
src/onectx/state_machines/
```

This folder holds machine definitions written in that Python-embedded
language. That split matters:

```text
language/runtime   reusable 1Context tooling
plugin definition  shareable memory-system design
runtime ledger     local record of what happened
```

The plugin can be copied, edited, published, or swapped. The runtime ledger
records which version actually ran.

## Language Version

The current language is:

```text
ai_state_machine 0.1.0
```

Every compiled machine records both its own version and the language version.
Machine versions describe the plugin author's intent. Language versions
describe the DSL and IR shape.

The plugin must also declare the DSL as a dependency:

```toml
[[dependencies]]
id = "ai-state-machine-dsl"
kind = "state_machine_language"
required = true
language = "ai_state_machine"
version_spec = ">=0.1.0,<0.2.0"
```

The loader validates this before importing machine definitions, selects the
highest available compatible runtime, and records the exact selected runtime in
compiled IR. A plugin that ships `state_machines/*.py` without a compatible
language dependency should not load.

## Model

A state machine is not one giant state enum. It is:

```text
one event ledger
many scoped local states
explicit signals between scopes
actions that emit commands
artifacts that name durable outputs
evidence that proves completion
```

This is closer to a small institution than a single function. Persistent agents
are controllers with local state. Jobs have local lifecycle. Artifacts have
local validity. The whole system shares an event discipline and a ledger.

Scopes keep the system readable:

```text
system      idle / active / paused / complete
day         pending / discovering_hours / writing_hourlies / reviewing / complete
job         queued / running / done / skipped / failed / needs_approval
artifact    expected / produced / verified / accepted
```

Artifacts are durable things the system can point at later: talk entries,
rendered pages, transcripts, lab records, query snapshots, patches, commits,
or published URLs. Evidence is the named proof that an artifact exists and is
valid enough for a state transition.

This distinction prevents agent output from becoming hidden chat state:

```text
agent says done      not enough
artifact exists      useful
evidence validates   state may advance
```

Declare both in the machine:

```python
m.artifact(
    "hourly_talk_entry",
    path="{talk_folder}/{hour}.conversation.md",
    schema="talk_entry.v1",
    policies=["append_only", "single_writer"],
)

m.evidence(
    "hourly_talk_entry.valid",
    artifact="hourly_talk_entry",
    checks=[
        "file exists",
        "frontmatter.kind == conversation",
        "body is non-empty",
    ],
)
```

Then wait for evidence in actions:

```python
expect("hourly_talk_entry.valid")
```

Keep checks as readable names in v0. The runner can later map those names to
real validators.

The first useful runner should stay local and small: read the ledger, inspect
declared artifacts, evaluate pure guards, emit commands, run jobs, validate
evidence, and append new ledger events. More serious substrates can come later
without taking over the ontology.

The runner should evaluate machines on ticks:

```text
time tick
user message tick
job completed tick
artifact changed tick
activity detected tick
approval or cancel tick
```

In v0, a tick is just a normalized event that causes guards to be evaluated.

## Concurrency Policy

State machines may describe fanout, but the host/runtime owns how much work may
run at once. The repo-level runtime policy starts with:

```toml
[runtime_policy]
max_concurrent_agents = 8
default_harness_isolation = "account_clean"
```

Use symbolic references in machine definitions rather than hardcoding a number:

```python
parallel(
    spawn("memory.hourly.scribe", for_each="active_hours"),
    fail="collect",
    max_concurrent="runtime_policy.max_concurrent_agents",
)
```

This keeps the plugin portable. A laptop might run four hires at once; this
repo's default is eight because the target is month-scale catchup, not a slow
one-day lab replay. A larger box can go higher. The machine's meaning is the
same, while the runner applies the local capacity policy.

The first runner primitive that enforces this is
`onectx.memory.execute_hired_agents(...)`. It accepts a list of hired-agent
execution specs and runs at most `runtime_policy.max_concurrent_agents` at a
time unless the caller supplies a stricter explicit cap. Single-job execution
still uses `execute_hired_agent(...)`; batch/state-machine execution should use
the capped batch primitive.

## Production Lessons From e08

The e08 lab records changed what the state machine needs to prove. It is not
enough to show that agents can write good pages in a batch experiment. The
production fabric must prove that work fired when it should have, that quiet
outcomes are intentional, and that old artifacts are reconciled when the
contract evolves.

The top-level `memory_system_fabric` now carries three validation layers:

```text
audit/computed planning      route work from data, not hardcoded era lists
runtime invariants           pre-flight expected work, post-flight produced work
replay validation            historic event stream, snapshots, failure injection
```

This is the e08 0028-0030 distinction in machine form:

```text
replay validates behavior
runtime invariants validate execution
migrations keep old artifacts healthy after contract changes
```

The `cycle` scope includes `migrating_contracts` between planning and
experience rendering. That is Phase 0.5 from e08 0035: schema, prompt, role,
section, and talk-kind changes must ship with migration/backfill receipts before
normal generation depends on the new contract. A newly added biographer,
frontmatter field, curator section, or talk entry kind should not only work
going forward; the fabric must either backfill history, mark an explicit defer,
or fail loud.

The `validating` state now expects `runtime_invariants.passed`. That evidence is
the orchestrator's answer to the silent-no-op anti-pattern:

```text
expected work from inputs
minus produced / skipped / deferred / failed outcomes
= zero unexplained missing artifacts
```

Skip remains first-class. Forgetting remains first-class. `no_change` remains a
valid result. The important production rule is that every quiet result must say
which kind of quiet it is: empty data, already current, operator-touched,
deliberate forgetting, explicit defer, or real failure.

Replay is modeled separately because it answers a different question. The
`replay` scope includes `snapshotting` and `injecting_failure` states so Q7/Q10/
Q11-style experiments can inspect mid-run wiki state, kill or perturb a fire in
a sandbox, and then prove recovery through the same invariant layer.

The `wiki_growth_fabric` now has a page-governance artifact because the curator
pattern turned out to be load-bearing. The intended split is:

```text
librarian nominates concept/page changes
page curator adjudicates page-specific merges
operator-touched marker blocks mutation
deterministic reader loop renders whatever was accepted
```

That keeps the wiki extensible without making the librarian a global overwrite
machine. As pages grow, new page curators become active circuits in the fabric,
similar to adding configured logic to an FPGA.

## Authoring

Machine files are Python modules. They expose one of:

```text
build()
machine
MACHINES
```

The authoring API compiles to plain data. Keep guards pure and put side
effects behind named steps or spawned jobs.

```python
from onectx.state_machines.v0_1 import Machine, event, sequence, spawn, expect, emit


def build():
    m = Machine("example", version="0.1.0")
    day = m.scope("day", key="date", states=["pending", "running", "complete"])

    m.on(event("day.started")).to(
        day,
        "running",
        do=sequence(
            spawn("memory.hourly", for_each="active_hours"),
            expect("hourly_entries.closed"),
            emit("day.reviewed"),
        ),
    )

    return m
```

Use explicit versioned imports in plugin definitions. The root
`onectx.state_machines` package re-exports the current default for local
experiments, but shareable plugins should not depend on the default drifting.

## What Belongs Here

Put the visible control logic here:

```text
what wakes up
which scoped state changes
which jobs are spawned
which artifacts count as evidence
which user/operator events matter
which outcomes continue, wait, stop, or fail
```

Do not put prompt bodies, agent identity, model selection, or custom-tool
implementation here. Those belong in `agents/`, `jobs/`, `prompts/`,
`harnesses/`, and `custom-tools/`.

## Current Machines

`memory_system_fabric.py` is the top-level memory control fabric: raw events
are imported, source freshness is checked, route plans are derived, lived
experience packets are rendered, hired agents are born, artifacts are
validated, wiki submachines run, reader surfaces are rebuilt, replay evidence
can tune real-time policy, and the ledger feeds the next tick.

`for_you_day.py` is the proved first slice: raw activity becomes hourly talk
entries, daily proposals, and concept candidates.

`wiki_reader_loop.py` is the deterministic reader-side build: generated
indexes, open questions, bracket resolution, backlinks, staged concepts,
landing, and this-week.

`wiki_growth_fabric.py` is the dynamic layer for a growing wiki. It is
intentionally FPGA-like:

- scan the corpus into facts
- derive a role route plan from those facts
- activate only the historian, editor, curator, librarian, biographer,
  contradiction flagger, and redactor circuits needed for this tick
- respect operator-touched gates before mutation
- let skip, forget, defer, no-change, and needs-approval settle as valid
  outcomes
- rebuild the deterministic reader surface after source mutations

This keeps the system from becoming a one-time pipeline. New pages, new talk
folders, new concepts, and new operator edits should change the next route plan
without requiring a hand-authored DAG for every new week.

The first concrete host primitive for that is:

```bash
uv run 1context wiki plan-roles \
  --workspace /tmp/onecontext-wiki/wiki \
  --concept-dir /tmp/onecontext-wiki/concepts
```

It emits the `wiki_inventory` and `role_route_plan` shapes that the fabric
describes. It is intentionally simple and inspectable; it should remain a fast
fact router, not an agent-powered planning oracle.

The first concrete runner bridge is:

```bash
uv run 1context memory tick --wiki-only
```

That command does not pretend to be the whole state-machine runtime. It executes
the narrow path we now trust: optionally derive a wiki route dry-run, optionally
render wiki-engine families, write a cycle artifact, record evidence, and append
a cycle event. It gives future retry, timeout, stuck-agent, and live-daemon work
a durable place to attach without moving the DSL boundary.

The tick runner should consume compiled IR wherever the boundary is stable. The
current wiki-only tick extracts the `memory_system_fabric` transition from
`cycle.routing_wiki` on `wiki.agent_layer.closed` to
`cycle.building_reader_surface`, writes that as `ir_contract` in `cycle.json`,
and validates that the expected evidence from the transition was actually
recorded. That keeps the runner honest without making the DSL a heavyweight
execution engine.

Every concrete cycle should expose source freshness in preflight, even when the
answer is "skipped because this was a render-only tick." When freshness is
required and fails, the cycle should block intentionally, write failed
`source_import.fresh` evidence, and leave a valid cycle artifact. The important
e08 lesson is that stale imports must be visible state, not a hidden warning in
some route-specific command output.

The same rule applies to failures. If a concrete step fails after the runner has
enough context to identify the cycle, it should write a `recovery` section and
emit `memory.tick.retryable` or `memory.tick.failed` rather than dying before
the ledger can see it. Retry budget belongs in host/runtime policy and concrete
tick params; the DSL should only care that retryable, failed, blocked, skipped,
and completed are explicit terminal outcomes.

Transitions should declare source state explicitly. The supported production
style is:

```python
machine.from_(cycle, "ingesting").on(event("memory.events.ready")).to(
    cycle,
    "planning",
    do=sequence(...),
)
```

The older `machine.on(...).to(...)` shorthand still compiles for compatibility,
but production verification treats missing sources on scoped transitions as a
failure. Generated Mermaid diagrams use the explicit `source` field and only
fall back to inference for older IR.

## Production Verification

State-machine edits should go through the production harness:

```bash
uv run 1context state-machines compile
uv run 1context state-machines verify
```

`compile` writes runtime artifacts under
`memory/runtime/state-machines/production/<run_id>/`:

```text
manifest.json
<machine>/<machine>.ir.json
<machine>/<machine>.<scope>.mmd
```

`verify` writes `checks.json` and `summary.md` beside those artifacts. The
current checks assert that machines compile, IR and diagrams are written, spawn
actions reference real job manifests, the top-level fabric declares the runner
evidence emitted by concrete ticks, and the cycle scope has terminal states for
completed, blocked, retryable, and failed outcomes.

This is the ruler for translating experiments into production state machines:
edit DSL source, compile IR, generate diagrams from IR, run verification, then
use concrete tick/replay artifacts as runtime proof.

## How Much Belongs In The DSL?

It is reasonable to expect the memory system's control surface to be expressed
in the DSL:

```text
ingest -> freshness -> route plan -> experience packet -> hired-agent birth
-> execution -> validation -> wiki routing -> reader build -> ledger feedback
```

It is not reasonable to expect the DSL to contain the whole product as code.
The DSL should name source windows, evidence gates, concurrency, retry,
timeouts, ownership, and artifacts. It should reference jobs and deterministic
steps by name. It should not inline role prompts, librarian judgment, wiki
rendering internals, or every future page-specialist role.

The useful invariant:

```text
if a state transition matters for safety, speed, replay, or operator trust,
it should be visible in the DSL or in a first-class artifact the DSL expects.
```

Real-time work follows the same rule. A live daemon cadence should not be
enabled just because it feels plausible. Historic event replay should produce
evidence first: fire schedule, snapshots, latency, cost, failed/retried jobs,
and source freshness. The top-level fabric can then treat that replay evidence
as policy input.

## Wiki Engine Integration Notes

The wiki engine is the reader surface, not the memory planner. Keep this split
visible while integrating the e08 renderer work:

```text
src/onectx/memory/wiki.py
  deterministic planner inputs, role route plans, route dry-runs,
  source freshness, and hired-agent preview artifacts

src/onectx/wiki/
  family discovery, source/talk folder scaffolding, rendering,
  render manifests, local route tables, browser serving, and render evidence

wiki-engine/
  portable renderer/theme/tools/schemas copied from the e08 wiki branch
```

The state machines should not learn Node renderer internals. They should name
the evidence produced by the renderer:

```text
wiki.render.succeeded
wiki.manifest.recorded
wiki.generated.available
wiki.render.completed
```

This gives the DSL a concrete reader-surface gate without making it a template
engine. `wiki_reader_loop` can say "build reader surface and wait for render
evidence"; `wiki_growth_fabric` can say "after role mutations, rebuild the
reader surface"; `memory_system_fabric` can say "a tick is not complete until
the visible wiki output has been regenerated or intentionally skipped."

Do not replace the planner CLI while integrating renderer commands. The public
surface should be one additive `1context wiki` namespace:

```text
planner/control:
  build-inputs
  plan-roles
  route-dry-run
  brackify

renderer/browser:
  list
  ensure
  render
  routes
  serve
  open
```

The important design tension is that wiki growth is FPGA-like and dynamic:
new pages create new talk folders, new talk folders activate new curators, and
operator edits can change routing on the next tick. The DSL should express the
control law and evidence gates; the route planner should derive the active
circuits from current files; the renderer should make the result readable and
record proof. That three-way split is the thing to preserve.
