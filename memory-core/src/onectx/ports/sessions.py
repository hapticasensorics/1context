from __future__ import annotations

import base64
import hashlib
import json
import re
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Iterable

from onectx.storage import LakeStore, stable_id

from . import PortDefinition, PortError, resolve_port_files
from .session_extract import (
    clamp_text,
    default_session_id,
    file_sha256,
    hash_event,
    parse_row,
    sha256,
    source_for_adapter,
)


_BASE64_IMAGE_RE = re.compile(r"data:image/(\w+);base64,([A-Za-z0-9+/=]+)")
_WINDOW_RE = re.compile(r"^(\d+)([mhdw])$")
_BATCH_SIZE = 10000


@dataclass
class SessionStats:
    session_id: str
    source: str
    cwd: str = ""
    first_ts: str = ""
    last_ts: str = ""
    event_count: int = 0
    files: set[str] = field(default_factory=set)

    def observe(self, *, ts: str, cwd: str, path: Path) -> None:
        self.event_count += 1
        self.files.add(str(path))
        if cwd and not self.cwd:
            self.cwd = cwd
        if ts and (not self.first_ts or ts < self.first_ts):
            self.first_ts = ts
        if ts and (not self.last_ts or ts > self.last_ts):
            self.last_ts = ts

    def merge(self, other: "SessionStats") -> None:
        self.event_count += other.event_count
        self.files.update(other.files)
        if other.cwd and not self.cwd:
            self.cwd = other.cwd
        if other.first_ts and (not self.first_ts or other.first_ts < self.first_ts):
            self.first_ts = other.first_ts
        if other.last_ts and (not self.last_ts or other.last_ts > self.last_ts):
            self.last_ts = other.last_ts


@dataclass
class SessionImportResult:
    port_id: str
    adapter: str
    files_seen: int = 0
    files_changed: int = 0
    lines_scanned: int = 0
    events_imported: int = 0
    events_skipped: int = 0
    duplicate_events: int = 0
    sessions_imported: int = 0
    artifacts_imported: int = 0
    partial_lines: int = 0
    limited: bool = False
    skipped: bool = False
    reason: str = ""

    def to_payload(self) -> dict[str, Any]:
        return {
            "port_id": self.port_id,
            "adapter": self.adapter,
            "files_seen": self.files_seen,
            "files_changed": self.files_changed,
            "lines_scanned": self.lines_scanned,
            "events_imported": self.events_imported,
            "events_skipped": self.events_skipped,
            "duplicate_events": self.duplicate_events,
            "sessions_imported": self.sessions_imported,
            "artifacts_imported": self.artifacts_imported,
            "partial_lines": self.partial_lines,
            "limited": self.limited,
            "skipped": self.skipped,
            "reason": self.reason,
        }


