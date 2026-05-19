#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
import queue
import shutil
import signal
import socket
import struct
import subprocess
import sys
import threading
import time
import uuid
from collections import deque
from contextlib import nullcontext
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


def run_command(args: list[str], timeout: float = 20.0) -> dict[str, Any]:
    started = time.monotonic()
    command_lock = PEEKABOO_LOCK if args and Path(args[0]).name == "peekaboo" else nullcontext()
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


def clean_text(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return str(value)


def sha256_file(path: Path) -> str | None:
    if not path.exists():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
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


@dataclass
class Args:
    host: str
    port: int
    mode: str
    work_screens: list[int]
    codex_app: str
    codex_window_title: str
    codex_window_id: int | None
    codex_window_index: int | None
    screen_labels: dict[int, str]
    screen_interval: float
    codex_fps: float
    ui_interval: float
    inventory_interval: float
    burst_interval: float
    burst_duration: int
    evidence_root: Path


class TextLedger:
    def __init__(self) -> None:
        self._lines: dict[str, dict[str, Any]] = {}
        self._lock = threading.Lock()

    def observe(self, text: str, ts: str) -> list[dict[str, Any]]:
        lines = []
        for raw in text.splitlines():
            line = " ".join(raw.strip().split())
            if len(line) >= 3:
                lines.append(line[:260])

        now_ms = time.time() * 1000
        with self._lock:
            for line in lines:
                entry = self._lines.setdefault(
                    line,
                    {
                        "line": line,
                        "first_seen": ts,
                        "last_seen": ts,
                        "first_seen_ms": now_ms,
                        "last_seen_ms": now_ms,
                        "observations": 0,
                    },
                )
                entry["last_seen"] = ts
                entry["last_seen_ms"] = now_ms
                entry["observations"] += 1

            ranked = sorted(
                self._lines.values(),
                key=lambda item: (item["last_seen_ms"], item["observations"]),
                reverse=True,
            )
            for entry in ranked:
                entry["visible_ms"] = max(0, entry["last_seen_ms"] - entry["first_seen_ms"])
            return [dict(item) for item in ranked[:80]]

    def latest(self) -> list[dict[str, Any]]:
        with self._lock:
            ranked = sorted(
                self._lines.values(),
                key=lambda item: (item["last_seen_ms"], item["observations"]),
                reverse=True,
            )
            now_ms = time.time() * 1000
            result = []
            for item in ranked[:80]:
                entry = dict(item)
                entry["visible_ms"] = max(entry.get("visible_ms", 0), min(now_ms - entry["first_seen_ms"], entry["last_seen_ms"] - entry["first_seen_ms"]))
                result.append(entry)
            return result


class EvidenceState:
    def __init__(self, args: Args) -> None:
        self.args = args
        self.run_id = datetime.now().strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:6]
        self.run_root = args.evidence_root / self.run_id
        self.run_root.mkdir(parents=True, exist_ok=True)
        self.lock = threading.Lock()
        self.events: deque[dict[str, Any]] = deque(maxlen=300)
        self.lanes: dict[str, dict[str, Any]] = {}
        self.capabilities: dict[str, Any] = {}
        self.latest_ui: dict[str, Any] | None = None
        self.latest_live_burst: dict[str, Any] | None = None
        self.terminal_ledger = TextLedger()
        self.subscribers: list[queue.Queue[dict[str, Any]]] = []

    def rel_url(self, path: Path) -> str:
        return "/evidence/" + path.relative_to(self.args.evidence_root).as_posix()

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "run": {
                    "id": self.run_id,
                    "mode": self.args.mode,
                    "evidence_root": str(self.run_root),
                    "work_screens": self.args.work_screens,
                    "codex_app": self.args.codex_app,
                    "codex_window_title": self.args.codex_window_title,
                },
                "lanes": dict(self.lanes),
                "capabilities": dict(self.capabilities),
                "latest_ui": dict(self.latest_ui) if self.latest_ui else None,
                "latest_live_burst": dict(self.latest_live_burst) if self.latest_live_burst else None,
                "terminal_lines": self.terminal_ledger.latest(),
                "events": list(self.events)[-120:],
            }

    def publish(self, event_type: str, payload: dict[str, Any]) -> None:
        event = {
            "id": uuid.uuid4().hex,
            "ts": utc_now(),
            "type": event_type,
            "payload": payload,
        }
        if event_type == "frame":
            with self.lock:
                self.lanes[payload["lane_id"]] = payload
        elif event_type == "capabilities":
            with self.lock:
                self.capabilities.update(payload)
        elif event_type == "ui_snapshot":
            with self.lock:
                self.latest_ui = payload
        elif event_type == "live_burst":
            with self.lock:
                self.latest_live_burst = payload

        with self.lock:
            self.events.append(dict(event))
            feed = self.snapshot_unlocked()
            subscribers = list(self.subscribers)

        event = {**event, "feed": feed}
        for subscriber in subscribers:
            try:
                subscriber.put_nowait(event)
            except queue.Full:
                pass

    def snapshot_unlocked(self) -> dict[str, Any]:
        return {
            "run": {
                "id": self.run_id,
                "mode": self.args.mode,
                "evidence_root": str(self.run_root),
                "work_screens": self.args.work_screens,
                "codex_app": self.args.codex_app,
                "codex_window_title": self.args.codex_window_title,
            },
            "lanes": dict(self.lanes),
            "capabilities": dict(self.capabilities),
            "latest_ui": dict(self.latest_ui) if self.latest_ui else None,
            "latest_live_burst": dict(self.latest_live_burst) if self.latest_live_burst else None,
            "terminal_lines": self.terminal_ledger.latest(),
            "events": list(self.events)[-120:],
        }

    def subscribe(self) -> queue.Queue[dict[str, Any]]:
        subscriber: queue.Queue[dict[str, Any]] = queue.Queue(maxsize=100)
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


