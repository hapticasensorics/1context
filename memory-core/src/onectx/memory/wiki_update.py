from __future__ import annotations

import json
import os
import shutil
import subprocess
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from onectx.agent.launch_plan import AgentLaunchPlan, build_agent_launch_plan
from onectx.config import MemorySystem
from onectx.io_utils import atomic_write_json
from onectx.memory.perception_db import DEFAULT_AGENT_SOURCES, PerceptionSnapshot, load_perception_snapshot
from onectx.memory.tick import MemoryTickResult, run_memory_tick
from onectx.memory.wiki_memory_plan import WikiMemoryPacket, WikiMemoryPlan, build_wiki_memory_plan
from onectx.memory.wiki_synthesis import WikiPriorPage, WikiSynthesisResult, load_prior_wiki_pages, synthesize_and_write_wiki
from onectx.storage import stable_id, utc_now


class WikiUpdateError(RuntimeError):
    """Raised when the app-facing wiki update orchestration cannot be planned."""


@dataclass(frozen=True)
class WikiUpdateJob:
    phase: str
    job_id: str
    params: dict[str, str]

    def to_payload(self) -> dict[str, Any]:
        return {
            "phase": self.phase,
            "job_id": self.job_id,
            "params": dict(self.params),
        }


@dataclass(frozen=True)
class WikiUpdateJobResult:
    phase: str
    job_id: str
    run_id: str
    status: str
    plan: AgentLaunchPlan
    returncode: int | None = None
    duration_ms: int = 0
    stdout_path: Path | None = None
    stderr_path: Path | None = None
    agent_report_path: Path | None = None
    mail_context_path: Path | None = None
    harness_unit_id: str = ""
    harness_call: dict[str, Any] | None = None
    harness_turn_start: dict[str, Any] | None = None
    harness_turn_complete: dict[str, Any] | None = None
    harness_adapter_events: tuple[dict[str, Any], ...] = ()
    talk_receipt: dict[str, Any] | None = None
    error: str = ""

    def to_payload(self, *, root: Path) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "phase": self.phase,
            "job_id": self.job_id,
            "run_id": self.run_id,
            "status": self.status,
            "returncode": self.returncode,
            "duration_ms": self.duration_ms,
            "plan": self.plan.to_payload(root=root),
            "stdout_path": format_path(self.stdout_path, root),
            "stderr_path": format_path(self.stderr_path, root),
            "agent_report_path": format_path(self.agent_report_path, root),
            "mail_context_path": format_path(self.mail_context_path, root),
            "harness_unit_id": self.harness_unit_id,
            "harness_call": self.harness_call,
            "harness_turn_start": self.harness_turn_start,
            "harness_turn_complete": self.harness_turn_complete,
            "harness_adapter_events": list(self.harness_adapter_events),
            "talk_receipt": self.talk_receipt,
        }
        if self.error:
            payload["error"] = self.error
        return payload


@dataclass(frozen=True)
class WikiUpdateResult:
    run_id: str
    status: str
    execute_agents: bool
    provider: str
    path: Path
    jobs: tuple[WikiUpdateJobResult, ...]
    wiki_tick: MemoryTickResult
    source_packet_path: Path | None = None
    perception_snapshot: PerceptionSnapshot | None = None
    source_imports: tuple[Any, ...] = ()
    wiki_synthesis: WikiSynthesisResult | None = None
    wiki_memory_plan: WikiMemoryPlan | None = None

    @property
    def planned_count(self) -> int:
        return len(self.jobs)

    @property
    def completed_count(self) -> int:
        return sum(1 for job in self.jobs if job.status == "completed")

    @property
    def failed_count(self) -> int:
        return sum(1 for job in self.jobs if job.status == "failed")

    def to_payload(self, *, root: Path) -> dict[str, Any]:
        return {
            "schema_version": 1,
            "operation": "memory.update_wiki",
            "status": self.status,
            "run_id": self.run_id,
            "execute_agents": self.execute_agents,
            "provider": self.provider,
            "path": format_path(self.path, root),
            "planned_count": self.planned_count,
            "completed_count": self.completed_count,
            "failed_count": self.failed_count,
            "phases": wiki_update_phase_payload(
                execute_agents=self.execute_agents,
                perception_snapshot=self.perception_snapshot,
                jobs=self.jobs,
                wiki_synthesis=self.wiki_synthesis,
                wiki_tick=self.wiki_tick,
            ),
            "source_packet_path": format_path(self.source_packet_path, root),
            "perception_snapshot": self.perception_snapshot.to_payload(root=root) if self.perception_snapshot else None,
            "wiki_memory_plan": self.wiki_memory_plan.to_payload(root=root) if self.wiki_memory_plan else None,
            "source_imports": [item.to_payload() for item in self.source_imports],
            "wiki_synthesis": self.wiki_synthesis.to_payload(root=root) if self.wiki_synthesis else None,
            "jobs": [job.to_payload(root=root) for job in self.jobs],
            "wiki_refresh": self.wiki_tick.to_payload(),
        }


DEFAULT_WIKI_UPDATE_PHASES: tuple[tuple[str, tuple[str, ...]], ...] = (
    (
        "hourly_scribes",
        (
            "memory.hourly.scribe",
            "memory.hourly.block_scribe",
            "memory.hourly.aggregate_scribe",
            "memory.hourly.answerer",
        ),
    ),
    (
        "daily_memory",
        ("memory.daily.editor",),
    ),
    (
        "wiki_specialists",
        (
            "memory.wiki.biographer",
            "memory.wiki.librarian",
        ),
    ),
    (
        "wiki_curators",
        (
            "memory.wiki.for_you_curator",
            "memory.wiki.context_curator",
        ),
    ),
)

_WIKI_TALK_APPEND_LOCK = threading.Lock()


