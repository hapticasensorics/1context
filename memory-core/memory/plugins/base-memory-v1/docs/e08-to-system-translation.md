# E08 To System Translation

This note preserves the e08 prototype lessons that are load-bearing for
`base-memory-v1`. It is intentionally compact; the implementation lanes live in
[parallel-production-build-plan.md](parallel-production-build-plan.md), and the
larger control-fabric checklist lives in
[../control-fabric-milestone.md](../control-fabric-milestone.md).

## Translation Rule

Treat e08 as evidence, not as runtime architecture.

- Preserve the product and memory lessons.
- Replace shell-runner mechanics with route plans, hired-agent birth records,
  durable artifacts, validation, and state-machine reconciliation.
- Keep source capture generous, then compress or forget deliberately at
  synthesis boundaries.
- Make skip, forget, defer, no-change, needs-approval, and failure typed
  outcomes instead of invisible no-ops.

## Load-Bearing Lessons

- Compute routes from inventory instead of enumerating dates, eras, pages, or
  concept lists.
- Fresh importer state is a gate. Do not pretend a multi-week route is valid if
  Codex or Claude source windows are stale or incomplete.
- Runtime invariants and replay are complementary. Replay catches behavior;
  invariants catch missing execution and silent no-ops.
- Every contract change needs a migration or backfill receipt before downstream
  generation trusts the new shape.
- Curators apply accepted edits; generators propose when mutation authority is
  not theirs.
- Operator-touched content wins. Agents should defer rather than overwrite
  contested human edits.
- Talk folders are the collaboration substrate. Decisions should be visible as
  timestamped entries, not hidden session chatter.
- Reader surfaces should be deterministic wherever possible: indexes,
  backlinks, open questions, route records, and manifests are builders first.

## System Shape

```text
facts -> signals -> route plan -> guarded jobs -> evidence -> next facts
```

The DSL owns clocks, scopes, facts, route plans, budgets, guards, state
transitions, reconciliation, and evidence. Agents own judgment, summaries,
proposals, and meaning. Storage owns durable truth. The wiki engine owns
deterministic rendering.

## Current Landing Zones

- Route planning and evidence: `src/onectx/memory/wiki.py`,
  `src/onectx/memory/wiki_authoring.py`, and the memory fabric state machines.
- Job contracts: `jobs/`.
- Prompt contracts: `prompts/`.
- Hired-agent execution shape: `harnesses/`, `linking.toml`, and runtime
  experience artifacts.
- Deterministic rendering: top-level `wiki-engine/` invoked by the Swift wiki
  runtime after memory-core requests `wiki.refresh`.