class ImageWorker(Worker):
    def __init__(
        self,
        state: EvidenceState,
        stop_event: threading.Event,
        lane_id: str,
        source_label: str,
        interval: float,
        command_args: list[str],
        mock_text: str,
        ocr: bool = False,
    ) -> None:
        super().__init__(state, f"image-{lane_id}", stop_event)
        self.lane_id = lane_id
        self.source_label = source_label
        self.interval = interval
        self.command_args = command_args
        self.mock_text = mock_text
        self.ocr = ocr
        self.previous_hash: str | None = None
        self.index = 0
        self.last_ocr = 0.0

    def run(self) -> None:
        lane_dir = self.state.run_root / self.lane_id
        lane_dir.mkdir(parents=True, exist_ok=True)
        while not self.stop_event.is_set():
            started = time.monotonic()
            self.index += 1
            ts = utc_now()
            image_path = lane_dir / f"frame-{self.index:06d}.png"
            json_path = lane_dir / f"frame-{self.index:06d}.json"

            if self.state.args.mode == "mock":
                svg_path = lane_dir / f"frame-{self.index:06d}.svg"
                self.write_mock_svg(svg_path, ts)
                image_path = svg_path
                result = {"ok": True, "args": ["mock", self.lane_id], "elapsed_ms": 1.0, "json": {"success": True}}
            else:
                args = [*self.command_args, "--path", str(image_path), "--json", "--no-remote"]
                result = run_command(args, timeout=max(8.0, self.interval * 4))
                json_path.write_text(json.dumps(result, indent=2, default=str))

            digest = sha256_file(image_path)
            changed = digest != self.previous_hash
            self.previous_hash = digest

            ocr_payload: dict[str, Any] | None = None
            if self.ocr and image_path.exists() and time.monotonic() - self.last_ocr >= 2.0:
                self.last_ocr = time.monotonic()
                ocr_payload = self.run_ocr(image_path, lane_dir, self.index, ts)

            frame = {
                "lane_id": self.lane_id,
                "source_label": self.source_label,
                "ts": ts,
                "ok": bool(result["ok"]) and image_path.exists(),
                "image_url": self.state.rel_url(image_path) if image_path.exists() else None,
                "artifact_path": str(image_path),
                "command": result.get("args"),
                "elapsed_ms": result.get("elapsed_ms"),
                "sha256": digest,
                "changed": changed,
                "dimensions": png_dimensions(image_path),
                "peekaboo": compact_peekaboo_result(result.get("json")),
                "ocr": ocr_payload,
                "error": None if result["ok"] else (result.get("stderr") or result.get("stdout") or result.get("error")),
            }
            self.state.publish("frame", frame)

            elapsed = time.monotonic() - started
            self.sleep(max(0.01, self.interval - elapsed))

    def write_mock_svg(self, path: Path, ts: str) -> None:
        escaped = html.escape(self.mock_text)
        path.write_text(
            f"""<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900" viewBox="0 0 1440 900">
<rect width="1440" height="900" fill="#111"/>
<rect x="32" y="32" width="1376" height="836" rx="8" fill="#1f1f1f" stroke="#555"/>
<text x="56" y="86" fill="#f3f0e8" font-family="Menlo, monospace" font-size="34">{html.escape(self.source_label)}</text>
<text x="56" y="140" fill="#65d478" font-family="Menlo, monospace" font-size="24">{html.escape(ts)}</text>
<text x="56" y="220" fill="#d8d8d8" font-family="Menlo, monospace" font-size="28">{escaped}</text>
<text x="56" y="280" fill="#f4b34f" font-family="Menlo, monospace" font-size="24">mock frame {self.index:06d}</text>
</svg>
"""
        )

    def run_ocr(self, image_path: Path, lane_dir: Path, index: int, ts: str) -> dict[str, Any]:
        if not shutil.which("tesseract"):
            return {"ok": False, "error": "tesseract not found"}
        text_path = lane_dir / f"ocr-{index:06d}.txt"
        if image_path.suffix == ".svg":
            ocr_text = self.mock_text
            text_path.write_text(ocr_text)
            lines = self.state.terminal_ledger.observe(ocr_text, ts)
            return {"ok": True, "text_url": self.state.rel_url(text_path), "line_count": len(lines), "engine": "mock"}

        result = run_command(["tesseract", str(image_path), "stdout", "--psm", "6", "-l", "eng"], timeout=8)
        ocr_text = result.get("stdout") or ""
        text_path.write_text(ocr_text)
        lines = self.state.terminal_ledger.observe(ocr_text, ts)
        return {
            "ok": bool(result["ok"]),
            "text_url": self.state.rel_url(text_path),
            "line_count": len([line for line in ocr_text.splitlines() if line.strip()]),
            "engine": "tesseract",
            "elapsed_ms": result.get("elapsed_ms"),
            "latest_lines": lines[:16],
            "error": None if result["ok"] else result.get("stderr"),
        }


