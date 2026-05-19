#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import html
import json
import queue
import shutil
import signal
import socket
import sqlite3
import struct
import subprocess
import sys
import threading
import time
import uuid
from collections import deque
from contextlib import nullcontext, suppress
from dataclasses import dataclass
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse


ROOT = Path(__file__).resolve().parent
STATIC = ROOT / "static"
PEEKABOO_LOCK = threading.Lock()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def safe_name(value: str) -> str:
    keep = []
    for char in value.lower():
        if char.isalnum():
            keep.append(char)
        elif char in ("-", "_", "."):
            keep.append(char)
        elif char.isspace():
            keep.append("-")
    return "".join(keep).strip("-") or "item"


def clean_text(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return str(value)


def fmt_number(value: float | int) -> str:
    if isinstance(value, int) or float(value).is_integer():
        return str(int(value))
    return f"{value:g}"


def run_command(args: list[str], timeout: float = 20.0, lock_peekaboo: bool = True) -> dict[str, Any]:
    started = time.monotonic()
    command_lock = PEEKABOO_LOCK if lock_peekaboo and args and Path(args[0]).name == "peekaboo" else nullcontext()
    try:
        with command_lock:
            proc = subprocess.run(
                args,
                check=False,
                capture_output=True,
                text=True,
                timeout=timeout,
            )
    except subprocess.TimeoutExpired as exc:
        return {
            "ok": False,
            "args": args,
            "elapsed_ms": (time.monotonic() - started) * 1000,
            "error": f"timeout after {timeout}s",
            "stdout": clean_text(exc.stdout),
            "stderr": clean_text(exc.stderr),
        }
    except OSError as exc:
        return {
            "ok": False,
            "args": args,
            "elapsed_ms": (time.monotonic() - started) * 1000,
            "error": str(exc),
            "stdout": "",
            "stderr": str(exc),
        }

    stdout = proc.stdout or ""
    parsed = None
    if stdout.strip():
        try:
            parsed = json.loads(stdout)
        except json.JSONDecodeError:
            parsed = None

    return {
        "ok": proc.returncode == 0,
        "returncode": proc.returncode,
        "args": args,
        "elapsed_ms": (time.monotonic() - started) * 1000,
        "stdout": clean_text(stdout),
        "stderr": clean_text(proc.stderr),
        "json": parsed,
    }


def sha256_file(path: Path) -> str | None:
    if not path.exists():
        return None
    digest = hashlib.sha256()
    try:
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError:
        return None
    return digest.hexdigest()


def png_dimensions(path: Path) -> dict[str, int] | None:
    try:
        with path.open("rb") as handle:
            header = handle.read(24)
        if header[:8] != b"\x89PNG\r\n\x1a\n":
            return None
        width, height = struct.unpack(">II", header[16:24])
        return {"width": width, "height": height}
    except OSError:
        return None


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return None


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, default=str))


FEATURE_CATALOG: list[dict[str, Any]] = [
    {
        "id": "capture-live-screen",
        "category": "stream",
        "name": "Capture Live Screens",
        "summary": "Long-running screen streams for Studio Display and MacBook internal display in showcase/maximal profiles.",
        "status": "starting",
    },
    {
        "id": "image-screen-snapshots",
        "category": "stream",
        "name": "Work Screen Keyframes",
        "summary": "Low-power full-screen keyframes for the two work displays.",
        "status": "starting",
    },
    {
        "id": "capture-live-window",
        "category": "stream",
        "name": "Capture Live Window",
        "summary": "Long-running Codex window stream for terminal/log evidence.",
        "status": "starting",
    },
    {
        "id": "image-codex-snapshots",
        "category": "stream",
        "name": "Codex Keyframe Stream",
        "summary": "Low-power Codex window keyframes for terminal/log evidence.",
        "status": "starting",
    },
    {
        "id": "image-retina",
        "category": "keyframe",
        "name": "Retina Window Keyframe",
        "summary": "Sparse high-fidelity window screenshot using `peekaboo image --retina`.",
        "status": "pending",
    },
    {
        "id": "image-multi",
        "category": "keyframe",
        "name": "Multi-screen Image",
        "summary": "Read-only proof for `peekaboo image --mode multi`.",
        "status": "pending",
    },
    {
        "id": "image-area",
        "category": "keyframe",
        "name": "Area Image",
        "summary": "Read-only proof for `peekaboo image --mode area --region ...`.",
        "status": "pending",
    },
    {
        "id": "see-annotate",
        "category": "ui",
        "name": "Annotated UI Map",
        "summary": "Accessibility/UI map plus annotated screenshot from `peekaboo see --annotate`.",
        "status": "pending",
    },
    {
        "id": "see-menubar-ocr",
        "category": "ui",
        "name": "Menu Bar OCR",
        "summary": "Peekaboo's built-in OCR-ish path for menu bar popovers via `see --menubar`.",
        "status": "pending",
    },
    {
        "id": "capture-video-ingest",
        "category": "video",
        "name": "Video Ingest",
        "summary": "`peekaboo capture video` is shown as available; live video ingest is not run in low-power mode.",
        "status": "available",
    },
    {
        "id": "inventory",
        "category": "inventory",
        "name": "System Inventory",
        "summary": "Screens, apps, windows, menu bar, menu list, spaces, bridge, daemon, permissions, tools.",
        "status": "pending",
    },
    {
        "id": "ai-analyze",
        "category": "analysis",
        "name": "AI Analyze",
        "summary": "`image --analyze` and `see --analyze` are opt-in because they send pixels to a configured model.",
        "status": "opt-in",
    },
    {
        "id": "mcp-inspect-ui",
        "category": "ui",
        "name": "MCP Inspect UI",
        "summary": "MCP-only accessibility text path; this CLI build does not expose `inspect_ui`.",
        "status": "not-cli",
    },
    {
        "id": "automation-actions",
        "category": "automation",
        "name": "Automation Actions",
        "summary": "Click/type/window/app/menu actions are inventoried but intentionally not invoked.",
        "status": "disabled",
    },
    {
        "id": "ocr-derived",
        "category": "ocr",
        "name": "Terminal OCR Layer",
        "summary": "Deferred derived layer over kept frames; capture keeps running even if OCR is slow.",
        "status": "planned",
    },
]


OCR_NOTE = {
    "status": "deferred",
    "headline": "Peekaboo does not expose a general continuous OCR pipeline in this CLI build.",
    "peekaboo_builtin": [
        "`peekaboo see --menubar` uses a menu-bar OCR path.",
        "`image --analyze` / `see --analyze` can ask an AI provider to extract text, but that is opt-in and not local-only.",
        "The CLI does not expose `inspect_ui`; that remains MCP-only for accessibility text when available.",
    ],
    "demo_plan": [
        "Keep every sampled frame as immutable evidence first.",
        "Run OCR as a derived queue over Codex-window frames, never in the capture loop.",
        "Prefer Apple Vision or Tesseract locally for terminal text, then store raw OCR text, normalized visible-line spans, frame IDs, and confidence.",
        "Mark OCR as pending/deferred if it falls behind; do not drop frames to let OCR catch up.",
    ],
}


@dataclass
class Args:
    host: str
    port: int
    mode: str
    profile: str
    work_screens: list[int]
    codex_app: str
    codex_window_title: str
    codex_window_id: int | None
    codex_window_index: int | None
    codex_region: str | None
    screen_labels: dict[int, str]
    stream_work_screens: bool
    stream_codex: bool
    screen_snapshot_interval: float
    codex_snapshot_interval: float
    sample_see: bool
    segment_duration: int
    screen_idle_fps: float
    screen_active_fps: float
    codex_idle_fps: float
    codex_active_fps: float
    threshold: float
    heartbeat_sec: float
    quiet_ms: int
    max_frames: int
    resolution_cap: int
    diff_strategy: str
    capture_engine: str
    poll_interval: float
    feature_interval: float
    inventory_interval: float
    metrics_interval: float
    sample_video: bool
    evidence_root: Path