def import_session_port(
    *,
    root: Path,
    port: PortDefinition,
    store: LakeStore,
    cursors: Any,
    source_root: Path | None = None,
    include_disabled: bool = False,
) -> SessionImportResult:
    result = SessionImportResult(port_id=port.id, adapter=port.adapter)
    if not port.enabled and not include_disabled and not source_root:
        result.skipped = True
        result.reason = "disabled"
        return result
    if port.adapter not in {"codex_rollout_jsonl", "claude_code_jsonl"}:
        result.skipped = True
        result.reason = f"unsupported adapter {port.adapter!r}"
        return result

    # `since` is a source import horizon, not a storage-retention policy.
    # It gates which native transcript rows/files are eligible to import on
    # this tick. It must never be used to prune already-imported lakestore rows.
    cutoff = None if source_root else cutoff_for_port(port)
    files = resolve_port_files(root, port, source_root=source_root)
    result.files_seen = len(files)
    files = files_with_new_bytes(port=port, files=files, cursors=cursors, cutoff=cutoff)
    if not files:
        return result

    existing_event_ids = existing_values(store, "events", "event_id")
    existing_event_hashes = existing_values(store, "events", "hash")
    existing_artifact_ids = existing_values(store, "artifacts", "artifact_id")
    session_stats: dict[str, SessionStats] = {}
    remaining_events = positive_limit(port.max_events_per_tick)
    remaining_lines = positive_limit(port.max_lines_per_tick)

    for path in files:
        if remaining_events is not None and remaining_events <= 0:
            result.limited = True
            break
        if remaining_lines is not None and remaining_lines <= 0:
            result.limited = True
            break
        if cutoff and file_is_before_cutoff(path, cutoff):
            continue
        file_result, sessions = import_jsonl_file(
            port=port,
            path=path,
            store=store,
            cursors=cursors,
            cutoff=cutoff,
            existing_event_ids=existing_event_ids,
            existing_event_hashes=existing_event_hashes,
            existing_artifact_ids=existing_artifact_ids,
            max_events=remaining_events,
            max_lines=remaining_lines,
        )
        merge_results(result, file_result)
        if remaining_events is not None:
            remaining_events -= file_result.events_imported
        if remaining_lines is not None:
            remaining_lines -= file_result.lines_scanned
        for stats in sessions:
            existing = session_stats.get(stats.session_id)
            if existing:
                existing.merge(stats)
            else:
                session_stats[stats.session_id] = stats
        if file_result.files_changed and hasattr(cursors, "save"):
            cursors.save()
        if file_result.limited:
            result.limited = True
            break

    if session_stats:
        refresh_session_summaries(store, session_stats.values(), port)
        result.sessions_imported += len(session_stats)

    return result


def files_with_new_bytes(
    *,
    port: PortDefinition,
    files: list[Path],
    cursors: Any,
    cutoff: datetime | None,
) -> list[Path]:
    candidates: list[Path] = []
    for path in files:
        if cutoff and file_is_before_cutoff(path, cutoff):
            continue
        key = f"{port.id}:{path}"
        cursor = cursors.get(key)
        offset = cursor_offset(cursor)
        try:
            size = path.stat().st_size
        except OSError:
            continue
        if size != offset:
            candidates.append(path)
    return candidates


def import_jsonl_file(
    *,
    port: PortDefinition,
    path: Path,
    store: LakeStore,
    cursors: Any,
    cutoff: datetime | None,
    existing_event_ids: set[str],
    existing_event_hashes: set[str],
    existing_artifact_ids: set[str],
    max_events: int | None = None,
    max_lines: int | None = None,
) -> tuple[SessionImportResult, list[SessionStats]]:
    result = SessionImportResult(port_id=port.id, adapter=port.adapter)
    key = f"{port.id}:{path}"
    cursor = cursors.get(key)
    offset = cursor_offset(cursor)
    try:
        size = path.stat().st_size
    except OSError:
        size = 0
    if size < offset:
        offset = 0
    state = parser_state_from_cursor(cursor, port, path, offset)
    sessions: dict[str, SessionStats] = {}
    saw_new_line = False
    last_good_offset = offset
    event_rows: list[dict[str, Any]] = []
    artifact_rows: list[dict[str, Any]] = []

    for raw, raw_line, next_offset in iter_jsonl(path, offset):
        saw_new_line = True
        last_good_offset = next_offset
        result.lines_scanned += 1
        if raw is None:
            result.events_skipped += 1
            continue
        parsed = parse_row(port.adapter, raw, path=path, state=state)
        if parsed:
            if cutoff and not timestamp_on_or_after(parsed.ts, cutoff):
                result.events_skipped += 1
            else:
                event_row, image_artifact_rows = build_parsed_event_rows(
                    store=store,
                    port=port,
                    path=path,
                    raw_line=raw_line,
                    parsed=parsed,
                    existing_event_ids=existing_event_ids,
                    existing_event_hashes=existing_event_hashes,
                    existing_artifact_ids=existing_artifact_ids,
                )
                if event_row:
                    event_rows.append(event_row)
                    artifact_rows.extend(image_artifact_rows)
                    result.events_imported += 1
                    result.artifacts_imported += len(image_artifact_rows)
                    stats = sessions.setdefault(
                        parsed.session_id,
                        SessionStats(session_id=parsed.session_id, source=parsed.source),
                    )
                    stats.observe(ts=parsed.ts, cwd=parsed.cwd, path=path)
                else:
                    result.duplicate_events += 1

        if max_events is not None and result.events_imported >= max_events:
            result.limited = True
            break
        if max_lines is not None and result.lines_scanned >= max_lines:
            result.limited = True
            break

    if saw_new_line:
        result.files_changed = 1
        if result.events_imported or result.artifacts_imported:
            log_artifact = build_log_artifact_row(
                store=store,
                port=port,
                path=path,
                existing_artifact_ids=existing_artifact_ids,
            )
            if log_artifact:
                artifact_rows.append(log_artifact)
                result.artifacts_imported += 1
        append_in_batches(store, "events", event_rows)
        append_in_batches(store, "artifacts", artifact_rows)
        write_cursor(cursors, key, port, path, last_good_offset, state)

    if path.stat().st_size > last_good_offset:
        result.partial_lines += 1

    return result, list(sessions.values())