class UiMapWorker(Worker):
    def run(self) -> None:
        lane_dir = self.state.run_root / "ui-map"
        lane_dir.mkdir(parents=True, exist_ok=True)
        index = 0
        while not self.stop_event.is_set():
            index += 1
            ts = utc_now()
            screenshot = lane_dir / f"ui-{index:06d}.png"
            output = lane_dir / f"ui-{index:06d}.json"
            if self.state.args.mode == "mock":
                payload = self.mock_payload(ts, screenshot)
            else:
                command = [
                    "peekaboo",
                    "see",
                    "--app",
                    self.state.args.codex_app,
                ]
                if self.state.args.codex_window_id is not None:
                    command += ["--window-id", str(self.state.args.codex_window_id)]
                else:
                    command += ["--window-title", self.state.args.codex_window_title]
                command += [
                    "--path",
                    str(screenshot),
                    "--json",
                    "--no-remote",
                    "--timeout-seconds",
                    "10",
                ]
                result = run_command(command, timeout=15)
                output.write_text(json.dumps(result, indent=2, default=str))
                payload = self.summarize(result, screenshot, output, ts)
            self.state.publish("ui_snapshot", payload)
            self.sleep(self.state.args.ui_interval)

    def mock_payload(self, ts: str, screenshot: Path) -> dict[str, Any]:
        return {
            "ts": ts,
            "ok": True,
            "application_name": "Codex",
            "window_title": "Codex",
            "element_count": 4,
            "interactable_count": 2,
            "screenshot_url": None,
            "ui_map_url": None,
            "snapshot_id": "mock",
            "text_elements": [
                {"id": "T1", "role": "text", "label": "uv run python demos/peekaboo-evidence-wall"},
                {"id": "B1", "role": "button", "label": "Agent JSON"},
                {"id": "T2", "role": "text", "label": "terminal output visible for 8s"},
            ],
        }

    def summarize(self, result: dict[str, Any], screenshot: Path, output: Path, ts: str) -> dict[str, Any]:
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
            "ts": ts,
            "ok": bool(result["ok"]),
            "application_name": data.get("application_name"),
            "window_title": data.get("window_title"),
            "element_count": data.get("element_count", len(elements)),
            "interactable_count": data.get("interactable_count"),
            "screenshot_url": self.state.rel_url(screenshot) if screenshot.exists() else None,
            "ui_map_path": data.get("ui_map"),
            "raw_json_url": self.state.rel_url(output),
            "snapshot_id": data.get("snapshot_id"),
            "text_elements": text_elements[:80],
            "elapsed_ms": result.get("elapsed_ms"),
            "error": None if result["ok"] else (result.get("stderr") or result.get("stdout") or result.get("error")),
        }


