from __future__ import annotations

import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.day_hourlies import (
    MonthHourlyBlocksResult,
    MonthHourlyRetriesResult,
    run_month_hourly_block_scribes,
    run_month_hourly_retries,
)
from onectx.memory.jobs import CONCEPT_SCOUT_JOB_ID, DAILY_EDITOR_JOB_ID, PreparedMemoryJob, prepare_memory_job
from onectx.memory.ledger import Ledger, ledger_events_path
from onectx.memory.runner import HiredAgentBatchResult, execute_hired_agents
from onectx.memory.talk import read_talk_entries, render_talk_folder, validate_talk_entry


class ForYouRunnerError(RuntimeError):
    """Raised when the For You state-machine slice cannot be executed."""


@dataclass(frozen=True)
class DayReviewResult:
    date: str
    talk_folder: Path
    render_before: dict[str, Any]
    prepared_jobs: tuple[PreparedMemoryJob, ...]
    batch: HiredAgentBatchResult
    render_after: dict[str, Any]

    def to_payload(self) -> dict[str, Any]:
        return {
            "date": self.date,
            "talk_folder": str(self.talk_folder),
            "render_before": self.render_before,
            "prepared_count": len(self.prepared_jobs),
            "batch": self.batch.to_payload(),
            "render_after": self.render_after,
        }


@dataclass(frozen=True)
class ForYouMonthResult:
    month: str
    state_machine: dict[str, Any]
    workspace: Path
    blocks: MonthHourlyBlocksResult
    retries: MonthHourlyRetriesResult
    day_reviews: tuple[DayReviewResult, ...]
    started_at: str
    completed_at: str
    duration_ms: int

    def to_payload(self) -> dict[str, Any]:
        return {
            "month": self.month,
            "state_machine": self.state_machine,
            "workspace": str(self.workspace),
            "blocks": self.blocks.to_payload(),
            "retries": self.retries.to_payload(),
            "day_reviews": [review.to_payload() for review in self.day_reviews],
            "started_at": self.started_at,
            "completed_at": self.completed_at,
            "duration_ms": self.duration_ms,
        }


