from __future__ import annotations

import json
import time
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.hour_experience import DEFAULT_EXPERIENCE_MODE, group_streams, source_events_sha256
from onectx.memory.jobs import (
    HOURLY_AGGREGATE_SCRIBE_JOB_ID,
    HOURLY_BLOCK_SCRIBE_JOB_ID,
    HOURLY_SCRIBE_JOB_ID,
    HOURLY_SHARD_SCRIBE_JOB_ID,
    PreparedMemoryJob,
    prepare_memory_job,
)
from onectx.memory.prompt_stack import estimate_token_count
from onectx.memory.runner import HiredAgentBatchResult, execute_hired_agents
from onectx.memory.talk import validate_hourly_block_result, validate_talk_entry
from onectx.storage import LakeStore, stable_id
from onectx.storage.hour_events import events_between, format_ts, parse_ts


class DayHourliesError(RuntimeError):
    """Raised when day hourly fanout cannot be prepared or executed."""


DEFAULT_PROMPT_WARNING_TOKENS = 128_000
ROUTE_SAFETY_MARGIN = 0.90
BLOCK_PROMPT_OVERHEAD_TOKENS = 3_000
HOURLY_PROMPT_OVERHEAD_TOKENS = 2_200
SHARD_PROMPT_OVERHEAD_TOKENS = 2_000
AGGREGATE_PROMPT_OVERHEAD_TOKENS = 2_500
EVENT_RENDER_OVERHEAD_TOKENS = 80
STREAM_RENDER_OVERHEAD_TOKENS = 250
HOUR_RENDER_OVERHEAD_TOKENS = 1_500
ESTIMATED_SHARD_NOTE_TOKENS = 1_200


@dataclass(frozen=True)
class ActiveHour:
    hour: str
    event_count: int
    stream_count: int

    def to_payload(self) -> dict[str, Any]:
        return {
            "hour": self.hour,
            "event_count": self.event_count,
            "stream_count": self.stream_count,
        }


@dataclass(frozen=True)
class DayHourliesResult:
    date: str
    active_hours: tuple[ActiveHour, ...]
    prepared_jobs: tuple[PreparedMemoryJob, ...]
    batch: HiredAgentBatchResult
    skipped_existing: tuple[str, ...] = ()

    def to_payload(self) -> dict[str, Any]:
        return {
            "date": self.date,
            "active_hours": [hour.to_payload() for hour in self.active_hours],
            "prepared_count": len(self.prepared_jobs),
            "skipped_existing_count": len(self.skipped_existing),
            "skipped_existing": list(self.skipped_existing),
            "batch": self.batch.to_payload(),
        }


@dataclass(frozen=True)
class ActiveDay:
    date: str
    active_hours: tuple[ActiveHour, ...]

    @property
    def event_count(self) -> int:
        return sum(hour.event_count for hour in self.active_hours)

    @property
    def hour_count(self) -> int:
        return len(self.active_hours)

    def to_payload(self) -> dict[str, Any]:
        return {
            "date": self.date,
            "hour_count": self.hour_count,
            "event_count": self.event_count,
            "active_hours": [hour.to_payload() for hour in self.active_hours],
        }


@dataclass(frozen=True)
class MonthHourliesResult:
    month: str
    active_days: tuple[ActiveDay, ...]
    prepared_jobs: tuple[PreparedMemoryJob, ...]
    batch: HiredAgentBatchResult
    skipped_existing: tuple[str, ...] = ()

    @property
    def active_hour_count(self) -> int:
        return sum(day.hour_count for day in self.active_days)

    @property
    def event_count(self) -> int:
        return sum(day.event_count for day in self.active_days)

    def to_payload(self) -> dict[str, Any]:
        return {
            "month": self.month,
            "active_day_count": len(self.active_days),
            "active_hour_count": self.active_hour_count,
            "event_count": self.event_count,
            "active_days": [day.to_payload() for day in self.active_days],
            "prepared_count": len(self.prepared_jobs),
            "skipped_existing_count": len(self.skipped_existing),
            "skipped_existing": list(self.skipped_existing),
            "batch": self.batch.to_payload(),
        }


@dataclass(frozen=True)
class ActiveBlock:
    date: str
    block_start: str
    block_end: str
    active_hours: tuple[ActiveHour, ...]

    @property
    def event_count(self) -> int:
        return sum(hour.event_count for hour in self.active_hours)

    @property
    def hour_count(self) -> int:
        return len(self.active_hours)

    @property
    def label(self) -> str:
        return f"{self.date}T{self.block_start}-{self.block_end}Z"

    def to_payload(self) -> dict[str, Any]:
        return {
            "date": self.date,
            "block_start": self.block_start,
            "block_end": self.block_end,
            "label": self.label,
            "hour_count": self.hour_count,
            "event_count": self.event_count,
            "active_hours": [hour.to_payload() for hour in self.active_hours],
        }


@dataclass(frozen=True)
class RouteShard:
    shard_id: str
    shard_label: str
    events: tuple[Any, ...]
    estimated_tokens: int

    @property
    def event_count(self) -> int:
        return len(self.events)

    def to_payload(self) -> dict[str, Any]:
        return {
            "shard_id": self.shard_id,
            "shard_label": self.shard_label,
            "event_count": self.event_count,
            "estimated_tokens": self.estimated_tokens,
        }


@dataclass(frozen=True)
class RouteHour:
    date: str
    hour: str
    active_hour: ActiveHour
    route: str
    events: tuple[Any, ...]
    estimated_tokens: int
    shards: tuple[RouteShard, ...] = ()

    @property
    def label(self) -> str:
        return f"{self.date}T{self.hour}:00Z"

    def to_payload(self) -> dict[str, Any]:
        return {
            "date": self.date,
            "hour": self.hour,
            "label": self.label,
            "route": self.route,
            "event_count": self.active_hour.event_count,
            "stream_count": self.active_hour.stream_count,
            "estimated_tokens": self.estimated_tokens,
            "shard_count": len(self.shards),
            "shards": [shard.to_payload() for shard in self.shards],
        }