class LiveBurstWorker(Worker):
    def run(self) -> None:
        index = 0
        while not self.stop_event.is_set():
            index += 1
            burst_dir = self.state.run_root / "live-bursts" / f"burst-{index:04d}"
            burst_dir.mkdir(parents=True, exist_ok=True)
            ts = utc_now()
            if self.state.args.mode == "mock":
                payload = self.mock_burst(ts, burst_dir)
            else:
                result = run_command(
                    [
                        "peekaboo",
                        "capture",
                        "live",
                        "--app",
                        self.state.args.codex_app,
                        "--mode",
                        "window",
                        *codex_live_window_args(self.state.args),
                        "--duration",
                        str(self.state.args.burst_duration),
                        "--active-fps",
                        "12",
                        "--idle-fps",
                        "2",
                        "--threshold",
                        "0.2",
                        "--highlight-changes",
                        "--capture-engine",
                        "modern",
                        "--path",
                        str(burst_dir),
                        "--json",
                        "--no-remote",
                    ],
                    timeout=self.state.args.burst_duration + 20,
                )
                (burst_dir / "command-output.json").write_text(json.dumps(result, indent=2, default=str))
                payload = self.summarize(result, burst_dir, ts)
            self.state.publish("live_burst", payload)
            self.sleep(self.state.args.burst_interval)

    def mock_burst(self, ts: str, burst_dir: Path) -> dict[str, Any]:
        contact = burst_dir / "contact.svg"
        contact.write_text(
            """<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="260">
<rect width="1200" height="260" fill="#111"/>
<text x="28" y="52" fill="#f3f0e8" font-family="Menlo" font-size="34">Mock Peekaboo capture live burst</text>
<rect x="28" y="86" width="180" height="130" fill="#222" stroke="#65d478"/>
<rect x="228" y="86" width="180" height="130" fill="#2a1d1d" stroke="#ff6a5f"/>
<rect x="428" y="86" width="180" height="130" fill="#1d262a" stroke="#57c7d4"/>
</svg>
"""
        )
        return {
            "ts": ts,
            "ok": True,
            "summary": "mock live burst",
            "frames_kept": 3,
            "contact_sheet_url": self.state.rel_url(contact),
            "metadata_url": None,
            "frames": [
                {"file": "keep-0001.svg", "reason": "first", "changePercent": 100},
                {"file": "keep-0002.svg", "reason": "change", "changePercent": 9.2},
                {"file": "keep-0003.svg", "reason": "heartbeat", "changePercent": 0.4},
            ],
        }

    def summarize(self, result: dict[str, Any], burst_dir: Path, ts: str) -> dict[str, Any]:
        data = result.get("json", {}).get("data", {}) if result.get("json") else {}
        contact = data.get("contactSheet", {})
        contact_path = burst_dir / contact.get("file", "contact.png") if contact.get("file") else None
        metadata_file = Path(data["metadataFile"]) if data.get("metadataFile") else None
        frames = []
        for frame in data.get("frames", [])[:80]:
            item = dict(frame)
            if item.get("file"):
                item["url"] = self.state.rel_url(burst_dir / item["file"])
            frames.append(item)
        return {
            "ts": ts,
            "ok": bool(result["ok"]),
            "summary": f"capture live {len(frames)} kept frames",
            "frames_kept": data.get("stats", {}).get("framesKept", len(frames)),
            "stats": data.get("stats"),
            "scope": data.get("scope"),
            "warnings": data.get("warnings", []),
            "contact_sheet_url": self.state.rel_url(contact_path) if contact_path and contact_path.exists() else None,
            "metadata_url": self.state.rel_url(metadata_file) if metadata_file and metadata_file.exists() else None,
            "command_output_url": self.state.rel_url(burst_dir / "command-output.json"),
            "frames": frames,
            "error": None if result["ok"] else (result.get("stderr") or result.get("stdout") or result.get("error")),
        }


