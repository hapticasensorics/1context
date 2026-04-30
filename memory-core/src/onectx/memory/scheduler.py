from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.wiki import evaluate_wiki_route_source_freshness
from onectx.storage import LakeStore, stable_id, utc_now
from onectx.storage.hour_events import format_ts, normalize_source, parse_ts


class SchedulerError(RuntimeError):
    """Raised when memory scheduler planning cannot run."""


JOBS_BY_BOUNDARY: dict[str, tuple[str, ...]] = {
    "hour": ("memory.hourly.block_scribe",),
    "day": ("memory.daily.editor",),
    "week": (
        "memory.wiki.for_you_curator",
        "memory.wiki.context_curator",
        "memory.wiki.biographer",
        "memory.wiki.librarian_sweep",
        "memory.wiki.contradiction_flagger",
    ),
    "month": ("memory.wiki.librarian_sweep", "memory.wiki.redactor"),
}


@dataclass(frozen=True)
class SchedulerFire:
    phase: str
    job: str
    t_stream: str
    prior_window_start: str
    prior_window_end: str
    status: str
    reason: str
    source_derived: bool = True

    def to_payload(self) -> dict[str, Any]:
        return {
            "phase": self.phase,
            "job": self.job,
            "t_stream": self.t_stream,
            "prior_window_start": self.prior_window_start,
            "prior_window_end": self.prior_window_end,
            "status": self.status,
            "reason": self.reason,
            "source_derived": self.source_derived,
        }


@dataclass(frozen=True)
class SchedulerPlan:
    start: str
    end: str
    sources: tuple[str, ...]
    freshness: dict[str, Any]
    fires: tuple[SchedulerFire, ...]

    @property
    def ready_count(self) -> int:
        return sum(1 for fire in self.fires if fire.status == "ready")

    @property
    def blocked_count(self) -> int:
        return sum(1 for fire in self.fires if fire.status == "blocked")

    @property
    def fire_count(self) -> int:
        return len(self.fires)

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": "memory_scheduler_plan.v1",
            "start": self.start,
            "end": self.end,
            "sources": list(self.sources),
            "freshness": self.freshness,
            "fire_count": self.fire_count,
            "ready_count": self.ready_count,
            "blocked_count": self.blocked_count,
            "fires": [fire.to_payload() for fire in self.fires],
        }


def plan_cadence_fires(start: str | datetime, end: str | datetime) -> tuple[SchedulerFire, ...]:
    start_dt = parse_scheduler_ts(start)
    end_dt = parse_scheduler_ts(end)
    if end_dt <= start_dt:
        raise SchedulerError("end must be after start")
    fires: list[SchedulerFire] = []
    cursor = floor_hour(start_dt) + timedelta(hours=1)
    while cursor <= end_dt:
        for boundary in crossed_boundaries(cursor):
            window_start = prior_window_start(cursor, boundary)
            for job in JOBS_BY_BOUNDARY[boundary]:
                fires.append(
                    SchedulerFire(
                        phase=boundary,
                        job=job,
                        t_stream=format_ts(cursor),
                        prior_window_start=format_ts(window_start),
                        prior_window_end=format_ts(cursor),
                        status="ready",
                        reason="cadence boundary crossed",
                    )
                )
        cursor += timedelta(hours=1)
    return tuple(fires)


def plan_scheduler_tick(
    store: LakeStore,
    *,
    start: str | datetime,
    end: str | datetime,
    sources: tuple[str, ...] = ("codex", "claude-code"),
    max_source_age_hours: int = 24,
    require_fresh: bool = True,
    now: str | datetime | None = None,
) -> SchedulerPlan:
    normalized_sources = tuple(normalize_source(source) for source in sources if str(source).strip())
    now_dt = parse_scheduler_ts(now) if now is not None else None
    freshness = evaluate_wiki_route_source_freshness(
        store,
        required_sources=normalized_sources,
        max_age_hours=max_source_age_hours,
        now=now_dt,
    )
    fires = plan_cadence_fires(start, end)
    if require_fresh and not freshness.get("passed"):
        fires = tuple(
            SchedulerFire(
                phase=fire.phase,
                job=fire.job,
                t_stream=fire.t_stream,
                prior_window_start=fire.prior_window_start,
                prior_window_end=fire.prior_window_end,
                status="blocked" if fire.source_derived else fire.status,
                reason="source import freshness failed" if fire.source_derived else fire.reason,
                source_derived=fire.source_derived,
            )
            for fire in fires
        )
    return SchedulerPlan(
        start=format_ts(parse_scheduler_ts(start)),
        end=format_ts(parse_scheduler_ts(end)),
        sources=normalized_sources,
        freshness=freshness,
        fires=fires,
    )


def write_scheduler_plan(system: MemorySystem, plan: SchedulerPlan, *, run_id: str = "") -> dict[str, Any]:
    resolved_run_id = run_id or stable_id("scheduler", plan.start, plan.end, utc_now())
    out_dir = system.runtime_dir / "scheduler" / resolved_run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / "plan.json"
    payload = plan.to_payload()
    payload["run_id"] = resolved_run_id
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    path.write_text(text, encoding="utf-8")
    content_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()

    store = LakeStore(system.storage_dir)
    store.ensure()
    state = "blocked" if plan.blocked_count else "ready"
    artifact = store.append_artifact(
        "memory_scheduler_plan",
        uri=path.as_uri(),
        path=str(path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=path.stat().st_size,
        source="memory.scheduler",
        state=state,
        text=f"memory scheduler plan {resolved_run_id}: {state}",
        metadata={"run_id": resolved_run_id, "fire_count": plan.fire_count, "blocked_count": plan.blocked_count},
    )
    evidence = store.append_evidence(
        "memory_scheduler.planned",
        artifact_id=artifact["artifact_id"],
        status="passed" if not plan.blocked_count else "failed",
        checker="memory.scheduler",
        text="memory scheduler cadence plan written",
        checks=["cadence fires derived", "source freshness gate evaluated"],
        payload=payload,
    )
    event = store.append_event(
        "memory.scheduler.planned",
        source="memory.scheduler",
        kind="scheduler_plan",
        subject=resolved_run_id,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence["evidence_id"],
        payload={"run_id": resolved_run_id, "state": state, "fire_count": plan.fire_count, "blocked_count": plan.blocked_count},
    )
    return {
        "run_id": resolved_run_id,
        "path": str(path),
        "artifact_id": artifact["artifact_id"],
        "evidence_id": evidence["evidence_id"],
        "event_id": event["event_id"],
        "state": state,
    }


def crossed_boundaries(value: datetime) -> tuple[str, ...]:
    boundaries = ["hour"]
    if value.hour == 0:
        boundaries.append("day")
        if value.weekday() == 0:
            boundaries.append("week")
        if value.day == 1:
            boundaries.append("month")
    return tuple(boundaries)


def prior_window_start(value: datetime, boundary: str) -> datetime:
    if boundary == "hour":
        return value - timedelta(hours=1)
    if boundary == "day":
        return value - timedelta(days=1)
    if boundary == "week":
        return value - timedelta(days=7)
    if boundary == "month":
        month = value.month - 1 or 12
        year = value.year - 1 if value.month == 1 else value.year
        return value.replace(year=year, month=month)
    return value


def floor_hour(value: datetime) -> datetime:
    return value.astimezone(timezone.utc).replace(minute=0, second=0, microsecond=0)


def parse_scheduler_ts(value: str | datetime) -> datetime:
    if isinstance(value, datetime):
        return value.astimezone(timezone.utc)
    return parse_ts(value)