class EvidenceState:
    def __init__(self, args: Args) -> None:
        self.args = args
        self.run_id = datetime.now().strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:6]
        self.run_root = args.evidence_root / self.run_id
        self.run_root.mkdir(parents=True, exist_ok=True)
        self.db_path = self.run_root / "events.sqlite3"
        self.ndjson_path = self.run_root / "events.ndjson"
        self.lock = threading.RLock()
        self.db_lock = threading.Lock()
        self.events: deque[dict[str, Any]] = deque(maxlen=400)
        self.lanes: dict[str, dict[str, Any]] = {}
        self.features: dict[str, dict[str, Any]] = {item["id"]: dict(item) for item in FEATURE_CATALOG}
        self.capabilities: dict[str, Any] = {}
        self.metrics: dict[str, Any] = {}
        self.latest_ui: dict[str, Any] | None = None
        self.latest_menubar: dict[str, Any] | None = None
        self.latest_keyframes: dict[str, Any] = {}
        self.subscribers: list[queue.Queue[dict[str, Any]]] = []
        self._seq = 0
        self._conn = sqlite3.connect(self.db_path, check_same_thread=False)
        self._init_db()
        self.publish(
            "run_started",
            {
                "summary": "Peekaboo evidence wall started",
                "profile": args.profile,
                "target_cpu_percent": 5,
                "ocr": OCR_NOTE,
            },
        )

    def _init_db(self) -> None:
        with self.db_lock:
            self._conn.execute("PRAGMA journal_mode=WAL")
            self._conn.execute(
                """
                CREATE TABLE IF NOT EXISTS events (
                    seq INTEGER PRIMARY KEY,
                    ts TEXT NOT NULL,
                    type TEXT NOT NULL,
                    lane_id TEXT,
                    payload_json TEXT NOT NULL
                )
                """
            )
            self._conn.commit()

    def close(self) -> None:
        with suppress(Exception):
            self._conn.close()

    def rel_url(self, path: Path | None) -> str | None:
        if path is None:
            return None
        try:
            rel = path.resolve().relative_to(self.args.evidence_root.resolve())
        except (OSError, ValueError):
            return None
        return "/evidence/" + rel.as_posix()

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "run": {
                    "id": self.run_id,
                    "mode": self.args.mode,
                    "profile": self.args.profile,
                    "evidence_root": str(self.run_root),
                    "work_screens": self.args.work_screens,
                    "screen_labels": self.args.screen_labels,
                    "codex_app": self.args.codex_app,
                    "codex_window_title": self.args.codex_window_title,
                    "codex_window_id": self.args.codex_window_id,
                    "codex_window_index": self.args.codex_window_index,
                    "codex_region": self.args.codex_region,
                    "target_cpu_percent": 5,
                    "capture": {
                        "segment_duration": self.args.segment_duration,
                        "stream_work_screens": self.args.stream_work_screens,
                        "stream_codex": self.args.stream_codex,
                        "screen_snapshot_interval": self.args.screen_snapshot_interval,
                        "codex_snapshot_interval": self.args.codex_snapshot_interval,
                        "sample_see": self.args.sample_see,
                        "screen_fps": {
                            "idle": self.args.screen_idle_fps,
                            "active": self.args.screen_active_fps,
                        },
                        "codex_fps": {
                            "idle": self.args.codex_idle_fps,
                            "active": self.args.codex_active_fps,
                        },
                        "threshold": self.args.threshold,
                        "heartbeat_sec": self.args.heartbeat_sec,
                        "quiet_ms": self.args.quiet_ms,
                        "resolution_cap": self.args.resolution_cap,
                        "max_frames": self.args.max_frames,
                        "diff_strategy": self.args.diff_strategy,
                    },
                },
                "lanes": dict(self.lanes),
                "features": dict(self.features),
                "capabilities": dict(self.capabilities),
                "metrics": dict(self.metrics),
                "agent_contract": self.agent_contract_unlocked(),
                "ocr": dict(OCR_NOTE),
                "latest_ui": dict(self.latest_ui) if self.latest_ui else None,
                "latest_menubar": dict(self.latest_menubar) if self.latest_menubar else None,
                "latest_keyframes": dict(self.latest_keyframes),
                "events": list(self.events)[-160:],
            }

    def agent_contract_unlocked(self) -> dict[str, Any]:
        return {
            "headline": "Agent-readable evidence is the JSON feed plus immutable files under the evidence root.",
            "endpoints": {
                "latest_json": "/agent-feed/latest",
                "sse_stream": "/agent-feed/stream",
                "metrics": "/metrics",
            },
            "ledgers": {
                "ndjson_url": self.rel_url(self.ndjson_path),
                "sqlite_url": self.rel_url(self.db_path),
                "ndjson_path": str(self.ndjson_path),
                "sqlite_path": str(self.db_path),
            },
            "included_now": [
                "latest frame URLs for each lane",
                "absolute artifact paths",
                "frame dimensions, byte sizes, and sha256 hashes",
                "Peekaboo commands and command-output JSON files for captures and probes",
                "permissions, bridge, daemon, screen, app, window, menu, menubar, space, and tool inventory",
                "CPU/process metrics",
                "OCR and Accessibility probe status",
            ],
            "not_included_by_default": [
                "continuous full-screen capture-live streams in low-power mode",
                "OCR text extraction over every frame",
                "AI image analysis",
                "automation actions such as click/type/window manipulation",
                "Peekaboo see probes unless --sample-see is enabled",
            ],
        }

    def publish(self, event_type: str, payload: dict[str, Any]) -> dict[str, Any]:
        event = {
            "seq": None,
            "id": uuid.uuid4().hex,
            "ts": utc_now(),
            "type": event_type,
            "payload": payload,
        }
        with self.lock:
            self._seq += 1
            event["seq"] = self._seq
            self._apply_event(event_type, payload)
            self.events.append(dict(event))
            subscribers = list(self.subscribers)

        self._persist_event(event)
        for subscriber in subscribers:
            try:
                subscriber.put_nowait(event)
            except queue.Full:
                pass
        return event

    def _apply_event(self, event_type: str, payload: dict[str, Any]) -> None:
        lane_id = payload.get("lane_id")
        if event_type in {"segment_started", "segment_completed", "frame", "lane_error", "lane_stopped"} and lane_id:
            lane = self.lanes.setdefault(
                lane_id,
                {
                    "lane_id": lane_id,
                    "source_label": payload.get("source_label", lane_id),
                    "frames_seen": 0,
                    "segments_seen": 0,
                },
            )
            lane["source_label"] = payload.get("source_label", lane.get("source_label", lane_id))
            lane["updated_at"] = utc_now()
            if event_type == "frame":
                lane.update(payload)
                lane["status"] = "live"
                lane["error"] = None
                lane["frames_seen"] = int(lane.get("frames_seen", 0)) + 1
            elif event_type == "segment_started":
                lane["status"] = "capturing"
                lane["active_segment"] = payload
            elif event_type == "segment_completed":
                lane["status"] = "rolling"
                lane["last_segment"] = payload
                lane["segments_seen"] = int(lane.get("segments_seen", 0)) + 1
            elif event_type == "lane_error":
                lane["status"] = "error"
                lane["error"] = payload.get("error")
            elif event_type == "lane_stopped":
                lane["status"] = "stopped"

        if event_type == "feature":
            feature_id = payload.get("feature_id")
            if feature_id:
                feature = self.features.setdefault(feature_id, {"id": feature_id, "name": feature_id})
                feature.update(payload)
                feature["updated_at"] = utc_now()
        elif event_type == "capabilities":
            self.capabilities.update(payload)
        elif event_type == "metrics":
            self.metrics.update(payload)
        elif event_type == "ui_snapshot":
            self.latest_ui = payload
        elif event_type == "menubar_snapshot":
            self.latest_menubar = payload
        elif event_type == "keyframe":
            key = payload.get("feature_id") or payload.get("kind") or "keyframe"
            self.latest_keyframes[key] = payload

    def _persist_event(self, event: dict[str, Any]) -> None:
        lane_id = event["payload"].get("lane_id") if isinstance(event.get("payload"), dict) else None
        encoded = json.dumps(event["payload"], separators=(",", ":"), default=str)
        with self.db_lock:
            self._conn.execute(
                "INSERT INTO events(seq, ts, type, lane_id, payload_json) VALUES (?, ?, ?, ?, ?)",
                (event["seq"], event["ts"], event["type"], lane_id, encoded),
            )
            self._conn.commit()
        with self.ndjson_path.open("a") as handle:
            handle.write(json.dumps(event, separators=(",", ":"), default=str) + "\n")

    def subscribe(self) -> queue.Queue[dict[str, Any]]:
        subscriber: queue.Queue[dict[str, Any]] = queue.Queue(maxsize=200)
        with self.lock:
            self.subscribers.append(subscriber)
        return subscriber

    def unsubscribe(self, subscriber: queue.Queue[dict[str, Any]]) -> None:
        with self.lock:
            if subscriber in self.subscribers:
                self.subscribers.remove(subscriber)