def build_parsed_event_rows(
    *,
    store: LakeStore,
    port: PortDefinition,
    path: Path,
    raw_line: bytes,
    parsed: Any,
    existing_event_ids: set[str],
    existing_event_hashes: set[str],
    existing_artifact_ids: set[str],
) -> tuple[dict[str, Any] | None, list[dict[str, Any]]]:
    text = clamp_text(parsed.text)
    event_hash = hash_event(parsed.session_id, parsed.ts, parsed.kind, text)
    event_id = f"source_event_{event_hash}"
    if event_id in existing_event_ids or event_hash in existing_event_hashes:
        return None, []
    image_artifact_ids, image_artifact_rows = materialize_inline_images(
        store=store,
        port=port,
        path=path,
        raw_line=raw_line,
        parsed=parsed,
        event_id=event_id,
        existing_artifact_ids=existing_artifact_ids,
    )

    event_row = store.event_row(
        parsed.event,
        event_id=event_id,
        hash=event_hash,
        session_id=parsed.session_id,
        ts=parsed.ts,
        source=parsed.source,
        kind=parsed.kind,
        actor=parsed.kind,
        subject=parsed.session_id,
        cwd=parsed.cwd,
        char_count=len(text),
        text=text,
        payload={
            **parsed.payload,
            "port_id": port.id,
            "adapter": port.adapter,
            "file": str(path),
            "raw_line_hash": sha256(raw_line),
            "image_artifact_ids": image_artifact_ids,
        },
    )
    existing_event_ids.add(event_id)
    existing_event_hashes.add(event_hash)
    return event_row, image_artifact_rows


def materialize_inline_images(
    *,
    store: LakeStore,
    port: PortDefinition,
    path: Path,
    raw_line: bytes,
    parsed: Any,
    event_id: str,
    existing_artifact_ids: set[str],
) -> tuple[list[str], list[dict[str, Any]]]:
    if b"data:image/" not in raw_line:
        return [], []
    text = raw_line.decode("utf-8", errors="ignore")

    artifact_ids: list[str] = []
    artifact_rows: list[dict[str, Any]] = []
    seen_hashes: set[str] = set()
    output_dir = store.path.parent / "artifacts" / "session-images"
    output_dir.mkdir(parents=True, exist_ok=True)

    for match in _BASE64_IMAGE_RE.finditer(text):
        fmt = match.group(1).lower()
        try:
            raw = base64.b64decode(match.group(2), validate=False)
        except Exception:
            continue
        digest = hashlib.sha256(raw).hexdigest()
        if digest in seen_hashes:
            continue
        seen_hashes.add(digest)

        image_path = output_dir / f"{digest[:12]}.{fmt}"
        if not image_path.exists():
            image_path.write_bytes(raw)
        artifact_id = stable_id("artifact", "session_inline_image", digest)
        artifact_ids.append(artifact_id)
        if artifact_id in existing_artifact_ids:
            continue
        artifact_rows.append(
            store.artifact_row(
                "session_inline_image",
                artifact_id=artifact_id,
                uri=image_path.as_uri(),
                path=str(image_path),
                content_type=f"image/{fmt}",
                content_hash=digest,
                bytes=len(raw),
                source=parsed.source,
                state="materialized",
                text=f"Inline session image extracted from {parsed.source} {parsed.kind} event.",
                metadata={
                    "port_id": port.id,
                    "adapter": port.adapter,
                    "file": str(path),
                    "session_id": parsed.session_id,
                    "event_id": event_id,
                },
            )
        )
        existing_artifact_ids.add(artifact_id)

    return artifact_ids, artifact_rows


