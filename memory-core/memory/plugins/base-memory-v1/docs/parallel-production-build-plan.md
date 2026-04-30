# Milestone: Parallel Production Build Readiness

## Goal

Get `base-memory-v1` to the point where multiple implementation agents can
work in parallel on the e08-derived production architecture without losing the
experiment's lessons or creating overlapping patches.

The milestone is not "the whole memory system is done." The milestone is:
**the architecture has been decomposed into isolated, proof-carrying work lanes
that can be assigned to parallel agents with clear ownership, inputs, outputs,
and validation commands.**

## Done When

- The e08 production checklist is represented as implementation lanes.
- Each lane has disjoint file ownership or an explicit integration owner.
- Each lane names the e08 records it must preserve.
- Each lane has proof commands or artifact checks.
- The first parallel wave can run without editing the same files.
- Runtime output remains governed by the state-machine fabric:
  route plan -> hired-agent birth -> artifact -> validation -> invariant report.

## Baseline Facts

- [x] e08 records `0000` through `0036` have been read at least once and are
  summarized in `e08-to-system-translation.md`.
- [x] e08 late-stage production-readiness records are known to be load-bearing:
  `0028` silent no-op, `0029` runtime invariants, `0030` live vs replay,
  `0032` production checklist, `0033` gaps, `0034` quality audit, `0035`
  migrations, `0036` closing handoff.
- [x] The state-machine DSL has four memory/wiki machines loaded through the
  plugin: `memory_system_fabric`, `for_you_day`, `wiki_growth_fabric`, and
  `wiki_reader_loop`.
- [x] The route planner can scan an e08-style workspace and produce concrete
  hired-agent route rows.
- [x] The route executor can birth hired agents from route rows.
- [x] Claude `account_clean` live execution works with local subscription auth,
  no session persistence, suppressed user hooks, empty MCP, disabled slash
  commands, and Claude default tools.
- [x] One live e08 route hire produced a valid runtime artifact and passed
  runtime invariants:
  `route-hire-e08-live-one-hookclean`.
- [x] Runtime invariant reports distinguish planned work, produced work,
  explicit skips/no-changes/failures, and silent no-ops.

## E08 Lessons That Must Survive Parallel Work

- **Compute, do not enumerate.** No hardcoded date/era/corpus lists.
- **Skip and forgetting are first-class.** `skip`, `forget`, `defer`,
  `no_change`, and `needs_approval` are successful typed outcomes when earned.
- **Do not clip source capture to save tokens.** Capture generously; compress,
  split, or forget deliberately at synthesis boundaries.
- **Runtime invariants are mandatory.** Every phase needs preflight inventory
  and postflight diff.
- **Replay is not invariants.** Replay catches behavior; invariants catch
  execution no-ops. Both are needed.
- **Every contract change ships with a migration.** Forward-only prompt/schema
  changes create stale artifacts.
- **Curators apply; generators propose.** Daily editors and librarians propose
  where appropriate; curators own article mutation.
- **Operator-touched content wins.** Agents defer rather than overwrite
  contested operator edits.
- **Talk folders are the collaboration substrate.** Agent decisions should be
  visible as timestamped entries, not hidden session chatter.
- **The reader surface is deterministic where possible.** Indexes, backlinks,
  open questions, and landing pages should be builders, not agents.

## Parallelization Gate

Parallel implementation is safe after these are true:

- [x] Route rows are concrete enough to become hired-agent execution specs.
- [x] Live route harness can write to runtime-only artifacts.
- [x] Runtime invariants account for route-hire birth and validation.
- [x] Role validators exist for the main artifact families.
- [x] Migration receipts exist so contract changes are not hidden in prompts.
- [x] Replay sandbox exists so failure/concurrency tests can run without
  corrupting the source workspace.
- [x] Structural quality probes exist for stale state, resolved-open-question,
  placeholder, empty-section, and frontmatter drift.
- [x] Curator apply sandbox exists for one-section article mutation with
  operator-touched protection.
