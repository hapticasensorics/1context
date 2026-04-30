from __future__ import annotations

import json
import hashlib
import shutil
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.storage import LakeStore, stable_id
from onectx.storage.hour_events import HourEvent, events_between, format_ts, normalize_source, parse_ts


class ReplayError(RuntimeError):
    """Raised when historic event replay cannot be planned."""


@dataclass(frozen=True)
class ReplayDryRunResult:
    replay_run_id: str
    path: Path
    start: str
    end: str
    sources: tuple[str, ...]
    event_count: int
    fires: tuple[dict[str, Any], ...]
    sandbox: dict[str, Any]
    injections: tuple[dict[str, Any], ...]
    artifact_id: str
    content_hash: str

    @property
    def fire_count(self) -> int:
        return len(self.fires)

    def to_payload(self) -> dict[str, Any]:
        return {
            "replay_run_id": self.replay_run_id,
            "path": str(self.path),
            "start": self.start,
            "end": self.end,
            "sources": list(self.sources),
            "event_count": self.event_count,
            "fire_count": self.fire_count,
            "sandbox": self.sandbox,
            "injection_count": len(self.injections),
            "injections": list(self.injections),
            "artifact_id": self.artifact_id,
            "content_hash": self.content_hash,
            "fires_by_agent": fires_by_agent(self.fires),
            "files": {
                "config": str(self.path / "config.json"),
                "events": str(self.path / "events.jsonl"),
                "fires": str(self.path / "fires.jsonl"),
                "sandbox": str(self.path / "sandbox"),
                "snapshots": str(self.path / "snapshots"),
                "injections": str(self.path / "injections.jsonl"),
                "summary": str(self.path / "summary.json"),
            },
        }


AGENTS_BY_BOUNDARY: dict[str, tuple[str, ...]] = {
    "hour": ("scribe",),
    "day": ("historian", "answerer", "editor"),
    "week": ("for-you-curator", "context-curator", "biographer", "librarian", "contradiction-flagger"),
    "month": ("librarian-sweep", "archive"),
}


