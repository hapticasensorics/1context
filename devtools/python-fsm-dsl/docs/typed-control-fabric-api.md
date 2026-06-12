# Typed Control Fabric API Design

## API Goal

The authoring API should read like a small hardware control sketch.

It should be declarative enough to review, but not so magical that the compiled
IR becomes surprising.

## Authoring Example

```python
from onectx.state_machines.v0_1 import (
    Machine,
    emit,
    event,
    expect,
    parallel,
    sequence,
    spawn,
    step,
)


def build() -> Machine:
    machine = Machine(
        "wiki_growth_fabric",
        version="0.1.0",
        title="Wiki Growth Fabric",
    )

    corpus = machine.scope(
        "corpus",
        key="wiki_workspace",
        states=["idle", "scanning", "routing", "running_agents", "review_ready"],
        initial="idle",
    )

    machine.artifact(
        "role_route_plan",
        kind="json",
        path="{runtime}/wiki/{wiki_id}/role-route-plan.json",
        schema="wiki_role_route_plan.v1",
        policies=["deterministic", "generated", "reviewable"],
    )

    machine.evidence(
        "role_route_plan.ready",
        artifact="role_route_plan",
        checks=["jobs have ownership", "expected outputs are declared"],
    )

    machine.from_(corpus, "scanning").on(event("wiki.inventory.ready")).to(
        corpus,
        "routing",
        do=sequence(
            step("derive_page_governance_map"),
            step("derive_role_route_plan"),
            expect("role_route_plan.ready"),
            emit("wiki.route_plan.ready"),
        ),
    )

    machine.from_(corpus, "routing").on(event("wiki.route_plan.ready")).to(
        corpus,
        "running_agents",
        do=sequence(
            parallel(
                spawn(
                    "memory.wiki.historian",
                    for_each="role_route_plan.historian_jobs",
                    key="job_key",
                    expects=["historian_entry.valid"],
                ),
                spawn(
                    "memory.wiki.librarian",
                    for_each="role_route_plan.librarian_jobs",
                    key="job_key",
                    expects=["concept_page.created_expanded_or_deferred"],
                ),
                max_concurrent="runtime_policy.max_concurrent_agents",
                fail="collect",
            ),
            expect("agent_layer.closed"),
            emit("wiki.agent_layer.closed"),
        ),
    )

    return machine
```

## Compiled Shape

The API compiles through Pydantic:

```python
raw = machine._to_raw_ir()
ir = MachineIR.model_validate(raw, context={"jobs": jobs})
payload = ir.model_dump(mode="json", exclude_none=True)
```

The raw builder is an implementation detail. `MachineIR` is the contract.

## Proposed Model Names

```text
LanguageRuntimeIR
MachineIR
ScopeIR
ClockIR
ArtifactIR
EvidenceIR
SignalIR
EventIR
TransitionIR
TargetIR
ActionIR
StepActionIR
SpawnActionIR
ExpectActionIR
WaitForActionIR
EmitActionIR
SequenceActionIR
ParallelActionIR
RaceActionIR
RetryActionIR
TimeoutActionIR
SetStateActionIR
```

Runtime payload models:

```text
TransitionRequestModel
TransitionPlanModel
TransitionExecutionModel
TransitionChainExecutionModel
ScopeStateModel
WorkItemModel
GrowthSignalModel
GrowthDecisionModel
GrowthPlanModel
SupervisionDecisionModel
SupervisionPlanModel
HiredAgentExecutionResultModel
EvidenceReceiptModel
```

## Action Union

Actions should be a discriminated union keyed by `kind`.

```python
ActionIR = Annotated[
    StepActionIR
    | SpawnActionIR
    | ExpectActionIR
    | WaitForActionIR
    | EmitActionIR
    | SequenceActionIR
    | ParallelActionIR
    | RaceActionIR
    | RetryActionIR
    | TimeoutActionIR
    | SetStateActionIR,
    Field(discriminator="kind"),
]
```

This removes the need for runtime code to guess whether a dict is a valid
action.

## Cross-Reference Checks

The compiler should fail when:

- a transition source scope is unknown
- a transition target state is not in the target scope
- `expect("x")` references undeclared evidence, unless marked external
- `spawn("job.id")` references an unknown job
- `artifact.schema` references an unknown schema, unless marked external
- `set_state(scope, state)` targets an invalid state
- `parallel(max_concurrent=...)` is neither a positive integer nor a named
  runtime policy reference

## Schema Export

The CLI should eventually expose:

```bash
uv run 1context state-machines schemas
```

That command should write JSON Schema for:

- the state-machine IR
- runtime queue items
- growth plans
- supervision plans
- transition execution records
- evidence receipts

## API Design Rules

1. Builder methods may be friendly.
2. Compiled IR must be strict.
3. Persisted payloads must be versioned.
4. Unknown extra fields should be rejected at stable boundaries.
5. Experimental fields must be explicitly marked experimental.
6. No helper should silently drop invalid work.
7. The API should prefer named policies over inline code.

## Things To Remove Or Demote

| Current Shape | Desired Shape |
| --- | --- |
| arbitrary `dict[str, Any]` actions | discriminated `ActionIR` union |
| prose-only evidence checks | evidence declaration plus validator binding |
| guard strings as implied logic | named predicate references or documentation |
| runtime `normalize_actions()` dropping bad dicts | compile-time validation failure |
| manual schema comments | generated JSON Schema |
| duplicated status vocabularies | shared typed enums |