class Worker(threading.Thread):
    def __init__(self, state: EvidenceState, name: str, stop_event: threading.Event) -> None:
        super().__init__(name=name, daemon=True)
        self.state = state
        self.stop_event = stop_event

    def sleep(self, seconds: float) -> bool:
        return self.stop_event.wait(seconds)


@dataclass
class LaneConfig:
    lane_id: str
    source_label: str
    feature_id: str
    mode: str
    command_target: list[str]
    idle_fps: float
    active_fps: float
    startup_delay: float = 0.0


class LiveLaneWorker(Worker):
    def __init__(self, state: EvidenceState, stop_event: threading.Event, config: LaneConfig) -> None:
        super().__init__(state, f"live-{config.lane_id}", stop_event)
        self.config = config
        self.segment_index = 0

    def run(self) -> None:
        if self.config.startup_delay:
            self.sleep(self.config.startup_delay)
        while not self.stop_event.is_set():
            self.segment_index += 1
            segment_id = f"segment-{self.segment_index:05d}"
            segment_dir = self.state.run_root / "streams" / self.config.lane_id / segment_id
            segment_dir.mkdir(parents=True, exist_ok=True)
            if self.state.args.mode == "mock":
                self.run_mock_segment(segment_id, segment_dir)
                continue
            self.run_peekaboo_segment(segment_id, segment_dir)

    def command_for_segment(self, segment_dir: Path) -> list[str]:
        args = self.state.args
        return [
            "peekaboo",
            "capture",
            "live",
            *self.config.command_target,
            "--duration",
            str(args.segment_duration),
            "--idle-fps",
            fmt_number(self.config.idle_fps),
            "--active-fps",
            fmt_number(self.config.active_fps),
            "--threshold",
            fmt_number(args.threshold),
            "--heartbeat-sec",
            fmt_number(args.heartbeat_sec),
            "--quiet-ms",
            str(args.quiet_ms),
            "--max-frames",
            str(args.max_frames),
            "--resolution-cap",
            str(args.resolution_cap),
            "--diff-strategy",
            args.diff_strategy,
            "--capture-engine",
            args.capture_engine,
            "--path",
            str(segment_dir),
            "--json",
        ]

    def run_peekaboo_segment(self, segment_id: str, segment_dir: Path) -> None:
        command = self.command_for_segment(segment_dir)
        started = time.monotonic()
        self.state.publish(
            "segment_started",
            {
                "lane_id": self.config.lane_id,
                "source_label": self.config.source_label,
                "feature_id": self.config.feature_id,
                "segment_id": segment_id,
                "segment_path": str(segment_dir),
                "segment_url": self.state.rel_url(segment_dir),
                "command": command,
                "idle_fps": self.config.idle_fps,
                "active_fps": self.config.active_fps,
            },
        )
        self.state.publish(
            "feature",
            {
                "feature_id": self.config.feature_id,
                "status": "streaming",
                "summary": f"{self.config.source_label} streaming via capture live",
                "lane_id": self.config.lane_id,
            },
        )

        seen: set[Path] = set()
        try:
            proc = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
        except OSError as exc:
            self.state.publish(
                "lane_error",
                {
                    "lane_id": self.config.lane_id,
                    "source_label": self.config.source_label,
                    "error": str(exc),
                    "command": command,
                },
            )
            self.sleep(3)
            return

        self.state.publish(
            "metrics",
            {"live_pids": {self.config.lane_id: proc.pid}, "latest_process_change": utc_now()},
        )
        try:
            while not self.stop_event.is_set() and proc.poll() is None:
                self.publish_new_frames(segment_id, segment_dir, seen, started)
                self.sleep(self.state.args.poll_interval)
            if self.stop_event.is_set() and proc.poll() is None:
                proc.terminate()
                with suppress(subprocess.TimeoutExpired):
                    proc.wait(timeout=3)
                if proc.poll() is None:
                    proc.kill()
            stdout, stderr = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()
        finally:
            self.publish_new_frames(segment_id, segment_dir, seen, started)

        result = {
            "ok": proc.returncode == 0,
            "returncode": proc.returncode,
            "args": command,
            "stdout": clean_text(stdout),
            "stderr": clean_text(stderr),
            "elapsed_ms": (time.monotonic() - started) * 1000,
            "json": None,
        }
        if result["stdout"].strip():
            with suppress(json.JSONDecodeError):
                result["json"] = json.loads(result["stdout"])
        write_json(segment_dir / "command-output.json", result)
        payload = self.summarize_segment(result, segment_id, segment_dir, started)
        self.state.publish("segment_completed", payload)
        if not result["ok"]:
            self.state.publish(
                "lane_error",
                {
                    "lane_id": self.config.lane_id,
                    "source_label": self.config.source_label,
                    "error": result.get("stderr") or result.get("stdout") or "capture live failed",
                    "segment_id": segment_id,
                },
            )
            self.sleep(2)

    def publish_new_frames(self, segment_id: str, segment_dir: Path, seen: set[Path], started: float) -> None:
        candidates = sorted(segment_dir.glob("keep-*.*"))
        for path in candidates:
            if path in seen or path.suffix.lower() not in {".png", ".jpg", ".jpeg", ".svg"}:
                continue
            if path.suffix.lower() == ".png" and png_dimensions(path) is None:
                continue
            size = path.stat().st_size if path.exists() else 0
            if size <= 0:
                continue
            seen.add(path)
            ts = utc_now()
            digest = sha256_file(path)
            frame = {
                "lane_id": self.config.lane_id,
                "source_label": self.config.source_label,
                "feature_id": self.config.feature_id,
                "ts": ts,
                "ok": True,
                "status": "live",
                "segment_id": segment_id,
                "file": path.name,
                "image_url": self.state.rel_url(path),
                "artifact_path": str(path),
                "sha256": digest,
                "size_bytes": size,
                "dimensions": png_dimensions(path),
                "elapsed_since_segment_ms": (time.monotonic() - started) * 1000,
                "idle_fps": self.config.idle_fps,
                "active_fps": self.config.active_fps,
                "threshold": self.state.args.threshold,
                "resolution_cap": self.state.args.resolution_cap,
            }
            self.state.publish("frame", frame)

    def summarize_segment(
        self,
        result: dict[str, Any],
        segment_id: str,
        segment_dir: Path,
        started: float,
    ) -> dict[str, Any]:
        data = result.get("json", {}).get("data", {}) if result.get("json") else {}
        contact_info = data.get("contactSheet") if isinstance(data.get("contactSheet"), dict) else {}
        contact_path = segment_dir / contact_info.get("file", "contact.png") if contact_info else None
        if not contact_path or not contact_path.exists():
            contact_path = next(iter(segment_dir.glob("contact.*")), None)
        metadata_file = Path(data["metadataFile"]) if data.get("metadataFile") else segment_dir / "metadata.json"
        metadata = read_json(metadata_file) if metadata_file and metadata_file.exists() else None
        frames = []
        raw_frames = data.get("frames") or (metadata.get("frames") if isinstance(metadata, dict) else []) or []
        for frame in raw_frames[:100]:
            item = dict(frame)
            if item.get("file"):
                item["url"] = self.state.rel_url(segment_dir / item["file"])
            frames.append(item)
        if not frames:
            for path in sorted(segment_dir.glob("keep-*.*"))[:100]:
                frames.append({"file": path.name, "url": self.state.rel_url(path), "size_bytes": path.stat().st_size})
        stats = data.get("stats") or (metadata.get("stats") if isinstance(metadata, dict) else {}) or {}
        return {
            "lane_id": self.config.lane_id,
            "source_label": self.config.source_label,
            "feature_id": self.config.feature_id,
            "segment_id": segment_id,
            "ok": bool(result["ok"]),
            "summary": f"{self.config.source_label} segment completed with {len(frames)} kept frames",
            "segment_path": str(segment_dir),
            "segment_url": self.state.rel_url(segment_dir),
            "contact_sheet_url": self.state.rel_url(contact_path) if contact_path else None,
            "metadata_url": self.state.rel_url(metadata_file) if metadata_file and metadata_file.exists() else None,
            "command_output_url": self.state.rel_url(segment_dir / "command-output.json"),
            "frames": frames,
            "stats": stats,
            "scope": data.get("scope"),
            "warnings": data.get("warnings", []),
            "elapsed_ms": (time.monotonic() - started) * 1000,
            "error": None if result["ok"] else (result.get("stderr") or result.get("stdout") or "capture live failed"),
        }

    def run_mock_segment(self, segment_id: str, segment_dir: Path) -> None:
        self.state.publish(
            "segment_started",
            {
                "lane_id": self.config.lane_id,
                "source_label": self.config.source_label,
                "feature_id": self.config.feature_id,
                "segment_id": segment_id,
                "segment_path": str(segment_dir),
            },
        )
        seen: set[Path] = set()
        started = time.monotonic()
        for index in range(1, 4):
            if self.stop_event.is_set():
                break
            frame = segment_dir / f"keep-{index:04d}.svg"
            frame.write_text(mock_svg(self.config.source_label, utc_now(), index))
            self.publish_new_frames(segment_id, segment_dir, seen, started)
            self.sleep(1)
        contact = segment_dir / "contact.svg"
        contact.write_text(mock_contact_svg(self.config.source_label))
        payload = {
            "lane_id": self.config.lane_id,
            "source_label": self.config.source_label,
            "feature_id": self.config.feature_id,
            "segment_id": segment_id,
            "ok": True,
            "summary": "mock segment completed",
            "segment_path": str(segment_dir),
            "segment_url": self.state.rel_url(segment_dir),
            "contact_sheet_url": self.state.rel_url(contact),
            "metadata_url": None,
            "command_output_url": None,
            "frames": [{"file": f"keep-{i:04d}.svg", "url": self.state.rel_url(segment_dir / f"keep-{i:04d}.svg")} for i in range(1, 4)],
            "stats": {"framesKept": 3},
            "elapsed_ms": (time.monotonic() - started) * 1000,
        }
        self.state.publish("segment_completed", payload)