class InventoryWorker(Worker):
    def run(self) -> None:
        self.capture_inventory()
        while not self.sleep(self.state.args.inventory_interval):
            self.capture_inventory()

    def capture_inventory(self) -> None:
        ts = utc_now()
        inv_dir = self.state.run_root / "inventory"
        inv_dir.mkdir(parents=True, exist_ok=True)
        payload: dict[str, Any] = {"inventory_ts": ts}

        if self.state.args.mode == "mock":
            payload.update(
                {
                    "peekaboo_version": "mock",
                    "permissions": "mock granted",
                    "screens": {"data": {"screens": [{"index": 0}, {"index": 1}, {"index": 2}]}},
                    "apps": {"data": {"applications": [{"name": "Codex"}]}},
                    "codex_windows": {"data": {"windows": [{"title": "Codex"}]}},
                    "menubar": {"data": {"count": 4}},
                    "codex_menu": {"data": {"menu_structure": []}},
                    "tools": {"data": {"count": 26, "tools": [{"name": "image"}, {"name": "see"}, {"name": "capture"}]}},
                }
            )
            self.state.publish("capabilities", payload)
            return

        commands = {
            "peekaboo_version": ["peekaboo", "--version"],
            "permissions": ["peekaboo", "permissions", "status"],
            "screens": ["peekaboo", "list", "screens", "--json", "--no-remote"],
            "apps": ["peekaboo", "list", "apps", "--json", "--no-remote"],
            "codex_windows": ["peekaboo", "list", "windows", "--app", self.state.args.codex_app, "--json", "--no-remote"],
            "menubar": ["peekaboo", "list", "menubar", "--json", "--no-remote"],
            "codex_menu": ["peekaboo", "menu", "list", "--app", self.state.args.codex_app, "--json", "--no-remote"],
            "tools": ["peekaboo", "tools", "--json", "--no-remote"],
        }

        for key, command in commands.items():
            result = run_command(command, timeout=18)
            (inv_dir / f"{key}.json").write_text(json.dumps(result, indent=2, default=str))
            if key in ("peekaboo_version", "permissions"):
                payload[key] = result.get("stdout", "").strip() if result["ok"] else result.get("stderr", "").strip()
            else:
                payload[key] = result.get("json") if result["ok"] and result.get("json") else {"ok": False, "error": result.get("stderr") or result.get("stdout")}

        self.state.publish("capabilities", payload)


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
        elif path in ("/state", "/agent-feed/latest"):
            self.send_json(self.state.snapshot())
        elif path in ("/events", "/agent-feed/stream"):
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
        hello = {"id": uuid.uuid4().hex, "ts": utc_now(), "type": "hello", "payload": {}, "feed": self.state.snapshot()}
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
        ".txt": "text/plain; charset=utf-8",
        ".png": "image/png",
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".svg": "image/svg+xml",
        ".mp4": "video/mp4",
    }.get(suffix, "application/octet-stream")


