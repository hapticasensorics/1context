from __future__ import annotations

import hashlib
import json
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

try:
    import fcntl
except ImportError:  # pragma: no cover - non-Unix fallback
    fcntl = None


QUEUE_VERSION = "0.1"
DEFAULT_QUEUE_FILENAME = "state_machine_work_queue.json"

STATUS_QUEUED = "queued"
STATUS_RUNNING = "running"
STATUS_NEEDS_RETRY = "needs_retry"
STATUS_DONE = "done"
STATUS_FAILED = "failed"

ACTIVE_STATUSES = {STATUS_QUEUED, STATUS_RUNNING, STATUS_NEEDS_RETRY}
TERMINAL_STATUSES = {STATUS_DONE, STATUS_FAILED}
VALID_STATUSES = ACTIVE_STATUSES | TERMINAL_STATUSES


class StateMachineQueueError(RuntimeError):
    """Raised when a persisted state-machine work item cannot be updated."""


@dataclass(frozen=True)
class WorkItem:
    queue_id: str
    machine: str
    scope: str
    key: str
    event: str
    source_state: str = ""
    target_state: str = ""
    status: str = STATUS_QUEUED
    attempts: int = 0
    max_attempts: int = 3
    retryable: bool = True
    scheduled_at: str = ""
    created_at: str = ""
    updated_at: str = ""
    payload: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_payload(cls, payload: dict[str, Any]) -> "WorkItem":
        status = str(payload.get("status") or STATUS_QUEUED)
        if status not in VALID_STATUSES:
            raise StateMachineQueueError(f"unknown state-machine queue status {status!r}")
        attempts = int(payload.get("attempts") or 0)
        max_attempts = max(1, int(payload.get("max_attempts") or 1))
        return cls(
            queue_id=str(payload["queue_id"]),
            machine=str(payload["machine"]),
            scope=str(payload["scope"]),
            key=str(payload["key"]),
            event=str(payload["event"]),
            source_state=str(payload.get("source_state") or ""),
            target_state=str(payload.get("target_state") or ""),
            status=status,
            attempts=max(0, attempts),
            max_attempts=max_attempts,
            retryable=bool(payload.get("retryable", True)),
            scheduled_at=str(payload.get("scheduled_at") or ""),
            created_at=str(payload.get("created_at") or ""),
            updated_at=str(payload.get("updated_at") or ""),
            payload=_object_payload(payload.get("payload")),
        )

    def to_payload(self) -> dict[str, Any]:
        return {
            "queue_id": self.queue_id,
            "machine": self.machine,
            "scope": self.scope,
            "key": self.key,
            "event": self.event,
            "source_state": self.source_state,
            "target_state": self.target_state,
            "status": self.status,
            "attempts": self.attempts,
            "max_attempts": self.max_attempts,
            "retryable": self.retryable,
            "scheduled_at": self.scheduled_at,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "payload": _json_safe(self.payload),
        }

    @property
    def attempts_remaining(self) -> int:
        return max(0, self.max_attempts - self.attempts)


