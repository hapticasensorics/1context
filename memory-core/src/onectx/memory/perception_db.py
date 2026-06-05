from __future__ import annotations

import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.storage import stable_id, utc_now


DEFAULT_LOCAL_USER_ID = "00000000-0000-0000-0000-000000000001"
DEFAULT_AGENT_SOURCES = ("codex", "claude")
SOURCE_IDS = {
    "10000000-0000-0000-0000-000000000001": "codex",
    "10000000-0000-0000-0000-000000000002": "claude",
}
SOURCE_SOURCE_IDS = {value: key for key, value in SOURCE_IDS.items()}


class PerceptionDBError(RuntimeError):
    """Raised when the Perception DB process protocol cannot be called."""


@dataclass(frozen=True)
class PerceptionProtocolCall:
    method: str
    status: str
    request_id: str
    result: dict[str, Any] | None = None
    error: dict[str, Any] | None = None
    duration_ms: int = 0

    def to_payload(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "method": self.method,
            "status": self.status,
            "request_id": self.request_id,
            "duration_ms": self.duration_ms,
        }
        if self.result is not None:
            payload["result"] = summarized_result(self.method, self.result)
        if self.error is not None:
            payload["error"] = self.error
        return payload


@dataclass(frozen=True)
class PerceptionSnapshot:
    status: str
    memoryd_bin: Path | None
    user_id: str
    sources: tuple[str, ...]
    window_days: int
    cursor_name: str
    ingest_calls: tuple[PerceptionProtocolCall, ...]
    query_call: PerceptionProtocolCall | None
    events: tuple[dict[str, Any], ...]
    sessions: tuple[dict[str, Any], ...]
    error: str = ""

    @property
    def event_count(self) -> int:
        return len(self.events)

    @property
    def session_count(self) -> int:
        return len(self.sessions)

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "status": self.status,
            "store": "perception_db",
            "memoryd_bin": format_path(self.memoryd_bin, root),
            "user_id": self.user_id,
            "sources": list(self.sources),
            "window_days": self.window_days,
            "cursor_name": self.cursor_name,
            "event_count": self.event_count,
            "session_count": self.session_count,
            "ingest_calls": [call.to_payload() for call in self.ingest_calls],
            "query_call": self.query_call.to_payload() if self.query_call else None,
        }
        if self.error:
            payload["error"] = self.error
        return payload


def load_perception_snapshot(
    system: MemorySystem,
    *,
    memoryd_bin: str | Path | None = None,
    sources: tuple[str, ...] = DEFAULT_AGENT_SOURCES,
    window_days: int = 30,
    ingest_ticks: int = 4,
    max_events: int = 5_000,
    max_lines: int = 250_000,
    query_limit: int = 2_400,
    timeout_seconds: int = 60,
    cursor_name: str = "",
) -> PerceptionSnapshot:
    resolved_sources = normalize_sources(sources)
    resolved_cursor_name = cursor_name or f"wiki_backfill_{max(1, window_days)}d_v1"
    resolved_memoryd = discover_memoryd_bin(system, memoryd_bin)
    if resolved_memoryd is None:
        return PerceptionSnapshot(
            status="unavailable",
            memoryd_bin=None,
            user_id=DEFAULT_LOCAL_USER_ID,
            sources=resolved_sources,
            window_days=max(1, window_days),
            cursor_name=resolved_cursor_name,
            ingest_calls=(),
            query_call=None,
            events=(),
            sessions=(),
            error="onecontext-memoryd executable was not found",
        )

    ingest_calls: list[PerceptionProtocolCall] = []
    for _ in range(max(0, ingest_ticks)):
        call = call_protocol(
            resolved_memoryd,
            "memory.ingestSources",
            {
                "user_id": DEFAULT_LOCAL_USER_ID,
                "sources": list(resolved_sources),
                "max_events": max(1, max_events),
                "max_lines": max(1, max_lines),
                "cursor_name": resolved_cursor_name,
                "session_profile": "hot_memory",
                "include_sensitive_text": False,
            },
            timeout_seconds=timeout_seconds,
        )
        ingest_calls.append(call)
        if call.status != "ok":
            break
        result = call.result or {}
        if result.get("ok") is False:
            break
        source_results = result.get("source_results") if isinstance(result.get("source_results"), list) else []
        read_count = sum(int(item.get("read_count") or 0) for item in source_results if isinstance(item, dict))
        reached_limit = any(source_reached_limit(item) for item in source_results if isinstance(item, dict))
        if read_count == 0 or not reached_limit:
            break

    start, end = window_bounds(window_days)
    query_call = call_protocol(
        resolved_memoryd,
        "memory.queryViewport",
        {
            "user_id": DEFAULT_LOCAL_USER_ID,
            "time": {"start": start, "end": end},
            "filters": {
                "source_ids": [SOURCE_SOURCE_IDS[source] for source in resolved_sources if source in SOURCE_SOURCE_IDS],
                "kinds": ["agent_message"],
                "roles": ["user", "assistant"],
            },
            "pagination": {"limit": max(1, query_limit)},
            "include": {
                "payload": True,
                "blob_descriptor": False,
                "source_record": True,
                "edges_count": False,
            },
            "explain": False,
        },
        timeout_seconds=timeout_seconds,
    )
    if query_call.status != "ok":
        return PerceptionSnapshot(
            status="failed",
            memoryd_bin=resolved_memoryd,
            user_id=DEFAULT_LOCAL_USER_ID,
            sources=resolved_sources,
            window_days=max(1, window_days),
            cursor_name=resolved_cursor_name,
            ingest_calls=tuple(ingest_calls),
            query_call=query_call,
            events=(),
            sessions=(),
            error=protocol_error_message(query_call),
        )

    objects = []
    result = query_call.result or {}
    if isinstance(result.get("objects"), list):
        objects = [item for item in result["objects"] if isinstance(item, dict)]
    events = tuple(perception_object_to_event(item) for item in objects)
    events = tuple(event for event in events if event)
    sessions = tuple(session_rows_from_events(events))
    ingest_error = first_ingest_error(ingest_calls)
    return PerceptionSnapshot(
        status="failed" if ingest_error else "ok",
        memoryd_bin=resolved_memoryd,
        user_id=DEFAULT_LOCAL_USER_ID,
        sources=resolved_sources,
        window_days=max(1, window_days),
        cursor_name=resolved_cursor_name,
        ingest_calls=tuple(ingest_calls),
        query_call=query_call,
        events=events,
        sessions=sessions,
        error=ingest_error,
    )