def compact_peekaboo_result(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    data = value.get("data", {})
    files = data.get("files")
    observations = data.get("observations")
    compact: dict[str, Any] = {"success": value.get("success")}
    if files:
        compact["files"] = files[:3]
    if observations:
        compact["observation_count"] = len(observations)
    return compact


def resolve_screen_labels() -> dict[int, str]:
    labels: dict[int, str] = {}
    screens_result = run_command(["peekaboo", "list", "screens", "--json", "--no-remote"], timeout=10)
    profiler_result = run_command(["system_profiler", "SPDisplaysDataType", "-json"], timeout=15)
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


def resolve_codex_window(args: Args) -> None:
    result = run_command(["peekaboo", "list", "windows", "--app", args.codex_app, "--json", "--no-remote"], timeout=12)
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
    if chosen.get("title"):
        args.codex_window_title = str(chosen["title"])


def codex_image_window_args(args: Args) -> list[str]:
    if args.codex_window_id is not None:
        return ["--window-id", str(args.codex_window_id)]
    return ["--window-title", args.codex_window_title]


def codex_live_window_args(args: Args) -> list[str]:
    if args.codex_window_index is not None:
        return ["--window-index", str(args.codex_window_index)]
    return ["--window-title", args.codex_window_title]


def parse_args() -> Args:
    parser = argparse.ArgumentParser(description="Run the local Peekaboo Evidence Wall demo.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--mode", choices=["auto", "real", "mock"], default="auto")
    parser.add_argument("--work-screens", default="0,1", help="Comma-separated Peekaboo screen indexes to capture.")
    parser.add_argument("--codex-app", default="Codex")
    parser.add_argument("--codex-window-title", default="Codex")
    parser.add_argument("--screen-interval", type=float, default=2.0)
    parser.add_argument("--codex-fps", type=float, default=4.0)
    parser.add_argument("--ui-interval", type=float, default=8.0)
    parser.add_argument("--inventory-interval", type=float, default=25.0)
    parser.add_argument("--burst-interval", type=float, default=35.0)
    parser.add_argument("--burst-duration", type=int, default=6)
    parser.add_argument("--evidence-root", default=str(ROOT / ".evidence"))
    ns = parser.parse_args()

    work_screens = [int(item.strip()) for item in ns.work_screens.split(",") if item.strip()]
    mode = ns.mode
    if mode == "auto":
        mode = "real" if shutil.which("peekaboo") else "mock"

    return Args(
        host=ns.host,
        port=ns.port,
        mode=mode,
        work_screens=work_screens,
        codex_app=ns.codex_app,
        codex_window_title=ns.codex_window_title,
        codex_window_id=None,
        codex_window_index=None,
        screen_labels={},
        screen_interval=max(0.25, ns.screen_interval),
        codex_fps=max(0.2, ns.codex_fps),
        ui_interval=max(3.0, ns.ui_interval),
        inventory_interval=max(10.0, ns.inventory_interval),
        burst_interval=max(10.0, ns.burst_interval),
        burst_duration=max(2, min(180, ns.burst_duration)),
        evidence_root=Path(ns.evidence_root).resolve(),
    )


def build_workers(state: EvidenceState, stop_event: threading.Event) -> list[Worker]:
    workers: list[Worker] = []
    for idx, screen_index in enumerate(state.args.work_screens[:2]):
        lane_id = f"screen-{screen_index}"
        source_label = state.args.screen_labels.get(screen_index, f"Screen {screen_index}")
        workers.append(
            ImageWorker(
                state,
                stop_event,
                lane_id=lane_id,
                source_label=source_label,
                interval=state.args.screen_interval,
                command_args=[
                    "peekaboo",
                    "image",
                    "--mode",
                    "screen",
                    "--screen-index",
                    str(screen_index),
                    "--capture-engine",
                    "modern",
                    "--format",
                    "png",
                ],
                mock_text=f"work screen lane {idx + 1}",
            )
        )

    workers.append(
        ImageWorker(
            state,
            stop_event,
            lane_id="codex-terminal",
            source_label=f"{state.args.codex_app} high-fps",
            interval=1.0 / state.args.codex_fps,
            command_args=[
                "peekaboo",
                "image",
                "--app",
                state.args.codex_app,
                "--mode",
                "window",
                *codex_image_window_args(state.args),
                "--capture-engine",
                "modern",
                "--format",
                "png",
            ],
            mock_text="uv run pytest -q\nERROR test_terminal_visible_line failed\nreading focus paused near stack trace",
            ocr=True,
        )
    )
    workers.append(UiMapWorker(state, "ui-map", stop_event))
    workers.append(LiveBurstWorker(state, "live-burst", stop_event))
    workers.append(InventoryWorker(state, "inventory", stop_event))
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
        args.screen_labels = resolve_screen_labels()
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
    try:
        while not stop_event.is_set():
            server.handle_request()
    finally:
        stop_event.set()
        server.server_close()
        for worker in workers:
            worker.join(timeout=2)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