def run_replay_dry_run(
    system: MemorySystem,
    *,
    start: str,
    end: str,
    sources: tuple[str, ...] = ("codex", "claude-code"),
    replay_run_id: str = "",
    sandbox: Path | None = None,
    failure_injections: tuple[str, ...] = (),
    operator_edit_injections: tuple[str, ...] = (),
) -> ReplayDryRunResult:
    store = LakeStore(system.storage_dir)
    store.ensure()
    start_dt = parse_ts(start)
    end_dt = parse_ts(end)
    if end_dt <= start_dt:
        raise ReplayError("end must be after start")
    normalized_sources = tuple(normalize_source(source) for source in sources if str(source).strip())
    events = events_between(store, start=format_ts(start_dt), end=format_ts(end_dt), sources=normalized_sources)
    replay_id = replay_run_id or stable_id("replay", format_ts(start_dt), format_ts(end_dt), ",".join(normalized_sources))
    out_dir = system.runtime_dir / "replay-runs" / replay_id
    out_dir.mkdir(parents=True, exist_ok=True)

    fires = derive_replay_fires(start_dt, end_dt, events)
    sandbox_payload, injection_rows = prepare_replay_sandbox(
        sandbox,
        out_dir=out_dir,
        fires=fires,
        failure_injections=failure_injections,
        operator_edit_injections=operator_edit_injections,
    )
    config = {
        "replay_run_id": replay_id,
        "start": format_ts(start_dt),
        "end": format_ts(end_dt),
        "sources": list(normalized_sources),
        "mode": "dry_run",
        "sandbox": sandbox_payload,
        "requested_injections": {
            "failure": list(failure_injections),
            "operator_edit": list(operator_edit_injections),
        },
        "scheduler": {
            "hour": list(AGENTS_BY_BOUNDARY["hour"]),
            "day": list(AGENTS_BY_BOUNDARY["day"]),
            "week": list(AGENTS_BY_BOUNDARY["week"]),
            "month": list(AGENTS_BY_BOUNDARY["month"]),
        },
    }
    write_json(out_dir / "config.json", config)
    write_jsonl(out_dir / "events.jsonl", [event_to_replay_row(event) for event in events])
    write_jsonl(out_dir / "fires.jsonl", fires)
    write_jsonl(out_dir / "injections.jsonl", injection_rows)
    summary = {
        "replay_run_id": replay_id,
        "start": format_ts(start_dt),
        "end": format_ts(end_dt),
        "event_count": len(events),
        "fire_count": len(fires),
        "fires_by_agent": fires_by_agent(fires),
        "fires_by_boundary": fires_by_boundary(fires),
        "sandbox": sandbox_payload,
        "injection_count": len(injection_rows),
        "retryable_injection_count": sum(1 for row in injection_rows if row.get("retryable")),
        "protected_outcome_count": sum(1 for row in injection_rows if row.get("expected_outcome") in {"defer", "needs_approval"}),
        "injections": injection_rows,
        "skip_candidate_count": sum(1 for fire in fires if fire.get("prior_window_event_count") == 0),
        "notes": [
            "dry-run only; no agents launched",
            "cadence fires are unconditional; runners decide skip/no-talk/no-change",
            "sandbox injections mutate only the replay workspace copy",
        ],
    }
    summary_text = write_json(out_dir / "summary.json", summary)
    content_hash = hashlib.sha256(summary_text.encode("utf-8")).hexdigest()
    artifact_id = stable_id("artifact", "memory_replay_dry_run", replay_id, content_hash)
    artifact_row = store.artifact_row(
        "memory_replay_dry_run",
        artifact_id=artifact_id,
        uri=f"file://{out_dir}",
        path=str(out_dir),
        content_type="application/json",
        content_hash=content_hash,
        bytes=sum(path.stat().st_size for path in out_dir.iterdir() if path.is_file()),
        source="memory.replay",
        state="planned",
        text=f"memory replay dry-run {replay_id}",
        metadata={
            "replay_run_id": replay_id,
            "start": format_ts(start_dt),
            "end": format_ts(end_dt),
            "event_count": len(events),
            "fire_count": len(fires),
            "sandbox_enabled": bool(sandbox_payload.get("enabled")),
            "injection_count": len(injection_rows),
        },
    )
    store.replace_rows("artifacts", "artifact_id", [artifact_row])
    store.append_evidence(
        "replay_schedule.ready",
        artifact_id=artifact_id,
        status="passed",
        checker="memory.replay",
        text="historic event replay dry-run schedule written",
        checks=["config.json exists", "events.jsonl exists", "fires.jsonl exists", "summary.json exists"],
        payload=summary,
    )
    if sandbox_payload.get("enabled"):
        store.append_evidence(
            "replay_snapshot.ready",
            artifact_id=artifact_id,
            status="passed" if sandbox_payload.get("source_unchanged") else "failed",
            checker="memory.replay",
            text="replay sandbox snapshots and source-mutation check written",
            checks=[
                "source snapshot captured",
                "sandbox snapshot captured",
                "source tree unchanged after sandbox work",
            ],
            payload=sandbox_payload,
        )
    if injection_rows:
        store.append_evidence(
            "replay_failure_injection.applied",
            artifact_id=artifact_id,
            status="passed",
            checker="memory.replay",
            text="replay failure/operator-edit injections applied in sandbox",
            checks=["injections.jsonl exists", "each injection records expected recovery path"],
            payload={"replay_run_id": replay_id, "injections": injection_rows},
        )
    store.append_evidence(
        "replay_run.completed",
        artifact_id=artifact_id,
        status="passed",
        checker="memory.replay",
        text="historic event replay dry-run completed",
        checks=["schedule written", "summary written", "no live agents launched"],
        payload=summary,
    )
    store.append_event(
        "memory.replay.dry_run_completed",
        source="memory.replay",
        actor="replay-dry-run",
        subject=replay_id,
        artifact_id=artifact_id,
        text=f"Replay dry-run scheduled {len(fires)} fires over {len(events)} events.",
        payload=summary,
    )
    return ReplayDryRunResult(
        replay_run_id=replay_id,
        path=out_dir,
        start=format_ts(start_dt),
        end=format_ts(end_dt),
        sources=normalized_sources,
        event_count=len(events),
        fires=tuple(fires),
        sandbox=sandbox_payload,
        injections=tuple(injection_rows),
        artifact_id=artifact_id,
        content_hash=content_hash,
    )