def build_log_artifact_row(
    *,
    store: LakeStore,
    port: PortDefinition,
    path: Path,
    existing_artifact_ids: set[str],
) -> dict[str, Any] | None:
    content_hash = file_sha256(path)
    artifact_id = stable_id("artifact", "session_log_file", port.adapter, str(path), content_hash)
    if artifact_id in existing_artifact_ids:
        return None
    row = store.artifact_row(
        "session_log_file",
        artifact_id=artifact_id,
        uri=path.as_uri(),
        path=str(path),
        content_type="application/jsonl",
        content_hash=content_hash,
        bytes=path.stat().st_size,
        source=source_for_adapter(port.adapter),
        state="observed",
        text=f"Session log observed by port {port.id}.",
        metadata={"port_id": port.id, "adapter": port.adapter},
    )
    existing_artifact_ids.add(artifact_id)
    return row


def append_in_batches(store: LakeStore, table_name: str, rows: list[dict[str, Any]]) -> None:
    for index in range(0, len(rows), _BATCH_SIZE):
        batch = rows[index : index + _BATCH_SIZE]
        if table_name == "events":
            store.replace_rows(table_name, "event_id", batch)
        elif table_name == "artifacts":
            store.replace_rows(table_name, "artifact_id", batch)
        else:
            store.append_rows(table_name, batch)


def parser_state_from_cursor(cursor: dict[str, Any], port: PortDefinition, path: Path, offset: int) -> dict[str, str]:
    raw_state = cursor.get("parser_state") if isinstance(cursor.get("parser_state"), dict) else {}
    state = {
        "session_id": str(raw_state.get("session_id") or default_session_id(port.adapter, path)),
        "cwd": str(raw_state.get("cwd") or ""),
    }
    if raw_state or offset <= 0:
        return state
    return scan_parser_state_to_offset(port, path, offset, state)


def scan_parser_state_to_offset(port: PortDefinition, path: Path, offset: int, state: dict[str, str]) -> dict[str, str]:
    # Legacy cursor migration: cursors written before parser_state existed
    # can resume in the middle of a file. Reconstruct just the cheap state
    # fields needed for correct attribution; do not run full text shaping.
    try:
        with path.open("rb") as handle:
            while handle.tell() < offset:
                line = handle.readline()
                if not line:
                    break
                if handle.tell() > offset:
                    break
                try:
                    row = json.loads(line)
                except json.JSONDecodeError:
                    continue
                observe_parser_state(port.adapter, row, state)
    except OSError:
        pass
    return state


