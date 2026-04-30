from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.scheduler import SchedulerPlan
from onectx.storage import LakeStore, stable_id, utc_now


SUCCESS_EVENTS = {
    "memory.scheduler.fire_completed",
    "memory.tick.completed",
    "memory.replay.dry_run_completed",
}

FAILURE_EVENTS = {
    "memory.scheduler.fire_failed",
    "memory.tick.failed",
    "memory.tick.blocked",
    "memory.tick.retryable",
}


def build_memory_health_payload(store: LakeStore, *, scheduler_plan: SchedulerPlan | None = None) -> dict[str, Any]:
    store.ensure()
    events = store.rows("events", limit=0)
    success = last_events_by_phase(events, SUCCESS_EVENTS)
    failure = last_events_by_phase(events, FAILURE_EVENTS)
    scheduler_payload = scheduler_plan.to_payload() if scheduler_plan else {}
    blocked_count = int(scheduler_payload.get("blocked_count") or 0)
    status = "blocked" if blocked_count else "healthy"
    if failure and not success:
        status = "degraded"
    return {
        "kind": "memory_health.v1",
        "generated_at": utc_now(),
        "status": status,
        "last_success_by_phase": success,
        "last_failure_by_phase": failure,
        "scheduler": scheduler_payload,
        "summary": {
            "success_phase_count": len(success),
            "failure_phase_count": len(failure),
            "scheduler_blocked_count": blocked_count,
        },
    }


def write_memory_health_artifact(system: MemorySystem, payload: dict[str, Any], *, run_id: str = "") -> dict[str, Any]:
    resolved_run_id = run_id or stable_id("health", payload.get("generated_at", utc_now()))
    out_dir = system.runtime_dir / "health" / resolved_run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / "health.json"
    payload = {**payload, "run_id": resolved_run_id}
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    path.write_text(text, encoding="utf-8")
    content_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()

    store = LakeStore(system.storage_dir)
    store.ensure()
    status = str(payload.get("status") or "unknown")
    artifact = store.append_artifact(
        "memory_health",
        uri=path.as_uri(),
        path=str(path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=path.stat().st_size,
        source="memory.health",
        state=status,
        text=f"memory health {resolved_run_id}: {status}",
        metadata={"run_id": resolved_run_id, "status": status},
    )
    evidence = store.append_evidence(
        "memory_health.ready",
        artifact_id=artifact["artifact_id"],
        status="passed" if status in {"healthy", "blocked"} else "failed",
        checker="memory.health",
        text="memory health payload written",
        checks=["last success/failure scan completed", "scheduler status included when supplied"],
        payload=payload,
    )
    event = store.append_event(
        "memory.health.ready",
        source="memory.health",
        kind="health",
        subject=resolved_run_id,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence["evidence_id"],
        payload={"run_id": resolved_run_id, "status": status},
    )
    return {
        "run_id": resolved_run_id,
        "path": str(path),
        "artifact_id": artifact["artifact_id"],
        "evidence_id": evidence["evidence_id"],
        "event_id": event["event_id"],
        "status": status,
    }


def last_events_by_phase(rows: list[dict[str, Any]], event_names: set[str]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for row in rows:
        event_name = str(row.get("event") or "")
        if event_name not in event_names:
            continue
        payload = parse_payload(row.get("payload_json"))
        phase = str(payload.get("phase") or payload.get("boundary") or row.get("scope") or row.get("subject") or "system")
        ts = str(row.get("ts") or "")
        previous = result.get(phase)
        if previous and str(previous.get("ts") or "") > ts:
            continue
        result[phase] = {
            "event": event_name,
            "event_id": str(row.get("event_id") or ""),
            "ts": ts,
            "phase": phase,
            "subject": str(row.get("subject") or ""),
            "status": str(payload.get("status") or event_status(event_name)),
            "payload": payload,
        }
    return dict(sorted(result.items()))


def parse_payload(value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        return value
    if not value:
        return {}
    try:
        parsed = json.loads(str(value))
    except json.JSONDecodeError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def event_status(event_name: str) -> str:
    if event_name in SUCCESS_EVENTS:
        return "completed"
    if event_name == "memory.tick.retryable":
        return "retryable"
    return "failed"