@dataclass(frozen=True)
class RouteBlock:
    block: ActiveBlock
    route: str
    estimated_tokens: int
    hours: tuple[RouteHour, ...]

    def to_payload(self) -> dict[str, Any]:
        return {
            **self.block.to_payload(),
            "route": self.route,
            "estimated_tokens": self.estimated_tokens,
            "hours": [hour.to_payload() for hour in self.hours],
        }


@dataclass(frozen=True)
class MonthHourlyRoutePlan:
    month: str
    active_days: tuple[ActiveDay, ...]
    routes: tuple[RouteBlock, ...]
    skipped_existing: tuple[str, ...]
    max_prompt_tokens: int
    split_large_blocks_enabled: bool
    source_event_sha256: str
    artifact_id: str
    artifact_path: Path
    cache_hit: bool
    duration_ms: int

    @property
    def active_hour_count(self) -> int:
        return sum(day.hour_count for day in self.active_days)

    @property
    def event_count(self) -> int:
        return sum(day.event_count for day in self.active_days)

    @property
    def prepared_hour_count(self) -> int:
        return len({(route.block.date, hour.hour) for route in self.routes for hour in route.hours})

    @property
    def planned_hire_count(self) -> int:
        count = 0
        for route in self.routes:
            if route.route == "block":
                count += 1
                continue
            for hour in route.hours:
                if hour.route == "hourly":
                    count += 1
                elif hour.route == "sharded":
                    count += len(hour.shards) + 1
        return count

    @property
    def split_large_block_labels(self) -> tuple[str, ...]:
        return tuple(route.block.label for route in self.routes if route.route == "split_hours")

    @property
    def sharded_hours(self) -> tuple[str, ...]:
        return tuple(hour.label for route in self.routes for hour in route.hours if hour.route == "sharded")

    @property
    def route_counts(self) -> dict[str, int]:
        counts = {
            HOURLY_BLOCK_SCRIBE_JOB_ID: 0,
            HOURLY_SCRIBE_JOB_ID: 0,
            HOURLY_SHARD_SCRIBE_JOB_ID: 0,
            HOURLY_AGGREGATE_SCRIBE_JOB_ID: 0,
        }
        for route in self.routes:
            if route.route == "block":
                counts[HOURLY_BLOCK_SCRIBE_JOB_ID] += 1
                continue
            for hour in route.hours:
                if hour.route == "hourly":
                    counts[HOURLY_SCRIBE_JOB_ID] += 1
                elif hour.route == "sharded":
                    counts[HOURLY_SHARD_SCRIBE_JOB_ID] += len(hour.shards)
                    counts[HOURLY_AGGREGATE_SCRIBE_JOB_ID] += 1
        return counts

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": "memory_route_plan",
            "month": self.month,
            "active_day_count": len(self.active_days),
            "active_hour_count": self.active_hour_count,
            "prepared_hour_count": self.prepared_hour_count,
            "event_count": self.event_count,
            "planned_hire_count": self.planned_hire_count,
            "route_counts": self.route_counts,
            "split_large_block_count": len(self.split_large_block_labels),
            "split_large_blocks": list(self.split_large_block_labels),
            "sharded_hour_count": len(self.sharded_hours),
            "sharded_hours": list(self.sharded_hours),
            "skipped_existing_count": len(self.skipped_existing),
            "skipped_existing": list(self.skipped_existing),
            "max_prompt_tokens": self.max_prompt_tokens,
            "split_large_blocks_enabled": self.split_large_blocks_enabled,
            "source_event_sha256": self.source_event_sha256,
            "artifact_id": self.artifact_id,
            "artifact_path": str(self.artifact_path),
            "cache": {"hit": self.cache_hit},
            "duration_ms": self.duration_ms,
            "active_days": [day.to_payload() for day in self.active_days],
            "routes": [route.to_payload() for route in self.routes],
        }