def run_wiki_update(
    system: MemorySystem,
    *,
    provider: str = "codex",
    run_id: str = "",
    execute_agents: bool = False,
    max_concurrent: int | None = None,
    timeout_seconds: int = 1800,
    now: datetime | None = None,
    import_sources: bool = False,
    import_ticks: int = 1,
    source_window_days: int = 30,
    source_max_events: int = 5_000,
    source_max_lines: int = 250_000,
    source_query_limit: int = 2_400,
    source_cursor_name: str = "",
    memoryd_bin: str | Path | None = None,
    runtime_root: str | Path | None = None,
    wiki_core_bin: str | Path | None = None,
) -> WikiUpdateResult:
    resolved_run_id = run_id or stable_id("wiki_update", utc_now())
    update_dir = system.runtime_dir / "wiki-updates" / resolved_run_id
    update_dir.mkdir(parents=True, exist_ok=True)
    resolved_runtime_root = Path(runtime_root).expanduser().resolve() if runtime_root else None
    prior_pages = load_prior_wiki_pages(resolved_runtime_root)
    perception_snapshot = (
        load_perception_snapshot(
            system,
            memoryd_bin=memoryd_bin,
            sources=DEFAULT_AGENT_SOURCES,
            window_days=source_window_days,
            ingest_ticks=import_ticks,
            max_events=source_max_events,
            max_lines=source_max_lines,
            query_limit=source_query_limit,
            timeout_seconds=timeout_seconds,
            cursor_name=source_cursor_name,
        )
        if import_sources
        else None
    )
    source_packet_path = write_source_packet(update_dir, perception_snapshot, prior_pages=prior_pages)
    wiki_memory_plan = (
        build_wiki_memory_plan(
            run_id=resolved_run_id,
            update_dir=update_dir,
            events=perception_snapshot.events,
            cursor_name=perception_snapshot.cursor_name,
            window_days=source_window_days,
            cache_root=system.runtime_dir,
            model_policy=agent_model_policy(system, "hourly-scribe"),
        )
        if perception_snapshot is not None and perception_snapshot.events
        else None
    )
    jobs = scribe_jobs_for_plan(wiki_memory_plan, now=now) if wiki_memory_plan else default_wiki_update_jobs(now=now)
    results = materialize_wiki_update_jobs(
        system,
        jobs,
        provider=provider,
        run_id=resolved_run_id,
        start_index=1,
        source_packet_path=source_packet_path,
        perception_snapshot=perception_snapshot,
        prior_pages=prior_pages,
    )

    if execute_agents:
        effective_max_concurrent = max_concurrent or int(system.runtime_policy.get("max_concurrent_agents", 1))
        resolved_wiki_core_bin = Path(wiki_core_bin).expanduser().resolve() if wiki_core_bin else None
        if wiki_memory_plan is not None:
            results = execute_memory_wave_pipeline(
                system,
                results,
                provider=provider,
                run_id=resolved_run_id,
                wiki_memory_plan=wiki_memory_plan,
                perception_snapshot=perception_snapshot,
                prior_pages=prior_pages,
                max_concurrent=effective_max_concurrent,
                timeout_seconds=timeout_seconds,
                runtime_root=resolved_runtime_root,
                wiki_core_bin=resolved_wiki_core_bin,
            )
        else:
            results = execute_launch_plans(
                system,
                results,
                max_concurrent=effective_max_concurrent,
                timeout_seconds=timeout_seconds,
                runtime_root=resolved_runtime_root,
                wiki_core_bin=resolved_wiki_core_bin,
            )

    wiki_synthesis = synthesize_and_write_wiki(
        system,
        run_id=resolved_run_id,
        output_dir=update_dir,
        runtime_root=resolved_runtime_root,
        wiki_core_bin=Path(wiki_core_bin).expanduser().resolve() if wiki_core_bin else None,
        timeout_seconds=timeout_seconds,
        source_events=perception_snapshot.events if perception_snapshot else (),
        source_sessions=perception_snapshot.sessions if perception_snapshot else (),
        source_store="perception_db",
        source_status=perception_snapshot.status if perception_snapshot else "not_requested",
        window_days=source_window_days,
        source_cursor_name=perception_snapshot.cursor_name if perception_snapshot else source_cursor_name,
        prior_pages=prior_pages,
        agent_reports=agent_reports_for_synthesis(results),
    )

    wiki_tick = run_memory_tick(
        system,
        wiki_only=True,
        freshness_check="skip",
        execute_render=True,
        record_evidence=False,
        cycle_id=f"{resolved_run_id}-wiki-refresh",
    )
    status = (
        "failed"
        if (
            any(result.status == "failed" for result in results)
            or wiki_tick.status == "failed"
            or (wiki_synthesis is not None and wiki_synthesis.status == "failed")
            or (perception_snapshot is not None and perception_snapshot.status not in {"ok"})
        )
        else "completed"
    )
    result = WikiUpdateResult(
        run_id=resolved_run_id,
        status=status,
        execute_agents=execute_agents,
        provider=provider,
        path=update_dir / "update.json",
        jobs=tuple(results),
        wiki_tick=wiki_tick,
        source_packet_path=source_packet_path,
        perception_snapshot=perception_snapshot,
        wiki_synthesis=wiki_synthesis,
        wiki_memory_plan=wiki_memory_plan,
    )
    atomic_write_json(result.path, result.to_payload(root=system.root))
    return result


def write_source_packet(
    update_dir: Path,
    snapshot: PerceptionSnapshot | None,
    *,
    prior_pages: tuple[WikiPriorPage, ...] = (),
) -> Path:
    packet_path = update_dir / "perception-source-packet.md"
    lines = [
        "# Perception Source Packet",
        "",
        "This packet is injected into each hired agent launch plan for the wiki update.",
        "",
    ]
    if snapshot is None:
        lines.extend(
            [
                "## Status",
                "",
                "- Source import was not requested for this update.",
            ]
        )
    else:
        lines.extend(
            [
                "## Status",
                "",
                "- Store: `perception_db`",
                f"- Status: `{snapshot.status}`",
                f"- Sources: `{', '.join(snapshot.sources)}`",
                f"- Window days: `{snapshot.window_days}`",
                f"- Events: `{snapshot.event_count}`",
                f"- Sessions: `{snapshot.session_count}`",
                f"- Cursor: `{snapshot.cursor_name}`",
            ]
        )
        if snapshot.error:
            lines.append(f"- Error: `{snapshot.error}`")
        lines.extend(["", "## Recent Session Evidence", ""])
        if snapshot.events:
            for event in snapshot.events[-80:]:
                lines.append(
                    "- "
                    f"`{event.get('ts')}` "
                    f"`{event.get('source')}` "
                    f"`{event.get('kind')}` "
                    f"`{event.get('session_id')}`: "
                    f"{truncate_packet_text(event.get('text'))}"
                )
        else:
            lines.append("- No Perception DB session events were returned for this window.")
    lines.extend(["", "## Existing Wiki Snapshot", ""])
    if prior_pages:
        for page in prior_pages:
            lines.append(
                "- "
                f"`{page.page_id}` "
                f"`{page.source_path}`: "
                f"{truncate_packet_text(page.body_markdown, limit=320)}"
            )
    else:
        lines.append("- No existing wiki page bodies were found for this update.")
    packet_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    return packet_path


def source_enriched_params(
    params: dict[str, str],
    *,
    source_packet_path: Path,
    perception_snapshot: PerceptionSnapshot | None,
    prior_pages: tuple[WikiPriorPage, ...] = (),
) -> dict[str, str]:
    enriched = {key: str(value) for key, value in params.items()}
    enriched["source_store"] = "perception_db"
    enriched.setdefault("source_packet_path", str(source_packet_path))
    enriched.setdefault("source_packet_kind", "perception_db_session_history")
    enriched.setdefault("source_window_days", str(perception_snapshot.window_days if perception_snapshot else 0))
    enriched.setdefault("source_event_count", str(perception_snapshot.event_count if perception_snapshot else 0))
    enriched.setdefault("source_session_count", str(perception_snapshot.session_count if perception_snapshot else 0))
    enriched["source_status"] = perception_snapshot.status if perception_snapshot else "not_requested"
    enriched["prior_wiki_page_count"] = str(len(prior_pages))
    return enriched


def truncate_packet_text(value: Any, *, limit: int = 420) -> str:
    text = " ".join(str(value or "").split())
    if len(text) <= limit:
        return text
    return text[: limit - 1].rstrip() + "..."