class SnapshotLaneWorker(Worker):
    def __init__(
        self,
        state: EvidenceState,
        stop_event: threading.Event,
        lane_id: str,
        source_label: str,
        screen_index: int,
        interval: float,
        startup_delay: float = 0.0,
    ) -> None:
        super().__init__(state, f"snapshot-{lane_id}", stop_event)
        self.lane_id = lane_id
        self.source_label = source_label
        self.screen_index = screen_index
        self.interval = interval
        self.startup_delay = startup_delay
        self.index = 0

    def run(self) -> None:
        if self.startup_delay:
            self.sleep(self.startup_delay)
        lane_dir = self.state.run_root / "streams" / self.lane_id / "snapshots"
        lane_dir.mkdir(parents=True, exist_ok=True)
        self.state.publish(
            "feature",
            {
                "feature_id": "image-screen-snapshots",
                "status": "streaming",
                "summary": f"{self.source_label} sampled every {self.interval:g}s",
                "lane_id": self.lane_id,
            },
        )
        while not self.stop_event.is_set():
            started = time.monotonic()
            self.index += 1
            ts = utc_now()
            if self.state.args.mode == "mock":
                image_path = lane_dir / f"frame-{self.index:06d}.svg"
                image_path.write_text(mock_svg(self.source_label, ts, self.index))
                command = ["mock", self.lane_id]
                result = {"ok": True, "args": command, "elapsed_ms": 1.0, "json": None}
            else:
                image_path = lane_dir / f"frame-{self.index:06d}.png"
                command = [
                    "peekaboo",
                    "image",
                    "--mode",
                    "screen",
                    "--screen-index",
                    str(self.screen_index),
                    "--capture-engine",
                    self.state.args.capture_engine,
                    "--format",
                    "png",
                    "--path",
                    str(image_path),
                    "--json",
                ]
                result = run_command(command, timeout=max(10.0, self.interval))
                command_output = lane_dir / f"frame-{self.index:06d}.json"
                write_json(command_output, result)

            if result["ok"] and image_path.exists():
                command_output = lane_dir / f"frame-{self.index:06d}.json"
                payload = {
                    "lane_id": self.lane_id,
                    "source_label": self.source_label,
                    "feature_id": "image-screen-snapshots",
                    "capture_kind": "screen-keyframe",
                    "ts": ts,
                    "ok": True,
                    "status": "live",
                    "segment_id": "snapshots",
                    "file": image_path.name,
                    "image_url": self.state.rel_url(image_path),
                    "artifact_path": str(image_path),
                    "command": command,
                    "command_output_url": self.state.rel_url(command_output) if command_output.exists() else None,
                    "sha256": sha256_file(image_path),
                    "size_bytes": image_path.stat().st_size,
                    "dimensions": png_dimensions(image_path),
                    "elapsed_since_segment_ms": result.get("elapsed_ms"),
                    "idle_fps": round(1 / self.interval, 3),
                    "active_fps": round(1 / self.interval, 3),
                    "threshold": None,
                    "resolution_cap": None,
                }
                self.state.publish("frame", payload)
            else:
                self.state.publish(
                    "lane_error",
                    {
                        "lane_id": self.lane_id,
                        "source_label": self.source_label,
                        "error": result.get("stderr") or result.get("stdout") or result.get("error") or "image capture failed",
                    },
                )
            elapsed = time.monotonic() - started
            self.sleep(max(0.1, self.interval - elapsed))


class ImageSnapshotWorker(Worker):
    def __init__(
        self,
        state: EvidenceState,
        stop_event: threading.Event,
        lane_id: str,
        source_label: str,
        feature_id: str,
        command_target: list[str],
        interval: float,
        startup_delay: float = 0.0,
    ) -> None:
        super().__init__(state, f"image-snapshot-{lane_id}", stop_event)
        self.lane_id = lane_id
        self.source_label = source_label
        self.feature_id = feature_id
        self.command_target = command_target
        self.interval = interval
        self.startup_delay = startup_delay
        self.index = 0

    def run(self) -> None:
        if self.startup_delay:
            self.sleep(self.startup_delay)
        lane_dir = self.state.run_root / "streams" / self.lane_id / "snapshots"
        lane_dir.mkdir(parents=True, exist_ok=True)
        self.state.publish(
            "feature",
            {
                "feature_id": self.feature_id,
                "status": "streaming",
                "summary": f"{self.source_label} sampled every {self.interval:g}s",
                "lane_id": self.lane_id,
            },
        )
        while not self.stop_event.is_set():
            started = time.monotonic()
            self.index += 1
            ts = utc_now()
            if self.state.args.mode == "mock":
                image_path = lane_dir / f"frame-{self.index:06d}.svg"
                image_path.write_text(mock_svg(self.source_label, ts, self.index))
                command = ["mock", self.lane_id]
                result = {"ok": True, "args": command, "elapsed_ms": 1.0, "json": None}
            else:
                image_path = lane_dir / f"frame-{self.index:06d}.png"
                command = [
                    "peekaboo",
                    "image",
                    *self.command_target,
                    "--capture-engine",
                    self.state.args.capture_engine,
                    "--format",
                    "png",
                    "--path",
                    str(image_path),
                    "--json",
                ]
                result = run_command(command, timeout=max(8.0, self.interval * 2))
                command_output = lane_dir / f"frame-{self.index:06d}.json"
                write_json(command_output, result)

            if result["ok"] and image_path.exists():
                command_output = lane_dir / f"frame-{self.index:06d}.json"
                payload = {
                    "lane_id": self.lane_id,
                    "source_label": self.source_label,
                    "feature_id": self.feature_id,
                    "capture_kind": "window-keyframe",
                    "ts": ts,
                    "ok": True,
                    "status": "live",
                    "segment_id": "snapshots",
                    "file": image_path.name,
                    "image_url": self.state.rel_url(image_path),
                    "artifact_path": str(image_path),
                    "command": command,
                    "command_output_url": self.state.rel_url(command_output) if command_output.exists() else None,
                    "sha256": sha256_file(image_path),
                    "size_bytes": image_path.stat().st_size,
                    "dimensions": png_dimensions(image_path),
                    "elapsed_since_segment_ms": result.get("elapsed_ms"),
                    "idle_fps": round(1 / self.interval, 3),
                    "active_fps": round(1 / self.interval, 3),
                    "threshold": None,
                    "resolution_cap": None,
                }
                self.state.publish("frame", payload)
            else:
                self.state.publish(
                    "lane_error",
                    {
                        "lane_id": self.lane_id,
                        "source_label": self.source_label,
                        "error": result.get("stderr") or result.get("stdout") or result.get("error") or "image capture failed",
                    },
                )
            elapsed = time.monotonic() - started
            self.sleep(max(0.1, self.interval - elapsed))


