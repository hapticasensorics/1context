from __future__ import annotations

import subprocess
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from onectx.agent.launch_plan import AgentLaunchPlan, build_agent_launch_plan
from onectx.config import MemorySystem
from onectx.io_utils import atomic_write_json
from onectx.memory.perception_db import DEFAULT_AGENT_SOURCES, PerceptionSnapshot, load_perception_snapshot
from onectx.memory.tick import MemoryTickResult, run_memory_tick
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
            "source_packet_path": format_path(self.source_packet_path, root),
            "perception_snapshot": self.perception_snapshot.to_payload(root=root) if self.perception_snapshot else None,
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
            "memory.hourly.shard_scribe",
            "memory.hourly.aggregate_scribe",
            "memory.hourly.answerer",
        ),
    ),
    (
        "daily_memory",
        (
            "memory.daily.editor",
            "memory.concept.scout",
        ),
    ),
    (
        "wiki_sources",
        (
            "memory.wiki.source_packet_shard",
            "memory.wiki.source_packet_aggregate",
        ),
    ),
    (
        "wiki_curators",
        (
            "memory.wiki.for_you_curator",
            "memory.wiki.context_curator",
            "memory.wiki.biographer",
            "memory.wiki.historian",
            "memory.wiki.librarian",
            "memory.wiki.librarian_sweep",
            "memory.wiki.contradiction_flagger",
            "memory.wiki.redactor",
        ),
    ),
)


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
    jobs = default_wiki_update_jobs(now=now)
    results: list[WikiUpdateJobResult] = []
    for index, job in enumerate(jobs, start=1):
        job_run_id = f"{resolved_run_id}-{index:02d}-{safe_slug(job.job_id)}"
        job_params = source_enriched_params(
            job.params,
            source_packet_path=source_packet_path,
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
        result = WikiUpdateJobResult(
            phase=job.phase,
            job_id=job.job_id,
            run_id=job_run_id,
            status="planned",
            plan=plan,
        )
        results.append(result)

    if execute_agents:
        results = execute_launch_plans(
            results,
            max_concurrent=max_concurrent or int(system.runtime_policy.get("max_concurrent_agents", 1)),
            timeout_seconds=timeout_seconds,
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
        prior_pages=prior_pages,
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
    enriched["source_packet_path"] = str(source_packet_path)
    enriched["source_packet_kind"] = "perception_db_session_history"
    enriched["source_window_days"] = str(perception_snapshot.window_days if perception_snapshot else 0)
    enriched["source_event_count"] = str(perception_snapshot.event_count if perception_snapshot else 0)
    enriched["source_session_count"] = str(perception_snapshot.session_count if perception_snapshot else 0)
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
            if "shard_scribe" in job_id or "source_packet_shard" in job_id:
                params["shard_id"] = "manual-update"
                params["shard_label"] = "Manual update"
            if "aggregate_scribe" in job_id or "source_packet_aggregate" in job_id:
                params["shard_paths"] = ""
            update_jobs.append(WikiUpdateJob(phase=phase, job_id=job_id, params=params))
    return tuple(update_jobs)


def execute_launch_plans(
    jobs: list[WikiUpdateJobResult],
    *,
    max_concurrent: int,
    timeout_seconds: int,
) -> list[WikiUpdateJobResult]:
    # The first working pass executes serially; the explicit cap is recorded for
    # the contract and leaves room for a later ThreadPoolExecutor without changing
    # the public command shape.
    if max_concurrent < 1:
        raise WikiUpdateError("max_concurrent must be >= 1")
    executed: list[WikiUpdateJobResult] = []
    for job in jobs:
        start = time.perf_counter()
        stdout_path = job.plan.run_dir / "run.stdout.jsonl"
        stderr_path = job.plan.run_dir / "run.stderr.log"
        if not job.plan.command.get("available"):
            executed.append(
                WikiUpdateJobResult(
                    phase=job.phase,
                    job_id=job.job_id,
                    run_id=job.run_id,
                    status="failed",
                    plan=job.plan,
                    stdout_path=stdout_path,
                    stderr_path=stderr_path,
                    error=f"{job.plan.harness_id} command is unavailable on this host",
                )
            )
            continue
        try:
            completed = subprocess.run(
                [str(job.plan.script_path)],
                cwd=job.plan.workspace_dir,
                text=True,
                capture_output=True,
                check=False,
                timeout=max(1, timeout_seconds),
            )
        except subprocess.TimeoutExpired as exc:
            stdout_path.write_text(coerce_output(exc.stdout), encoding="utf-8")
            stderr_path.write_text(coerce_output(exc.stderr) + f"\ntimeout after {timeout_seconds}s\n", encoding="utf-8")
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
                    error=f"timed out after {timeout_seconds}s",
                )
            )
            continue
        stdout_path.write_text(completed.stdout, encoding="utf-8")
        stderr_path.write_text(completed.stderr, encoding="utf-8")
        executed.append(
            WikiUpdateJobResult(
                phase=job.phase,
                job_id=job.job_id,
                run_id=job.run_id,
                status="completed" if completed.returncode == 0 else "failed",
                plan=job.plan,
                returncode=completed.returncode,
                duration_ms=elapsed_ms(start),
                stdout_path=stdout_path,
                stderr_path=stderr_path,
                error="" if completed.returncode == 0 else f"run script exited {completed.returncode}",
            )
        )
    return executed


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