def call_protocol(memoryd_bin: Path, method: str, params: dict[str, Any], *, timeout_seconds: int) -> PerceptionProtocolCall:
    request_id = stable_id("perception_request", method, utc_now(), json.dumps(params, sort_keys=True, default=str))
    started = datetime.now(timezone.utc)
    request = {
        "schema_version": 1,
        "request_id": request_id,
        "method": method,
        "params": params,
    }
    try:
        completed = subprocess.run(
            [str(memoryd_bin), "protocol", method, "--request-json", "-"],
            input=json.dumps(request, sort_keys=True, default=str),
            text=True,
            capture_output=True,
            check=False,
            timeout=max(1, timeout_seconds),
        )
    except subprocess.TimeoutExpired as exc:
        return PerceptionProtocolCall(
            method=method,
            status="error",
            request_id=request_id,
            error={"code": "PROCESS_TIMEOUT", "message": f"timed out after {timeout_seconds}s", "stderr": str(exc.stderr or "")},
            duration_ms=elapsed_ms(started),
        )
    if completed.returncode != 0:
        return PerceptionProtocolCall(
            method=method,
            status="error",
            request_id=request_id,
            error={
                "code": "PROCESS_EXITED",
                "message": (completed.stderr or completed.stdout).strip() or f"onecontext-memoryd exited {completed.returncode}",
                "returncode": completed.returncode,
            },
            duration_ms=elapsed_ms(started),
        )
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        return PerceptionProtocolCall(
            method=method,
            status="error",
            request_id=request_id,
            error={"code": "INVALID_JSON", "message": str(exc), "stdout": completed.stdout[:1000]},
            duration_ms=elapsed_ms(started),
        )
    status = str(payload.get("status") or "")
    if status != "ok":
        error = payload.get("error") if isinstance(payload.get("error"), dict) else {"message": completed.stdout.strip()}
        return PerceptionProtocolCall(method=method, status="error", request_id=request_id, error=error, duration_ms=elapsed_ms(started))
    result = payload.get("result") if isinstance(payload.get("result"), dict) else {}
    return PerceptionProtocolCall(method=method, status="ok", request_id=request_id, result=result, duration_ms=elapsed_ms(started))


def discover_memoryd_bin(system: MemorySystem, explicit: str | Path | None = None) -> Path | None:
    candidates: list[Path] = []
    if explicit:
        candidates.append(Path(explicit).expanduser())
    env_bin = os.environ.get("ONECONTEXT_MEMORYD_BIN")
    if env_bin:
        candidates.append(Path(env_bin).expanduser())
    which = shutil.which("onecontext-memoryd")
    if which:
        candidates.append(Path(which))
    repo_root = system.root.parent
    candidates.extend(
        [
            repo_root / "target" / "release" / "onecontext-memoryd",
            repo_root / "target" / "debug" / "onecontext-memoryd",
            Path("/Applications/1Context Dev.app/Contents/MacOS/onecontext-memoryd"),
            Path("/Applications/1Context.app/Contents/MacOS/onecontext-memoryd"),
        ]
    )
    for candidate in candidates:
        resolved = candidate.expanduser()
        if resolved.is_file() and os.access(resolved, os.X_OK):
            return resolved.resolve()
    return None