class FeatureProbeWorker(Worker):
    def run(self) -> None:
        self.capture_inventory()
        self.capture_feature_samples()
        last_inventory = time.monotonic()
        last_features = time.monotonic()
        while not self.stop_event.is_set():
            now = time.monotonic()
            if now - last_inventory >= self.state.args.inventory_interval:
                self.capture_inventory()
                last_inventory = now
            if now - last_features >= self.state.args.feature_interval:
                self.capture_feature_samples()
                last_features = now
            self.sleep(1.0)

    def capture_inventory(self) -> None:
        ts = utc_now()
        inv_dir = self.state.run_root / "inventory" / ts.replace(":", "-")
        inv_dir.mkdir(parents=True, exist_ok=True)
        if self.state.args.mode == "mock":
            payload = mock_inventory(ts)
            self.state.publish("capabilities", payload)
            self.state.publish("feature", {"feature_id": "inventory", "status": "sampled", "summary": "mock inventory sampled"})
            return

        commands = {
            "peekaboo_version": ["peekaboo", "--version"],
            "permissions": ["peekaboo", "permissions", "status"],
            "bridge_status": ["peekaboo", "bridge", "status", "--json"],
            "daemon_status": ["peekaboo", "daemon", "status", "--json"],
            "screens": ["peekaboo", "list", "screens", "--json"],
            "apps": ["peekaboo", "list", "apps", "--json"],
            "codex_windows": ["peekaboo", "list", "windows", "--app", self.state.args.codex_app, "--json"],
            "menubar": ["peekaboo", "list", "menubar", "--json"],
            "codex_menu": ["peekaboo", "menu", "list", "--app", self.state.args.codex_app, "--json"],
            "window_list": ["peekaboo", "window", "list", "--app", self.state.args.codex_app, "--json"],
            "space_list": ["peekaboo", "space", "list", "--json"],
            "tools": ["peekaboo", "tools", "--json"],
        }
        inventory_files: dict[str, str | None] = {}
        manifest_path = inv_dir / "manifest.json"
        payload: dict[str, Any] = {
            "inventory_ts": ts,
            "inventory_manifest_url": self.state.rel_url(manifest_path),
            "inventory_manifest_path": str(manifest_path),
            "inventory_files": inventory_files,
        }
        failures = 0
        for key, command in commands.items():
            result = run_command(command, timeout=18, lock_peekaboo=False)
            output_path = inv_dir / f"{key}.json"
            write_json(output_path, result)
            inventory_files[key] = self.state.rel_url(output_path)
            if key in {"peekaboo_version", "permissions"}:
                payload[key] = result.get("stdout", "").strip() if result["ok"] else result.get("stderr", "").strip()
            else:
                payload[key] = result.get("json") if result["ok"] and result.get("json") else {
                    "ok": False,
                    "error": result.get("stderr") or result.get("stdout") or result.get("error"),
                }
            if not result["ok"]:
                failures += 1
        write_json(manifest_path, payload)
        self.state.publish("capabilities", payload)
        self.state.publish(
            "feature",
            {
                "feature_id": "inventory",
                "status": "sampled" if failures == 0 else "partial",
                "summary": f"inventory sampled with {failures} command failures",
                "artifact_url": self.state.rel_url(manifest_path),
            },
        )

    def capture_feature_samples(self) -> None:
        ts = utc_now()
        sample_dir = self.state.run_root / "feature-samples" / ts.replace(":", "-")
        sample_dir.mkdir(parents=True, exist_ok=True)
        if self.state.args.mode == "mock":
            self.mock_feature_samples(sample_dir)
            return

        samples: list[tuple[str, str, list[str], Path, float]] = [
            (
                "image-retina",
                "retina Codex keyframe",
                [
                    "peekaboo",
                    "image",
                    "--app",
                    self.state.args.codex_app,
                    "--mode",
                    "window",
                    *codex_image_window_args(self.state.args),
                    "--retina",
                    "--format",
                    "png",
                    "--path",
                    str(sample_dir / "codex-retina.png"),
                    "--json",
                ],
                sample_dir / "codex-retina.png",
                20,
            ),
            (
                "image-multi",
                "multi-screen keyframe",
                [
                    "peekaboo",
                    "image",
                    "--mode",
                    "multi",
                    "--format",
                    "png",
                    "--path",
                    str(sample_dir / "multi.png"),
                    "--json",
                ],
                sample_dir / "multi.png",
                25,
            ),
            (
                "image-area",
                "area keyframe",
                [
                    "peekaboo",
                    "image",
                    "--mode",
                    "area",
                    "--region",
                    "0,0,900,520",
                    "--format",
                    "png",
                    "--path",
                    str(sample_dir / "area.png"),
                    "--json",
                ],
                sample_dir / "area.png",
                15,
            ),
        ]
        if self.state.args.sample_see:
            samples.extend(
                [
                    (
                        "see-annotate",
                        "annotated Codex UI map",
                        [
                            "peekaboo",
                            "see",
                            "--app",
                            self.state.args.codex_app,
                            *codex_see_window_args(self.state.args),
                            "--path",
                            str(sample_dir / "codex-see-annotated.png"),
                            "--json",
                            "--annotate",
                            "--timeout-seconds",
                            "8",
                        ],
                        sample_dir / "codex-see-annotated.png",
                        10,
                    ),
                    (
                        "see-menubar-ocr",
                        "menu bar OCR/UI sample",
                        [
                            "peekaboo",
                            "see",
                            "--app",
                            "menubar",
                            "--menubar",
                            "--path",
                            str(sample_dir / "menubar-see.png"),
                            "--json",
                            "--timeout-seconds",
                            "8",
                        ],
                        sample_dir / "menubar-see.png",
                        10,
                    ),
                ]
            )
        else:
            self.state.publish(
                "feature",
                {
                    "feature_id": "see-annotate",
                    "status": "available",
                    "summary": "`peekaboo see --annotate` is listed but not sampled in low-power mode because it can hang this local build",
                },
            )
            self.state.publish(
                "feature",
                {
                    "feature_id": "see-menubar-ocr",
                    "status": "available",
                    "summary": "`peekaboo see --menubar` is the built-in OCR-ish path; sampling is opt-in with --sample-see",
                },
            )
        for feature_id, label, command, artifact, timeout in samples:
            if self.stop_event.is_set():
                return
            result = run_command(command, timeout=timeout)
            raw_path = sample_dir / f"{feature_id}.json"
            write_json(raw_path, result)
            payload = self.summarize_sample(feature_id, label, result, artifact, raw_path)
            self.state.publish("feature", payload)
            self.state.publish("keyframe", payload)
            if feature_id == "see-annotate":
                self.state.publish("ui_snapshot", summarize_ui_snapshot(result, artifact, raw_path, self.state))
            elif feature_id == "see-menubar-ocr":
                self.state.publish("menubar_snapshot", summarize_ui_snapshot(result, artifact, raw_path, self.state))

        if self.state.args.sample_video:
            self.state.publish(
                "feature",
                {
                    "feature_id": "capture-video-ingest",
                    "status": "available",
                    "summary": "video ingest requires an input video; no generated input was captured in this low-power proof",
                },
            )

    def summarize_sample(
        self,
        feature_id: str,
        label: str,
        result: dict[str, Any],
        artifact: Path,
        raw_path: Path,
    ) -> dict[str, Any]:
        files = []
        data = result.get("json", {}).get("data", {}) if result.get("json") else {}
        if isinstance(data.get("files"), list):
            for item in data["files"][:20]:
                file_path = Path(item.get("path", "")) if isinstance(item, dict) and item.get("path") else None
                if file_path and file_path.exists():
                    files.append({"path": str(file_path), "url": self.state.rel_url(file_path), "size_bytes": file_path.stat().st_size})
        if artifact.exists() and not files:
            files.append({"path": str(artifact), "url": self.state.rel_url(artifact), "size_bytes": artifact.stat().st_size})
        return {
            "feature_id": feature_id,
            "status": "sampled" if result["ok"] else "error",
            "summary": label,
            "ok": bool(result["ok"]),
            "artifact_url": self.state.rel_url(artifact) if artifact.exists() else (files[0]["url"] if files else None),
            "artifact_path": str(artifact) if artifact.exists() else None,
            "raw_json_url": self.state.rel_url(raw_path),
            "files": files,
            "elapsed_ms": result.get("elapsed_ms"),
            "error": None if result["ok"] else (result.get("stderr") or result.get("stdout") or result.get("error")),
        }

    def mock_feature_samples(self, sample_dir: Path) -> None:
        for feature_id in ["image-retina", "image-multi", "image-area", "see-annotate", "see-menubar-ocr"]:
            artifact = sample_dir / f"{feature_id}.svg"
            artifact.write_text(mock_svg(feature_id, utc_now(), 1))
            payload = {
                "feature_id": feature_id,
                "status": "sampled",
                "summary": f"mock {feature_id}",
                "ok": True,
                "artifact_url": self.state.rel_url(artifact),
                "artifact_path": str(artifact),
            }
            self.state.publish("feature", payload)
            self.state.publish("keyframe", payload)