def prepare_replay_sandbox(
    source: Path | None,
    *,
    out_dir: Path,
    fires: list[dict[str, Any]],
    failure_injections: tuple[str, ...],
    operator_edit_injections: tuple[str, ...],
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    if source is None:
        if failure_injections or operator_edit_injections:
            raise ReplayError("failure/operator-edit injection requires --sandbox")
        return {"enabled": False}, []

    source = source.expanduser().resolve()
    if not source.exists():
        raise ReplayError(f"sandbox source does not exist: {source}")
    sandbox_root = out_dir / "sandbox" / source.name
    if sandbox_root.exists():
        raise ReplayError(f"replay sandbox already exists: {sandbox_root}")

    source_before = snapshot_tree(source)
    if source.is_dir():
        shutil.copytree(source, sandbox_root)
    else:
        sandbox_root.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, sandbox_root)
    sandbox_before = snapshot_tree(sandbox_root)

    injection_rows: list[dict[str, Any]] = []
    for spec in failure_injections:
        injection_rows.append(build_failure_injection(spec, fires=fires))
    for relative_path in operator_edit_injections:
        injection_rows.append(apply_operator_edit_injection(sandbox_root, relative_path, fires=fires))

    source_after = snapshot_tree(source)
    sandbox_after = snapshot_tree(sandbox_root)
    snapshots_dir = out_dir / "snapshots"
    write_json(snapshots_dir / "source-before.json", source_before)
    write_json(snapshots_dir / "source-after.json", source_after)
    write_json(snapshots_dir / "sandbox-before.json", sandbox_before)
    write_json(snapshots_dir / "sandbox-after.json", sandbox_after)
    source_diff = diff_snapshots(source_before, source_after)
    sandbox_diff = diff_snapshots(sandbox_before, sandbox_after)
    write_json(snapshots_dir / "source-diff.json", source_diff)
    write_json(snapshots_dir / "sandbox-diff.json", sandbox_diff)

    payload = {
        "enabled": True,
        "source": str(source),
        "workspace": str(sandbox_root),
        "source_file_count": len(source_before["files"]),
        "sandbox_file_count": len(sandbox_after["files"]),
        "source_unchanged": not any(source_diff[key] for key in ("added", "removed", "changed")),
        "source_diff": source_diff,
        "sandbox_diff": sandbox_diff,
        "snapshots": {
            "source_before": str(snapshots_dir / "source-before.json"),
            "source_after": str(snapshots_dir / "source-after.json"),
            "sandbox_before": str(snapshots_dir / "sandbox-before.json"),
            "sandbox_after": str(snapshots_dir / "sandbox-after.json"),
            "source_diff": str(snapshots_dir / "source-diff.json"),
            "sandbox_diff": str(snapshots_dir / "sandbox-diff.json"),
        },
    }
    return payload, injection_rows


def build_failure_injection(spec: str, *, fires: list[dict[str, Any]]) -> dict[str, Any]:
    injection_type, _, target = spec.partition(":")
    injection_type = injection_type.strip() or "agent_timeout"
    if injection_type not in {"agent_timeout", "tool_failure", "process_kill"}:
        raise ReplayError(f"unsupported replay failure injection {injection_type!r}")
    target_fire = target.strip() or first_fire_target(fires)
    return {
        "id": stable_id("replay_injection", injection_type, target_fire),
        "kind": "failure",
        "type": injection_type,
        "target": target_fire,
        "status": "applied",
        "retryable": True,
        "expected_outcome": "needs_retry",
        "recovery_path": "record failure, preserve sandbox state, retry or operator-review according to retry budget",
    }


def apply_operator_edit_injection(sandbox_root: Path, relative_path: str, *, fires: list[dict[str, Any]]) -> dict[str, Any]:
    clean_relative = relative_path.strip().lstrip("/")
    if not clean_relative:
        clean_relative = "operator-edit-injection.md"
    target = (sandbox_root / clean_relative).resolve()
    sandbox_resolved = sandbox_root.resolve()
    if sandbox_resolved not in (target, *target.parents):
        raise ReplayError(f"operator edit target escapes sandbox: {relative_path}")
    target.parent.mkdir(parents=True, exist_ok=True)
    original = target.read_text(encoding="utf-8") if target.exists() else ""
    marker = "\n<!-- operator-touched: replay fixture -->\n"
    target.write_text(original.rstrip() + marker + "\n", encoding="utf-8")
    target_fire = first_fire_target(fires)
    return {
        "id": stable_id("replay_injection", "operator_edit", clean_relative, target_fire),
        "kind": "operator_edit",
        "type": "operator_edit",
        "target": target_fire,
        "path": str(target),
        "relative_path": clean_relative,
        "status": "applied",
        "retryable": False,
        "expected_outcome": "needs_approval",
        "recovery_path": "curator/generator must defer rather than overwrite operator-touched sandbox content",
    }