def perception_object_to_event(row: dict[str, Any]) -> dict[str, Any]:
    payload = row.get("payload") if isinstance(row.get("payload"), dict) else {}
    role = str(row.get("role") or payload.get("role") or "").strip()
    text = clean_text(row.get("text_value") or row.get("display_text_preview") or payload.get("text") or "")
    if role not in {"user", "assistant"} or len(text) < 1:
        return {}
    source = str(payload.get("agent_source") or SOURCE_IDS.get(str(row.get("source_id") or ""), "") or "agent")
    raw_ref = payload.get("raw_ref") if isinstance(payload.get("raw_ref"), dict) else {}
    cwd = str(payload.get("cwd") or "")
    source_uri = str(payload.get("source_uri") or raw_ref.get("source_uri") or "")
    return {
        "event_id": str(row.get("object_id") or row.get("source_record_id") or ""),
        "source_record_id": str(row.get("source_record_id") or ""),
        "source_record_key": str(row.get("source_record_key") or ""),
        "session_id": str(payload.get("session_id") or row.get("series_key") or ""),
        "ts": str(row.get("event_start") or row.get("event_time") or row.get("captured_at") or ""),
        "event": str(row.get("kind") or row.get("object_kind") or "agent_message"),
        "source": source,
        "kind": role,
        "actor": role,
        "subject": str(payload.get("turn_id") or ""),
        "cwd": cwd,
        "source_uri": source_uri,
        "project_key": str(payload.get("project_key") or ""),
        "char_count": len(text),
        "text": text,
        "payload_json": json.dumps(payload, sort_keys=True, default=str),
    }


def session_rows_from_events(events: tuple[dict[str, Any], ...]) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for event in events:
        key = (str(event.get("source") or ""), str(event.get("session_id") or ""))
        grouped.setdefault(key, []).append(event)
    sessions: list[dict[str, Any]] = []
    for (source, session_id), rows in grouped.items():
        sorted_rows = sorted(rows, key=lambda row: str(row.get("ts") or ""))
        first = sorted_rows[0]
        last = sorted_rows[-1]
        sessions.append(
            {
                "session_id": session_id,
                "source": source,
                "cwd": str(last.get("cwd") or ""),
                "source_uri": str(last.get("source_uri") or ""),
                "first_ts": str(first.get("ts") or ""),
                "last_ts": str(last.get("ts") or ""),
                "event_count": len(sorted_rows),
                "metadata_json": json.dumps({"store": "perception_db"}, sort_keys=True),
            }
        )
    return sorted(sessions, key=lambda row: str(row.get("last_ts") or ""))


def source_reached_limit(source_result: dict[str, Any]) -> bool:
    report = source_result.get("adapter_report")
    if not isinstance(report, dict):
        return False
    return bool(report.get("reached_event_limit") or report.get("reached_line_limit"))


def window_bounds(days: int) -> tuple[str, str]:
    end = datetime.now(timezone.utc)
    start = end - timedelta(days=max(1, days))
    return (
        start.isoformat(timespec="milliseconds").replace("+00:00", "Z"),
        end.isoformat(timespec="milliseconds").replace("+00:00", "Z"),
    )


def normalize_sources(sources: tuple[str, ...]) -> tuple[str, ...]:
    normalized = []
    for source in sources:
        value = source.strip()
        if value and value not in normalized:
            normalized.append(value)
    return tuple(normalized or DEFAULT_AGENT_SOURCES)


def protocol_error_message(call: PerceptionProtocolCall | None) -> str:
    if call is None or call.error is None:
        return ""
    return str(call.error.get("message") or call.error.get("code") or call.error)


def first_ingest_error(calls: list[PerceptionProtocolCall]) -> str:
    for call in calls:
        if call.status != "ok":
            return protocol_error_message(call)
        result = call.result or {}
        if result.get("ok") is False:
            source_results = result.get("source_results") if isinstance(result.get("source_results"), list) else []
            for item in source_results:
                if not isinstance(item, dict):
                    continue
                status = str(item.get("status") or "")
                if status and status != "ok":
                    source = item.get("source") or "source"
                    error = item.get("error") or item.get("error_code") or status
                    return f"{source} ingest {status}: {error}"
            return "Perception DB ingestSources returned ok=false"
    return ""


def summarized_result(method: str, result: dict[str, Any]) -> dict[str, Any]:
    if method == "memory.queryViewport":
        objects = result.get("objects") if isinstance(result.get("objects"), list) else []
        return {
            "ok": bool(result.get("ok", True)),
            "object_count": len(objects),
            "next_cursor": result.get("next_cursor"),
        }
    return result


def clean_text(value: Any) -> str:
    return " ".join(str(value or "").split())


def elapsed_ms(started: datetime) -> int:
    return int((datetime.now(timezone.utc) - started).total_seconds() * 1000)


def format_path(path: Path | None, root: Path | None) -> str | None:
    if path is None:
        return None
    if root:
        try:
            return str(path.relative_to(root))
        except ValueError:
            pass
    return str(path)