class MetricsWorker(Worker):
    def run(self) -> None:
        while not self.stop_event.is_set():
            self.capture_metrics()
            self.sleep(self.state.args.metrics_interval)

    def capture_metrics(self) -> None:
        result = run_command(["ps", "-axo", "pid,ppid,pcpu,pmem,rss,command"], timeout=4, lock_peekaboo=False)
        processes: list[dict[str, Any]] = []
        total_cpu = 0.0
        python_cpu = 0.0
        capture_cpu = 0.0
        if result["ok"]:
            for line in result["stdout"].splitlines()[1:]:
                parts = line.strip().split(None, 5)
                if len(parts) < 6:
                    continue
                pid, ppid, pcpu, pmem, rss, command = parts
                command_lower = command.lower()
                include = (
                    "peekaboo_evidence_wall.py" in command
                    or "peekaboo capture live" in command
                    or "peekaboo.app" in command_lower
                    or "peekaboo daemon" in command_lower
                )
                if not include:
                    continue
                cpu = parse_float(pcpu)
                item = {
                    "pid": int(pid),
                    "ppid": int(ppid),
                    "cpu_percent": cpu,
                    "mem_percent": parse_float(pmem),
                    "rss_kb": int(float(rss)),
                    "command": command[:220],
                }
                total_cpu += cpu
                if "peekaboo_evidence_wall.py" in command:
                    python_cpu += cpu
                if "peekaboo capture live" in command:
                    capture_cpu += cpu
                processes.append(item)
        payload = {
            "metrics_ts": utc_now(),
            "target_cpu_percent": 5,
            "total_cpu_percent": round(total_cpu, 2),
            "python_cpu_percent": round(python_cpu, 2),
            "capture_cpu_percent": round(capture_cpu, 2),
            "process_count": len(processes),
            "processes": sorted(processes, key=lambda item: item["cpu_percent"], reverse=True)[:20],
        }
        self.state.publish("metrics", payload)


class EvidenceHandler(BaseHTTPRequestHandler):
    state: EvidenceState

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path
        if path == "/":
            self.serve_file(STATIC / "index.html", "text/html; charset=utf-8")
        elif path == "/favicon.ico":
            self.send_response(HTTPStatus.NO_CONTENT)
            self.end_headers()
        elif path.startswith("/static/"):
            rel = Path(unquote(path.removeprefix("/static/")))
            self.serve_file(STATIC / rel, content_type_for(STATIC / rel))
        elif path.startswith("/evidence/"):
            rel = Path(unquote(path.removeprefix("/evidence/")))
            target = (self.state.args.evidence_root / rel).resolve()
            if not str(target).startswith(str(self.state.args.evidence_root.resolve())):
                self.send_error(HTTPStatus.FORBIDDEN)
            else:
                self.serve_file(target, content_type_for(target))
        elif path in {"/state", "/agent-feed/latest"}:
            self.send_json(self.state.snapshot())
        elif path == "/metrics":
            self.send_json(self.state.snapshot().get("metrics", {}))
        elif path in {"/events", "/agent-feed/stream"}:
            self.serve_events()
        else:
            self.send_error(HTTPStatus.NOT_FOUND)

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/api/refresh":
            self.state.publish("notice", {"summary": "manual refresh requested"})
            self.send_json({"ok": True})
        else:
            self.send_error(HTTPStatus.NOT_FOUND)

    def serve_file(self, path: Path, content_type: str) -> None:
        if not path.exists() or not path.is_file():
            self.send_error(HTTPStatus.NOT_FOUND)
            return
        data = path.read_bytes()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(data)

    def send_json(self, payload: Any) -> None:
        data = json.dumps(payload, indent=2, default=str).encode()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def serve_events(self) -> None:
        subscriber = self.state.subscribe()
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "keep-alive")
        self.end_headers()
        hello = {"seq": 0, "id": uuid.uuid4().hex, "ts": utc_now(), "type": "hello", "payload": {"run_id": self.state.run_id}}
        try:
            self.write_event(hello)
            while True:
                try:
                    event = subscriber.get(timeout=15)
                    self.write_event(event)
                except queue.Empty:
                    self.wfile.write(b": heartbeat\n\n")
                    self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            self.state.unsubscribe(subscriber)

    def write_event(self, event: dict[str, Any]) -> None:
        data = json.dumps(event, separators=(",", ":"), default=str).encode()
        self.wfile.write(b"data: " + data + b"\n\n")
        self.wfile.flush()

    def log_message(self, fmt: str, *args: Any) -> None:
        sys.stderr.write("[%s] %s\n" % (self.log_date_time_string(), fmt % args))


def content_type_for(path: Path) -> str:
    suffix = path.suffix.lower()
    return {
        ".html": "text/html; charset=utf-8",
        ".css": "text/css; charset=utf-8",
        ".js": "application/javascript; charset=utf-8",
        ".json": "application/json; charset=utf-8",
        ".ndjson": "application/x-ndjson; charset=utf-8",
        ".txt": "text/plain; charset=utf-8",
        ".png": "image/png",
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".svg": "image/svg+xml",
        ".mp4": "video/mp4",
        ".sqlite3": "application/octet-stream",
    }.get(suffix, "application/octet-stream")


def parse_float(value: str) -> float:
    with suppress(ValueError):
        return float(value)
    return 0.0


def summarize_ui_snapshot(result: dict[str, Any], screenshot: Path, output: Path, state: EvidenceState) -> dict[str, Any]:
    data = result.get("json", {}).get("data", {}) if result.get("json") else {}
    elements = data.get("ui_elements") or []
    text_elements = []
    for element in elements:
        label = element.get("label") or element.get("title") or element.get("value") or element.get("description")
        if label:
            text_elements.append(
                {
                    "id": element.get("id"),
                    "role": element.get("role"),
                    "label": str(label)[:240],
                    "bounds": element.get("bounds"),
                    "is_actionable": element.get("is_actionable"),
                }
            )
    return {
        "ts": utc_now(),
        "ok": bool(result["ok"]),
        "application_name": data.get("application_name"),
        "window_title": data.get("window_title"),
        "element_count": data.get("element_count", len(elements)),
        "interactable_count": data.get("interactable_count"),
        "screenshot_url": state.rel_url(screenshot) if screenshot.exists() else None,
        "raw_json_url": state.rel_url(output),
        "snapshot_id": data.get("snapshot_id"),
        "text_elements": text_elements[:80],
        "elapsed_ms": result.get("elapsed_ms"),
        "error": None if result["ok"] else (result.get("stderr") or result.get("stdout") or result.get("error")),
    }


def mock_svg(label: str, ts: str, index: int) -> str:
    escaped = html.escape(label)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900" viewBox="0 0 1440 900">
