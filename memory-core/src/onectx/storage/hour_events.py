from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Iterable

from . import LakeStore


class HourEventError(RuntimeError):
    """Raised when a bounded event window cannot be queried."""


@dataclass(frozen=True)
class HourEvent:
    event_id: str
    hash: str
    session_id: str
    ts: str
    event: str
    source: str
    kind: str
    actor: str
    cwd: str
    text: str
    payload: dict[str, Any]


def events_between(
    store: LakeStore,
    *,
    start: str,
    end: str,
    sources: Iterable[str] = (),
) -> list[HourEvent]:
    """Return normalized session events in [start, end), ordered by timestamp.

    This intentionally starts with the simple LanceDB snapshot API already used
    elsewhere in the repo. The helper gives renderers one boring place to grow
    better filtering/query pushdown later.
    """

    start_dt = parse_ts(start)
    end_dt = parse_ts(end)
    if end_dt <= start_dt:
        raise HourEventError(f"end must be after start: {start!r} -> {end!r}")

    allowed_sources = {normalize_source(source) for source in sources if str(source).strip()}
    result: list[HourEvent] = []
    for row in store.rows("events", limit=0):
        row_ts = str(row.get("ts") or "")
        if not row_ts:
            continue
        try:
            row_dt = parse_ts(row_ts)
        except HourEventError:
            continue
        if row_dt < start_dt or row_dt >= end_dt:
            continue
        source = normalize_source(str(row.get("source") or ""))
        if allowed_sources and source not in allowed_sources:
            continue
        event_name = str(row.get("event") or "")
        if not event_name.startswith("session."):
            continue
        result.append(hour_event_from_row(row, source=source))

    return sorted(result, key=lambda item: (item.ts, item.source, item.session_id, item.event_id))


def hour_event_from_row(row: dict[str, Any], *, source: str) -> HourEvent:
    payload = parse_payload(row.get("payload_json"))
    return HourEvent(
        event_id=str(row.get("event_id") or ""),
        hash=str(row.get("hash") or ""),
        session_id=str(row.get("session_id") or ""),
        ts=str(row.get("ts") or ""),
        event=str(row.get("event") or ""),
        source=source,
        kind=str(row.get("kind") or ""),
        actor=str(row.get("actor") or ""),
        cwd=str(row.get("cwd") or ""),
        text=str(row.get("text") or ""),
        payload=payload,
    )


def parse_payload(value: Any) -> dict[str, Any]:
    if not value:
        return {}
    if isinstance(value, dict):
        return dict(value)
    if not isinstance(value, str):
        return {}
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def normalize_source(value: str) -> str:
    normalized = value.strip().lower().replace("_", "-")
    if normalized in {"claude", "claude-code", "claude_code_jsonl"}:
        return "claude-code"
    if normalized in {"codex", "codex-rollout", "codex_rollout_jsonl"}:
        return "codex"
    return normalized


def parse_ts(value: str) -> datetime:
    text = value.strip()
    if not text:
        raise HourEventError("timestamp is empty")
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError as exc:
        raise HourEventError(f"invalid timestamp {value!r}") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def format_ts(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