def run_for_you_month(
    system: MemorySystem,
    *,
    month: str,
    audience: str = "private",
    workspace: Path | None = None,
    run_harness: bool = False,
    model: str | None = None,
    max_concurrent: int | None = None,
    limit_blocks: int | None = None,
    limit_days: int | None = None,
    skip_existing: bool = True,
    run_day_layer: bool = True,
    split_large_blocks: bool = False,
    max_prompt_tokens: int | None = None,
    max_prompt_bytes: int | None = None,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> ForYouMonthResult:
    """Execute the proved For You state-machine slice for one month."""
    state_machine = require_state_machine(system, "for_you_day")
    workspace = workspace or Path("/tmp") / f"onecontext-for-you-month-{month}-machine"
    started_at = now_iso()
    start = time.perf_counter()
    run_id = f"for-you-month-{month}-machine"
    ledger = Ledger(ledger_events_path(system.runtime_dir), storage_path=system.storage_dir)
    ledger.append(
        "state_machine.run_started",
        ledger_schema_version="0.1",
        plugin_id=system.active_plugin,
        run_id=run_id,
        state_machine={"id": state_machine.get("id"), "version": state_machine.get("version")},
        summary=f"Started For You month run for {month}.",
        outcome="started",
    )

    block_kwargs: dict[str, Any] = {}
    if max_prompt_tokens is not None:
        block_kwargs["max_prompt_tokens"] = max_prompt_tokens
    if max_prompt_bytes is not None:
        block_kwargs["max_prompt_bytes"] = max_prompt_bytes
    blocks = run_month_hourly_block_scribes(
        system,
        month=month,
        audience=audience,
        workspace=workspace,
        run_harness=run_harness,
        model=model,
        max_concurrent=max_concurrent,
        limit_blocks=limit_blocks,
        skip_existing=skip_existing,
        split_large_blocks=split_large_blocks,
        sources=sources,
        **block_kwargs,
    )
    retries = run_month_hourly_retries(
        system,
        month=month,
        audience=audience,
        workspace=workspace,
        run_harness=run_harness,
        model=model,
        max_concurrent=max_concurrent,
        skip_existing=skip_existing,
        sources=sources,
    )
    day_reviews = (
        run_day_reviews(
            system,
            month=month,
            audience=audience,
            workspace=workspace,
            run_harness=run_harness,
            model=model,
            max_concurrent=max_concurrent,
            limit_days=limit_days,
            run_id=run_id,
        )
        if run_day_layer
        else ()
    )

    completed_at = now_iso()
    duration_ms = int((time.perf_counter() - start) * 1000)
    result = ForYouMonthResult(
        month=month,
        state_machine={"id": state_machine.get("id"), "version": state_machine.get("version")},
        workspace=workspace,
        blocks=blocks,
        retries=retries,
        day_reviews=day_reviews,
        started_at=started_at,
        completed_at=completed_at,
        duration_ms=duration_ms,
    )
    ledger.append(
        "state_machine.run_completed",
        ledger_schema_version="0.1",
        plugin_id=system.active_plugin,
        run_id=run_id,
        state_machine=result.state_machine,
        summary=f"Completed For You month run for {month}.",
        result_summary={
            "block_jobs": len(blocks.prepared_jobs),
            "retry_jobs": len(retries.prepared_jobs),
            "day_reviews": len(day_reviews),
            "duration_ms": duration_ms,
        },
        outcome="done" if blocks.batch.ok and retries.batch.ok and all(review.batch.ok for review in day_reviews) else "failure",
    )
    return result


def run_day_reviews(
    system: MemorySystem,
    *,
    month: str,
    audience: str,
    workspace: Path,
    run_harness: bool,
    model: str | None,
    max_concurrent: int | None,
    limit_days: int | None,
    run_id: str,
) -> tuple[DayReviewResult, ...]:
    reviews: list[DayReviewResult] = []
    for talk_folder in month_talk_folders(workspace, month=month, audience=audience):
        date = date_from_talk_folder(talk_folder)
        if not date:
            continue
        entries = read_talk_entries(talk_folder)
        if not any(entry.kind == "conversation" for entry in entries):
            continue
        if limit_days is not None and len(reviews) >= limit_days:
            continue
        render_before = render_talk_folder(talk_folder)
        editor_output = talk_folder / f"{date}T23-59Z.proposal.editor-day-{date}.md"
        concept_output = talk_folder / f"{date}T23-59Z.proposal.concept-candidates.md"
        prepared = (
            prepare_memory_job(
                system,
                job_id=DAILY_EDITOR_JOB_ID,
                params={
                    "date": date,
                    "audience": audience,
                    "talk_folder": str(talk_folder),
                    "output_path": str(editor_output),
                },
                workspace=workspace,
                run_harness=run_harness,
                model=model,
                run_id=run_id,
                completed_event="memory.daily_editor.state_machine_completed",
                validator=lambda path: validate_talk_entry(path, expected_kind="proposal"),
            ),
            prepare_memory_job(
                system,
                job_id=CONCEPT_SCOUT_JOB_ID,
                params={
                    "date": date,
                    "audience": audience,
                    "talk_folder": str(talk_folder),
                    "output_path": str(concept_output),
                },
                workspace=workspace,
                run_harness=run_harness,
                model=model,
                run_id=run_id,
                completed_event="memory.concept_scout.state_machine_completed",
                validator=lambda path: validate_talk_entry(path, expected_kind=("proposal", "question", "concern")),
            ),
        )
        batch = execute_hired_agents(
            system,
            [item.execution_spec for item in prepared],
            max_concurrent=max_concurrent,
            run_id=run_id,
        )
        render_after = render_talk_folder(talk_folder)
        reviews.append(
            DayReviewResult(
                date=date,
                talk_folder=talk_folder,
                render_before=render_before,
                prepared_jobs=prepared,
                batch=batch,
                render_after=render_after,
            )
        )
    return tuple(reviews)


def month_talk_folders(workspace: Path, *, month: str, audience: str) -> tuple[Path, ...]:
    return tuple(
        path
        for path in sorted(workspace.glob(f"for-you-{month}-*.{audience}.talk"))
        if path.is_dir()
    )


def date_from_talk_folder(talk_folder: Path) -> str:
    prefix = "for-you-"
    suffix = "."
    name = talk_folder.name
    if not name.startswith(prefix):
        return ""
    return name[len(prefix) :].split(suffix, 1)[0]


def require_state_machine(system: MemorySystem, machine_id: str) -> dict[str, Any]:
    machine = system.state_machines.get(machine_id)
    if not machine:
        raise ForYouRunnerError(f"missing state machine {machine_id!r}")
    return machine


def now_iso() -> str:
    from datetime import datetime, timezone

    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