def default_wiki_update_jobs(*, now: datetime | None = None) -> tuple[WikiUpdateJob, ...]:
    timestamp = (now or datetime.now(timezone.utc)).astimezone(timezone.utc)
    date = timestamp.strftime("%Y-%m-%d")
    hour = f"{timestamp.hour:02d}"
    block_start = f"{(timestamp.hour // 4) * 4:02d}"
    block_hours = ",".join(f"{value:02d}" for value in range((timestamp.hour // 4) * 4, (timestamp.hour // 4) * 4 + 4))
    base = {
        "date": date,
        "hour": hour,
        "audience": "private",
        "page_slug": f"for-you-{date}",
        "update_trigger": "manual_or_scheduled_wiki_update",
    }
    update_jobs: list[WikiUpdateJob] = []
    for phase, job_ids in DEFAULT_WIKI_UPDATE_PHASES:
        for job_id in job_ids:
            params = dict(base)
            if "block_scribe" in job_id:
                params["block_start"] = block_start
                params["hours"] = block_hours
            if "aggregate_scribe" in job_id or "source_packet_aggregate" in job_id:
                params["scribe_report_paths"] = ""
            update_jobs.append(WikiUpdateJob(phase=phase, job_id=job_id, params=params))
    return tuple(update_jobs)


def scribe_jobs_for_plan(memory_plan: WikiMemoryPlan | None, *, now: datetime | None = None) -> tuple[WikiUpdateJob, ...]:
    if memory_plan is None:
        return default_wiki_update_jobs(now=now)
    jobs: list[WikiUpdateJob] = []
    artifacts_dir = memory_plan.path.parent / "scribe-artifacts"
    for packet in memory_plan.selected_packets:
        output_path = artifacts_dir / f"{packet.date}T{packet.hour}-{packet.packet_id}.md"
        jobs.append(
            WikiUpdateJob(
                phase="scribe_wave",
                job_id="memory.hourly.scribe",
                params={
                    "date": packet.date,
                    "hour": packet.hour,
                    "audience": "private",
                    "page_slug": f"for-you-{packet.date}",
                    "talk_folder": "user-wiki://page/for-you/talk",
                    "output_path": str(output_path),
                    "packet_id": packet.packet_id,
                    "packet_label": f"{packet.date}T{packet.hour} packet {packet.shard_index}/{packet.shard_count}",
                    "source_packet_path": str(packet.source_packet_path),
                    "source_packet_kind": f"perception_db_{packet.packet_kind}",
                    "source_window_days": "1",
                    "source_event_count": str(packet.event_count),
                    "source_session_count": str(len(packet.session_ids)),
                    "source_packet_estimated_tokens": str(packet.estimated_tokens),
                    "source_packet_target_tokens": str(memory_plan.target_packet_tokens),
                    "source_packet_content_sha256": packet.content_sha256,
                    "source_packet_cache_path": str(packet.cache_path),
                    "wiki_memory_packet_id": packet.packet_id,
                    "update_trigger": "wiki_memory_scribe_wave",
                },
            )
        )
    return tuple(jobs)


def agent_model_policy(system: MemorySystem, agent_id: str) -> dict[str, Any]:
    agent = system.agents.get(agent_id, {})
    policy = agent.get("model_policy") if isinstance(agent, dict) else None
    return dict(policy) if isinstance(policy, dict) else {}


def materialize_wiki_update_jobs(
    system: MemorySystem,
    jobs: tuple[WikiUpdateJob, ...],
    *,
    provider: str,
    run_id: str,
    start_index: int,
    source_packet_path: Path,
    perception_snapshot: PerceptionSnapshot | None,
    prior_pages: tuple[WikiPriorPage, ...],
) -> list[WikiUpdateJobResult]:
    results: list[WikiUpdateJobResult] = []
    for offset, job in enumerate(jobs, start=start_index):
        job_run_id = f"{run_id}-{offset:02d}-{safe_slug(job.job_id)}"
        job_source_packet = Path(str(job.params.get("source_packet_path") or source_packet_path)).expanduser()
        job_params = source_enriched_params(
            job.params,
            source_packet_path=job_source_packet,
            perception_snapshot=perception_snapshot,
            prior_pages=prior_pages,
        )
        plan = build_agent_launch_plan(
            system,
            job_id=job.job_id,
            provider=provider,
            run_id=job_run_id,
            params=job_params,
        )
        results.append(
            WikiUpdateJobResult(
                phase=job.phase,
                job_id=job.job_id,
                run_id=job_run_id,
                status="planned",
                plan=plan,
            )
        )
    return results


def execute_memory_wave_pipeline(
    system: MemorySystem,
    scribe_results: list[WikiUpdateJobResult],
    *,
    provider: str,
    run_id: str,
    wiki_memory_plan: WikiMemoryPlan,
    perception_snapshot: PerceptionSnapshot | None,
    prior_pages: tuple[WikiPriorPage, ...],
    max_concurrent: int,
    timeout_seconds: int,
    runtime_root: Path | None,
    wiki_core_bin: Path | None,
) -> list[WikiUpdateJobResult]:
    if not scribe_results:
        return []
    executed_scribes = execute_launch_plans(
        system,
        scribe_results,
        max_concurrent=max_concurrent,
        timeout_seconds=timeout_seconds,
        runtime_root=runtime_root,
        wiki_core_bin=wiki_core_bin,
    )
    record_scribe_cache(executed_scribes)

    update_dir = wiki_memory_plan.path.parent
    all_results: list[WikiUpdateJobResult] = list(executed_scribes)
    next_index = len(all_results) + 1

    scribe_packet = write_agent_artifact_packet(update_dir, "scribe-wave-reports", executed_scribes)
    aggregate_jobs = aggregate_jobs_for_plan(wiki_memory_plan, executed_scribes, scribe_packet)
    if aggregate_jobs:
        aggregate_results = materialize_wiki_update_jobs(
            system,
            tuple(aggregate_jobs),
            provider=provider,
            run_id=run_id,
            start_index=next_index,
            source_packet_path=scribe_packet,
            perception_snapshot=perception_snapshot,
            prior_pages=prior_pages,
        )
        executed_aggregates = execute_launch_plans(
            system,
            aggregate_results,
            max_concurrent=max_concurrent,
            timeout_seconds=timeout_seconds,
            runtime_root=runtime_root,
            wiki_core_bin=wiki_core_bin,
        )
        all_results.extend(executed_aggregates)
        next_index = len(all_results) + 1

    daily_packet = write_agent_artifact_packet(update_dir, "hourly-memory-reports", all_results)
    daily_jobs = daily_jobs_for_plan(wiki_memory_plan, daily_packet, runtime_root=runtime_root)
    if daily_jobs:
        daily_results = materialize_wiki_update_jobs(
            system,
            tuple(daily_jobs),
            provider=provider,
            run_id=run_id,
            start_index=next_index,
            source_packet_path=daily_packet,
            perception_snapshot=perception_snapshot,
            prior_pages=prior_pages,
        )
        executed_daily = execute_launch_plans(
            system,
            daily_results,
            max_concurrent=max_concurrent,
            timeout_seconds=timeout_seconds,
            runtime_root=runtime_root,
            wiki_core_bin=wiki_core_bin,
        )
        all_results.extend(executed_daily)
        next_index = len(all_results) + 1

    specialist_packet = write_agent_artifact_packet(update_dir, "daily-and-scribe-reports", all_results)
    specialist_jobs = specialist_jobs_for_plan(wiki_memory_plan, specialist_packet, runtime_root=runtime_root)
    if specialist_jobs:
        specialist_results = materialize_wiki_update_jobs(
            system,
            tuple(specialist_jobs),
            provider=provider,
            run_id=run_id,
            start_index=next_index,
            source_packet_path=specialist_packet,
            perception_snapshot=perception_snapshot,
            prior_pages=prior_pages,
        )
        executed_specialists = execute_launch_plans(
            system,
            specialist_results,
            max_concurrent=max_concurrent,
            timeout_seconds=timeout_seconds,
            runtime_root=runtime_root,
            wiki_core_bin=wiki_core_bin,
        )
        all_results.extend(executed_specialists)
        next_index = len(all_results) + 1

    curator_packet = write_agent_artifact_packet(update_dir, "specialist-and-editor-reports", all_results)
    curator_jobs = curator_jobs_for_plan(wiki_memory_plan, curator_packet, runtime_root=runtime_root)
    if curator_jobs:
        curator_results = materialize_wiki_update_jobs(
            system,
            tuple(curator_jobs),
            provider=provider,
            run_id=run_id,
            start_index=next_index,
            source_packet_path=curator_packet,
            perception_snapshot=perception_snapshot,
            prior_pages=prior_pages,
        )
        executed_curators = execute_launch_plans(
            system,
            curator_results,
            max_concurrent=max_concurrent,
            timeout_seconds=timeout_seconds,
            runtime_root=runtime_root,
            wiki_core_bin=wiki_core_bin,
        )
        all_results.extend(executed_curators)
    return all_results


def aggregate_jobs_for_plan(
    memory_plan: WikiMemoryPlan,
    scribe_results: list[WikiUpdateJobResult],
    source_packet_path: Path,
) -> list[WikiUpdateJob]:
    by_hour: dict[tuple[str, str], list[WikiUpdateJobResult]] = {}
    for result in scribe_results:
        if result.status != "completed":
            continue
        date = str(result.plan.params.get("date") or "")
        hour = str(result.plan.params.get("hour") or "")
        if not date or not hour:
            continue
        by_hour.setdefault((date, hour), []).append(result)
    update_dir = memory_plan.path.parent
    jobs: list[WikiUpdateJob] = []
    for (date, hour), results in sorted(by_hour.items()):
        if len(results) <= 1:
            continue
        report_paths = "\n".join(str(result.agent_report_path or "") for result in results if result.agent_report_path)
        jobs.append(
            WikiUpdateJob(
                phase="hour_aggregate_wave",
                job_id="memory.hourly.aggregate_scribe",
                params={
                    "date": date,
                    "hour": hour,
                    "audience": "private",
                    "talk_folder": "user-wiki://page/for-you/talk",
                    "output_path": str(update_dir / "hour-aggregate-artifacts" / f"{date}T{hour}.md"),
                    "scribe_report_paths": report_paths,
                    "source_packet_path": str(source_packet_path),
                    "source_packet_kind": "multiple_scribe_reports",
                    "source_event_count": str(sum(int(result.plan.params.get("source_event_count") or 0) for result in results)),
                    "source_session_count": str(len({result.plan.params.get("session_id", "") for result in results})),
                    "update_trigger": "wiki_memory_hour_aggregate_wave",
                },
            )
        )
    return jobs


def daily_jobs_for_plan(
    memory_plan: WikiMemoryPlan,
    source_packet_path: Path,
    *,
    runtime_root: Path | None,
) -> list[WikiUpdateJob]:
    dates = sorted({packet.date for packet in memory_plan.selected_packets})
    jobs: list[WikiUpdateJob] = []
    for date in dates:
        jobs.append(
            WikiUpdateJob(
                phase="daily_editor_wave",
                job_id="memory.daily.editor",
                params={
                    "date": date,
                    "audience": "private",
                    "talk_folder": page_talk_path("for-you", runtime_root),
                    "output_path": str(memory_plan.path.parent / "daily-artifacts" / f"{date}.md"),
                    "source_packet_path": str(source_packet_path),
                    "source_packet_kind": "compact_scribe_artifacts",
                    "source_event_count": "0",
                    "source_session_count": "0",
                    "update_trigger": "wiki_memory_daily_editor_wave",
                },
            )
        )
    return jobs


def specialist_jobs_for_plan(
    memory_plan: WikiMemoryPlan,
    source_packet_path: Path,
    *,
    runtime_root: Path | None,
) -> list[WikiUpdateJob]:
    era = memory_plan.selected_packets[-1].date if memory_plan.selected_packets else datetime.now(timezone.utc).strftime("%Y-%m-%d")
    job_ids = [
        "memory.wiki.biographer",
        "memory.wiki.librarian",
    ]

    jobs: list[WikiUpdateJob] = []
    for job_id in dedupe(job_ids):
        is_librarian = job_id == "memory.wiki.librarian"
        is_biographer = job_id == "memory.wiki.biographer"
        params = {
            "date": era,
            "era": era,
            "audience": "private",
            "article_path": page_source_path("for-you", runtime_root),
            "your_context_path": page_source_path("your-context", runtime_root),
            "prior_articles": "",
            "talk_folder": page_talk_path("for-you", runtime_root),
            "concept_dir": str((runtime_root or Path("user-wiki")) / "user-wiki" / "source" / "concepts"),
            "output_path": str(memory_plan.path.parent / "specialist-artifacts" / f"{safe_slug(job_id)}.md"),
            "source_packet_path": str(source_packet_path),
            "source_packet_kind": "compact_daily_and_scribe_artifacts",
            "source_event_count": "0",
            "source_session_count": "0",
            "update_trigger": "wiki_memory_specialist_wave",
        }
        if is_librarian:
            params.update(
                {
                    "cleanup_contract": "think big-picture across the wiki; flag contradictions, stale claims, duplicate concepts, and material that no longer belongs; propose remove, merge, archive, or keep; page curators apply accepted page-body rewrites",
                    "cleanup_priority": "aggressive_generated_junk_removal",
                    "librarian_mode": "big_picture_prune_and_contradiction_review",
                }
            )
        if is_biographer:
            params.update(
                {
                    "biographer_mode": "two_week_cover_story",
                    "scribe_window_days": "14",
                    "source_packet_kind": "two_week_scribe_artifacts_and_for_you_page",
                    "for_you_article_path": page_source_path("for-you", runtime_root),
                    "biographer_contract": "read the current For You page and the last 14 days of scribe/editor artifacts; propose a holistic cover-story rewrite for the For You curator; do not persist agent memory beyond the turn",
                }
            )
        jobs.append(
            WikiUpdateJob(
                phase="specialist_wave",
                job_id=job_id,
                params=params,
            )
        )
    return jobs


def curator_jobs_for_plan(
    memory_plan: WikiMemoryPlan,
    source_packet_path: Path,
    *,
    runtime_root: Path | None,
) -> list[WikiUpdateJob]:
    era = memory_plan.selected_packets[-1].date if memory_plan.selected_packets else datetime.now(timezone.utc).strftime("%Y-%m-%d")
    jobs: list[WikiUpdateJob] = []
    for job_id in ("memory.wiki.for_you_curator", "memory.wiki.context_curator"):
        page_id = "your-context" if "context" in job_id else "for-you"
        jobs.append(
            WikiUpdateJob(
                phase="promotion_wave",
                job_id=job_id,
                params={
                    "date": era,
                    "era": era,
                    "audience": "private",
                    "article_path": page_source_path(page_id, runtime_root),
                    "your_context_path": page_source_path("your-context", runtime_root),
                    "prior_articles": "",
                    "talk_folder": page_talk_path(page_id, runtime_root),
                    "source_packet_path": str(source_packet_path),
                    "source_packet_kind": "compact_editor_biographer_librarian_artifacts",
                    "source_event_count": "0",
                    "source_session_count": "0",
                    "update_trigger": "wiki_memory_curator_promotion_wave",
                },
            )
        )
    return jobs


def write_agent_artifact_packet(update_dir: Path, name: str, results: list[WikiUpdateJobResult]) -> Path:
    packet_path = update_dir / f"{name}.md"
    lines = [
        "# Compact Agent Artifact Packet",
        "",
        "This packet contains agent receipts and reports. It is the downstream context surface; it intentionally excludes raw transcript events.",
        "",
    ]
    for result in results:
        report = ""
        if result.agent_report_path and result.agent_report_path.is_file():
            try:
                report = result.agent_report_path.read_text(encoding="utf-8").strip()
            except OSError:
                report = ""
        lines.extend(
            [
                f"## {result.job_id}",
                "",
                f"- Phase: `{result.phase}`",
                f"- Run: `{result.run_id}`",
                f"- Status: `{result.status}`",
                f"- Talk receipt: `{'yes' if result.talk_receipt else 'no'}`",
                f"- Report path: `{result.agent_report_path or ''}`",
                "",
                truncate_packet_text(report, limit=2400) if report else "_No final report was available._",
                "",
            ]
        )
    packet_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    return packet_path


def record_scribe_cache(results: list[WikiUpdateJobResult]) -> None:
    for result in results:
        if result.status != "completed" or result.agent_report_path is None or not result.agent_report_path.is_file():
            continue
        cache_path = str(result.plan.params.get("source_packet_cache_path") or "").strip()
        content_sha = str(result.plan.params.get("source_packet_content_sha256") or "").strip()
        if not cache_path or not content_sha:
            continue
        atomic_write_json(
            Path(cache_path),
            {
                "schema_version": 1,
                "operation": "memory.wiki.scribe_packet_cache",
                "status": "completed",
                "content_sha256": content_sha,
                "job_id": result.job_id,
                "run_id": result.run_id,
                "agent_report_path": str(result.agent_report_path),
                "talk_receipt": result.talk_receipt,
                "cached_at": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
            },
        )


def page_source_path(page_id: str, runtime_root: Path | None) -> str:
    if runtime_root is None:
        return f"user-wiki://page/{page_id}/source"
    matches = sorted((runtime_root / "user-wiki" / "source").glob(f"families/*/*/source/{page_id}.md"))
    return str(matches[0]) if matches else f"user-wiki://page/{page_id}/source"


def page_talk_path(page_id: str, runtime_root: Path | None) -> str:
    if runtime_root is None:
        return f"user-wiki://page/{page_id}/talk"
    matches = sorted((runtime_root / "user-wiki" / "source").glob(f"families/*/*/talk/{page_id}.talk"))
    return str(matches[0]) if matches else f"user-wiki://page/{page_id}/talk"


def dedupe(items: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for item in items:
        if item in seen:
            continue
        seen.add(item)
        result.append(item)
    return result


def env_bool(name: str, default: bool) -> bool:
    value = os.environ.get(name, "").strip().lower()
    if not value:
        return default
    return value in {"1", "true", "yes", "on"}


def execute_launch_plans(
    system: MemorySystem,
    jobs: list[WikiUpdateJobResult],
    *,
    max_concurrent: int,
    timeout_seconds: int,
    runtime_root: Path | None,
    wiki_core_bin: Path | None,
) -> list[WikiUpdateJobResult]:
    if max_concurrent < 1:
        raise WikiUpdateError("max_concurrent must be >= 1")
    if max_concurrent == 1 or len(jobs) <= 1:
        return _execute_launch_plans_serial(
            system,
            jobs,
            max_concurrent=max_concurrent,
            timeout_seconds=timeout_seconds,
            runtime_root=runtime_root,
            wiki_core_bin=wiki_core_bin,
        )

    ordered: list[WikiUpdateJobResult | None] = [None] * len(jobs)
    worker_count = min(max_concurrent, len(jobs))
    with ThreadPoolExecutor(max_workers=worker_count, thread_name_prefix="wiki-agent") as executor:
        futures = {
            executor.submit(
                _execute_launch_plans_serial,
                system,
                [job],
                max_concurrent=max_concurrent,
                timeout_seconds=timeout_seconds,
                runtime_root=runtime_root,
                wiki_core_bin=wiki_core_bin,
            ): index
            for index, job in enumerate(jobs)
        }
        for future in as_completed(futures):
            index = futures[future]
            try:
                batch = future.result()
                ordered[index] = batch[0] if batch else failed_launch_result(jobs[index], "agent execution returned no result")
            except Exception as exc:  # pragma: no cover - defensive boundary for worker crashes
                ordered[index] = failed_launch_result(jobs[index], f"agent execution crashed: {exc}")
    return [result for result in ordered if result is not None]


def _execute_launch_plans_serial(
    system: MemorySystem,
    jobs: list[WikiUpdateJobResult],
    *,
    max_concurrent: int,
    timeout_seconds: int,
    runtime_root: Path | None,
    wiki_core_bin: Path | None,
) -> list[WikiUpdateJobResult]:
    if max_concurrent < 1:
        raise WikiUpdateError("max_concurrent must be >= 1")
    harness_root = runtime_root or (system.runtime_dir / "agent-harness-root")
    harness_bin = discover_agent_harness_bin(system, wiki_core_bin=wiki_core_bin)
    executed: list[WikiUpdateJobResult] = []
    for job in jobs:
        start = time.perf_counter()
        stdout_path = job.plan.run_dir / "run.stdout.jsonl"
        stderr_path = job.plan.run_dir / "run.stderr.log"
        agent_report_path = final_message_path(job.plan)
        mail_context_path = job.plan.run_dir / "mail-context.md"
        harness_call: dict[str, Any] | None = None
        harness_turn_start: dict[str, Any] | None = None
        harness_turn_complete: dict[str, Any] | None = None
        harness_adapter_events: list[dict[str, Any]] = []
        talk_receipt: dict[str, Any] | None = None
        turn_id = f"{job.run_id}.turn-1"

        if harness_bin is None:
            executed.append(
                WikiUpdateJobResult(
                    phase=job.phase,
                    job_id=job.job_id,
                    run_id=job.run_id,
                    status="failed",
                    plan=job.plan,
                    stdout_path=stdout_path,
                    stderr_path=stderr_path,
                    agent_report_path=agent_report_path,
                    mail_context_path=mail_context_path,
                    error="onecontext-agent-harness executable was not found",
                )
            )
            continue

        try:
            harness_call = call_agent_harness(
                harness_bin,
                root=harness_root,
                command="call",
                request=agent_harness_request(job.plan),
                timeout_seconds=timeout_seconds,
            )
            harness_turn_start = call_agent_harness(
                harness_bin,
                root=harness_root,
                command="start-turn",
                request={
                    "unit_id": job.run_id,
                    "turn_id": turn_id,
                    "reason": "memory.update_wiki.agent_company",
                    "expected_transport": harness_expected_transport(job.plan),
                    "context": {
                        "source_packet_path": job.plan.params.get("source_packet_path", ""),
                        "prompt_path": str(job.plan.prompt_path),
                    },
                    "metadata": {
                        "phase": job.phase,
                        "job_id": job.job_id,
                        "run_id": job.run_id,
                        "max_concurrent": max_concurrent,
                    },
                },
                timeout_seconds=timeout_seconds,
            )
            if runtime_root is None or wiki_core_bin is None:
                raise WikiUpdateError("runtime_root and wiki_core_bin are required to inject wiki mail context")
            mail_context = inject_wiki_mail_context(
                wiki_core_bin,
                runtime_root=runtime_root,
                job=job,
                mail_context_path=mail_context_path,
                timeout_seconds=timeout_seconds,
            )
            harness_adapter_events.append(
                record_harness_adapter_event(
                    harness_bin,
                    root=harness_root,
                    unit_id=job.run_id,
                    turn_id=turn_id,
                    kind="context_injection_executed",
                    status="accepted",
                    evidence={
                        "source_packet_path": job.plan.params.get("source_packet_path", ""),
                        "prompt_path": str(job.plan.prompt_path),
                        "mail_context_path": str(mail_context_path),
                        "agent_id": str(mail_context.get("agent_id") or ""),
                        "mail_message_count": int(mail_context.get("message_count") or 0),
                    },
                    timeout_seconds=timeout_seconds,
                )
            )
        except (subprocess.SubprocessError, WikiUpdateError) as exc:
            executed.append(
                WikiUpdateJobResult(
                    phase=job.phase,
                    job_id=job.job_id,
                    run_id=job.run_id,
                    status="failed",
                    plan=job.plan,
                    duration_ms=elapsed_ms(start),
                    stdout_path=stdout_path,
                    stderr_path=stderr_path,
                    agent_report_path=agent_report_path,
                    mail_context_path=mail_context_path,
                    harness_unit_id=job.run_id,
                    harness_call=harness_call,
                    harness_turn_start=harness_turn_start,
                    harness_adapter_events=tuple(harness_adapter_events),
                    error=f"harness birth/start failed: {exc}",
                )
            )
            continue

        if not job.plan.command.get("available"):
            error = f"{job.plan.harness_id} adapter command is unavailable on this host"
            try:
                harness_adapter_events.append(
                    record_harness_adapter_event(
                        harness_bin,
                        root=harness_root,
                        unit_id=job.run_id,
                        turn_id=turn_id,
                        kind="runtime_wakeup_failed",
                        status="failed",
                        evidence={"error": error, "command_kind": job.plan.command.get("kind", "")},
                        timeout_seconds=timeout_seconds,
                    )
                )
                harness_turn_complete = complete_harness_turn(
                    harness_bin,
                    root=harness_root,
                    unit_id=job.run_id,
                    turn_id=turn_id,
                    outcome="waiting",
                    duration_ms=elapsed_ms(start),
                    error=error,
                    timeout_seconds=timeout_seconds,
                )
            except (subprocess.SubprocessError, WikiUpdateError) as exc:
                error = f"{error}; failed to complete harness turn: {exc}"
            executed.append(
                WikiUpdateJobResult(
                    phase=job.phase,
                    job_id=job.job_id,
                    run_id=job.run_id,
                    status="failed",
                    plan=job.plan,
                    stdout_path=stdout_path,
                    stderr_path=stderr_path,
                    agent_report_path=agent_report_path,
                    mail_context_path=mail_context_path,
                    harness_unit_id=job.run_id,
                    harness_call=harness_call,
                    harness_turn_start=harness_turn_start,
                    harness_turn_complete=harness_turn_complete,
                    harness_adapter_events=tuple(harness_adapter_events),
                    error=error,
                )
            )
            continue
        try:
            harness_adapter_events.append(
                record_harness_adapter_event(
                    harness_bin,
                    root=harness_root,
                    unit_id=job.run_id,
                    turn_id=turn_id,
                    kind="runtime_wakeup_attempted",
                    status="accepted",
                    evidence={"command_kind": job.plan.command.get("kind", ""), "cwd": job.plan.command.get("cwd", "")},
                    timeout_seconds=timeout_seconds,
                )
            )
            completed = run_plan_adapter(job.plan, timeout_seconds=timeout_seconds)
        except subprocess.TimeoutExpired as exc:
            stdout_path.write_text(coerce_output(exc.stdout), encoding="utf-8")
            stderr_path.write_text(coerce_output(exc.stderr) + f"\ntimeout after {timeout_seconds}s\n", encoding="utf-8")
            error = f"timed out after {timeout_seconds}s"
            try:
                harness_adapter_events.append(
                    record_harness_adapter_event(
                        harness_bin,
                        root=harness_root,
                        unit_id=job.run_id,
                        turn_id=turn_id,
                        kind="runtime_wakeup_failed",
                        status="failed",
                        evidence={"error": error, "stdout_path": str(stdout_path), "stderr_path": str(stderr_path)},
                        timeout_seconds=timeout_seconds,
                    )
                )
                harness_turn_complete = complete_harness_turn(
                    harness_bin,
                    root=harness_root,
                    unit_id=job.run_id,
                    turn_id=turn_id,
                    outcome="waiting",
                    duration_ms=elapsed_ms(start),
                    error=error,
                    timeout_seconds=timeout_seconds,
                )
            except (subprocess.SubprocessError, WikiUpdateError) as harness_exc:
                error = f"{error}; failed to complete harness turn: {harness_exc}"
            executed.append(
                WikiUpdateJobResult(
                    phase=job.phase,
                    job_id=job.job_id,
                    run_id=job.run_id,
                    status="failed",
                    plan=job.plan,
                    duration_ms=elapsed_ms(start),
                    stdout_path=stdout_path,
                    stderr_path=stderr_path,
                    agent_report_path=agent_report_path,
                    mail_context_path=mail_context_path,
                    harness_unit_id=job.run_id,
                    harness_call=harness_call,
                    harness_turn_start=harness_turn_start,
                    harness_turn_complete=harness_turn_complete,
                    harness_adapter_events=tuple(harness_adapter_events),
                    error=error,
                )
            )
            continue
        stdout_path.write_text(completed.stdout, encoding="utf-8")
        stderr_path.write_text(completed.stderr, encoding="utf-8")
        report_error = required_report_error(agent_report_path)
        adapter_succeeded = completed.returncode == 0 and not report_error
        error = ""
        if completed.returncode != 0:
            error = f"adapter exited {completed.returncode}"
        elif report_error:
            error = report_error

        try:
            harness_adapter_events.append(
                record_harness_adapter_event(
                    harness_bin,
                    root=harness_root,
                    unit_id=job.run_id,
                    turn_id=turn_id,
                    kind="runtime_wakeup_accepted" if adapter_succeeded else "runtime_wakeup_failed",
                    status="accepted" if adapter_succeeded else "failed",
                    evidence={
                        "returncode": completed.returncode,
                        "stdout_path": str(stdout_path),
                        "stderr_path": str(stderr_path),
                        "agent_report_path": str(agent_report_path) if agent_report_path else "",
                        "error": error,
                    },
                    timeout_seconds=timeout_seconds,
                )
            )
            if adapter_succeeded and runtime_root and wiki_core_bin:
                talk_receipt = append_agent_report_to_talk(
                    wiki_core_bin,
                    runtime_root=runtime_root,
                    job=job,
                    report_path=agent_report_path,
                    timeout_seconds=timeout_seconds,
                )
            elif adapter_succeeded:
                error = "runtime_root and wiki_core_bin are required to post agent report to wiki talk/mail"
                adapter_succeeded = False
            harness_turn_complete = complete_harness_turn(
                harness_bin,
                root=harness_root,
                unit_id=job.run_id,
                turn_id=turn_id,
                outcome="done" if adapter_succeeded else "waiting",
                duration_ms=elapsed_ms(start),
                error="" if adapter_succeeded else error,
                timeout_seconds=timeout_seconds,
            )
        except (subprocess.SubprocessError, WikiUpdateError) as exc:
            adapter_succeeded = False
            error = f"{error}; harness/talk completion failed: {exc}" if error else f"harness/talk completion failed: {exc}"
        executed.append(
            WikiUpdateJobResult(
                phase=job.phase,
                job_id=job.job_id,
                run_id=job.run_id,
                status="completed" if adapter_succeeded else "failed",
                plan=job.plan,
                returncode=completed.returncode,
                duration_ms=elapsed_ms(start),
                stdout_path=stdout_path,
                stderr_path=stderr_path,
                agent_report_path=agent_report_path,
                mail_context_path=mail_context_path,
                harness_unit_id=job.run_id,
                harness_call=harness_call,
                harness_turn_start=harness_turn_start,
                harness_turn_complete=harness_turn_complete,
                harness_adapter_events=tuple(harness_adapter_events),
                talk_receipt=talk_receipt,
                error=error,
            )
        )
    return executed


def failed_launch_result(job: WikiUpdateJobResult, error: str) -> WikiUpdateJobResult:
    return WikiUpdateJobResult(
        phase=job.phase,
        job_id=job.job_id,
        run_id=job.run_id,
        status="failed",
        plan=job.plan,
        stdout_path=job.plan.run_dir / "run.stdout.jsonl",
        stderr_path=job.plan.run_dir / "run.stderr.log",
        agent_report_path=final_message_path(job.plan),
        mail_context_path=job.plan.run_dir / "mail-context.md",
        harness_unit_id=job.run_id,
        error=error,
    )


def agent_reports_for_synthesis(jobs: list[WikiUpdateJobResult]) -> list[dict[str, Any]]:
    reports: list[dict[str, Any]] = []
    for job in jobs:
        if job.status != "completed" or job.agent_report_path is None or not job.agent_report_path.is_file():
            continue
        try:
            report = job.agent_report_path.read_text(encoding="utf-8").strip()
        except OSError:
            continue
        if not report:
            continue
        artifact_path = str(job.plan.params.get("output_path") or "")
        artifact_body = ""
        if artifact_path:
            try:
                artifact_file = Path(artifact_path)
                if artifact_file.is_file():
                    artifact_body = artifact_file.read_text(encoding="utf-8").strip()
            except OSError:
                artifact_body = ""
        talk_receipt = job.talk_receipt or {}
        reports.append(
            {
                "job_id": job.job_id,
                "phase": job.phase,
                "run_id": job.run_id,
                "status": job.status,
                "page_id": talk_page_for_job(job.job_id),
                "report": report,
                "artifact_path": artifact_path,
                "artifact_body": artifact_body,
                "talk_status": str(talk_receipt.get("status") or talk_receipt.get("operation") or ""),
                "talk_delivery_mode": str(talk_receipt.get("delivery_mode") or ""),
            }
        )
    return reports


def discover_agent_harness_bin(system: MemorySystem, *, wiki_core_bin: Path | None) -> Path | None:
    candidates: list[Path] = []
    override = os.environ.get("ONECONTEXT_AGENT_HARNESS_BIN", "").strip()
    if override:
        candidates.append(Path(override).expanduser())
    if wiki_core_bin is not None:
        candidates.append(wiki_core_bin.parent / "onecontext-agent-harness")
    which = shutil.which("onecontext-agent-harness")
    if which:
        candidates.append(Path(which))
    candidates.extend(
        [
            system.root / "target" / "debug" / "onecontext-agent-harness",
            system.root / "target" / "release" / "onecontext-agent-harness",
        ]
    )
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate.resolve()
    return None


def call_agent_harness(
    harness_bin: Path,
    *,
    root: Path,
    command: str,
    request: dict[str, Any],
    timeout_seconds: int,
) -> dict[str, Any]:
    root.mkdir(parents=True, exist_ok=True)
    deadline = time.monotonic() + max(5, timeout_seconds)
    attempt = 0
    while True:
        completed = subprocess.run(
            [
                str(harness_bin),
                "--root",
                str(root),
                command,
                "--request-json",
                json.dumps(request, sort_keys=True),
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=max(5, timeout_seconds),
        )
        payload = parse_json_output(completed.stdout, completed.stderr)
        if completed.returncode == 0:
            return payload
        message = harness_error_message(payload) or (completed.stderr or completed.stdout).strip()
        if not harness_store_locked(message) or time.monotonic() >= deadline:
            raise WikiUpdateError(message or f"agent harness {command} exited {completed.returncode}")
        attempt += 1
        time.sleep(min(0.5, 0.05 * attempt))


def agent_harness_request(plan: AgentLaunchPlan) -> dict[str, Any]:
    payload = plan.to_payload()
    request = payload.get("agent_harness_call", {}).get("request")
    if not isinstance(request, dict):
        raise WikiUpdateError(f"launch plan {plan.run_id} did not include an agent_harness_call request")
    return request


def harness_expected_transport(plan: AgentLaunchPlan) -> str:
    return "codex_skill" if plan.harness_id == "codex-harness" else "host_hook"


def record_harness_adapter_event(
    harness_bin: Path,
    *,
    root: Path,
    unit_id: str,
    turn_id: str,
    kind: str,
    status: str,
    evidence: dict[str, Any],
    timeout_seconds: int,
) -> dict[str, Any]:
    return call_agent_harness(
        harness_bin,
        root=root,
        command="record-adapter-event",
        request={
            "unit_id": unit_id,
            "adapter": "codex_cli",
            "kind": kind,
            "status": status,
            "correlation": {"turn_id": turn_id},
            "evidence": evidence,
        },
        timeout_seconds=timeout_seconds,
    )


def complete_harness_turn(
    harness_bin: Path,
    *,
    root: Path,
    unit_id: str,
    turn_id: str,
    outcome: str,
    duration_ms: int,
    error: str,
    timeout_seconds: int,
) -> dict[str, Any]:
    return call_agent_harness(
        harness_bin,
        root=root,
        command="complete-turn",
        request={
            "unit_id": unit_id,
            "turn_id": turn_id,
            "outcome": outcome,
            "duration_ms": max(0, duration_ms),
            "error": {"message": error} if error else {},
            "metadata": {"source": "memory.update_wiki.execute_launch_plans"},
        },
        timeout_seconds=timeout_seconds,
    )


def run_plan_adapter(plan: AgentLaunchPlan, *, timeout_seconds: int) -> subprocess.CompletedProcess[str]:
    command = plan.command
    argv = [str(item) for item in command.get("argv", [])]
    if not argv:
        raise WikiUpdateError(f"launch plan {plan.run_id} has no adapter argv")
    stdin_path = Path(str(command.get("stdin_path") or plan.prompt_path))
    env = os.environ.copy()
    env.update({str(key): str(value) for key, value in dict(command.get("env") or {}).items()})
    stdin_text = stdin_path.read_text(encoding="utf-8") if stdin_path.is_file() else ""
    return subprocess.run(
        argv,
        cwd=str(command.get("cwd") or plan.workspace_dir),
        env=env,
        input=stdin_text,
        text=True,
        capture_output=True,
        check=False,
        timeout=max(1, timeout_seconds),
    )


def final_message_path(plan: AgentLaunchPlan) -> Path | None:
    path = str(plan.command.get("final_message_path") or "").strip()
    return Path(path) if path else None


def required_report_error(path: Path | None) -> str:
    if path is None:
        return "adapter command did not declare a required final message path"
    if not path.is_file():
        return f"required agent report was not written: {path}"
    try:
        if not path.read_text(encoding="utf-8").strip():
            return f"required agent report was empty: {path}"
    except OSError as exc:
        return f"required agent report could not be read: {exc}"
    return ""


def inject_wiki_mail_context(
    wiki_core_bin: Path,
    *,
    runtime_root: Path,
    job: WikiUpdateJobResult,
    mail_context_path: Path,
    timeout_seconds: int,
) -> dict[str, Any]:
    page = talk_page_for_job(job.job_id)
    role_address = role_address_for_job(job.job_id)
    page_mailbox = f"mailbox://page/{page}"
    thread_id = mail_safe_id(job.run_id)
    identify_args = [
        "agent-identify",
        "--thread-id",
        thread_id,
        "--role",
        role_address,
        "--role",
        page_mailbox,
        "--capability",
        "wiki.mail",
        "--capability",
        "wiki.talk.append",
        "--ttl-seconds",
        str(max(3600, timeout_seconds * 2)),
    ]
    identify_payload = run_wiki_core_json(
        wiki_core_bin,
        runtime_root=runtime_root,
        args=identify_args,
        timeout_seconds=timeout_seconds,
    )
    agent = identify_payload.get("agent")
    if not isinstance(agent, dict):
        raise WikiUpdateError(f"wiki agent-identify did not return an agent record: {identify_payload}")
    agent_id = str(agent.get("agent_id") or "").strip()
    if not agent_id:
        raise WikiUpdateError(f"wiki agent-identify returned an empty agent_id: {identify_payload}")
    inbox_payload = run_wiki_core_json(
        wiki_core_bin,
        runtime_root=runtime_root,
        args=["agent-inbox", agent_id],
        timeout_seconds=timeout_seconds,
    )
    appendix = render_mail_context_appendix(
        job=job,
        page=page,
        role_address=role_address,
        page_mailbox=page_mailbox,
        identify_payload=identify_payload,
        inbox_payload=inbox_payload,
    )
    mail_context_path.write_text(appendix, encoding="utf-8")
    marker = f"<!-- wiki-mail-context:{job.run_id} -->"
    prompt = job.plan.prompt_path.read_text(encoding="utf-8") if job.plan.prompt_path.is_file() else ""
    if marker not in prompt:
        job.plan.prompt_path.write_text(prompt.rstrip() + "\n\n" + marker + "\n" + appendix, encoding="utf-8")
    return {
        "agent_id": agent_id,
        "primary_address": str(agent.get("primary_address") or ""),
        "role_address": role_address,
        "page_mailbox": page_mailbox,
        "message_count": int(inbox_payload.get("message_count") or 0),
        "mail_context_path": str(mail_context_path),
    }


def run_wiki_core_json(
    wiki_core_bin: Path,
    *,
    runtime_root: Path,
    args: list[str],
    timeout_seconds: int,
) -> dict[str, Any]:
    completed = subprocess.run(
        [str(wiki_core_bin), "--root", str(runtime_root), *args],
        text=True,
        capture_output=True,
        check=False,
        timeout=max(5, timeout_seconds),
    )
    payload = parse_json_output(completed.stdout, completed.stderr)
    if completed.returncode != 0:
        message = (completed.stderr or completed.stdout).strip()
        raise WikiUpdateError(message or f"onecontext-wiki {' '.join(args[:1])} exited {completed.returncode}")
    return payload


def render_mail_context_appendix(
    *,
    job: WikiUpdateJobResult,
    page: str,
    role_address: str,
    page_mailbox: str,
    identify_payload: dict[str, Any],
    inbox_payload: dict[str, Any],
) -> str:
    deliveries = inbox_payload.get("deliveries")
    if not isinstance(deliveries, list):
        deliveries = []
    visible_deliveries = deliveries[:20]
    lines = [
        "## Wiki Mail Context",
        "",
        "This appendix was injected by `memory.update_wiki` after harness turn start and before the Codex adapter ran.",
        "",
        f"- Run: `{job.run_id}`",
        f"- Job: `{job.job_id}`",
        f"- Page mailbox: `{page_mailbox}`",
        f"- Role mailbox: `{role_address}`",
        f"- Talk page: `{page}`",
        "",
        "### Agent Identity",
        "",
        fenced_json(identify_payload),
        "",
        "### Agent Inbox",
        "",
        f"- Message count: `{inbox_payload.get('message_count', 0)}`",
        "",
    ]
    if visible_deliveries:
        lines.append("### Inbox Deliveries")
        lines.append("")
        for item in visible_deliveries:
            if isinstance(item, dict):
                lines.append(inbox_delivery_header(item))
            else:
                lines.append(f"- {item}")
        lines.extend(
            [
                "",
                "Only inbox headers are injected by default. Request or open full mail bodies only when the job truly needs them.",
                "",
            ]
        )
    else:
        lines.extend(["No open wiki mail deliveries were returned for this agent identity.", ""])
    return "\n".join(lines).rstrip() + "\n"


def inbox_delivery_header(item: dict[str, Any]) -> str:
    delivery_id = str(item.get("delivery_id") or item.get("id") or "")
    state = str(item.get("state") or item.get("status") or "")
    subject = str(item.get("subject") or "")
    sender = str(item.get("from") or item.get("from_address") or item.get("sender") or "")
    kind = str(item.get("kind") or "")
    parts = [f"`{delivery_id}`", f"`{state}`"]
    if kind:
        parts.append(f"`{kind}`")
    if sender:
        parts.append(f"from `{sender}`")
    if subject:
        parts.append(subject)
    return "- " + " ".join(parts)


def append_agent_report_to_talk(
    wiki_core_bin: Path,
    *,
    runtime_root: Path,
    job: WikiUpdateJobResult,
    report_path: Path | None,
    timeout_seconds: int,
) -> dict[str, Any]:
    if report_path is None or not report_path.is_file():
        raise WikiUpdateError("cannot append agent report to talk without a report file")
    page = talk_page_for_job(job.job_id)
    body_path = job.plan.run_dir / "talk-report.md"
    report = report_path.read_text(encoding="utf-8").strip()
    body_path.write_text(
        "\n".join(
            [
                f"Agent report from `{job.job_id}`.",
                "",
                f"- Run: `{job.run_id}`",
                f"- Phase: `{job.phase}`",
                f"- Harness unit: `{job.run_id}`",
                f"- Source packet: `{job.plan.params.get('source_packet_path', '')}`",
                "",
                "## Report",
                "",
                report,
                "",
            ]
        ),
        encoding="utf-8",
    )
    with _WIKI_TALK_APPEND_LOCK:
        subprocess.run(
            [str(wiki_core_bin), "--root", str(runtime_root), "page-create-all"],
            text=True,
            capture_output=True,
            check=False,
            timeout=max(5, timeout_seconds),
        )
        completed = subprocess.run(
            [
                str(wiki_core_bin),
                "--root",
                str(runtime_root),
                "talk-append",
                "--page",
                page,
                "--kind",
                "proposal",
                "--subject",
                f"Agent report: {job.job_id}",
                "--thread-id",
                mail_safe_id(job.run_id),
                "--operation-id",
                stable_id("agent_report", job.run_id, job.job_id),
                "--delivery-mode",
                "mail",
                "--from",
                agent_from_address(job.run_id),
                "--to",
                talk_recipient_for_job(job.job_id),
                "--cc",
                f"mailbox://page/{page}",
                "--body-file",
                str(body_path),
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=max(5, timeout_seconds),
        )
    payload = parse_json_output(completed.stdout, completed.stderr)
    if completed.returncode != 0:
        message = (completed.stderr or completed.stdout).strip()
        raise WikiUpdateError(message or f"wiki talk-append exited {completed.returncode}")
    if str(payload.get("status") or "") not in {"appended", "ok"}:
        raise WikiUpdateError(f"wiki talk-append did not append report: {payload}")
    return payload


def talk_page_for_job(job_id: str) -> str:
    if "context" in job_id:
        return "your-context"
    if "librarian" in job_id or "source_packet" in job_id:
        return "topics"
    return "for-you"


def talk_recipient_for_job(job_id: str) -> str:
    if job_id.startswith("memory.hourly") or "source_packet" in job_id:
        return "role://memory.daily.editor"
    if "daily" in job_id or "librarian" in job_id:
        return "role://memory.wiki.for_you_curator"
    return "role://memory.wiki.curator"


def role_address_for_job(job_id: str) -> str:
    if "." in job_id:
        scope, role = job_id.rsplit(".", 1)
        return f"role://{mail_safe_id(scope)}.{mail_safe_id(role)}"
    return f"role://memory.{mail_safe_id(job_id)}"


def agent_from_address(run_id: str) -> str:
    return f"agent://codex/{mail_safe_id(run_id)}"


def mail_safe_id(value: str) -> str:
    safe = "".join(character if character.isascii() and (character.isalnum() or character in "-_.") else "-" for character in value)
    safe = safe.strip("-_.")
    return safe or "agent-run"


def fenced_json(payload: Any) -> str:
    return "```json\n" + json.dumps(payload, indent=2, sort_keys=True) + "\n```"


def parse_json_output(stdout: str, stderr: str = "") -> dict[str, Any]:
    text = stdout.strip() or stderr.strip()
    if not text:
        return {}
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        return {"raw": text}
    return parsed if isinstance(parsed, dict) else {"value": parsed}


def harness_error_message(payload: dict[str, Any]) -> str:
    error = payload.get("error")
    if isinstance(error, dict):
        message = str(error.get("message") or "").strip()
        if message:
            return message
    return str(payload.get("message") or "").strip()


def harness_store_locked(message: str) -> bool:
    lowered = message.lower()
    return "agent_harness_store_locked" in lowered or "agent harness store is locked" in lowered


def wiki_update_phase_payload(
    *,
    execute_agents: bool,
    perception_snapshot: PerceptionSnapshot | None,
    jobs: tuple[WikiUpdateJobResult, ...],
    wiki_synthesis: WikiSynthesisResult | None,
    wiki_tick: MemoryTickResult,
) -> list[dict[str, Any]]:
    born = sum(1 for job in jobs if job.harness_call)
    reports = sum(1 for job in jobs if job.agent_report_path and job.agent_report_path.is_file())
    talks = sum(1 for job in jobs if job.talk_receipt)
    failed = sum(1 for job in jobs if job.status == "failed")
    return [
        {
            "id": "ingest_perception",
            "status": perception_snapshot.status if perception_snapshot else "not_requested",
            "event_count": perception_snapshot.event_count if perception_snapshot else 0,
            "session_count": perception_snapshot.session_count if perception_snapshot else 0,
        },
        {
            "id": "plan_agent_company",
            "status": "planned",
            "job_count": len(jobs),
        },
        {
            "id": "birth_agent_units",
            "status": "skipped" if not execute_agents else ("failed" if failed and born < len(jobs) else "completed"),
            "born_count": born,
            "job_count": len(jobs),
        },
        {
            "id": "run_agent_turns",
            "status": "skipped" if not execute_agents else ("failed" if failed else "completed"),
            "completed_count": sum(1 for job in jobs if job.status == "completed"),
            "failed_count": failed,
            "report_count": reports,
            "talk_receipt_count": talks,
        },
        {
            "id": "write_or_patch_wiki",
            "status": wiki_synthesis.status if wiki_synthesis else "not_started",
            "source_event_count": wiki_synthesis.source_event_count if wiki_synthesis else 0,
        },
        {
            "id": "publish",
            "status": wiki_tick.status,
            "render_count": wiki_tick.render_count,
        },
    ]


def safe_slug(value: str) -> str:
    return "".join(character if character.isalnum() else "-" for character in value).strip("-")


def elapsed_ms(start: float) -> int:
    return int((time.perf_counter() - start) * 1000)


def coerce_output(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return str(value)


def format_path(path: Path | None, root: Path) -> str | None:
    if path is None:
        return None
    try:
        return str(path.resolve().relative_to(root.resolve()))
    except ValueError:
        return str(path)