<rect width="1440" height="900" fill="#111111"/>
<rect x="32" y="32" width="1376" height="836" rx="8" fill="#1f1f1f" stroke="#555555"/>
<text x="56" y="86" fill="#f3f0e8" font-family="Menlo, monospace" font-size="34">{escaped}</text>
<text x="56" y="140" fill="#65d478" font-family="Menlo, monospace" font-size="24">{html.escape(ts)}</text>
<text x="56" y="220" fill="#d8d8d8" font-family="Menlo, monospace" font-size="28">mock capture-live frame {index:04d}</text>
<text x="56" y="280" fill="#f4b34f" font-family="Menlo, monospace" font-size="24">full artifact preserved, dashboard is metadata-first</text>
</svg>
"""


def mock_contact_svg(label: str) -> str:
    escaped = html.escape(label)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="260" viewBox="0 0 1200 260">
<rect width="1200" height="260" fill="#111111"/>
<text x="28" y="52" fill="#f3f0e8" font-family="Menlo, monospace" font-size="30">{escaped} contact sheet</text>
<rect x="28" y="86" width="180" height="130" fill="#222222" stroke="#65d478"/>
<rect x="228" y="86" width="180" height="130" fill="#2a1d1d" stroke="#ff6a5f"/>
<rect x="428" y="86" width="180" height="130" fill="#1d262a" stroke="#57c7d4"/>
</svg>
"""


def mock_inventory(ts: str) -> dict[str, Any]:
    return {
        "inventory_ts": ts,
        "peekaboo_version": "mock",
        "permissions": "mock granted",
        "screens": {"data": {"screens": [{"index": 0}, {"index": 1}, {"index": 2}]}},
        "apps": {"data": {"applications": [{"name": "Codex"}]}},
        "codex_windows": {"data": {"windows": [{"title": "Codex", "index": 0}]}},
        "menubar": {"data": {"count": 4}},
        "codex_menu": {"data": {"menu_structure": []}},
        "tools": {"data": {"count": 40, "tools": [{"name": "image"}, {"name": "see"}, {"name": "capture"}]}},
    }


def resolve_screen_labels() -> dict[int, str]:
    labels: dict[int, str] = {}
    screens_result = run_command(["peekaboo", "list", "screens", "--json"], timeout=10)
    profiler_result = run_command(["system_profiler", "SPDisplaysDataType", "-json"], timeout=15, lock_peekaboo=False)
    display_names: dict[str, str] = {}
    profiler = profiler_result.get("json") if profiler_result.get("ok") else None
    if profiler:
        for gpu in profiler.get("SPDisplaysDataType", []):
            for display in gpu.get("spdisplays_ndrvs", []) or []:
                display_id = str(display.get("_spdisplays_displayID", ""))
                name = display.get("_name")
                if display_id and name:
                    display_names[display_id] = name

    screens = None
    if screens_result.get("json"):
        screens = screens_result["json"].get("data", {}).get("screens")
    for screen in screens or []:
        index = screen.get("index")
        display_id = str(screen.get("displayID", ""))
        name = display_names.get(display_id) or screen.get("name") or f"Screen {index}"
        if isinstance(index, int):
            labels[index] = f"{name} (screen {index})"
    return labels


def parse_screen_label_overrides(value: str) -> dict[int, str]:
    labels: dict[int, str] = {}
    for raw_item in value.split(","):
        item = raw_item.strip()
        if not item:
            continue
        if "=" not in item:
            raise ValueError(f"screen label override must use index=label: {item!r}")
        raw_index, raw_label = item.split("=", 1)
        raw_index = raw_index.strip().removeprefix("screen-")
        label = raw_label.strip()
        if not raw_index or not label:
            raise ValueError(f"screen label override must include both index and label: {item!r}")
        try:
            index = int(raw_index)
        except ValueError as exc:
            raise ValueError(f"screen label index must be an integer: {raw_index!r}") from exc
        labels[index] = label
    return labels


def resolve_codex_window(args: Args) -> None:
    result = run_command(["peekaboo", "list", "windows", "--app", args.codex_app, "--json"], timeout=12)
    if not result.get("ok") or not result.get("json"):
        return
    windows = result["json"].get("data", {}).get("windows") or []
    candidates = []
    for window in windows:
        if window.get("isMinimized") or window.get("isOffScreen"):
            continue
        bounds = window.get("bounds") or [[0, 0], [0, 0]]
        try:
            width = float(bounds[1][0])
            height = float(bounds[1][1])
        except (TypeError, ValueError, IndexError):
            width = height = 0
        title = str(window.get("title") or "")
        title_score = 1 if args.codex_window_title.lower() in title.lower() else 0
        candidates.append((title_score, width * height, window))
    if not candidates:
        return
    _, _, chosen = sorted(candidates, key=lambda item: (item[0], item[1]), reverse=True)[0]
    args.codex_window_id = chosen.get("window_id")
    args.codex_window_index = chosen.get("index")
    bounds = chosen.get("bounds") or []
    try:
        origin = bounds[0]
        size = bounds[1]
        args.codex_region = ",".join(
            str(int(round(value))) for value in (origin[0], origin[1], size[0], size[1])
        )
    except (TypeError, ValueError, IndexError):
        args.codex_region = None
    if chosen.get("title"):
        args.codex_window_title = str(chosen["title"])


def codex_image_window_args(args: Args) -> list[str]:
    if args.codex_window_id is not None:
        return ["--window-id", str(args.codex_window_id)]
    return ["--window-title", args.codex_window_title]


def codex_see_window_args(args: Args) -> list[str]:
    if args.codex_window_id is not None:
        return ["--window-id", str(args.codex_window_id)]
    return ["--window-title", args.codex_window_title]


def codex_live_window_args(args: Args) -> list[str]:
    if args.codex_window_index is not None:
        return ["--window-index", str(args.codex_window_index)]
    return ["--window-title", args.codex_window_title]


def apply_profile_defaults(ns: argparse.Namespace) -> argparse.Namespace:
    profile_defaults = {
        "low-power": {
            "segment_duration": 45,
            "screen_idle_fps": 1.0,
            "screen_active_fps": 1.0,
            "codex_idle_fps": 0.5,
            "codex_active_fps": 1.0,
            "threshold": 0.0,
            "heartbeat_sec": 5.0,
            "quiet_ms": 1000,
            "resolution_cap": 1440,
            "max_frames": 500,
            "screen_snapshot_interval": 8.0,
            "codex_snapshot_interval": 1.0,
            "feature_interval": 120.0,
            "inventory_interval": 90.0,
            "metrics_interval": 5.0,
        },
        "showcase": {
            "segment_duration": 60,
            "screen_idle_fps": 1.0,
            "screen_active_fps": 3.0,
            "codex_idle_fps": 2.0,
            "codex_active_fps": 8.0,
            "threshold": 0.0,
            "heartbeat_sec": 3.0,
            "quiet_ms": 500,
            "resolution_cap": 2560,
            "max_frames": 1000,
            "screen_snapshot_interval": 4.0,
            "codex_snapshot_interval": 0.5,
            "feature_interval": 90.0,
            "inventory_interval": 60.0,
            "metrics_interval": 4.0,
        },
        "maximal": {
            "segment_duration": 180,
            "screen_idle_fps": 5.0,
            "screen_active_fps": 15.0,
            "codex_idle_fps": 5.0,
            "codex_active_fps": 15.0,
            "threshold": 0.0,
            "heartbeat_sec": 1.0,
            "quiet_ms": 0,
            "resolution_cap": 10000,
            "max_frames": 3000,
            "screen_snapshot_interval": 2.0,
            "codex_snapshot_interval": 0.25,
            "feature_interval": 60.0,
            "inventory_interval": 45.0,
            "metrics_interval": 3.0,
        },
    }
    defaults = profile_defaults[ns.profile]
    for key, value in defaults.items():
        if getattr(ns, key) is None:
            setattr(ns, key, value)
    return ns