@dataclass(frozen=True)
class MonthHourlyBlocksResult:
    month: str
    active_days: tuple[ActiveDay, ...]
    active_blocks: tuple[ActiveBlock, ...]
    prepared_jobs: tuple[PreparedMemoryJob, ...]
    batch: HiredAgentBatchResult
    skipped_existing: tuple[str, ...] = ()
    split_large_blocks: tuple[str, ...] = ()
    sharded_hours: tuple[str, ...] = ()
    prompt_warning_tokens: int = DEFAULT_PROMPT_WARNING_TOKENS
    prompt_warning_bytes: int | None = None

    @property
    def active_hour_count(self) -> int:
        return sum(day.hour_count for day in self.active_days)

    @property
    def prepared_hour_count(self) -> int:
        return len(
            {
                (block.date, hour.hour)
                for block in self.active_blocks
                for hour in block.active_hours
            }
        )

    @property
    def event_count(self) -> int:
        return sum(day.event_count for day in self.active_days)

    @property
    def prompt_footprints(self) -> tuple[dict[str, Any], ...]:
        footprints = []
        for index, job in enumerate(self.prepared_jobs):
            block = self.active_blocks[index] if index < len(self.active_blocks) else None
            prompt_bytes = job.prompt_stack.bytes
            estimated_tokens = job.prompt_stack.estimated_tokens
            large_by_tokens = estimated_tokens > self.prompt_warning_tokens
            large_by_bytes = self.prompt_warning_bytes is not None and prompt_bytes > self.prompt_warning_bytes
            footprints.append(
                {
                    "index": index,
                    "job_id": job.job_id,
                    "label": block.label if block else "",
                    "date": block.date if block else job.job_params.get("date"),
                    "hours": [hour.hour for hour in block.active_hours] if block else str(job.job_params.get("hours", "")).split(","),
                    "event_count": block.event_count if block else None,
                    "prompt_bytes": prompt_bytes,
                    "estimated_tokens": estimated_tokens,
                    "large_prompt": large_by_tokens or large_by_bytes,
                    "large_by_tokens": large_by_tokens,
                    "large_by_bytes": large_by_bytes,
                    "threshold_tokens": self.prompt_warning_tokens,
                    "threshold_bytes": self.prompt_warning_bytes,
                }
            )
        return tuple(footprints)

    @property
    def large_prompt_count(self) -> int:
        return len([item for item in self.prompt_footprints if item["large_prompt"]])

    @property
    def oversized_single_hour_count(self) -> int:
        return len(
            [
                item
                for item in self.prompt_footprints
                if item["large_prompt"] and len([hour for hour in item["hours"] if hour]) == 1
            ]
        )

    def to_payload(self) -> dict[str, Any]:
        return {
            "month": self.month,
            "active_day_count": len(self.active_days),
            "active_hour_count": self.active_hour_count,
            "prepared_hour_count": self.prepared_hour_count,
            "event_count": self.event_count,
            "active_blocks": [block.to_payload() for block in self.active_blocks],
            "prepared_count": len(self.prepared_jobs),
            "skipped_existing_count": len(self.skipped_existing),
            "skipped_existing": list(self.skipped_existing),
            "split_large_block_count": len(self.split_large_blocks),
            "split_large_blocks": list(self.split_large_blocks),
            "sharded_hour_count": len(self.sharded_hours),
            "sharded_hours": list(self.sharded_hours),
            "prompt_warning_tokens": self.prompt_warning_tokens,
            "prompt_warning_bytes": self.prompt_warning_bytes,
            "large_prompt_count": self.large_prompt_count,
            "oversized_single_hour_count": self.oversized_single_hour_count,
            "prompt_footprints": list(self.prompt_footprints),
            "batch": self.batch.to_payload(),
        }


@dataclass(frozen=True)
class RetryHour:
    date: str
    hour: str
    reason: str
    manifest_path: Path

    def to_payload(self) -> dict[str, Any]:
        return {
            "date": self.date,
            "hour": self.hour,
            "reason": self.reason,
            "manifest_path": str(self.manifest_path),
        }


@dataclass(frozen=True)
class MonthHourlyRetriesResult:
    month: str
    retry_hours: tuple[RetryHour, ...]
    prepared_jobs: tuple[PreparedMemoryJob, ...]
    batch: HiredAgentBatchResult
    skipped_existing: tuple[str, ...] = ()

    def to_payload(self) -> dict[str, Any]:
        return {
            "month": self.month,
            "retry_hour_count": len(self.retry_hours),
            "retry_hours": [hour.to_payload() for hour in self.retry_hours],
            "prepared_count": len(self.prepared_jobs),
            "skipped_existing_count": len(self.skipped_existing),
            "skipped_existing": list(self.skipped_existing),
            "batch": self.batch.to_payload(),
        }


def hour_event_buckets(active_days: tuple[ActiveDay, ...]) -> dict[str, int]:
    buckets = {
        "tiny_1_to_10": 0,
        "small_11_to_50": 0,
        "medium_51_to_500": 0,
        "large_501_to_5000": 0,
        "huge_over_5000": 0,
    }
    for day in active_days:
        for hour in day.active_hours:
            if hour.event_count <= 10:
                buckets["tiny_1_to_10"] += 1
            elif hour.event_count <= 50:
                buckets["small_11_to_50"] += 1
            elif hour.event_count <= 500:
                buckets["medium_51_to_500"] += 1
            elif hour.event_count <= 5000:
                buckets["large_501_to_5000"] += 1
            else:
                buckets["huge_over_5000"] += 1
    return buckets


