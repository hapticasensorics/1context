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
        "for_you_day",
        version="0.1.0",
        title="For You Day Memory Loop",
        description=(
            "A scoped state machine for turning one day of raw activity into "
            "hourly talk entries, a For You day-section proposal, and concept questions. "
            "The fast path uses fixed 4-hour Opus block scribes while preserving "
            "separate hourly artifacts and retry evidence."
        ),
    )

    day = machine.scope(
        "day",
        key="date",
        states=[
            "pending",
            "discovering_hours",
            "writing_hourlies",
            "reviewing",
            "complete",
            "blocked",
        ],
        initial="pending",
        description="One calendar day inside a For You week.",
    )
    job = machine.scope(
        "job",
        key="hired_agent_uuid",
        states=["queued", "running", "done", "skipped", "failed", "needs_approval"],
        description="One concrete hired agent.",
    )
    artifact = machine.scope(
        "artifact",
        key="path",
        states=["expected", "produced", "verified", "accepted"],
        description="One expected output file or rendered artifact.",
    )

    machine.clock("timer", source="wall_time")
    machine.clock("human", source="user_message")
    machine.clock("ledger", source="append_only_events")
    machine.clock("artifact", source="filesystem_or_artifact_store")
    machine.clock("activity", source="session_activity_detection")

    machine.artifact(
        "sessions.active_hours",
        kind="ledger_event",
        schema="sessions.active_hours.v1",
        policies=["single_writer"],
        description="The discovered active hours for one day of raw session activity.",
    )
    machine.artifact(
        "hourly_talk_entry",
        kind="talk_entry_file",
        path="{talk_folder}/{hour}.conversation.md",
        schema="talk_entry.v1",
        policies=["append_only", "single_writer"],
        description="One hourly witness entry, or a skip recorded in the ledger for an empty hour.",
    )
    machine.artifact(
        "hourly_block_result",
        kind="json_manifest",
        path="{talk_folder}/{date}T{block_start}-{block_end}Z.block-result.json",
        schema="hourly_block_result.v1",
        policies=["append_only", "single_writer"],
        description=(
            "The fixed 4-hour scribe manifest. Each active hour is marked "
            "written, no-talk, or needs-retry."
        ),
    )
    machine.artifact(
        "hourly_shard_note",
        kind="talk_entry_file",
        path="{talk_folder}/.shards/{date}T{hour}-00Z.{shard_id}.synthesis.md",
        schema="hourly_shard_note.v1",
        policies=["append_only", "single_writer"],
        description=(
            "One shard witness note for an oversized hour. Shards are not final "
            "talk entries; an aggregate scribe combines them."
        ),
    )
    machine.artifact(
        "hourly_aggregate_entry",
        kind="talk_entry_file",
        path="{talk_folder}/{date}T{hour}-00Z.conversation.md",
        schema="talk_entry.v1",
        policies=["append_only", "single_writer"],
        description="The canonical hourly talk entry written from shard witness notes.",
    )
    machine.artifact(
        "daily_section_proposal",
        kind="talk_entry_file",
        path="{talk_folder}/{date}T23-59Z.proposal.editor-day-{date}.md",
        schema="daily_section_proposal.v1",
        policies=["append_only", "single_writer"],
        description="One For You editor proposal for the day's article section.",
    )
    machine.artifact(
        "concept_candidates",
        kind="talk_entry_file",
        path="{talk_folder}/{date}T23-59Z.proposal.concept-candidates.md",
        schema="concept_candidates.v1",
        policies=["append_only", "single_writer"],
        description="Concept candidates and questions that can feed future agents.",
    )
    machine.artifact(
        "rendered_talk_page",
        kind="rendered_html",
        path="{render_dir}/{talk_page}.html",
        schema="rendered_talk_page.v1",
        policies=["deterministic_render"],
        description="The deterministic render of the talk folder after entries are written.",
    )

    machine.evidence(
        "sessions.active_hours.ready",
        artifact="sessions.active_hours",
        checks=[
            "ledger_has:sessions.active_hours",
            "payload.date matches day.date",
            "payload.active_hours is a sorted list",
        ],
        description="The machine knows which hourly witness jobs should exist.",
    )
    machine.evidence(
        "hourly_talk_entries.closed",
        artifact="hourly_block_result",
        checks=[
            "each active hour has a block result status",
            "written hours have valid talk entry frontmatter",
            "written entries have ts inside the target hour",
            "no-talk is recorded as a deliberate memory decision",
            "needs-retry hours enqueue memory.hourly.scribe fallback",
            "block agents do not read sibling talk entries by default",
        ],
        description="Hourly witness work for the day is complete enough for review or retry.",
    )
    machine.evidence(
        "hourly_shards.closed",
        artifact="hourly_shard_note",
        checks=[
            "oversized single-hour prompt was split by stream and contiguous event slice",
            "each shard note has kind == synthesis",
            "no shard prompt exceeds runtime_policy.max_prompt_tokens when avoidable",
            "shards preserve evidence rather than writing final talk prose",
        ],
        description="Shard witnesses are complete for an oversized hour.",
    )
    machine.evidence(
        "hourly_aggregate_entry.verified",
        artifact="hourly_aggregate_entry",
        checks=[
            "file exists",
            "frontmatter.kind == conversation",
            "frontmatter.ts matches the oversized hour",
            "body is grounded in shard witness notes",
            "body includes uncertainty/cross-shard cautions when needed",
        ],
        description="The oversized hour has one final hourly talk entry.",
    )
    machine.evidence(
        "daily_section_proposal.verified",
        artifact="daily_section_proposal",
        checks=[
            "file exists",
            "frontmatter.kind == proposal",
            "frontmatter.target-section matches day.date",
            "body uses second-person For You register",
            "body is grounded in hourly talk entries",
        ],
    )
    machine.evidence(
        "concept_candidates.verified",
        artifact="concept_candidates",
        checks=[
            "file exists",
            "frontmatter.kind in proposal, question, concern",
            "proposal includes evidence references or an uncertainty note",
        ],
    )
    machine.evidence(
        "rendered_talk_page.valid",
        artifact="rendered_talk_page",
        checks=[
            "render command completed",
            "html exists",
            "render includes new talk entries",
        ],
    )
    machine.evidence(
        "day.review_ready",
        requires=[
            "daily_section_proposal.verified",
            "concept_candidates.verified",
            "rendered_talk_page.valid",
        ],
        description="The day can close after synthesis, concept candidates, and render all validate.",
    )

    machine.signal(
        "day.active_hours_discovered",
        expr="evidence sessions.active_hours.ready exists",
        reads=["evidence", "day"],
    )
    machine.signal(
        "day.hourlies_closed",
        expr="evidence hourly_talk_entries.closed exists",
        reads=["job", "artifact", "evidence"],
    )
    machine.signal(
        "day.review_ready",
        expr="evidence day.review_ready exists",
        reads=["artifact", "evidence"],
    )

    machine.from_(day, "pending").on(event("day.started")).to(
        day,
        "discovering_hours",
        do=sequence(
            step("sessions.discover_active_hours"),
            expect("sessions.active_hours.ready", scope="day"),
            emit("day.discovery_started"),
        ),
    )

    machine.from_(day, "discovering_hours").on(signal_edge("day.active_hours_discovered")).to(
        day,
        "writing_hourlies",
        do=sequence(
            parallel(
                spawn(
                    "memory.hourly.block_scribe",
                    for_each="fixed_4_hour_blocks",
                    key="block_start",
                    grants=[
                        "read.sessions",
                        "read.concepts",
                        "web.search",
                        "write.private_talk_entry",
                    ],
                    denies=["read.sibling_talk_entries"],
                    expects=["hourly_talk_entries.closed"],
                ),
                spawn(
                    "memory.hourly.shard_scribe",
                    for_each="oversized_hour_shards",
                    key="shard_id",
                    grants=[
                        "read.runtime_experience_packet",
                        "write.hourly_shard_note",
                    ],
                    denies=["read.sibling_shards", "read.sibling_talk_entries"],
                    expects=["hourly_shards.closed"],
                ),
                fail="collect",
                max_concurrent="runtime_policy.max_concurrent_agents",
            ),
            expect("hourly_shards.closed", scope="day", optional=True),
            spawn(
                "memory.hourly.aggregate_scribe",
                for_each="oversized_hours",
                key="hour",
                grants=[
                    "read.hourly_shard_notes",
                    "write.private_talk_entry",
                ],
                denies=["read.raw_sessions", "read.sibling_talk_entries"],
                expects=["hourly_aggregate_entry.verified"],
            ),
            wait_for("day.hourlies_closed"),
        ),
    )

    machine.from_(day, "writing_hourlies").on(signal_edge("day.hourlies_closed")).to(
        day,
        "reviewing",
        do=sequence(
            spawn(
                "memory.daily.editor",
                grants=[
                    "read.day_hourlies",
                    "read.talk_folder",
                    "read.concepts",
                    "write.private_talk_proposal",
                ],
                expects=["daily_section_proposal.verified"],
            ),
            spawn(
                "memory.concept.scout",
                grants=[
                    "read.day_hourlies",
                    "read.talk_folder",
                    "read.concepts",
                    "write.concept_questions",
                ],
                expects=["concept_candidates.verified"],
            ),
            step("talk_folder.render"),
            expect("rendered_talk_page.valid"),
            wait_for("day.review_ready"),
        ),
    )

    machine.from_(day, "reviewing").on(signal_edge("day.review_ready")).to(
        day,
        "complete",
        do=sequence(
            expect("daily_section_proposal.verified"),
            emit("day.reviewed"),
        ),
    )

    machine.from_(day, "reviewing").on(event("user.message", target="daily_editor")).stay(
        do=emit("daily_editor.human_note")
    )

    machine.from_(day, "writing_hourlies").on(tick("activity")).when("new session activity for open day").stay(
        do=emit("day.activity_observed")
    )

    machine.from_(job, "running").on(event("job.failed")).when("job scope belongs to this day").to(
        job,
        "failed",
        do=emit("day.job_failed"),
    )

    machine.from_(day, "reviewing").on(event("approval.needed")).when("scope is this day").to(
        day,
        "blocked",
        do=emit("day.blocked_on_approval"),
    )

    return machine