def first_fire_target(fires: list[dict[str, Any]]) -> str:
    if not fires:
        return "no-fire"
    first = fires[0]
    return f"{first.get('agent', 'agent')}@{first.get('t_stream', 'time')}"


def snapshot_tree(root: Path) -> dict[str, Any]:
    root = root.resolve()
    if root.is_file():
        paths = [root]
    else:
        paths = sorted(path for path in root.rglob("*") if path.is_file())
    files = []
    for path in paths:
        rel = path.name if root.is_file() else path.relative_to(root).as_posix()
        data = path.read_bytes()
        files.append(
            {
                "path": rel,
                "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
    return {
        "root": str(root),
        "file_count": len(files),
        "files": files,
    }


def diff_snapshots(before: dict[str, Any], after: dict[str, Any]) -> dict[str, list[str]]:
    before_files = {str(item.get("path") or ""): str(item.get("sha256") or "") for item in before.get("files", [])}
    after_files = {str(item.get("path") or ""): str(item.get("sha256") or "") for item in after.get("files", [])}
    before_paths = set(before_files)
    after_paths = set(after_files)
    return {
        "added": sorted(after_paths - before_paths),
        "removed": sorted(before_paths - after_paths),
        "changed": sorted(path for path in before_paths & after_paths if before_files[path] != after_files[path]),
    }


def derive_replay_fires(start: datetime, end: datetime, events: list[HourEvent]) -> list[dict[str, Any]]:
    fires: list[dict[str, Any]] = []
    cursor = floor_hour(start) + timedelta(hours=1)
    event_times = [parse_ts(event.ts) for event in events]
    while cursor <= end:
        boundaries = crossed_boundaries(cursor)
        for boundary in boundaries:
            window_start = prior_window_start(cursor, boundary)
            event_count = count_events(event_times, window_start, cursor)
            for agent in AGENTS_BY_BOUNDARY[boundary]:
                fires.append(
                    {
                        "agent": agent,
                        "boundary": boundary,
                        "t_stream": format_ts(cursor),
                        "prior_window_start": format_ts(window_start),
                        "prior_window_end": format_ts(cursor),
                        "prior_window_event_count": event_count,
                        "mode": "dry_run",
                        "note": "skip-candidate" if event_count == 0 and boundary == "hour" else "scheduled",
                    }
                )
        cursor += timedelta(hours=1)
    return fires


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


def count_events(event_times: list[datetime], start: datetime, end: datetime) -> int:
    return sum(1 for ts in event_times if start <= ts < end)


def floor_hour(value: datetime) -> datetime:
    return value.astimezone(timezone.utc).replace(minute=0, second=0, microsecond=0)


def event_to_replay_row(event: HourEvent) -> dict[str, Any]:
    return {
        "event_id": event.event_id,
        "session_id": event.session_id,
        "ts": event.ts,
        "source": event.source,
        "kind": event.kind,
        "actor": event.actor,
        "cwd": event.cwd,
        "char_count": len(event.text),
        "event": event.event,
    }


def fires_by_agent(fires: tuple[dict[str, Any], ...] | list[dict[str, Any]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for fire in fires:
        agent = str(fire.get("agent") or "")
        counts[agent] = counts.get(agent, 0) + 1
    return dict(sorted(counts.items()))


def fires_by_boundary(fires: list[dict[str, Any]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for fire in fires:
        boundary = str(fire.get("boundary") or "")
        counts[boundary] = counts.get(boundary, 0) + 1
    return dict(sorted(counts.items()))


def write_json(path: Path, payload: dict[str, Any]) -> str:
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return text


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "".join(json.dumps(row, sort_keys=True, default=str) + "\n" for row in rows),
        encoding="utf-8",
    )