def parse_args() -> Args:
    parser = argparse.ArgumentParser(description="Run the local Peekaboo Evidence Wall demo.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--mode", choices=["auto", "real", "mock"], default="auto")
    parser.add_argument("--profile", choices=["low-power", "showcase", "maximal"], default="low-power")
    parser.add_argument("--work-screens", default="0,1", help="Comma-separated Peekaboo screen indexes to capture.")
    parser.add_argument("--screen-labels", default="", help='Comma-separated screen label overrides, like "0=DELL U3219Q,1=Studio Display".')
    parser.add_argument("--codex-app", default="Codex")
    parser.add_argument("--codex-window-title", default="Codex")
    parser.add_argument("--stream-work-screens", action="store_true", help="Use capture-live for work screens even in low-power mode.")
    parser.add_argument("--stream-codex", action="store_true", help="Use capture-live for Codex even in low-power mode.")
    parser.add_argument("--screen-snapshot-interval", type=float)
    parser.add_argument("--codex-snapshot-interval", type=float)
    parser.add_argument("--sample-see", action="store_true", help="Run Peekaboo see probes; disabled by default because this local build can hang.")
    parser.add_argument("--segment-duration", type=int)
    parser.add_argument("--screen-idle-fps", type=float)
    parser.add_argument("--screen-active-fps", type=float)
    parser.add_argument("--codex-idle-fps", type=float)
    parser.add_argument("--codex-active-fps", type=float)
    parser.add_argument("--threshold", type=float)
    parser.add_argument("--heartbeat-sec", type=float)
    parser.add_argument("--quiet-ms", type=int)
    parser.add_argument("--max-frames", type=int)
    parser.add_argument("--resolution-cap", type=int)
    parser.add_argument("--diff-strategy", choices=["fast", "quality"], default="fast")
    parser.add_argument("--capture-engine", choices=["auto", "classic", "cg", "modern", "sckit"], default="modern")
    parser.add_argument("--poll-interval", type=float, default=0.5)
    parser.add_argument("--feature-interval", type=float)
    parser.add_argument("--inventory-interval", type=float)
    parser.add_argument("--metrics-interval", type=float)
    parser.add_argument("--sample-video", action="store_true", help="Mark video ingest explicitly; does not run automation.")
    parser.add_argument("--evidence-root", default=str(ROOT / ".evidence"))
    ns = apply_profile_defaults(parser.parse_args())

    work_screens = [int(item.strip()) for item in ns.work_screens.split(",") if item.strip()]
    try:
        screen_labels = parse_screen_label_overrides(ns.screen_labels)
    except ValueError as exc:
        parser.error(str(exc))
    mode = ns.mode
    if mode == "auto":
        mode = "real" if shutil.which("peekaboo") else "mock"

    return Args(
        host=ns.host,
        port=ns.port,
        mode=mode,
        profile=ns.profile,
        work_screens=work_screens,
        codex_app=ns.codex_app,
        codex_window_title=ns.codex_window_title,
        codex_window_id=None,
        codex_window_index=None,
        codex_region=None,
        screen_labels=screen_labels,
        stream_work_screens=bool(ns.stream_work_screens or ns.profile in {"showcase", "maximal"}),
        stream_codex=bool(ns.stream_codex or ns.profile in {"showcase", "maximal"}),
        screen_snapshot_interval=max(1.0, float(ns.screen_snapshot_interval)),
        codex_snapshot_interval=max(0.5, float(ns.codex_snapshot_interval)),
        sample_see=bool(ns.sample_see),
        segment_duration=max(2, min(180, int(ns.segment_duration))),
        screen_idle_fps=max(0.1, float(ns.screen_idle_fps)),
        screen_active_fps=max(0.1, min(15.0, float(ns.screen_active_fps))),
        codex_idle_fps=max(0.1, float(ns.codex_idle_fps)),
        codex_active_fps=max(0.1, min(15.0, float(ns.codex_active_fps))),
        threshold=max(0.0, float(ns.threshold)),
        heartbeat_sec=max(0.1, float(ns.heartbeat_sec)),
        quiet_ms=max(0, int(ns.quiet_ms)),
        max_frames=max(1, int(ns.max_frames)),
        resolution_cap=max(480, int(ns.resolution_cap)),
        diff_strategy=ns.diff_strategy,
        capture_engine=ns.capture_engine,
        poll_interval=max(0.2, float(ns.poll_interval)),
        feature_interval=max(30.0, float(ns.feature_interval)),
        inventory_interval=max(20.0, float(ns.inventory_interval)),
        metrics_interval=max(2.0, float(ns.metrics_interval)),
        sample_video=bool(ns.sample_video),
        evidence_root=Path(ns.evidence_root).resolve(),
    )


def build_workers(state: EvidenceState, stop_event: threading.Event) -> list[Worker]:
    workers: list[Worker] = []
    if not state.args.stream_work_screens:
        state.publish(
            "feature",
            {
                "feature_id": "capture-live-screen",
                "status": "available",
                "summary": "continuous work-screen capture-live is reserved for showcase/maximal profiles to stay near 5% CPU",
            },
        )
    if not state.args.stream_codex:
        state.publish(
            "feature",
            {
                "feature_id": "capture-live-window",
                "status": "available",
                "summary": "continuous Codex capture-live is available with --stream-codex or showcase/maximal; low-power uses keyframes",
            },
        )
    for idx, screen_index in enumerate(state.args.work_screens[:2]):
        lane_id = f"screen-{screen_index}"
        source_label = state.args.screen_labels.get(screen_index, f"Screen {screen_index}")
        if state.args.stream_work_screens:
            workers.append(
                LiveLaneWorker(
                    state,
                    stop_event,
                    LaneConfig(
                        lane_id=lane_id,
                        source_label=source_label,
                        feature_id="capture-live-screen",
                        mode="screen",
                        command_target=["--mode", "screen", "--screen-index", str(screen_index)],
                        idle_fps=state.args.screen_idle_fps,
                        active_fps=state.args.screen_active_fps,
                        startup_delay=idx * 1.5,
                    ),
                )
            )
        else:
            workers.append(
                SnapshotLaneWorker(
                    state,
                    stop_event,
                    lane_id=lane_id,
                    source_label=source_label,
                    screen_index=screen_index,
                    interval=state.args.screen_snapshot_interval,
                    startup_delay=idx * 1.5,
                )
            )

    if state.args.stream_codex:
        codex_target = [
            "--app",
            state.args.codex_app,
            "--mode",
            "window",
            *codex_live_window_args(state.args),
        ]
        codex_label = f"{state.args.codex_app} window"
        if state.args.profile == "low-power" and state.args.codex_region:
            codex_target = ["--mode", "area", "--region", state.args.codex_region]
            codex_label = f"{state.args.codex_app} region"
        workers.append(
            LiveLaneWorker(
                state,
                stop_event,
                LaneConfig(
                    lane_id="codex-window",
                    source_label=codex_label,
                    feature_id="capture-live-window",
                    mode="area" if state.args.profile == "low-power" and state.args.codex_region else "window",
                    command_target=codex_target,
                    idle_fps=state.args.codex_idle_fps,
                    active_fps=state.args.codex_active_fps,
                    startup_delay=3.0,
                ),
            )
        )
    else:
        workers.append(
            ImageSnapshotWorker(
                state,
                stop_event,
                lane_id="codex-window",
                source_label=f"{state.args.codex_app} keyframes",
                feature_id="image-codex-snapshots",
                command_target=[
                    "--app",
                    state.args.codex_app,
                    "--mode",
                    "window",
                    *codex_image_window_args(state.args),
                ],
                interval=state.args.codex_snapshot_interval,
                startup_delay=3.0,
            )
        )
    workers.append(FeatureProbeWorker(state, "feature-probes", stop_event))
    workers.append(MetricsWorker(state, "metrics", stop_event))
    return workers


def choose_port(host: str, requested: int) -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            sock.bind((host, requested))
            return requested
        except OSError:
            pass
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind((host, 0))
        return int(sock.getsockname()[1])


def main() -> int:
    args = parse_args()
    args.evidence_root.mkdir(parents=True, exist_ok=True)

    if args.mode == "real" and not shutil.which("peekaboo"):
        print("peekaboo not found; install with: brew install steipete/tap/peekaboo", file=sys.stderr)
        return 2

    if args.mode == "real":
        manual_screen_labels = dict(args.screen_labels)
        resolved_screen_labels = resolve_screen_labels()
        resolved_screen_labels.update(manual_screen_labels)
        args.screen_labels = {
            index: resolved_screen_labels.get(index, f"Screen {index}")
            for index in args.work_screens
        }
        resolve_codex_window(args)

    stop_event = threading.Event()
    state = EvidenceState(args)
    workers = build_workers(state, stop_event)
    for worker in workers:
        worker.start()

    def stop(_signum: int, _frame: Any) -> None:
        stop_event.set()

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)

    port = choose_port(args.host, args.port)
    EvidenceHandler.state = state
    server = ThreadingHTTPServer((args.host, port), EvidenceHandler)
    server.timeout = 0.5
    print(f"Peekaboo Evidence Wall: http://{args.host}:{port}")
    print(f"Evidence root: {state.run_root}")
    print(f"Profile: {args.profile}; target CPU <= 5%")
    try:
        while not stop_event.is_set():
            server.handle_request()
    finally:
        stop_event.set()
        server.server_close()
        for worker in workers:
            worker.join(timeout=3)
        state.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
