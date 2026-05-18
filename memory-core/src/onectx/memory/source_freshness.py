from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from onectx.storage import LakeStore
from onectx.storage.hour_events import HourEventError, normalize_source, parse_ts


def evaluate_source_freshness(
    store: LakeStore,
    *,
    required_sources: tuple[str, ...] = ("codex", "claude-code"),
    max_age_hours: int = 24,
    now: datetime | None = None,
) -> dict[str, Any]:
    now_dt = now or datetime.now(timezone.utc)
    required = tuple(normalize_source(source) for source in required_sources if str(source).strip())
    rows_by_source = _rows_by_source(store)
    sources: dict[str, Any] = {}
    passed = True

    for source in required:
        rows = rows_by_source.get(source, [])
        latest_ts = _latest_timestamp(rows)
        event_count = sum(int(row.get("event_count") or 0) for row in rows)
        if latest_ts is None:
            status = "missing"
            age_seconds = None
            passed = False
        else:
            age_seconds = max(0, int((now_dt - latest_ts).total_seconds()))
            status = "fresh" if age_seconds <= max_age_hours * 3600 else "stale"
            if status != "fresh":
                passed = False
        sources[source] = {
            "status": status,
            "latest_ts": _format_ts(latest_ts),
            "age_seconds": age_seconds,
            "max_age_hours": max_age_hours,
            "session_or_event_rows": len(rows),
            "event_count": event_count,
        }

    return {
        "kind": "source_import_freshness",
        "checked_at": _format_ts(now_dt),
        "required_sources": list(required),
        "max_age_hours": max_age_hours,
        "passed": passed,
        "sources": sources,
    }


def _rows_by_source(store: LakeStore) -> dict[str, list[dict[str, Any]]]:
    by_source: dict[str, list[dict[str, Any]]] = {}
    sessions = store.rows("sessions", limit=0)
    if sessions:
        for row in sessions:
            source = normalize_source(str(row.get("source") or ""))
            if source:
                by_source.setdefault(source, []).append(row)
        return by_source

    for row in store.rows("events", limit=0):
        source = normalize_source(str(row.get("source") or ""))
        if source:
            by_source.setdefault(source, []).append(
                {"source": source, "last_ts": row.get("ts", ""), "event_count": 1}
            )
    return by_source


def _latest_timestamp(rows: list[dict[str, Any]]) -> datetime | None:
    latest: datetime | None = None
    for row in rows:
        candidate = str(row.get("last_ts") or row.get("ts") or "")
        if not candidate:
            continue
        try:
            parsed = parse_ts(candidate)
        except HourEventError:
            continue
        if latest is None or parsed > latest:
            latest = parsed
    return latest


def _format_ts(value: datetime | None) -> str:
    if value is None:
        return ""
    return value.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
