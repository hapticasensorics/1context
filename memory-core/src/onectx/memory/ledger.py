from __future__ import annotations

import json
import threading
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from onectx.storage import LakeStore, storage_dir_path

try:
    import fcntl
except ImportError:  # pragma: no cover - non-Unix fallback
    fcntl = None


_LEDGER_APPEND_LOCK = threading.RLock()


@dataclass(frozen=True)
class Ledger:
    path: Path
    storage_path: Path | None = None

    def append(self, event: str, **payload: Any) -> dict[str, Any]:
        with _LEDGER_APPEND_LOCK:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            record = {
                "ts": utc_now(),
                "event": event,
                **payload,
            }
            with self.path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(record, sort_keys=True, default=str) + "\n")
            storage_path = self.storage_path or storage_dir_path(runtime_dir_for_ledger_path(self.path))
            LakeStore(storage_path).append_event(
                event,
                ts=record["ts"],
                source="runtime-ledger",
                actor=str(record.get("agent_id") or record.get("hired_agent_uuid") or ""),
                subject=str(record.get("plugin_id") or ""),
                hired_agent_uuid=str(record.get("hired_agent_uuid") or ""),
                run_id=str(record.get("run_id") or ""),
                text=str(record.get("summary") or ""),
                payload=record,
            )
            return record

    def read(self, *, limit: int | None = None) -> list[dict[str, Any]]:
        if not self.path.exists():
            return []
        rows: list[dict[str, Any]] = []
        with self.path.open("r", encoding="utf-8") as handle:
            for line in handle:
                text = line.strip()
                if text:
                    rows.append(json.loads(text))
        return rows[-limit:] if limit else rows


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def ledger_events_path(runtime_dir: Path) -> Path:
    return runtime_dir / "ledger" / "events.jsonl"


def ledger_lock_path(runtime_dir: Path) -> Path:
    return runtime_dir / "ledger" / "ledger.lock"


def runtime_dir_for_ledger_path(path: Path) -> Path:
    if path.parent.name == "ledger":
        return path.parent.parent
    return path.parent


@contextmanager
def exclusive_file_lock(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+", encoding="utf-8") as handle:
        if fcntl is not None:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            if fcntl is not None:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
