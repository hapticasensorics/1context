# Typed Control Fabric Architecture

## Purpose

The state-machine system exists to make agent work durable enough to trust.

Agents can reason, synthesize, criticize, and propose. The control fabric does
not try to make cognition deterministic. It makes the institution around
cognition deterministic:

```text
facts -> signals -> route plans -> guarded jobs -> evidence -> persisted state
```

The system should let 1Context answer:

- What facts were observed?
- Which work was allowed to start?
- Which agent or deterministic step owned the work?
- What artifact was expected?
- What evidence proved the artifact was valid?
- Which state advanced, and can it resume after restart?
- Which skips, deferrals, no-changes, failures, and retries were explicit?

## Layer Model

| Layer | Owns | Must Not Own |
| --- | --- | --- |
| DSL authoring API | orchestration intent and readable machine shape | validation internals or runtime side effects |
| Pydantic IR | legal shape, cross references, serialization, schemas | agent reasoning or job execution |
| Runtime planner | transition selection and persisted scope state | hidden business logic not visible in IR |
| Queue | retryable work identity and durable status | deciding semantic quality |
| Supervision | capacity, timeout, retry, dead-letter decisions | mutating wiki source directly |
| Hired-agent runner | bounded agent launch and result envelope | deciding global orchestration policy |
| Evidence validators | proof that artifacts satisfy contracts | accepting an agent's word as completion |
| Ledger/lakestore | durable event and artifact memory | implicit state not visible to review |

## Core Boundary

The authoring DSL should compile into typed data:

```text
Machine builder objects
  -> MachineIR
  -> JSON Schema / JSON payloads
  -> runtime queue and execution
  -> evidence receipts
  -> persisted scope state
```

The DSL is allowed to be pleasant Python. The compiled result must be boring,
portable, inspectable JSON.

## Ownership Flow

```text
plugin author
  writes versioned DSL definitions

Pydantic compiler
  validates machine IR and emits schemas

daemon tick
  observes facts and selects transitions

route planner
  materializes route plans and queue rows

supervisor
  launches bounded work or records why it waited

agent/deterministic step
  produces artifacts

validator
  produces evidence receipts

runtime
  advances persisted scope state
```

No layer should need to guess what a previous layer meant.

## Current Prototype Pressure Points

The current system already has the right nouns: scopes, evidence, artifacts,
route plans, guarded spawns, queue items, supervision decisions, and persisted
state.

The weak point is that too many of those contracts are still plain dictionaries
and strings. That makes the prototype fast to evolve but easy to misunderstand.

The typed control fabric removes the weak point without throwing away the good
vocabulary.

## What Pydantic Improves

- Compiled IR shape is validated at compile time.
- Action payloads become discriminated unions instead of arbitrary dicts.
- Persisted queue and scope-state records have versioned schemas.
- Evidence and artifact contracts can emit JSON Schema.
- Cross references can fail early: missing scope, state, job, evidence, or
  artifact.
- Migration/backfill receipts can be typed before downstream readers trust new
  shapes.

## What The DSL Keeps

- `Machine`
- `scope`
- `clock`
- `artifact`
- `evidence`
- `signal`
- `from_(...).on(...).to(...)`
- `spawn`
- `expect`
- `emit`
- `parallel`
- `retry`
- `timeout`
- `set_state`

The DSL keeps the words humans review. Pydantic keeps the contracts machines
depend on.

## What The DSL Should Lose

- Loose dicts as the final ABI.
- Silent normalization that drops malformed actions.
- Stringly typed action payloads where a model can exist.
- Ad hoc verification that duplicates schema validation.
- Guard or signal strings pretending to execute business logic.
- Evidence declarations that cannot resolve to concrete validators over time.

## Architecture Rule

If it describes orchestration intent, keep it in the DSL.

If it crosses a subsystem boundary, persists, validates, migrates, or gets
consumed by another tool, make it a Pydantic model.

