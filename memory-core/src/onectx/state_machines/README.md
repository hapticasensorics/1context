# AI State Machine Language

This package is the reusable Python-embedded language for 1Context state
machines.

The theory is simple: 1Context puts deterministic rails around
non-deterministic cognition. Agents are allowed to interpret, synthesize,
question, criticize, and propose. The state machine does not try to make that
thinking deterministic. It makes the surrounding institution deterministic
enough to inspect:

```text
events enter
guards read declared state
commands start work
agents create artifacts
evidence validates artifacts
ledger records what happened
```

The durable workspace is the product surface. Agents do not merely chat; they
mutate a human-readable memory substrate through files, rendered pages,
transcripts, proposals, and ledger events.

For another agent entering this codebase, the important move is this:
state machines are not here to replace agent reasoning. They are here to make
agent reasoning leave durable, inspectable traces before the system believes
anything changed.

That gives the language a narrow job:

```text
describe what may happen
name who may do it
name what durable artifact should appear
name what evidence proves it
record the result in a ledger
```

The language is a semantic control plane, not a general workflow framework.
Python can host the authoring API, but the authored machine should compile
into plain data that another runner, reviewer, or future agent can inspect.

Current language:

```text
ai_state_machine 0.1.0
```

Compatible plugin requirement range:

```text
>=0.1.0,<0.2.0
```

The language belongs outside memory plugins. Plugins author definitions with
the language; the runtime later evaluates the compiled IR. Multiple DSL
versions can live in this package at once. A plugin dependency selects one
compatible runtime before any machine files are imported.

This split is deliberate:

```text
DSL/runtime       reusable source-code tooling
plugin machines   shareable memory-system design
runtime ledger    local record of concrete execution
```

Plugins carry the memory-system idea. The source tree carries the language
versions able to read those ideas. A user's local runtime carries the lived
history of which idea actually ran.

Versioned authoring modules should be explicit:

```python
from onectx.state_machines.v0_1 import Machine, event, sequence, spawn
```

The package root re-exports the current default for convenience, but plugin
definitions should prefer a versioned import once they are meant to be shared.

Plugins that ship state-machine definitions must declare a dependency:

```toml
kind = "state_machine_language"
language = "ai_state_machine"
version_spec = ">=0.1.0,<0.2.0"
```

The loader checks this dependency, selects the highest available compatible
runtime, and then imports plugin machine files. If a machine's compiled IR says
it was authored with a different DSL version than the selected runtime, loading
fails.

## Core Shape

```text
Machine
Scope
Clock
Artifact
Evidence
Event
Signal
Transition
Action
```

Actions are small:

```text
step       run a named deterministic or adapter-backed operation
spawn      hire or resume an agent job
expect     require evidence
wait_for   suspend until an event
emit       write a ledger event
sequence   ordered action group
parallel   concurrent action group
race       first-completing action group
retry      retry wrapper
timeout    timeout wrapper
set_state  scoped state update
```

Artifacts and evidence are declarations, not validators yet:

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

Use `expect("hourly_talk_entry.valid")` inside actions to wait for declared
evidence. The runner can later attach concrete validators to these check names.

This distinction is central:

```text
artifact = durable thing produced or observed
evidence = named proof that the thing is real and valid enough to advance
```

The machine should trust evidence, not an agent saying "done."

## Discipline

Guards should be pure ledger/state queries. Actions should emit commands or
named work, not hide arbitrary side effects inside conditions.

State machines use many scoped local states instead of one giant global state.
This keeps persistent agents, jobs, days, artifacts, and system lifecycle
independently observable.

Keep the language intentionally less powerful than Python. Python is only the
authoring surface; the compiled IR should stay boring, inspectable, versioned,
and eventually replayable.
