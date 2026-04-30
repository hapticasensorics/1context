from __future__ import annotations

import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterator

from onectx.config import load_system
from onectx.ports import load_ports, ports_watch_interval
from onectx.ports.sessions import SessionImportResult, import_session_port
from onectx.storage import LakeStore, utc_now

from .cursors import CursorStore

DEFAULT_DAEMON_INTERVAL_SECONDS = 300.0


class DaemonError(RuntimeError):
    """Raised when the daemon cannot resolve a local source or tick."""


@dataclass
class DaemonTickResult:
    root: Path
    storage_dir: Path
    cursor_path: Path
    ts: str
    port_results: list[dict[str, Any]]
    tick_event_id: str

    @property
    def events_imported(self) -> int:
        return sum(int(item.get("events_imported", 0)) for item in self.port_results)

    @property
    def sessions_imported(self) -> int:
        return sum(int(item.get("sessions_imported", 0)) for item in self.port_results)

    @property
    def artifacts_imported(self) -> int:
        return sum(int(item.get("artifacts_imported", 0)) for item in self.port_results)

    @property
    def lines_scanned(self) -> int:
        return sum(int(item.get("lines_scanned", 0)) for item in self.port_results)

    @property
    def limited(self) -> bool:
        return any(bool(item.get("limited")) for item in self.port_results)

    def to_payload(self) -> dict[str, Any]:
        return {
            "root": str(self.root),
            "storage_dir": str(self.storage_dir),
            "cursor_path": str(self.cursor_path),
            "ts": self.ts,
            "lines_scanned": self.lines_scanned,
            "events_imported": self.events_imported,
            "sessions_imported": self.sessions_imported,
            "artifacts_imported": self.artifacts_imported,
            "limited": self.limited,
            "port_results": self.port_results,
            "tick_event_id": self.tick_event_id,
        }


def cursor_path(root: Path) -> Path:
    return root / "storage" / "cursors" / "daemon.json"


def run_once(
    *,
    root: Path | str | None = None,
    active_plugin: str | None = None,
    experience_source: str | None = None,
) -> DaemonTickResult:
    system = load_system(root, active_plugin)
    source_root = resolve_experience_source(system, experience_source) if experience_source else None
    store = LakeStore(system.storage_dir)
    store.ensure()
    cursors = CursorStore.load(cursor_path(system.root))
    port_payloads: list[dict[str, Any]] = []

    for port in load_ports(system.root):
        try:
            result = import_session_port(
                root=system.root,
                port=port,
                store=store,
                cursors=cursors,
                source_root=source_root,
                include_disabled=bool(source_root),
            )
            payload = result.to_payload()
        except Exception as exc:
            failed = SessionImportResult(port_id=port.id, adapter=port.adapter, skipped=True, reason=str(exc))
            payload = {
                **failed.to_payload(),
                "error": {
                    "type": type(exc).__name__,
                    "message": str(exc),
                },
            }
        port_payloads.append(payload)

    cursors.save()
    now = utc_now()
    event = store.append_event(
        "daemon.tick",
        ts=now,
        source="1context-daemon",
        actor="daemon",
        subject="ports",
        text="Daemon tick scanned ports and advanced cursors.",
        payload={
            "experience_source": experience_source or "",
            "source_root": str(source_root) if source_root else "",
            "cursor": cursors.to_payload(),
            "ports": port_payloads,
        },
    )
    return DaemonTickResult(
        root=system.root,
        storage_dir=system.storage_dir,
        cursor_path=cursor_path(system.root),
        ts=now,
        port_results=port_payloads,
        tick_event_id=event["event_id"],
    )


def watch(
    *,
    root: Path | str | None = None,
    active_plugin: str | None = None,
    experience_source: str | None = None,
    interval: float | None = None,
    ticks: int = 0,
) -> Iterator[DaemonTickResult]:
    if interval is None:
        system = load_system(root, active_plugin)
        interval = ports_watch_interval(system.root, DEFAULT_DAEMON_INTERVAL_SECONDS)
    count = 0
    while True:
        yield run_once(root=root, active_plugin=active_plugin, experience_source=experience_source)
        count += 1
        if ticks and count >= ticks:
            return
        time.sleep(interval)


def resolve_experience_source(system: Any, value: str) -> Path:
    candidate = Path(value).expanduser()
    if candidate.exists():
        return source_sessions_dir(candidate)

    lived = system.lived_experience.get(value)
    if lived:
        return source_sessions_dir(Path(lived["path"]))

    runtime = system.runtime_dir / "experiences" / value
    if runtime.exists():
        return source_sessions_dir(runtime)

    raise DaemonError(
        f"unknown experience source {value!r}; expected a lived-experience id, "
        "runtime experience id, or path"
    )


def source_sessions_dir(path: Path) -> Path:
    source = path / "source-sessions"
    return source if source.exists() else path
