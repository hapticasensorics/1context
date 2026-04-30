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
)


def build() -> Machine:
    machine = Machine(
        "wiki_growth_fabric",
        version="0.1.0",
        title="Wiki Growth Fabric",
        description=(
            "A reconfigurable state-machine fabric for a growing personal context wiki. "
            "As pages, talk folders, concepts, and open questions appear, the machine "
            "derives the next set of curator/editor/librarian/redactor jobs instead of "
            "assuming a fixed pipeline forever. This is the FPGA-like layer: durable "
            "signals configure which agent circuits are active on the next tick."
        ),
    )

    corpus = machine.scope(
        "corpus",
        key="wiki_workspace",
        states=["idle", "scanning", "routing", "running_agents", "building_reader_surface", "review_ready", "blocked"],
        initial="idle",
        description="One wiki corpus: articles, concepts, talk folders, generated pages, and staging artifacts.",
    )
    page = machine.scope(
        "page",
        key="page_slug",
        states=[
            "unknown",
            "observed",
            "needs_editor",
            "needs_curator",
            "curator_adjudicating",
            "needs_migration",
            "needs_redaction",
            "stable",
            "operator_touched",
        ],
        description="One source page or concept page inside the corpus.",
    )
    talk = machine.scope(
        "talk",
        key="talk_folder",
        states=["quiet", "has_proposals", "has_concerns", "has_decisions", "archivable"],
        description="One talk folder as the mutable workbench for proposals, decisions, concerns, and closures.",
    )
    agent_role = machine.scope(
        "agent_role",
        key="role_id",
        states=["dormant", "eligible", "queued", "running", "done", "skipped", "failed"],
        description=(
            "One logical agent circuit. Roles become eligible from facts in the corpus: "
            "historian, editor, curator, librarian, biographer, contradiction flagger, "
            "redactor, and future page-specific specialists."
        ),
    )

    machine.clock("filesystem", source="wiki_workspace_file_changes")
    machine.clock("ledger", source="hired_agent_and_artifact_events")
    machine.clock("timer", source="scheduled_pipeline_tick")
    machine.clock("human", source="operator_trigger_or_operator_touched_marker")

    machine.artifact(
        "wiki_inventory",
        kind="json",
        path="{runtime}/wiki/{wiki_id}/inventory.json",
        schema="wiki_inventory.v1",
        policies=["deterministic", "generated"],
        description=(
            "A scan of source pages, concept pages, talk folders, section markers, "
            "operator-touched markers, frontmatter, open questions, and generated artifacts."
        ),
    )
    machine.artifact(
        "role_route_plan",
        kind="json",
        path="{runtime}/wiki/{wiki_id}/role-route-plan.json",
        schema="wiki_role_route_plan.v1",
        policies=["deterministic", "generated", "reviewable"],
        description=(
            "The dynamic configuration for this tick: which jobs are needed, why, "
            "their inputs, max concurrency, and which pages/talk folders they own."
        ),
    )
    machine.artifact(
        "page_governance",
        kind="json_manifest",
        path="{runtime}/wiki/{wiki_id}/page-governance.json",
        schema="wiki_page_governance.v1",
        policies=["curator_as_adjudicator", "librarian_as_nominator", "operator_touched_respected"],
        description=(
            "Per-page jurisdiction map: which curator owns the page, which sections "
            "are protected by operator-touched markers, which migrations/backfills "
            "are pending, and which librarian nominations need curator adjudication."
        ),
    )
    machine.artifact(
        "wiki_migration_receipts",
        kind="jsonl_receipts",
        path="{runtime}/wiki/{wiki_id}/migrations.jsonl",
        schema="wiki_migration_receipt.v1",
        policies=["idempotent", "schema_versioned", "backfill_first_class"],
        description=(
            "Receipts for wiki contract reconciliation: frontmatter schema backfills, "
            "new section forced-fill passes, newly introduced role backfills, and "
            "talk-kind migrations."
        ),
    )
    machine.artifact(
        "talk_decision_entries",
        kind="talk_entry_files",
        path="{workspace}/**/*.talk/*.md",
        schema="talk_entry.v1",
        policies=["append_only", "operator_touched_respected"],
        description="Agent and operator decisions, concerns, proposals, deferrals, and closures.",
    )
    machine.artifact(
        "article_pages",
        kind="markdown_pages",
        path="{workspace}/*.md",
        schema="wiki_article.v1",
        policies=["operator_touched_respected", "newest_overwrites_unless_touched"],
        description="For You, Your Context, landing-adjacent pages, and future article surfaces.",
    )
    machine.artifact(
        "concept_pages",
        kind="markdown_pages",
        path="{concept_dir}/*.md",
        schema="concept_page.v1",
        policies=["two_of_three_notability", "expand_before_duplicate", "sweep_fading"],
        description="Named-subject pages created, expanded, categorized, linked, and eventually archived.",
    )
    machine.artifact(
        "reader_surface",
        kind="rendered_wiki_site",
        path="{wiki}/generated",
        schema="wiki_reader_surface.v1",
        policies=["deterministic", "generated"],
        description=(
            "Generated indexes, resolved links, backlinks, landing page, this-week "
            "digest, staged concepts, rendered family outputs, and route manifests."
        ),
    )

    machine.evidence(
        "wiki_inventory.ready",
        artifact="wiki_inventory",
        checks=[
            "source pages are listed with frontmatter and section markers",
            "concept pages are listed with aliases, categories, status, and project metadata",
            "talk entries are indexed by kind, timestamp, target page, and parent",
            "operator-touched markers are indexed before any mutating job is routed",
            "open questions and concerns are first-class facts, not prose noise",
        ],
        description="The corpus has been scanned into facts that can drive routing.",
    )
    machine.evidence(
        "role_route_plan.ready",
        artifact="role_route_plan",
        checks=[
            "historian routes only when fresh day/week evidence exists",
            "editor routes when hourly/day evidence exists but no accepted proposal exists",
            "hourly answerer routes when historian questions target a specific hourly entry",
            "curator routes when proposals exist and target sections are not operator-touched",
            "librarian routes when concept candidates or repeated brackets cross the notability threshold",
            "librarian sweep routes when concepts have weak or stale reinforcement",
            "biographer routes when enough weekly material exists or prior biography needs continuity",
            "contradiction flagger routes when claims changed across the reference window",
            "redactor routes when private/internal/public tier outputs are stale",
            "reader build routes after any accepted source mutation",
            "page curators adjudicate page-specific merges; librarians nominate concept changes rather than overwriting curated pages directly",
            "topics/projects/open-questions index generation remains deterministic unless page-specific curator work is needed",
        ],
        description="The machine has configured the active agent circuits for this tick.",
    )
    machine.evidence(
        "page_governance.ready",
        artifact="page_governance",
        checks=[
            "every mutating route names the page or talk folder it owns",
            "page-specific curator jurisdiction is explicit for For You, Your Context, concept pages, topics, projects, and open questions",
            "librarian concept nominations that touch existing pages route through that page's curator when adjudication is needed",
            "operator-touched sections block or defer mutating jobs before an agent is launched",
        ],
        description="The corpus has a page-level governance map before agents mutate wiki memory.",
    )
    machine.evidence(
        "wiki_migrations.closed",
        artifact="wiki_migration_receipts",
        checks=[
            "schema-version mismatches become migration jobs or explicit deferrals",
            "newly added roles backfill historical pages that predate the role",
            "empty or initial-fill sections older than the configured threshold are routed for forced-fill or explicit defer",
            "resolved open questions write [RESOLVED] or equivalent closure entries where aggregators can see them",
        ],
        description="The wiki reconciles older artifacts when contracts evolve, instead of only generating forward.",
    )
    machine.evidence(
        "agent_layer.closed",
        artifact="talk_decision_entries",
        checks=[
            "all routed jobs completed, skipped, deferred, or failed with an explicit reason",
            "skip and forgetting decisions are recorded as valid outcomes",
            "no_change outcomes are visible and explain whether they mean empty data, already current, operator-touched, or deliberate forgetting",
            "operator-touched sections were not modified by agents",
            "new concepts expanded existing pages before creating duplicates",
            "new concerns or contradictions remain visible to the next route plan",
        ],
        description="The dynamic agent layer has settled for this tick.",
    )
    machine.evidence(
        "reader_surface.ready",
        artifact="reader_surface",
        checks=[
            "memory.wiki.build_inputs completed",
            "topics/projects/open-questions/landing/this-week exist",
            "bracket aliases and external fallbacks resolved in staging",
            "backlinks index exists",
            "concept pages are staged with What links here",
            "wiki.render.succeeded exists after source mutations",
            "wiki.manifest.recorded exists for the render manifest",
            "wiki.generated.available exposes localhost routes",
        ],
        description="The wiki is reader-ready after source mutations.",
    )

    machine.signal(
        "corpus.changed",
        expr="filesystem.changed(workspace) or ledger.has(event in ['talk.entry.created', 'concept.page.updated', 'article.page.updated'])",
        reads=["wiki_inventory", "talk_decision_entries", "article_pages", "concept_pages"],
    )
    machine.signal(
        "roles.need_reconfiguration",
        expr="wiki_inventory.ready and (corpus.changed or timer.tick or human.requested_review)",
        reads=["wiki_inventory"],
    )
    machine.signal(
        "operator_touch_blocks_mutation",
        expr="page.has_operator_touched_marker and proposed_job.mutates_same_section",
        reads=["wiki_inventory", "role_route_plan"],
    )
    machine.signal(
        "wiki_contract_migration_needed",
        expr="wiki_inventory.has_schema_version_mismatch or wiki_inventory.has_stale_initial_fill or new_role_requires_backfill",
        reads=["wiki_inventory", "article_pages", "concept_pages"],
    )
    machine.signal(
        "page_curator_adjudication_needed",
        expr="role_route_plan.has_page_specific_merge or librarian_nomination.targets_existing_page",
        reads=["role_route_plan", "page_governance", "talk_decision_entries"],
    )

    machine.from_(corpus, "idle").on(event("wiki.fabric.tick")).to(
        corpus,
        "scanning",
        do=sequence(
            step("scan_wiki_inventory"),
            expect("wiki_inventory.ready"),
            emit("wiki.inventory.ready"),
        ),
    )
    machine.from_(corpus, "scanning").on(event("wiki.inventory.ready")).to(
        corpus,
        "routing",
        do=sequence(
            step("derive_page_governance_map"),
            step("derive_role_route_plan"),
            expect("page_governance.ready"),
            expect("role_route_plan.ready"),
            emit("wiki.route_plan.ready"),
        ),
    )
    machine.from_(corpus, "routing").on(signal_edge("wiki_contract_migration_needed")).to(
        corpus,
        "routing",
        do=sequence(
            step("route_wiki_contract_migrations"),
            expect("wiki_migrations.closed"),
            emit("wiki.migrations.closed"),
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
                    "memory.daily.editor",
                    for_each="role_route_plan.editor_jobs",
                    key="job_key",
                    expects=["daily_section_proposal.verified"],
                ),
                spawn(
                    "memory.hourly.answerer",
                    for_each="role_route_plan.hourly_answerer_jobs",
                    key="job_key",
                    expects=["hourly_answer_reply.valid"],
                ),
                spawn(
                    "memory.wiki.for_you_curator",
                    for_each="role_route_plan.for_you_curator_jobs",
                    key="job_key",
                    expects=["article_section.updated_or_skipped"],
                ),
                spawn(
                    "memory.wiki.context_curator",
                    for_each="role_route_plan.context_curator_jobs",
                    key="job_key",
                    expects=["your_context.updated_or_skipped"],
                ),
                spawn(
                    "memory.wiki.librarian",
                    for_each="role_route_plan.librarian_jobs",
                    key="job_key",
                    expects=["concept_page.created_expanded_or_deferred"],
                ),
                spawn(
                    "memory.wiki.librarian_sweep",
                    for_each="role_route_plan.librarian_sweep_jobs",
                    key="job_key",
                    expects=["concept_sweep.decision_recorded"],
                ),
                spawn(
                    "memory.wiki.biographer",
                    for_each="role_route_plan.biographer_jobs",
                    key="job_key",
                    expects=["biography.updated_or_skipped"],
                ),
                spawn(
                    "memory.wiki.contradiction_flagger",
                    for_each="role_route_plan.contradiction_jobs",
                    key="job_key",
                    expects=["contradiction_entries.valid"],
                ),
                spawn(
                    "memory.wiki.redactor",
                    for_each="role_route_plan.redaction_jobs",
                    key="job_key",
                    expects=["tier_outputs.updated_or_skipped"],
                ),
                spawn(
                    "memory.wiki.source_packet_shard",
                    for_each="role_route_plan.source_packet_shard_jobs",
                    key="job_key",
                    expects=["source_packet_shard_note.valid"],
                ),
                spawn(
                    "memory.wiki.source_packet_aggregate",
                    for_each="role_route_plan.source_packet_aggregate_jobs",
                    key="job_key",
                    expects=["source_packet_aggregate.ready"],
                ),
                fail="collect",
                max_concurrent="runtime_policy.max_concurrent_agents",
            ),
            expect("agent_layer.closed"),
            emit("wiki.agent_layer.closed"),
        ),
    )
    machine.from_(page, "needs_curator").on(signal_edge("page_curator_adjudication_needed")).to(
        page,
        "curator_adjudicating",
        do=sequence(
            step("assign_page_curator_jurisdiction"),
            expect("page_governance.ready"),
            emit("wiki.page_curator.adjudication_started"),
        ),
    )
    machine.from_(page, "needs_migration").on(signal_edge("wiki_contract_migration_needed")).to(
        page,
        "needs_curator",
        do=sequence(
            step("write_page_migration_or_backfill_receipt"),
            expect("wiki_migrations.closed"),
            emit("wiki.page_migration.closed"),
        ),
    )
    machine.from_(corpus, "running_agents").on(event("wiki.agent_layer.closed")).to(
        corpus,
        "building_reader_surface",
        do=sequence(
            step("run_memory_wiki_build_inputs"),
            step("run_wiki_engine_render"),
            expect("reader_surface.ready"),
            emit("wiki.reader_surface.ready"),
        ),
    )
    machine.from_(corpus, "building_reader_surface").on(event("wiki.reader_surface.ready")).to(corpus, "review_ready")

    machine.from_(corpus, "review_ready").on(signal_edge("corpus.changed")).stay(
        do=emit("wiki.fabric.tick", reason="corpus changed; reconfigure active wiki roles")
    )
    machine.from_(corpus, "review_ready").on(signal_edge("roles.need_reconfiguration")).stay(
        do=emit("wiki.fabric.tick", reason="role route plan should be regenerated")
    )

    return machine