- [x] Scheduler/health surface exists for cadence planning, freshness gates,
  and runtime status payloads.
- [x] Source-mutation application is sandboxed before touching a real wiki.
- [ ] The assignment lanes below are accepted as the first parallel wave.

## First Parallel Wave

### Lane 1: Role Artifact Validators

Status: **implemented for the first structural pass.**

Owner files:

- `src/onectx/memory/wiki_validators.py`
- `tests/test_wiki_validators.py`

E08 records to preserve:

- `0006` talk-folder model
- `0029` runtime invariants
- `0032` production checklist
- `0034` quality audit

Build:

- Validators for `proposal`, `decided`, `concern`, `contradiction`,
  `concept`, `redaction_summary`, and generic `wiki_route_output`.
- Validators must distinguish valid `skip/no_change/defer/forget` artifacts
  from missing output.
- Validators should return structured `checks` and `failures`.

Proof:

- [x] `uv run --with pytest pytest tests/test_wiki_validators.py`
- [x] At least one fixture for a first-class skip/no-change artifact.
- [x] Route executor uses `validate_wiki_route_output` for live artifacts.

### Lane 2: Curator Apply Sandbox

Status: **implemented for one-section sandbox apply.**

Owner files:

- `src/onectx/memory/wiki_apply.py`
- `tests/test_wiki_apply.py`

E08 records to preserve:

- `0011` editor/curator split
- `0023` newest-overwrites + operator touched
- `0033` concurrent edit gap

Build:

- Apply a curator decision to a copied/sandboxed workspace only.
- Enforce ownership scope from route rows.
- Refuse edits when the target section has `operator-touched`.
- Produce a diff summary and validation payload.

Proof:

- Curator apply can replace exactly one day section in a sandbox.
- Operator-touched fixture returns `needs_approval` or `defer`.
- No source workspace mutation during tests.
- CLI proof:
  `uv run 1context memory wiki-apply --source-workspace tmp/wiki-apply-cli-source --decision tmp/wiki-apply-cli-decision.md --article 2026-04-20.md --section 2026-04-20 --run-id wiki-apply-cli-proof-2 --json`
- Verified with:
  `uv run --with pytest pytest tests/test_wiki_apply.py -q`

### Lane 3: Migration Phase 0.5

Status: **implemented for the receipt/verification spine.**

Owner files:

- `src/onectx/memory/migrations.py`
- `memory/plugins/base-memory-v1/migrations/`
- `tests/test_memory_migrations.py`

E08 records to preserve:

- `0028` silent no-op
- `0034` quality audit
- `0035` migration pattern

Build:

- Migration manifest format.
- Applied receipt format.
- Idempotent migration runner.
- Verification hook per migration.
- Initial migrations for concept frontmatter schema and missing role backfills
  can be stubs if they have receipts and checks.

Proof:

- Running the same migration twice records `applied` then `already_current`.
- Missing migration verification fails loud.
- `memory tick --run-migrations` records `contract_migrations.closed` evidence
  inside the cycle payload.
- Verified with:
  `uv run --with pytest pytest tests/test_memory_migrations.py tests/test_memory_tick.py tests/test_runtime_invariants.py tests/test_wiki_validators.py tests/test_wiki_route_executor.py -q`

### Lane 4: Librarian And Concept Writes

Owner files:

- `src/onectx/memory/concepts.py`
- `src/onectx/memory/wiki_executor.py` only for integration hooks agreed first
- `tests/test_concepts.py`

E08 records to preserve:

- `0012` two-of-three rule
- `0013` forgetting
- `0034` concept frontmatter drift

Build:

- Helpers for concept page create/expand/defer artifacts.
- Frontmatter schema audit.
- Expand-before-duplicate checks.
- Fading/archive metadata helpers.

Proof:

- Existing concept expands instead of duplicate creation.
- Ambiguous concept produces `defer` with reason.
- Missing frontmatter is detected as migration/backfill need.

### Lane 5: Replay Phase 3 Sandbox And Injection