def plan_month_hourly_routes(
    system: MemorySystem,
    *,
    month: str,
    audience: str = "private",
    workspace: Path | None = None,
    limit_blocks: int | None = None,
    skip_existing: bool = True,
    split_large_blocks: bool = False,
    max_prompt_tokens: int = DEFAULT_PROMPT_WARNING_TOKENS,
    experience_mode: str | None = None,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> MonthHourlyRoutePlan:
    """Cheaply choose block/hour/shard routes without rendering prompt stacks."""
    started = time.perf_counter()
    workspace = workspace or Path("/tmp") / f"onecontext-for-you-month-{month}-blocks"
    experience_mode = experience_mode or DEFAULT_EXPERIENCE_MODE
    start, end = month_bounds(month)
    grouped = collect_events_by_day_hour(system, start=start, end=end, sources=sources)
    active_days = active_days_from_group(grouped)
    planned_blocks = fixed_four_hour_blocks(active_days)
    source_hash = source_events_sha256(
        [
            event
            for day in sorted(grouped)
            for hour in sorted(grouped[day])
            for event in grouped[day][hour]
        ]
    )
    route_budget = int(max_prompt_tokens * ROUTE_SAFETY_MARGIN)
    routes: list[RouteBlock] = []
    skipped_existing: list[str] = []
    selected_block_count = 0
    for planned_block in planned_blocks:
        block_hours = []
        for active_hour in planned_block.active_hours:
            output_path = hourly_output_path(workspace, date=planned_block.date, hour=active_hour.hour, audience=audience)
            if skip_existing and valid_existing_hourly(output_path, date=planned_block.date, hour=active_hour.hour):
                skipped_existing.append(str(output_path))
                continue
            block_hours.append(active_hour)
        if not block_hours:
            continue
        if limit_blocks is not None and selected_block_count >= limit_blocks:
            continue
        selected_block_count += 1
        active_block = ActiveBlock(
            date=planned_block.date,
            block_start=planned_block.block_start,
            block_end=planned_block.block_end,
            active_hours=tuple(block_hours),
        )
        hour_routes = tuple(
            plan_hour_route(
                date=planned_block.date,
                active_hour=active_hour,
                events=grouped[planned_block.date][active_hour.hour],
                experience_mode=experience_mode,
                route_budget=route_budget,
            )
            for active_hour in block_hours
        )
        block_estimate = BLOCK_PROMPT_OVERHEAD_TOKENS + sum(
            estimate_context_tokens(list(hour.events), experience_mode=experience_mode)
            for hour in hour_routes
        )
        if split_large_blocks and len(block_hours) > 1 and block_estimate > route_budget:
            routes.append(
                RouteBlock(
                    block=active_block,
                    route="split_hours",
                    estimated_tokens=block_estimate,
                    hours=hour_routes,
                )
            )
        else:
            routes.append(
                RouteBlock(
                    block=active_block,
                    route="block",
                    estimated_tokens=block_estimate,
                    hours=hour_routes,
                )
            )
    artifact_id = stable_id(
        "artifact",
        "memory_route_plan",
        month,
        audience,
        tuple(sources),
        experience_mode,
        max_prompt_tokens,
        split_large_blocks,
        source_hash,
    )
    artifact_path = system.runtime_dir / "route-plans" / f"{artifact_id}.json"
    store = LakeStore(system.storage_dir)
    store.ensure()
    cache_hit = bool(store.row_by_value("artifacts", "artifact_id", artifact_id))
    plan = MonthHourlyRoutePlan(
        month=month,
        active_days=active_days,
        routes=tuple(routes),
        skipped_existing=tuple(skipped_existing),
        max_prompt_tokens=max_prompt_tokens,
        split_large_blocks_enabled=split_large_blocks,
        source_event_sha256=source_hash,
        artifact_id=artifact_id,
        artifact_path=artifact_path,
        cache_hit=cache_hit,
        duration_ms=int((time.perf_counter() - started) * 1000),
    )
    write_route_plan_artifact(system, plan)
    return plan


def plan_hour_route(
    *,
    date: str,
    active_hour: ActiveHour,
    events: list[Any],
    experience_mode: str,
    route_budget: int,
) -> RouteHour:
    hour = active_hour.hour
    hour_estimate = HOURLY_PROMPT_OVERHEAD_TOKENS + estimate_context_tokens(events, experience_mode=experience_mode)
    if hour_estimate <= route_budget:
        return RouteHour(
            date=date,
            hour=hour,
            active_hour=active_hour,
            route="hourly",
            events=tuple(events),
            estimated_tokens=hour_estimate,
        )
    shards = plan_hour_shards(
        date=date,
        hour=hour,
        events=events,
        experience_mode=experience_mode,
        route_budget=route_budget,
    )
    return RouteHour(
        date=date,
        hour=hour,
        active_hour=active_hour,
        route="sharded",
        events=tuple(events),
        estimated_tokens=hour_estimate,
        shards=shards,
    )


def plan_hour_shards(
    *,
    date: str,
    hour: str,
    events: list[Any],
    experience_mode: str,
    route_budget: int,
) -> tuple[RouteShard, ...]:
    stream_groups = group_streams(events)
    seeds = (
        [(slug_text(stream_id), stream_id, list(stream_events)) for stream_id, stream_events in sorted(stream_groups.items())]
        if stream_groups
        else [("empty", "empty hour", list(events))]
    )
    shards: list[RouteShard] = []
    for seed, label, stream_events in seeds:
        shards.extend(
            plan_shard_splits(
                date=date,
                hour=hour,
                shard_seed=seed,
                shard_label=label,
                events=stream_events,
                experience_mode=experience_mode,
                route_budget=route_budget,
            )
        )
    return tuple(shards)


def plan_shard_splits(
    *,
    date: str,
    hour: str,
    shard_seed: str,
    shard_label: str,
    events: list[Any],
    experience_mode: str,
    route_budget: int,
) -> tuple[RouteShard, ...]:
    shard_estimate = SHARD_PROMPT_OVERHEAD_TOKENS + estimate_context_tokens(events, experience_mode=experience_mode)
    shard_id = f"{date}T{hour}-{shard_seed}"
    if shard_estimate <= route_budget or len(events) <= 1:
        return (
            RouteShard(
                shard_id=shard_id,
                shard_label=shard_label,
                events=tuple(events),
                estimated_tokens=shard_estimate,
            ),
        )
    midpoint = max(1, len(events) // 2)
    left = events[:midpoint]
    right = events[midpoint:]
    if not right:
        return (
            RouteShard(
                shard_id=shard_id,
                shard_label=shard_label,
                events=tuple(events),
                estimated_tokens=shard_estimate,
            ),
        )
    return (
        *plan_shard_splits(
            date=date,
            hour=hour,
            shard_seed=f"{shard_seed}-a",
            shard_label=f"{shard_label} / part A",
            events=left,
            experience_mode=experience_mode,
            route_budget=route_budget,
        ),
        *plan_shard_splits(
            date=date,
            hour=hour,
            shard_seed=f"{shard_seed}-b",
            shard_label=f"{shard_label} / part B",
            events=right,
            experience_mode=experience_mode,
            route_budget=route_budget,
        ),
    )


def estimate_context_tokens(events: list[Any], *, experience_mode: str) -> int:
    selected = agent_facing_events_for_estimate(events, experience_mode=experience_mode)
    stream_count = len(group_streams(selected)) if selected else 0
    text_tokens = sum(estimate_token_count(event.text or "") for event in selected)
    return (
        HOUR_RENDER_OVERHEAD_TOKENS
        + stream_count * STREAM_RENDER_OVERHEAD_TOKENS
        + len(selected) * EVENT_RENDER_OVERHEAD_TOKENS
        + text_tokens
    )


def agent_facing_events_for_estimate(events: list[Any], *, experience_mode: str) -> list[Any]:
    selected = []
    for event in events:
        text = str(getattr(event, "text", "") or "").strip()
        if not text:
            continue
        kind = str(getattr(event, "kind", "") or "")
        if experience_mode == "braided_lived_messages":
            if kind in {"user", "assistant"}:
                selected.append(event)
        elif kind in {"user", "assistant", "tool_use", "tool_result"}:
            selected.append(event)
    return selected


def write_route_plan_artifact(system: MemorySystem, plan: MonthHourlyRoutePlan) -> None:
    payload = plan.to_payload()
    plan.artifact_path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    plan.artifact_path.write_text(text, encoding="utf-8")
    store = LakeStore(system.storage_dir)
    store.ensure()
    row = store.artifact_row(
        "memory_route_plan",
        artifact_id=plan.artifact_id,
        uri=f"file://{plan.artifact_path}",
        path=str(plan.artifact_path),
        content_type="application/json",
        content_hash=stable_id("route-plan-content", plan.source_event_sha256, plan.max_prompt_tokens, plan.split_large_blocks_enabled),
        bytes=len(text.encode("utf-8")),
        source="memory.route_planner",
        state="planned",
        text=f"memory route plan {plan.month}",
        metadata={
            "month": plan.month,
            "max_prompt_tokens": plan.max_prompt_tokens,
            "split_large_blocks": plan.split_large_blocks_enabled,
            "source_event_sha256": plan.source_event_sha256,
            "planned_hire_count": plan.planned_hire_count,
            "route_counts": plan.route_counts,
        },
    )
    store.replace_rows("artifacts", "artifact_id", [row])


def fixed_four_hour_blocks(active_days: tuple[ActiveDay, ...]) -> tuple[ActiveBlock, ...]:
    blocks: list[ActiveBlock] = []
    for active_day in active_days:
        by_hour = {hour.hour: hour for hour in active_day.active_hours}
        for block_start_int in range(0, 24, 4):
            hours = tuple(
                active_hour
                for hour_int in range(block_start_int, block_start_int + 4)
                if (active_hour := by_hour.get(f"{hour_int:02d}"))
            )
            if not hours:
                continue
            blocks.append(
                ActiveBlock(
                    date=active_day.date,
                    block_start=f"{block_start_int:02d}",
                    block_end=f"{block_start_int + 3:02d}",
                    active_hours=hours,
                )
            )
    return tuple(blocks)


def discover_active_hours(
    system: MemorySystem,
    *,
    date: str,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> tuple[ActiveHour, ...]:
    start = parse_ts(f"{date}T00:00:00Z")
    end = start + timedelta(days=1)
    grouped = collect_events_by_day_hour(system, start=start, end=end, sources=sources)
    return active_hours_from_group(grouped.get(date, {}))


def discover_month_active_hours(
    system: MemorySystem,
    *,
    month: str,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> tuple[ActiveDay, ...]:
    start, end = month_bounds(month)
    grouped = collect_events_by_day_hour(system, start=start, end=end, sources=sources)
    return active_days_from_group(grouped)


def collect_events_by_day_hour(
    system: MemorySystem,
    *,
    start: datetime,
    end: datetime,
    sources: tuple[str, ...],
) -> dict[str, dict[str, list[Any]]]:
    store = LakeStore(system.storage_dir)
    store.ensure()
    events = events_between(store, start=format_ts(start), end=format_ts(end), sources=sources)
    grouped: dict[str, dict[str, list[Any]]] = {}
    for event in events:
        parsed = parse_ts(event.ts)
        date = parsed.strftime("%Y-%m-%d")
        hour = f"{parsed.hour:02d}"
        grouped.setdefault(date, {}).setdefault(hour, []).append(event)
    return grouped


def active_days_from_group(grouped: dict[str, dict[str, list[Any]]]) -> tuple[ActiveDay, ...]:
    return tuple(
        ActiveDay(date=date, active_hours=active_hours_from_group(by_hour))
        for date, by_hour in sorted(grouped.items())
    )


def active_hours_from_group(grouped: dict[str, list[Any]]) -> tuple[ActiveHour, ...]:
    active = []
    for hour, hour_events in sorted(grouped.items()):
        streams = {(event.source, event.session_id or "unknown") for event in hour_events}
        active.append(ActiveHour(hour=hour, event_count=len(hour_events), stream_count=len(streams)))
    return tuple(active)


def run_day_hourly_scribes(
    system: MemorySystem,
    *,
    date: str,
    audience: str = "private",
    workspace: Path | None = None,
    run_harness: bool = False,
    model: str | None = None,
    max_concurrent: int | None = None,
    limit_hours: int | None = None,
    skip_existing: bool = True,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> DayHourliesResult:
    workspace = workspace or Path("/tmp") / "onecontext-for-you-day"
    start = parse_ts(f"{date}T00:00:00Z")
    end = start + timedelta(days=1)
    grouped = collect_events_by_day_hour(system, start=start, end=end, sources=sources)
    day_events = grouped.get(date, {})
    active_hours = active_hours_from_group(day_events)
    prepared = []
    skipped_existing: list[str] = []
    for active_hour in active_hours:
        output_path = hourly_output_path(workspace, date=date, hour=active_hour.hour, audience=audience)
        if skip_existing and valid_existing_hourly(output_path, date=date, hour=active_hour.hour):
            skipped_existing.append(str(output_path))
            continue
        if limit_hours is not None and len(prepared) >= limit_hours:
            continue
        prepared.append(
            prepare_memory_job(
                system,
                job_id="memory.hourly.scribe",
                params={
                    "date": date,
                    "hour": active_hour.hour,
                    "audience": audience,
                    "source_harnesses": sources,
                    "_hour_events": day_events[active_hour.hour],
                },
                workspace=workspace,
                run_harness=run_harness,
                model=model,
                run_id=f"for-you-day-{date}",
                completed_event="memory.hourly_scribe.day_fanout_completed",
                validator=lambda path, hour=active_hour.hour: validate_talk_entry(
                    path,
                    expected_ts=f"{date}T{hour}:00:00Z",
                ),
            )
        )
    batch = execute_hired_agents(
        system,
        [item.execution_spec for item in prepared],
        max_concurrent=max_concurrent,
        run_id=f"for-you-day-{date}",
    )
    return DayHourliesResult(
        date=date,
        active_hours=active_hours,
        prepared_jobs=tuple(prepared),
        batch=batch,
        skipped_existing=tuple(skipped_existing),
    )


def run_month_hourly_scribes(
    system: MemorySystem,
    *,
    month: str,
    audience: str = "private",
    workspace: Path | None = None,
    run_harness: bool = False,
    model: str | None = None,
    max_concurrent: int | None = None,
    limit_hours: int | None = None,
    skip_existing: bool = True,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> MonthHourliesResult:
    workspace = workspace or Path("/tmp") / f"onecontext-for-you-month-{month}"
    start, end = month_bounds(month)
    grouped = collect_events_by_day_hour(system, start=start, end=end, sources=sources)
    active_days = active_days_from_group(grouped)
    prepared = []
    skipped_existing: list[str] = []
    for active_day in active_days:
        for active_hour in active_day.active_hours:
            output_path = hourly_output_path(workspace, date=active_day.date, hour=active_hour.hour, audience=audience)
            if skip_existing and valid_existing_hourly(output_path, date=active_day.date, hour=active_hour.hour):
                skipped_existing.append(str(output_path))
                continue
            if limit_hours is not None and len(prepared) >= limit_hours:
                continue
            prepared.append(
                prepare_memory_job(
                    system,
                    job_id="memory.hourly.scribe",
                    params={
                        "date": active_day.date,
                        "hour": active_hour.hour,
                        "audience": audience,
                        "source_harnesses": sources,
                        "_hour_events": grouped[active_day.date][active_hour.hour],
                    },
                    workspace=workspace,
                    run_harness=run_harness,
                    model=model,
                    run_id=f"for-you-month-{month}",
                    completed_event="memory.hourly_scribe.month_fanout_completed",
                    validator=lambda path, date=active_day.date, hour=active_hour.hour: validate_talk_entry(
                        path,
                        expected_ts=f"{date}T{hour}:00:00Z",
                    ),
                )
            )
    batch = execute_hired_agents(
        system,
        [item.execution_spec for item in prepared],
        max_concurrent=max_concurrent,
        run_id=f"for-you-month-{month}",
    )
    return MonthHourliesResult(
        month=month,
        active_days=active_days,
        prepared_jobs=tuple(prepared),
        batch=batch,
        skipped_existing=tuple(skipped_existing),
    )


def run_month_hourly_block_scribes(
    system: MemorySystem,
    *,
    month: str,
    audience: str = "private",
    workspace: Path | None = None,
    run_harness: bool = False,
    model: str | None = None,
    max_concurrent: int | None = None,
    limit_blocks: int | None = None,
    skip_existing: bool = True,
    experience_mode: str | None = None,
    split_large_blocks: bool = False,
    max_prompt_tokens: int = DEFAULT_PROMPT_WARNING_TOKENS,
    max_prompt_bytes: int | None = None,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> MonthHourlyBlocksResult:
    workspace = workspace or Path("/tmp") / f"onecontext-for-you-month-{month}-blocks"
    route_plan = plan_month_hourly_routes(
        system,
        month=month,
        audience=audience,
        workspace=workspace,
        limit_blocks=limit_blocks,
        skip_existing=skip_existing,
        split_large_blocks=split_large_blocks,
        max_prompt_tokens=max_prompt_tokens,
        experience_mode=experience_mode,
        sources=sources,
    )
    active_days = route_plan.active_days
    prepared: list[PreparedMemoryJob] = []
    prepared_blocks: list[ActiveBlock] = []
    first_wave_prepared: list[PreparedMemoryJob] = []
    aggregate_requests: list[dict[str, Any]] = []
    skipped_existing = list(route_plan.skipped_existing)
    for block_route in route_plan.routes:
        planned_block = block_route.block
        talk_folder = workspace / f"for-you-{planned_block.date}.{audience}.talk"
        manifest_path = talk_folder / f"{planned_block.date}T{planned_block.block_start}-{planned_block.block_end}Z.block-result.json"
        if block_route.route == "block":
            hours = tuple(hour.hour for hour in block_route.hours)
            block_job = prepare_memory_job(
                system,
                job_id=HOURLY_BLOCK_SCRIBE_JOB_ID,
                params={
                    "date": planned_block.date,
                    "block_start": planned_block.block_start,
                    "hours": hours,
                    "audience": audience,
                    "source_harnesses": sources,
                    "talk_folder": str(talk_folder),
                    "manifest_path": str(manifest_path),
                    "_hour_events_by_hour": {
                        hour_route.hour: list(hour_route.events)
                        for hour_route in block_route.hours
                    },
                    **({"experience_mode": experience_mode} if experience_mode else {}),
                },
                workspace=workspace,
                run_harness=run_harness,
                model=model,
                run_id=f"for-you-month-{month}-blocks",
                completed_event="memory.hourly_block_scribe.month_fanout_completed",
                validator=lambda path, date=planned_block.date, hours=hours, folder=talk_folder: validate_hourly_block_result(
                    path,
                    talk_folder=folder,
                    date=date,
                    expected_hours=hours,
                ),
            )
            prepared.append(block_job)
            first_wave_prepared.append(block_job)
            prepared_blocks.append(planned_block)
            continue

        for hour_route in block_route.hours:
            hour = hour_route.hour
            active_hour = hour_route.active_hour
            if hour_route.route == "hourly":
                hour_job = prepare_memory_job(
                    system,
                    job_id=HOURLY_SCRIBE_JOB_ID,
                    params={
                        "date": planned_block.date,
                        "hour": hour,
                        "audience": audience,
                        "source_harnesses": sources,
                        "talk_folder": str(talk_folder),
                        "_hour_events": list(hour_route.events),
                        **({"experience_mode": experience_mode} if experience_mode else {}),
                    },
                    workspace=workspace,
                    run_harness=run_harness,
                    model=model,
                    run_id=f"for-you-month-{month}-blocks-split",
                    completed_event="memory.hourly_scribe.large_block_split_completed",
                    validator=lambda path, date=planned_block.date, hour=hour: validate_talk_entry(
                        path,
                        expected_ts=f"{date}T{hour}:00:00Z",
                    ),
                )
                prepared.append(hour_job)
                first_wave_prepared.append(hour_job)
                prepared_blocks.append(ActiveBlock(date=planned_block.date, block_start=hour, block_end=hour, active_hours=(active_hour,)))
                continue

            shard_jobs = [
                prepare_shard_job(
                    system,
                    date=planned_block.date,
                    hour=hour,
                    audience=audience,
                    workspace=workspace,
                    talk_folder=talk_folder,
                    run_harness=run_harness,
                    model=model,
                    run_id=f"for-you-month-{month}-hour-shards",
                    source_harnesses=sources,
                    events=list(shard.events),
                    experience_mode=experience_mode,
                    shard_id=shard.shard_id,
                    shard_label=shard.shard_label,
                )
                for shard in hour_route.shards
            ]
            for shard_job in shard_jobs:
                prepared.append(shard_job)
                first_wave_prepared.append(shard_job)
                prepared_blocks.append(
                    ActiveBlock(
                        date=planned_block.date,
                        block_start=hour,
                        block_end=hour,
                        active_hours=(active_hour,),
                    )
                )
            aggregate_requests.append(
                {
                    "date": planned_block.date,
                    "hour": hour,
                    "active_hour": active_hour,
                    "talk_folder": talk_folder,
                    "shard_paths": tuple(job.execution_spec.artifact.path for job in shard_jobs),
                    "audience": audience,
                }
            )
    first_batch = execute_hired_agents(
        system,
        [item.execution_spec for item in first_wave_prepared],
        max_concurrent=max_concurrent,
        run_id=f"for-you-month-{month}-blocks",
    )
    aggregate_prepared: list[PreparedMemoryJob] = []
    if aggregate_requests and first_batch.ok and first_batch.to_payload()["validation_failure_count"] == 0:
        for request in aggregate_requests:
            hour = str(request["hour"])
            date = str(request["date"])
            active_hour = request["active_hour"]
            aggregate_job = prepare_memory_job(
                system,
                job_id=HOURLY_AGGREGATE_SCRIBE_JOB_ID,
                params={
                    "date": date,
                    "hour": hour,
                    "audience": request["audience"],
                    "talk_folder": str(request["talk_folder"]),
                    "shard_paths": request["shard_paths"],
                },
                workspace=workspace,
                run_harness=run_harness,
                model=model,
                run_id=f"for-you-month-{month}-hour-aggregates",
                completed_event="memory.hourly_aggregate_scribe.large_hour_completed",
                validator=lambda path, date=date, hour=hour: validate_talk_entry(
                    path,
                    expected_ts=f"{date}T{hour}:00:00Z",
                ),
            )
            aggregate_prepared.append(aggregate_job)
            prepared.append(aggregate_job)
            prepared_blocks.append(
                ActiveBlock(
                    date=date,
                    block_start=hour,
                    block_end=hour,
                    active_hours=(active_hour,),
                )
            )
    aggregate_batch = execute_hired_agents(
        system,
        [item.execution_spec for item in aggregate_prepared],
        max_concurrent=max_concurrent,
        run_id=f"for-you-month-{month}-hour-aggregates",
    )
    batch = combine_batches(first_batch, aggregate_batch)
    return MonthHourlyBlocksResult(
        month=month,
        active_days=active_days,
        active_blocks=tuple(prepared_blocks),
        prepared_jobs=tuple(prepared),
        batch=batch,
        skipped_existing=tuple(skipped_existing),
        split_large_blocks=route_plan.split_large_block_labels,
        sharded_hours=route_plan.sharded_hours,
        prompt_warning_tokens=max_prompt_tokens,
        prompt_warning_bytes=max_prompt_bytes,
    )


def prepare_shard_job(
    system: MemorySystem,
    *,
    date: str,
    hour: str,
    audience: str,
    workspace: Path,
    talk_folder: Path,
    run_harness: bool,
    model: str | None,
    run_id: str,
    source_harnesses: tuple[str, ...],
    events: list[Any],
    experience_mode: str | None,
    shard_id: str,
    shard_label: str,
) -> PreparedMemoryJob:
    output_path = talk_folder / ".shards" / f"{date}T{hour}-00Z.{slug_text(shard_id)}.synthesis.md"
    return prepare_memory_job(
        system,
        job_id=HOURLY_SHARD_SCRIBE_JOB_ID,
        params={
            "date": date,
            "hour": hour,
            "audience": audience,
            "source_harnesses": source_harnesses,
            "talk_folder": str(talk_folder),
            "output_path": str(output_path),
            "shard_id": shard_id,
            "shard_label": shard_label,
            "_shard_events": events,
            **({"experience_mode": experience_mode} if experience_mode else {}),
        },
        workspace=workspace,
        run_harness=run_harness,
        model=model,
        run_id=run_id,
        completed_event="memory.hourly_shard_scribe.completed",
        validator=lambda path, date=date, hour=hour: validate_talk_entry(
            path,
            expected_ts=f"{date}T{hour}:00:00Z",
            expected_kind="synthesis",
        ),
    )


def combine_batches(first: HiredAgentBatchResult, second: HiredAgentBatchResult) -> HiredAgentBatchResult:
    return HiredAgentBatchResult(
        max_concurrent=max(first.max_concurrent, second.max_concurrent),
        results=tuple(first.results) + tuple(second.results),
        errors=tuple(first.errors) + tuple(second.errors),
        started_at=first.started_at or second.started_at,
        completed_at=second.completed_at or first.completed_at,
        duration_ms=first.duration_ms + second.duration_ms,
    )


def slug_text(value: str) -> str:
    return "".join(char if char.isalnum() or char in "._-" else "-" for char in value.strip()).strip("-") or "shard"


def discover_month_retry_hours(
    *,
    month: str,
    workspace: Path,
    audience: str = "private",
) -> tuple[RetryHour, ...]:
    retry_hours: list[RetryHour] = []
    for talk_folder in sorted(workspace.glob(f"for-you-{month}-*.{audience}.talk")):
        if not talk_folder.is_dir():
            continue
        for manifest_path in sorted(talk_folder.glob(f"{month}-*T*-*Z.block-result.json")):
            try:
                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                continue
            date = str(manifest.get("date") or "")
            if not date.startswith(month):
                continue
            for item in manifest.get("hours", []):
                if not isinstance(item, dict) or item.get("status") != "needs-retry":
                    continue
                try:
                    hour = f"{int(str(item.get('hour'))):02d}"
                except ValueError:
                    continue
                retry_hours.append(
                    RetryHour(
                        date=date,
                        hour=hour,
                        reason=str(item.get("reason") or ""),
                        manifest_path=manifest_path,
                    )
                )
    return tuple(retry_hours)


def run_month_hourly_retries(
    system: MemorySystem,
    *,
    month: str,
    audience: str = "private",
    workspace: Path | None = None,
    run_harness: bool = False,
    model: str | None = None,
    max_concurrent: int | None = None,
    limit_hours: int | None = None,
    skip_existing: bool = True,
    sources: tuple[str, ...] = ("codex", "claude-code"),
) -> MonthHourlyRetriesResult:
    workspace = workspace or Path("/tmp") / f"onecontext-for-you-month-{month}-blocks"
    retry_hours = discover_month_retry_hours(month=month, workspace=workspace, audience=audience)
    prepared: list[PreparedMemoryJob] = []
    skipped_existing: list[str] = []
    if not retry_hours:
        batch = execute_hired_agents(
            system,
            [],
            max_concurrent=max_concurrent,
            run_id=f"for-you-month-{month}-retries",
        )
        return MonthHourlyRetriesResult(
            month=month,
            retry_hours=retry_hours,
            prepared_jobs=tuple(prepared),
            batch=batch,
            skipped_existing=tuple(skipped_existing),
        )
    start, end = month_bounds(month)
    grouped = collect_events_by_day_hour(system, start=start, end=end, sources=sources)
    for retry_hour in retry_hours:
        output_path = hourly_output_path(workspace, date=retry_hour.date, hour=retry_hour.hour, audience=audience)
        if skip_existing and valid_existing_hourly(output_path, date=retry_hour.date, hour=retry_hour.hour):
            skipped_existing.append(str(output_path))
            continue
        if limit_hours is not None and len(prepared) >= limit_hours:
            continue
        day_events = grouped.get(retry_hour.date, {})
        hour_events = day_events.get(retry_hour.hour, [])
        prepared.append(
            prepare_memory_job(
                system,
                job_id="memory.hourly.scribe",
                params={
                    "date": retry_hour.date,
                    "hour": retry_hour.hour,
                    "audience": audience,
                    "source_harnesses": sources,
                    "experience_mode": "braided_lived_transcript",
                    "retry_reason": retry_hour.reason,
                    "retry_source_manifest": str(retry_hour.manifest_path),
                    "_hour_events": hour_events,
                },
                workspace=workspace,
                run_harness=run_harness,
                model=model,
                run_id=f"for-you-month-{month}-retries",
                completed_event="memory.hourly_scribe.retry_completed",
                validator=lambda path, date=retry_hour.date, hour=retry_hour.hour: validate_talk_entry(
                    path,
                    expected_ts=f"{date}T{hour}:00:00Z",
                ),
            )
        )
    batch = execute_hired_agents(
        system,
        [item.execution_spec for item in prepared],
        max_concurrent=max_concurrent,
        run_id=f"for-you-month-{month}-retries",
    )
    return MonthHourlyRetriesResult(
        month=month,
        retry_hours=retry_hours,
        prepared_jobs=tuple(prepared),
        batch=batch,
        skipped_existing=tuple(skipped_existing),
    )


def hourly_output_path(
    workspace: Path,
    *,
    date: str,
    hour: str,
    audience: str,
    page_slug: str | None = None,
) -> Path:
    slug = page_slug or f"for-you-{date}"
    return workspace / f"{slug}.{audience}.talk" / f"{date}T{int(hour):02d}-00Z.conversation.md"


def valid_existing_hourly(path: Path, *, date: str, hour: str) -> bool:
    if not path.is_file():
        return False
    return bool(validate_talk_entry(path, expected_ts=f"{date}T{int(hour):02d}:00:00Z").get("ok"))


def month_bounds(month: str) -> tuple[datetime, datetime]:
    try:
        year_text, month_text = month.split("-", 1)
        year = int(year_text)
        month_number = int(month_text)
        start = datetime(year, month_number, 1, tzinfo=timezone.utc)
    except ValueError as exc:
        raise DayHourliesError(f"month must be YYYY-MM, got {month!r}") from exc
    if month_number == 12:
        end = datetime(year + 1, 1, 1, tzinfo=timezone.utc)
    else:
        end = datetime(year, month_number + 1, 1, tzinfo=timezone.utc)
    return start, end