@dataclass
class StateMachineWorkQueue:
    path: Path
    items: dict[str, WorkItem] = field(default_factory=dict)

    @classmethod
    def load(cls, path: Path) -> "StateMachineWorkQueue":
        queue_path = queue_file_path(path)
        data = _read_queue_file(queue_path)
        return cls(
            path=queue_path,
            items={
                item.queue_id: item
                for item in (WorkItem.from_payload(raw) for raw in data.get("items", []))
            },
        )

    def reload(self) -> None:
        self.items = StateMachineWorkQueue.load(self.path).items

    def enqueue(
        self,
        *,
        machine: str,
        scope: str,
        key: str,
        event: str,
        source_state: str = "",
        target_state: str = "",
        queue_id: str = "",
        payload: dict[str, Any] | None = None,
        max_attempts: int = 3,
        retryable: bool = True,
        scheduled_at: str = "",
        now: str | None = None,
    ) -> WorkItem:
        """Create or update a queued work item without duplicating the same work."""

        timestamp = now or utc_now()
        resolved_queue_id = queue_id or stable_queue_id(
            machine=machine,
            scope=scope,
            key=key,
            event=event,
            source_state=source_state,
            target_state=target_state,
        )

        def apply(items: dict[str, WorkItem]) -> WorkItem:
            existing = items.get(resolved_queue_id)
            item = WorkItem(
                queue_id=resolved_queue_id,
                machine=machine,
                scope=scope,
                key=key,
                event=event,
                source_state=source_state,
                target_state=target_state,
                status=existing.status if existing else STATUS_QUEUED,
                attempts=existing.attempts if existing else 0,
                max_attempts=max(1, int(max_attempts)),
                retryable=bool(retryable),
                scheduled_at=scheduled_at or (existing.scheduled_at if existing else timestamp),
                created_at=existing.created_at if existing else timestamp,
                updated_at=timestamp,
                payload=existing.payload if existing and payload is None else _object_payload(payload),
            )
            items[resolved_queue_id] = item
            return item

        return self._mutate(apply)

    def upsert(self, **kwargs: Any) -> WorkItem:
        return self.enqueue(**kwargs)

    def list(self, status: str | Iterable[str] | None = None) -> list[WorkItem]:
        if status is None:
            statuses: set[str] | None = None
        elif isinstance(status, str):
            statuses = {status}
        else:
            statuses = set(status)
        return [
            item
            for item in sorted(self.items.values(), key=_sort_key)
            if statuses is None or item.status in statuses
        ]

    def get(self, queue_id: str) -> WorkItem:
        try:
            return self.items[queue_id]
        except KeyError as exc:
            raise StateMachineQueueError(f"unknown state-machine queue item {queue_id!r}") from exc

    def mark_running(self, queue_id: str, *, now: str | None = None) -> WorkItem:
        timestamp = now or utc_now()

        def apply(items: dict[str, WorkItem]) -> WorkItem:
            item = _require_item(items, queue_id)
            if item.status in TERMINAL_STATUSES:
                raise StateMachineQueueError(f"cannot run terminal queue item {queue_id!r}")
            if item.attempts >= item.max_attempts:
                failed = _replace(
                    item,
                    status=STATUS_FAILED,
                    retryable=False,
                    updated_at=timestamp,
                )
                items[queue_id] = failed
                return failed
            running = _replace(
                item,
                status=STATUS_RUNNING,
                attempts=item.attempts + 1,
                updated_at=timestamp,
            )
            items[queue_id] = running
            return running

        return self._mutate(apply)

    def mark_done(
        self,
        queue_id: str,
        *,
        payload: dict[str, Any] | None = None,
        now: str | None = None,
    ) -> WorkItem:
        return self._mark_terminal(
            queue_id,
            status=STATUS_DONE,
            retryable=False,
            payload=payload,
            now=now,
        )

    def mark_failed(
        self,
        queue_id: str,
        *,
        payload: dict[str, Any] | None = None,
        now: str | None = None,
    ) -> WorkItem:
        return self._mark_terminal(
            queue_id,
            status=STATUS_FAILED,
            retryable=False,
            payload=payload,
            now=now,
        )

    def mark_retryable(
        self,
        queue_id: str,
        *,
        scheduled_at: str = "",
        payload: dict[str, Any] | None = None,
        now: str | None = None,
    ) -> WorkItem:
        timestamp = now or utc_now()

        def apply(items: dict[str, WorkItem]) -> WorkItem:
            item = _require_item(items, queue_id)
            has_budget = item.retryable and item.attempts < item.max_attempts
            updated = _replace(
                item,
                status=STATUS_NEEDS_RETRY if has_budget else STATUS_FAILED,
                retryable=has_budget,
                scheduled_at=scheduled_at or item.scheduled_at,
                updated_at=timestamp,
                payload=_merged_payload(item.payload, payload),
            )
            items[queue_id] = updated
            return updated

        return self._mutate(apply)

    def _mark_terminal(
        self,
        queue_id: str,
        *,
        status: str,
        retryable: bool,
        payload: dict[str, Any] | None,
        now: str | None,
    ) -> WorkItem:
        timestamp = now or utc_now()

        def apply(items: dict[str, WorkItem]) -> WorkItem:
            item = _require_item(items, queue_id)
            updated = _replace(
                item,
                status=status,
                retryable=retryable,
                updated_at=timestamp,
                payload=_merged_payload(item.payload, payload),
            )
            items[queue_id] = updated
            return updated

        return self._mutate(apply)

    def _mutate(self, apply: Any) -> WorkItem:
        with exclusive_file_lock(lock_file_path(self.path)):
            data = _read_queue_file(self.path)
            self.items = {
                item.queue_id: item
                for item in (WorkItem.from_payload(raw) for raw in data.get("items", []))
            }
            item = apply(self.items)
            _write_queue_file(self.path, self.items.values())
            return item


def queue_file_path(path: Path) -> Path:
    return path if path.suffix else path / DEFAULT_QUEUE_FILENAME


def lock_file_path(path: Path) -> Path:
    return path.with_suffix(path.suffix + ".lock")


def stable_queue_id(
    *,
    machine: str,
    scope: str,
    key: str,
    event: str,
    source_state: str = "",
    target_state: str = "",
) -> str:
    payload = {
        "machine": machine,
        "scope": scope,
        "key": key,
        "event": event,
        "source_state": source_state,
        "target_state": target_state,
    }
    digest = hashlib.sha256(_stable_json(payload).encode("utf-8")).hexdigest()[:24]
    return f"smq_{digest}"


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


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


def _read_queue_file(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"version": QUEUE_VERSION, "items": []}
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise StateMachineQueueError(f"state-machine queue file {path} must contain a JSON object")
    items = data.get("items", [])
    if not isinstance(items, list):
        raise StateMachineQueueError(f"state-machine queue file {path} has non-list items")
    return data


def _write_queue_file(path: Path, items: Iterable[WorkItem]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    data = {
        "version": QUEUE_VERSION,
        "items": [item.to_payload() for item in sorted(items, key=_sort_key)],
    }
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2, sort_keys=True, default=str) + "\n", encoding="utf-8")
    tmp.replace(path)


def _sort_key(item: WorkItem) -> tuple[str, str, str]:
    return (item.scheduled_at or "", item.created_at or "", item.queue_id)


def _require_item(items: dict[str, WorkItem], queue_id: str) -> WorkItem:
    try:
        return items[queue_id]
    except KeyError as exc:
        raise StateMachineQueueError(f"unknown state-machine queue item {queue_id!r}") from exc


def _replace(item: WorkItem, **changes: Any) -> WorkItem:
    payload = item.to_payload()
    payload.update(changes)
    return WorkItem.from_payload(payload)


def _merged_payload(existing: dict[str, Any], update: dict[str, Any] | None) -> dict[str, Any]:
    if update is None:
        return existing
    merged = dict(existing)
    merged.update(_object_payload(update))
    return merged


def _object_payload(value: Any) -> dict[str, Any]:
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise StateMachineQueueError("state-machine queue payload must be a JSON object")
    return _json_safe(value)


def _json_safe(value: Any) -> Any:
    return json.loads(_stable_json(value))


def _stable_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)