Status: **implemented for dry-run sandbox, snapshot, and injection evidence.**

Owner files:

- `src/onectx/memory/replay.py`
- `tests/test_memory_replay.py`

E08 records to preserve:

- `0025` replay harness
- `0026` phase 1/2 verified
- `0030` live-vs-replay distinction
- `0033` replay gap

Build:

- `--sandbox` workspace copy.
- Failure injection records.
- Operator-edit injection fixture.
- Snapshot before/after diff.

Proof:

- Replay against sandbox does not mutate source.
- Injected failure is recorded and retryable.
- Injected operator edit causes a protected/deferred outcome.
- CLI proof:
  `uv run 1context memory replay-dry-run --start 2026-04-27T00:00:00Z --end 2026-04-27T01:00:00Z --sources codex --replay-run-id replay-sandbox-cli-proof --sandbox tmp/replay-sandbox-source --inject-failure agent_timeout --inject-operator-edit talk/day.md --json`
- Verified with:
  `uv run --with pytest pytest tests/test_memory_replay.py -q`

### Lane 6: Scheduler And Health Surface

Status: **implemented for deterministic cadence planning and health artifacts.**

Owner files:

- `src/onectx/memory/scheduler.py`
- `src/onectx/memory/health.py`
- CLI additions in `src/onectx/cli.py` coordinated with integrator
- `tests/test_memory_scheduler.py`

E08 records to preserve:

- `0027` fresh event importer gate
- `0029` runtime invariants
- `0033` real-time scheduler gap

Build:

- Cadence planner for hourly/daily/weekly/monthly fires.
- Last-success and last-failure status payloads.
- Health artifact that can feed a wiki status page.
- Freshness gate integrated with route execution.

Proof:

- A simulated clock produces expected hourly/daily/weekly routes.
- Stale importer blocks source-derived fires with explicit reason.
- Health payload names last successful fire per phase.
- CLI proof:
  `uv run 1context memory schedule --start 2026-04-26T23:00:00Z --end 2026-04-27T00:00:00Z --sources codex --allow-stale --run-id scheduler-cli-proof --json`
  and `uv run 1context memory health --run-id health-cli-proof --json`
- Verified with:
  `uv run --with pytest pytest tests/test_memory_scheduler.py -q`

### Lane 7: Quality Probes

Status: **implemented for cheap structural probes.**

Owner files:

- `src/onectx/memory/quality.py`
- `tests/test_memory_quality.py`

E08 records to preserve:

- `0024` multi-week validation
- `0033` voice-quality gap
- `0034` quality audit

Build:

- Cheap structural probes first: empty sections, stale current-state dates,
  unresolved open questions, initial-fill markers, missing frontmatter.
- Optional LLM rubric later; do not block the structural probes on it.

Proof:

- Fixture with stale `Current State` is flagged.
- Fixture with unresolved-but-resolved question is flagged.
- Fixture with persistent `initial fill` marker is flagged.
- CLI proof:
  `uv run 1context memory quality tmp/quality-fixture --now 2026-04-29 --run-id quality-cli-proof --json`
- Verified with:
  `uv run --with pytest pytest tests/test_memory_quality.py -q`

### Lane 8: Tier Rendering Reconciliation

Status: **implemented for canonical-private source files with explicit public
and internal siblings.**

Owner files:

- `src/onectx/wiki/`
- `src/onectx/memory/wiki.py` only for agreed integration hooks
- `tests/test_wiki_tiers.py`

E08 records to preserve:

- `0016` redaction tiers
- `0022` front door / renderer decision
- `0033` multi-tier reconciliation gap

Build:

- Pick one tier model and encode it.
- Recommended default: renderer reads explicit `.internal.md` and
  `.public.md` files when present, while canonical remains private source.
- Add manifest fields so tier source and generated output are traceable.

Proof:

- Private/internal/public fixture renders three distinct outputs.
- Public output never reads directly from private when internal exists.
- Source discovery treats `.internal.md` and `.public.md` as tier siblings,
  not duplicate canonical pages.
