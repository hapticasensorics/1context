from __future__ import annotations

from onectx.state_machines.v0_1 import (
    Machine,
    emit,
    event,
    expect,
    parallel,
    sequence,
    signal_edge,
    spawn,
    step,
    tick,
    wait_for,
)


def build() -> Machine:
    machine = Machine(
        "memory_system_fabric",
        version="0.1.0",
        title="Memory System Fabric",
        description=(
            "The top-level control fabric for turning raw Codex/Claude activity "
            "into durable personal context: ingest events, plan work, render lived "
            "experience packets, birth hired agents, collect artifacts, update the "
            "wiki, build reader surfaces, and feed the ledger back into the next tick."
        ),
    )

    cycle = machine.scope(
        "cycle",
        key="cycle_id",
        states=[
            "idle",
            "ingesting",
            "planning",
            "migrating_contracts",
            "rendering_experience",
            "birthing_agents",
            "running_agents",
            "validating",
            "routing_wiki",
            "building_reader_surface",
            "complete",
            "blocked",
            "retryable",
            "failed",
        ],
        initial="idle",
        description=(
            "One daemon or operator-triggered memory-processing cycle. The cycle "
            "may cover one hour, one fixed 4-hour block, a day, or a wiki route tick."
        ),
    )
    replay = machine.scope(
        "replay",
        key="replay_run_id",
        states=[
            "idle",
            "loading_events",
            "scheduling",
            "firing",
            "snapshotting",
            "injecting_failure",
            "analyzing",
            "complete",
            "blocked",
        ],
        description=(
            "One historic-event replay run. Replay uses real past Codex/Claude "
            "events as an event-stream clock to validate real-time cadence, "
            "latency, failure recovery, and mid-day wiki state before live rollout."
        ),
    )
    hire = machine.scope(
        "hire",
        key="hired_agent_uuid",
        states=[
            "planned",
            "experience_attached",
            "born",
            "running",
            "done",
            "skipped",
            "failed",
            "needs_retry",
            "needs_approval",
        ],
        description="One fresh hired agent with a birth certificate and explicit output contract.",
    )
    packet = machine.scope(
        "experience_packet",
        key="experience_id",
        states=["needed", "rendered", "attached", "reused", "too_large", "invalid"],
        description=(
            "One reproducible lived-experience artifact. It is not a side document; "
            "it is loaded into the hired agent as inherited operational history."
        ),
    )
    artifact = machine.scope(
        "artifact",
        key="path",
        states=["expected", "produced", "verified", "accepted", "rejected"],
        description="One durable output file, manifest, wiki page, or generated reader artifact.",
    )

    machine.clock("daemon", source="supervised_tick_or_schedule")
    machine.clock("activity", source="codex_claude_session_events")
    machine.clock("ledger", source="append_only_lakestore_events")
    machine.clock("filesystem", source="workspace_and_wiki_changes")
    machine.clock("human", source="operator_message_or_approval")

    machine.artifact(
        "runtime_events",
        kind="lakestore_table",
        path="{storage}/events",
        schema="runtime_event.v1",
        policies=["append_only", "ordered_by_timestamp"],
        description=(
            "Raw and normalized Codex/Claude activity rows: user messages, assistant "
            "messages, selected tool traces, session metadata, cwd, source harness, "
            "and stable row ids."
        ),
    )
    machine.artifact(
        "importer_cursor",
        kind="lakestore_cursor",
        path="{storage}/importers/{source}.json",
        schema="runtime_importer_cursor.v1",
        policies=["source_freshness_gate", "append_only_history"],
        description=(
            "Per-source freshness cursor for Codex, Claude Code, screen capture, "
            "and future event producers. Multi-day/month runs should defer when "
            "required sources are stale."
        ),
    )
    machine.artifact(
        "route_plan",
        kind="json_manifest",
        path="{runtime}/routes/{cycle_id}.json",
        schema="memory_route_plan.v1",
        policies=["deterministic", "reviewable"],
        description=(
            "The planner's cheap control output: active hours, fixed 4-hour blocks, "
            "oversized shards, daily editor work, concept scout work, and wiki roles."
        ),
    )
    machine.artifact(
        "contract_migrations",
        kind="migration_receipt_tree",
        path="{runtime}/migrations/{cycle_id}",
        schema="memory_contract_migrations.v1",
        policies=["idempotent", "computed_not_enumerated", "backfill_first_class"],
        description=(
            "Phase 0.5 migration and backfill receipts. Any prompt, schema, role, "
            "section, or talk-kind contract change must either be applied, skipped "
            "as already-applied, or fail loud before normal generation runs."
        ),
    )
    machine.artifact(
        "runtime_invariant_report",
        kind="json_manifest",
        path="{runtime}/invariants/{cycle_id}.json",
        schema="memory_runtime_invariants.v1",
        policies=["preflight_postflight_diff", "zero_silent_noops"],
        description=(
            "Per-cycle execution proof: what input implied should happen, what "
            "actually landed, which skips were legitimate, and which no-ops failed loud."
        ),
    )
    machine.artifact(
        "replay_run",
        kind="replay_artifact_tree",
        path="{runtime}/replay-runs/{replay_run_id}",
        schema="memory_replay_run.v1",
        policies=["deterministic_event_clock", "reproducible", "pauseable"],
        description=(
            "Historic-event replay output: config, events.jsonl, fires.jsonl, "
            "snapshots, summary, optional failure injections, and timing/cost measurements."
        ),
    )
    machine.artifact(
        "experience_packets",
        kind="markdown_packet_tree",
        path="{runtime}/experiences/{experience_id}",
        schema="experience_packet.v1",
        policies=["hashable", "reusable", "source_window_recorded"],
        description=(
            "Braided lived transcripts and agent-context files rendered from the "
            "route plan. Token budget decides whether a packet is full, block-sized, "
            "sharded, or reused."
        ),
    )
    machine.artifact(
        "birth_certificates",
        kind="ledger_event",
        schema="hired_agent.created.v1",
        policies=["append_only", "contains_prompt_hashes"],
        description=(
            "The exact agent birth record: job id, provider/model, account mode, "
            "tools, prompt stack, experience packet path/hash/window, output contract, "
            "and concurrency slot."
        ),
    )
    machine.artifact(
        "memory_artifacts",
        kind="markdown_or_json_outputs",
        path="{talk_folder}/**/*.md",
        schema="talk_entry_or_job_result.v1",
        policies=["append_only", "single_writer", "skip_is_valid"],
        description=(
            "Hourly talk entries, block results, shard notes, aggregate entries, "
            "daily proposals, concept candidates, decisions, concerns, and deferrals."
        ),
    )
    machine.artifact(
        "wiki_source",
        kind="markdown_workspace",
        path="{wiki_workspace}",
        schema="personal_context_wiki.v1",
        policies=["operator_touched_respected", "expand_before_duplicate"],
        description="For You, Your Context, concept pages, talk folders, and tiered public/internal/private source files.",
    )
    machine.artifact(
        "reader_surface",
        kind="rendered_wiki_site",
        path="{wiki}/generated",
        schema="wiki_reader_surface.v1",
        policies=["deterministic", "generated"],
        description=(
            "Generated wiki inputs plus rendered family outputs, render manifests, "
            "site manifest, content index, and localhost route table."
        ),
    )

    machine.evidence(
        "runtime_events.ready",
        artifact="runtime_events",
        checks=[
            "events_between(start,end,sources) returns rows in timestamp order",
            "rows carry source harness, session id, cwd, role/kind, and text",
            "retention policy chooses assistant/user by default and keeps tool traces only when configured",
        ],
        description="The system has enough ordered source activity to plan memory work.",
    )
    machine.evidence(
        "source_import.fresh",
        artifact="importer_cursor",
        checks=[
            "required source cursors advanced within runtime_policy.max_importer_staleness",
            "source windows requested by route plan are fully covered",
            "stale optional sources are recorded as degraded, not silently ignored",
        ],
        description="Fresh event import is available before multi-day, month, or real-time runs trust their inputs.",
    )
    machine.evidence(
        "route_plan.ready",
        artifact="route_plan",
        checks=[
            "active hours and empty hours are explicit",
            "fixed 4-hour chunks are assigned before one-hour fallbacks",
            "oversized hours split into stream-first shards when prompt budget is exceeded",
            "max_concurrent_agents is copied from runtime policy",
            "skip/forget/no-talk are valid planned outcomes",
        ],
        description="The cheap planner has decided what hired agents should exist.",
    )
    machine.evidence(
        "contract_migrations.closed",
        artifact="contract_migrations",
        checks=[
            "pending migrations are discovered from schema/prompt/role/section contract versions, not hardcoded era lists",
            "each migration is applied, already-applied, explicitly deferred, or failed with a receipt",
            "backfills for newly introduced roles and sections run before downstream generation depends on them",
            "post-migration verification proves affected artifacts now meet the declared contract",
        ],
        description="Forward contract changes have reconciled older wiki artifacts before new work proceeds.",
    )
    machine.evidence(
        "runtime_invariants.passed",
        artifact="runtime_invariant_report",
        checks=[
            "preflight inventory computes expected work from events, talk folders, wiki pages, and route plan",
            "postflight diff compares expected artifacts with produced, skipped, deferred, failed, and no_change outcomes",
            "skip-as-first-class outcomes include data-empty, already-current, operator-touched, or explicit forgetting reasons",
            "config-bug-shaped no-ops fail loud and block downstream phases",
            "run summary reports 0 silent no-ops before the cycle completes",
        ],
        description="The orchestrator can distinguish healthy skipping from missing work.",
    )
    machine.evidence(
        "replay_schedule.ready",
        artifact="replay_run",
        checks=[
            "event-stream start/end are recorded",
            "cadence boundary mapping is explicit",
            "agent fire schedule is derived before live fires",
            "dry-run can write events.jsonl and fires.jsonl without launching agents",
        ],
        description="Historic-event replay has a deterministic schedule before agents are fired.",
    )
    machine.evidence(
        "replay_snapshot.ready",
        artifact="replay_run",
        checks=[
            "snapshot stream time and wall time are recorded",
            "wiki source tree and generated reader surface are captured or referenced",
            "snapshot manifest lists open questions, page counts, talk-entry counts, and routeable URLs",
        ],
        description="Replay can inspect mid-run wiki state instead of only final output.",
    )
    machine.evidence(
        "replay_failure_injection.applied",
        artifact="replay_run",
        checks=[
            "injection type, target fire, stream timestamp, and expected recovery path are recorded",
            "operator edits and process kills happen only in replay sandbox mode",
            "recovery is later verified by runtime invariants or replay summary evidence",
        ],
        description="Replay deliberately exercised a failure or concurrent-edit path.",
    )
    machine.evidence(
        "replay_run.completed",
        artifact="replay_run",
        checks=[
            "fires.jsonl records every planned invocation",
            "summary includes per-role counts and latency/cost where available",
            "snapshots or failure injections are recorded when requested",
            "real-time questions answered by the run are named in the summary",
        ],
        description="Replay produced evidence that can tune real-time cadence and failure policy.",
    )
    machine.evidence(
        "experience_packets.ready",
        artifact="experience_packets",
        checks=[
            "every planned hire has an attached experience packet or an explicit no-experience reason",
            "agent-context.md contains the rendered lived experience loaded at birth",
            "source window, source sessions, renderer version, row count, and packet hash are recorded",
            "packet size stays under max_prompt_tokens or routes to shard/aggregate flow",
        ],
        description="The hires can be born with direct lived experience instead of rediscovering context.",
    )
    machine.evidence(
        "hired_agents.born",
        artifact="birth_certificates",
        checks=[
            "birth certificate exists for each routed hire",
            "experience_packet field is first-class, not buried in arbitrary params",
            "Claude account_clean launch mode records no-session-persistence, temp cwd, explicit tools, and disabled MCP/slash/plugin surfaces as configured",
            "concurrency slots do not exceed runtime_policy.max_concurrent_agents",
        ],
        description="The planned agents have been created with explicit bodies, memories, permissions, and outputs.",
    )
    machine.evidence(
        "agent_outputs.closed",
        artifact="memory_artifacts",
        checks=[
            "each hire ended done, skipped, no_change, needs_retry, needs_approval, or failed",
            "markdown frontmatter parses for written talk/wiki artifacts",
            "hourly outputs have timestamps inside the target window",
            "retryable failures are routed to fallback jobs instead of silently disappearing",
        ],
        description="The agent layer has settled enough for wiki routing or review.",
    )
    machine.evidence(
        "wiki_route_plan.ready",
        artifact="wiki_source",
        checks=[
            "wiki_growth_fabric scanned pages, concepts, talk folders, and operator-touched markers",
            "role jobs are configured from corpus facts rather than a fixed batch list",
            "curators/librarians/redactors/historians may skip or defer with reasons",
        ],
        description="The dynamic wiki fabric has decided what page-level work should run.",
    )
    machine.evidence(
        "reader_surface.ready",
        artifact="reader_surface",
        checks=[
            "wiki_reader_loop built indexes, bracket staging, backlinks, landing page, and this-week digest",
            "wiki.render.succeeded exists for rendered page families",
            "wiki.manifest.recorded records each render-manifest hash",
            "wiki.generated.available proves routeable generated files exist",
            "source markdown was not clobbered by generated staging output",
        ],
        description="The memory system has a rendered, navigable reader surface for humans and future agents.",
    )
    machine.evidence(
        "memory_cycle.artifact_written",
        checks=[
            "cycle.json exists under memory/runtime/cycles/{cycle_id}",
            "lakestore artifact row points at the cycle file",
            "artifact content_hash matches the current cycle file",
            "cycle payload records state_machine, scope, preflight, steps, recovery, and DSL contract",
        ],
        description="A concrete runner tick left durable proof before advancing or failing.",
    )
    machine.evidence(
        "memory_tick.recovery_recorded",
        checks=[
            "failed or retryable step has recovery.failure_count > 0",
            "recovery.next_action is retry_on_next_tick or operator_review",
            "terminal event is memory.tick.retryable or memory.tick.failed",
        ],
        description="A concrete runner failure became state-machine-readable recovery state.",
    )

    machine.signal(
        "memory.work_available",
        expr="activity.has_new_rows or filesystem.changed(wiki_workspace) or human.requested_memory_run or daemon.tick",
        reads=["runtime_events", "wiki_source", "ledger"],
    )
    machine.signal(
        "source_import.stale",
        expr="required importer cursor older than runtime_policy.max_importer_staleness",
        reads=["importer_cursor", "runtime_events"],
    )
    machine.signal(
        "replay.requested",
        expr="human.requested_replay or daemon.requested_replay_experiment",
        reads=["runtime_events", "importer_cursor"],
    )
    machine.signal(
        "replay.snapshot_due",
        expr="replay_clock.crossed_configured_snapshot_boundary",
        reads=["replay_run", "wiki_source", "reader_surface"],
    )
    machine.signal(
        "replay.failure_due",
        expr="replay_clock.crossed_configured_failure_injection_boundary",
        reads=["replay_run", "route_plan"],
    )
    machine.signal(
        "contract_migrations.pending",
        expr="route_plan.has_pending_contract_migrations or artifact_schema_version_mismatch",
        reads=["route_plan", "wiki_source", "memory_artifacts"],
    )
    machine.signal(
        "experience.too_large",
        expr="estimated_prompt_tokens > runtime_policy.max_prompt_tokens",
        reads=["route_plan", "experience_packets"],
    )
    machine.signal(
        "runtime_invariant.failed",
        expr="runtime_invariant_report.missing_expected_artifacts or runtime_invariant_report.silent_noops > 0",
        reads=["runtime_invariant_report", "route_plan", "memory_artifacts"],
    )
    machine.signal(
        "agent.layer_closed",
        expr="all planned hires have terminal outcome events",
        reads=["birth_certificates", "memory_artifacts", "ledger"],
    )
    machine.signal(
        "wiki.needs_routing",
        expr="memory_artifacts.changed or wiki_source.changed or human.requested_review",
        reads=["memory_artifacts", "wiki_source"],
    )
    machine.signal(
        "cycle.needs_retry",
        expr="ledger.has(needs_retry) and retry_budget.remaining",
        reads=["ledger", "route_plan"],
    )

    machine.from_(cycle, "idle").on(signal_edge("memory.work_available")).to(
        cycle,
        "ingesting",
        do=sequence(
            step("import_session_events"),
            step("normalize_runtime_events"),
            expect("runtime_events.ready"),
            expect("source_import.fresh", optional=True),
            emit("memory.events.ready"),
        ),
    )

    machine.from_(cycle, "ingesting").on(signal_edge("source_import.stale")).to(
        cycle,
        "blocked",
        do=sequence(
            step("record_source_import_staleness"),
            emit("memory.cycle.deferred_for_fresh_events"),
        ),
    )

    machine.from_(replay, "idle").on(signal_edge("replay.requested")).to(
        replay,
        "loading_events",
        do=sequence(
            step("load_historic_event_stream"),
            expect("runtime_events.ready"),
            emit("memory.replay.events_loaded"),
        ),
    )

    machine.from_(replay, "loading_events").on(event("memory.replay.events_loaded")).to(
        replay,
        "scheduling",
        do=sequence(
            step("derive_replay_fire_schedule"),
            expect("replay_schedule.ready"),
            emit("memory.replay.schedule_ready"),
        ),
    )

    machine.from_(replay, "scheduling").on(event("memory.replay.schedule_ready")).to(
        replay,
        "firing",
        do=sequence(
            step("execute_replay_fires"),
            expect("replay_run.completed"),
            emit("memory.replay.completed"),
        ),
    )

    machine.from_(replay, "firing").on(signal_edge("replay.snapshot_due")).to(
        replay,
        "snapshotting",
        do=sequence(
            step("capture_replay_wiki_snapshot"),
            expect("replay_snapshot.ready"),
            emit("memory.replay.snapshot_captured"),
        ),
    )

    machine.from_(replay, "snapshotting").on(event("memory.replay.snapshot_captured")).to(
        replay,
        "firing",
        do=emit("memory.replay.resume", reason="snapshot captured"),
    )

    machine.from_(replay, "firing").on(signal_edge("replay.failure_due")).to(
        replay,
        "injecting_failure",
        do=sequence(
            step("apply_replay_failure_injection"),
            expect("replay_failure_injection.applied"),
            emit("memory.replay.failure_injected"),
        ),
    )

    machine.from_(replay, "injecting_failure").on(event("memory.replay.failure_injected")).to(
        replay,
        "firing",
        do=emit("memory.replay.resume", reason="failure injection applied"),
    )

    machine.from_(replay, "firing").on(event("memory.replay.completed")).to(
        replay,
        "analyzing",
        do=sequence(
            step("write_replay_summary"),
            emit("memory.real_time_policy_evidence.ready"),
        ),
    )

    machine.from_(replay, "analyzing").on(event("memory.real_time_policy_evidence.ready")).to(
        replay,
        "complete",
        do=expect("replay_run.completed"),
    )

    machine.from_(cycle, "ingesting").on(event("memory.events.ready")).to(
        cycle,
        "planning",
        do=sequence(
            step("discover_active_hours"),
            step("derive_fixed_4_hour_blocks"),
            step("derive_oversized_hour_shards"),
            step("derive_daily_and_wiki_work"),
            expect("route_plan.ready"),
            emit("memory.route_plan.ready"),
        ),
    )

    machine.from_(cycle, "planning").on(event("memory.route_plan.ready")).to(
        cycle,
        "migrating_contracts",
        do=sequence(
            step("discover_contract_migrations_and_backfills"),
            step("run_contract_migrations_and_backfills"),
            expect("contract_migrations.closed"),
            emit("memory.contract_migrations.closed"),
        ),
    )

    machine.from_(cycle, "migrating_contracts").on(event("memory.contract_migrations.closed")).to(
        cycle,
        "rendering_experience",
        do=sequence(
            parallel(
                step("render_braided_block_experience", for_each="route_plan.block_scribe_jobs"),
                step("render_hour_or_shard_experience", for_each="route_plan.hour_or_shard_jobs"),
                step("reuse_or_render_day_experience", for_each="route_plan.daily_jobs"),
                fail="collect",
                max_concurrent="runtime_policy.max_concurrent_renderers",
            ),
            expect("experience_packets.ready"),
            emit("memory.experience_packets.ready"),
        ),
    )

    machine.from_(packet, "rendered").on(signal_edge("experience.too_large")).to(
        packet,
        "too_large",
        do=sequence(
            step("split_packet_by_stream_and_time"),
            emit("memory.route_plan.ready", reason="packet exceeded token budget; route through shard jobs"),
        ),
    )

    machine.from_(cycle, "rendering_experience").on(event("memory.experience_packets.ready")).to(
        cycle,
        "birthing_agents",
        do=sequence(
            parallel(
                spawn(
                    "memory.hourly.block_scribe",
                    for_each="route_plan.block_scribe_jobs",
                    key="block_start",
                    grants=["read.runtime_experience_packet", "write.private_talk_entry"],
                    expects=["agent_outputs.closed"],
                ),
                spawn(
                    "memory.hourly.shard_scribe",
                    for_each="route_plan.shard_scribe_jobs",
                    key="shard_id",
                    grants=["read.runtime_experience_packet", "write.hourly_shard_note"],
                    expects=["agent_outputs.closed"],
                ),
                spawn(
                    "memory.hourly.aggregate_scribe",
                    for_each="route_plan.aggregate_scribe_jobs",
                    key="hour",
                    grants=["read.hourly_shard_notes", "write.private_talk_entry"],
                    expects=["agent_outputs.closed"],
                ),
                spawn(
                    "memory.daily.editor",
                    for_each="route_plan.daily_editor_jobs",
                    key="date",
                    grants=["read.day_hourlies", "read.talk_folder", "write.private_talk_proposal"],
                    expects=["agent_outputs.closed"],
                ),
                spawn(
                    "memory.concept.scout",
                    for_each="route_plan.concept_scout_jobs",
                    key="date",
                    grants=["read.day_hourlies", "read.concepts", "write.concept_questions"],
                    expects=["agent_outputs.closed"],
                ),
                fail="collect",
                max_concurrent="runtime_policy.max_concurrent_agents",
            ),
            expect("hired_agents.born"),
            emit("memory.hired_agents.born"),
        ),
    )

    machine.from_(cycle, "birthing_agents").on(event("memory.hired_agents.born")).to(
        cycle,
        "running_agents",
        do=sequence(
            step("launch_claude_account_clean_harness"),
            wait_for("agent.layer_closed", timeout="runtime_policy.agent_timeout"),
        ),
    )

    machine.from_(cycle, "running_agents").on(signal_edge("agent.layer_closed")).to(
        cycle,
        "validating",
        do=sequence(
            step("run_runtime_invariant_preflight_postflight_diff"),
            step("validate_talk_entries_and_job_results"),
            expect("runtime_invariants.passed"),
            expect("agent_outputs.closed"),
            emit("memory.agent_outputs.closed"),
        ),
    )

    machine.from_(cycle, "validating").on(signal_edge("runtime_invariant.failed")).to(
        cycle,
        "blocked",
        do=sequence(
            step("record_runtime_invariant_failure"),
            emit("memory.cycle.blocked_on_invariant_failure"),
        ),
    )

    machine.from_(cycle, "validating").on(event("memory.agent_outputs.closed")).to(
        cycle,
        "routing_wiki",
        do=sequence(
            step("run_wiki_growth_fabric"),
            expect("wiki_route_plan.ready"),
            emit("wiki.fabric.tick"),
        ),
    )

    machine.from_(cycle, "validating").on(signal_edge("wiki.needs_routing")).to(
        cycle,
        "routing_wiki",
        do=sequence(
            step("run_wiki_growth_fabric"),
            expect("wiki_route_plan.ready"),
            emit("wiki.fabric.tick"),
        ),
    )

    machine.from_(cycle, "routing_wiki").on(event("wiki.agent_layer.closed")).to(
        cycle,
        "building_reader_surface",
        do=sequence(
            step("run_wiki_reader_loop"),
            step("render_wiki_engine_families"),
            expect("reader_surface.ready"),
            emit("memory.reader_surface.ready"),
        ),
    )

    machine.from_(cycle, "building_reader_surface").on(event("memory.reader_surface.ready")).to(
        cycle,
        "complete",
        do=sequence(
            step("append_cycle_summary_event"),
            emit("memory.cycle.complete"),
        ),
    )

    machine.from_(hire, "needs_retry").on(signal_edge("cycle.needs_retry")).to(
        hire,
        "needs_retry",
        do=sequence(
            step("derive_retry_plan"),
            emit("memory.route_plan.ready", reason="retryable hire outcome"),
        ),
    )

    machine.from_(cycle, "running_agents").on(event("approval.needed")).to(
        cycle,
        "blocked",
        do=emit("memory.cycle.blocked_on_approval"),
    )

    machine.from_(cycle, "idle").on(tick("daemon")).stay(
        do=emit("memory.work_available", reason="scheduled daemon tick")
    )

    return machine