def observe_parser_state(adapter: str, row: dict[str, Any], state: dict[str, str]) -> None:
    if adapter == "codex_rollout_jsonl":
        payload = row.get("payload") if isinstance(row.get("payload"), dict) else {}
        if row.get("type") == "session_meta":
            if payload.get("id"):
                state["session_id"] = str(payload["id"])
            if payload.get("cwd"):
                state["cwd"] = str(payload["cwd"])
        return

    if adapter == "claude_code_jsonl":
        if row.get("type") == "progress":
            inner = (row.get("data") or {}).get("message")
            if isinstance(inner, dict):
                observe_parser_state(adapter, inner, state)
        if row.get("sessionId"):
            state["session_id"] = str(row["sessionId"])
        if row.get("cwd"):
            state["cwd"] = str(row["cwd"])


def write_cursor(cursors: Any, key: str, port: PortDefinition, path: Path, offset: int, state: dict[str, str]) -> None:
    try:
        stat = path.stat()
        size = stat.st_size
        mtime_ns = stat.st_mtime_ns
    except OSError:
        size = 0
        mtime_ns = 0
    cursors.set(
        key,
        {
            "offset": offset,
            "path": str(path),
            "port_id": port.id,
            "adapter": port.adapter,
            "size": size,
            "mtime_ns": mtime_ns,
            "parser_state": {
                "session_id": state.get("session_id", ""),
                "cwd": state.get("cwd", ""),
            },
        },
    )


def refresh_session_summaries(store: LakeStore, stats_list: Iterable[SessionStats], port: PortDefinition) -> None:
    stats_by_session = {stats.session_id: stats for stats in stats_list}
    if not stats_by_session:
        return
    event_summaries = session_event_summaries(store, set(stats_by_session))

    existing_rows = {
        str(row.get("session_id") or ""): row
        for row in store.rows("sessions", limit=0)
        if row.get("session_id")
    }
    rows: list[dict[str, Any]] = []
    for session_id, stats in stats_by_session.items():
        summary = summarize_session_stats(stats, port, existing_rows.get(session_id))
        actual = event_summaries.get(session_id)
        if actual:
            summary.update(
                {
                    "source": actual.get("source") or summary["source"],
                    "cwd": actual.get("cwd") or summary["cwd"],
                    "first_ts": actual.get("first_ts") or summary["first_ts"],
                    "last_ts": actual.get("last_ts") or summary["last_ts"],
                    "event_count": actual.get("event_count", summary["event_count"]),
                }
            )
        rows.append(
            store.session_row(
                session_id,
                source=summary["source"],
                cwd=summary["cwd"],
                first_ts=summary["first_ts"],
                last_ts=summary["last_ts"],
                event_count=summary["event_count"],
                metadata=summary["metadata"],
            )
        )
    store.replace_rows("sessions", "session_id", rows)


def session_event_summaries(store: LakeStore, session_ids: set[str]) -> dict[str, dict[str, Any]]:
    summaries: dict[str, dict[str, Any]] = {}
    for row in store.rows("events", limit=0):
        session_id = str(row.get("session_id") or "")
        if session_id not in session_ids:
            continue
        item = summaries.setdefault(
            session_id,
            {
                "source": str(row.get("source") or ""),
                "cwd": str(row.get("cwd") or ""),
                "first_ts": "",
                "last_ts": "",
                "event_count": 0,
            },
        )
        ts = str(row.get("ts") or "")
        item["event_count"] += 1
        if not item["source"] and row.get("source"):
            item["source"] = str(row.get("source") or "")
        if not item["cwd"] and row.get("cwd"):
            item["cwd"] = str(row.get("cwd") or "")
        if ts and (not item["first_ts"] or ts < item["first_ts"]):
            item["first_ts"] = ts
        if ts and (not item["last_ts"] or ts > item["last_ts"]):
            item["last_ts"] = ts
    return summaries