- Missing `.public.md` while `.internal.md` exists fails loud instead of
  falling back to private content for public output.
- Render manifests record `tier_sources` plus `audience_tier` on generated
  outputs.
- Verified with:
  `uv run --with pytest pytest tests/test_wiki_tiers.py -q`

## Integration Owner Lane

Status: **promotion bridge implemented and wired into the route/tick runner for
one-section curator apply.**

This lane should not be parallelized with heavy code ownership. It coordinates
interfaces and keeps the state-machine contract coherent.

Owner files:

- `src/onectx/memory/tick.py`
- `src/onectx/cli.py`
- `src/onectx/memory/wiki_executor.py`
- `memory/plugins/base-memory-v1/state_machines/*.py`
- `memory/plugins/base-memory-v1/control-fabric-milestone.md`

Responsibilities:

- Merge lane outputs into `memory tick`.
- Keep CLI flags coherent.
- Keep state-machine diagrams honest.
- Ensure runtime invariants include new phase outputs.
- Prevent lane workers from editing shared integration files without a handoff.

Proof:

- `uv run --with pytest pytest tests/test_memory_tick.py tests/test_runtime_invariants.py`
- `uv run 1context state-machines verify --run-id <run-id>`
- e08 smoke over copied workspace with route hires limited first, then expanded.
- `memory wiki-apply --promote-to-source --operator-approval promote-source`
  runs sandbox apply first, verifies ownership/sandbox hashes, writes source
  backup/snapshots, mutates the source article, and records promotion evidence.
- `memory tick --execute-route-hires --run-route-harness --promote-route-outputs`
  can now advance an accepted route artifact through sandbox apply and guarded
  source promotion.
- CLI proof:
  `uv run 1context memory wiki-apply --source-workspace tmp/wiki-apply-promote-cli-source --decision tmp/wiki-apply-cli-decision.md --article 2026-04-20.md --section 2026-04-20 --run-id wiki-apply-promote-cli-proof --promote-to-source --operator-approval promote-source --json`
- Blocked-gate CLI proof:
  `uv run 1context memory wiki-apply --source-workspace tmp/wiki-apply-cli-source --decision tmp/wiki-apply-cli-decision.md --article 2026-04-20.md --section 2026-04-20 --run-id wiki-apply-promote-blocked-cli-proof --promote-to-source --json`
- Verified with:
  `uv run --with pytest pytest tests/test_wiki_apply.py tests/test_wiki_route_executor.py -q`

## Recommended First Assignment Wave

Start with lanes that do not overlap:

1. Lane 1 validators
2. Lane 3 migrations
3. Lane 5 replay sandbox
4. Lane 7 quality probes

These lanes give the next wave stronger safety rails. After they land, launch:

5. Lane 2 curator apply sandbox
6. Lane 4 librarian/concepts
7. Lane 6 scheduler/health
8. Lane 8 tiers

Reason: validators, migrations, replay sandbox, and quality probes are
infrastructure multipliers. They make the later mutation-heavy work safer and
faster.

## Current Readiness

Parallel implementation readiness: **high for infrastructure lanes, medium-high
for sandboxed mutation lanes.**

Production end-to-end readiness: **source-mutating in a guarded route-runner
slice.** Tier reconciliation is encoded and tested, and curator apply can now
promote a validated route artifact into a real source article with explicit
operator approval, backups, snapshots, and ledger evidence. The remaining work
is running a small live copied-workspace tick with Claude route hires, source
promotion, wiki render, and invariant review in one command.

## Immediate Next Step

- [ ] Run and harden the copied-workspace end-to-end tick:
  route row -> hired Claude artifact -> sandbox apply -> source promotion ->
  wiki render -> runtime invariant report.

These are the best next code steps because migration receipts, validators, and
replay sandboxing are now in place; sandboxed apply is the guard before real
source mutation, source promotion is wired into the route runner with a
receipt-bearing gate, scheduler/health turns the pieces into a live loop, and
tier rendering blocks accidental public exposure of private canonical source
text.