def summarize_session_stats(stats: SessionStats, port: PortDefinition, existing: dict[str, Any] | None) -> dict[str, Any]:
    files: set[str] = set(stats.files)
    existing_metadata: dict[str, Any] = {}
    if existing:
        try:
            existing_metadata = json.loads(existing.get("metadata_json") or "{}")
        except Exception:
            existing_metadata = {}
        files.update(str(item) for item in existing_metadata.get("files", []) if item)

    existing_first_ts = str(existing.get("first_ts") or "") if existing else ""
    existing_last_ts = str(existing.get("last_ts") or "") if existing else ""
    existing_count = int(existing.get("event_count") or 0) if existing else 0
    first_ts = min(ts for ts in (existing_first_ts, stats.first_ts) if ts) if (existing_first_ts or stats.first_ts) else ""
    last_ts = max(ts for ts in (existing_last_ts, stats.last_ts) if ts) if (existing_last_ts or stats.last_ts) else ""
    return {
        "source": str(existing.get("source") or stats.source) if existing else stats.source,
        "cwd": (str(existing.get("cwd") or "") if existing else "") or stats.cwd,
        "first_ts": first_ts,
        "last_ts": last_ts,
        "event_count": existing_count + stats.event_count,
        "metadata": {
            "port_id": port.id,
            "adapter": port.adapter,
            "files": sorted(files),
            "summary_scope": "whole_session",
        },
    }


def iter_jsonl(path: Path, offset: int) -> Iterable[tuple[dict[str, Any] | None, bytes, int]]:
    with path.open("rb") as handle:
        handle.seek(offset)
        while True:
            line = handle.readline()
            if not line:
                return
            next_offset = handle.tell()
            try:
                yield json.loads(line), line, next_offset
            except json.JSONDecodeError:
                if not line.endswith(b"\n"):
                    return
                yield None, line, next_offset


def cutoff_for_port(port: PortDefinition) -> datetime | None:
    spec = (port.since or "").strip().lower()
    if not spec or spec in {"all", "none", "off"}:
        return None
    if spec.startswith("since "):
        spec = spec.removeprefix("since ").strip()

    absolute = parse_timestamp(spec)
    if absolute:
        return absolute

    match = _WINDOW_RE.fullmatch(spec)
    if not match:
        raise PortError(
            f"port {port.id!r} has invalid since value {port.since!r}; "
            'use "30d", "12h", "2w", an ISO timestamp, or "all"'
        )
    amount = int(match.group(1))
    unit = match.group(2)
    delta_by_unit = {
        "m": timedelta(minutes=amount),
        "h": timedelta(hours=amount),
        "d": timedelta(days=amount),
        "w": timedelta(weeks=amount),
    }
    return datetime.now(timezone.utc) - delta_by_unit[unit]


def parse_timestamp(value: str) -> datetime | None:
    if not value:
        return None
    text = value.strip()
    if text.endswith("z"):
        text = f"{text[:-1]}Z"
    if text.endswith("Z"):
        text = f"{text[:-1]}+00:00"
    try:
        dt = datetime.fromisoformat(text)
    except ValueError:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def timestamp_on_or_after(value: str, cutoff: datetime) -> bool:
    ts = parse_timestamp(value)
    return True if ts is None else ts >= cutoff


def file_is_before_cutoff(path: Path, cutoff: datetime) -> bool:
    try:
        mtime = datetime.fromtimestamp(path.stat().st_mtime, tz=timezone.utc)
    except OSError:
        return False
    return mtime < cutoff


def existing_values(store: LakeStore, table_name: str, field_name: str) -> set[str]:
    return store.column_values(table_name, field_name)


def positive_limit(value: int) -> int | None:
    return value if value > 0 else None


def cursor_offset(cursor: dict[str, Any]) -> int:
    try:
        return int(cursor.get("offset", 0) or 0)
    except (TypeError, ValueError):
        return 0


def merge_results(total: SessionImportResult, item: SessionImportResult) -> None:
    total.files_changed += item.files_changed
    total.lines_scanned += item.lines_scanned
    total.events_imported += item.events_imported
    total.events_skipped += item.events_skipped
    total.duplicate_events += item.duplicate_events
    total.partial_lines += item.partial_lines
    total.artifacts_imported += item.artifacts_imported
    total.limited = total.limited or item.limited
