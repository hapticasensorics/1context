from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from ..config import MemorySystem
from ..io_utils import atomic_write_json, exclusive_file_lock


STATE_SCHEMA_VERSION = "wiki.user-state.v1"
SETTINGS = {
    "theme": {"light", "dark", "auto"},
    "toc": {"full", "hidden"},
    "article-width": {"s", "m", "l"},
    "font-size": {"s", "m", "l"},
    "links-style": {"underline", "color"},
    "cover-image": {"show", "hide"},
    "border-radius": {"rounded", "square"},
    "article-style": {"full", "pics", "text"},
    "ai-provider": {"auto", "codex", "claude"},
}


def user_state_path(system: MemorySystem) -> Path:
    return system.runtime_dir / "wiki" / "state.json"


def load_user_state(system: MemorySystem) -> tuple[dict[str, Any], bool]:
    return load_user_state_unlocked(system)


def load_user_state_unlocked(system: MemorySystem) -> tuple[dict[str, Any], bool]:
    path = user_state_path(system)
    if not path.exists():
        return default_state(), False
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return default_state(), False
    state = sanitize_state(raw)
    state.setdefault("created_at", utc_now())
    return state, True


def save_user_state(system: MemorySystem, payload: dict[str, Any]) -> dict[str, Any]:
    path = user_state_path(system)
    with exclusive_file_lock(path.with_suffix(path.suffix + ".lock")):
        existing, exists = load_user_state_unlocked(system)
        next_state = merge_state(existing, payload)
        if not exists:
            next_state["created_at"] = utc_now()
        next_state["updated_at"] = utc_now()
        atomic_write_json(path, next_state)
        return next_state


def default_state() -> dict[str, Any]:
    now = utc_now()
    return {
        "schema_version": STATE_SCHEMA_VERSION,
        "created_at": now,
        "updated_at": now,
        "settings": {},
        "bookmarks": [],
        "chat": {},
        "recent_searches": [],
    }


def merge_state(existing: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
    merged = default_state()
    merged.update({key: value for key, value in existing.items() if key in merged})
    merged["schema_version"] = STATE_SCHEMA_VERSION
    if "settings" in payload:
        merged["settings"] = sanitize_settings(payload.get("settings"))
    if "bookmarks" in payload:
        merged["bookmarks"] = sanitize_bookmarks(payload.get("bookmarks"))
    if "chat" in payload:
        merged["chat"] = sanitize_chat(payload.get("chat"))
    if "recent_searches" in payload:
        merged["recent_searches"] = sanitize_recent_searches(payload.get("recent_searches"))
    return merged


def sanitize_state(raw: Any) -> dict[str, Any]:
    if not isinstance(raw, dict):
        return default_state()
    state = default_state()
    state["created_at"] = text(raw.get("created_at")) or state["created_at"]
    state["updated_at"] = text(raw.get("updated_at")) or state["updated_at"]
    state["settings"] = sanitize_settings(raw.get("settings"))
    state["bookmarks"] = sanitize_bookmarks(raw.get("bookmarks"))
    state["chat"] = sanitize_chat(raw.get("chat"))
    state["recent_searches"] = sanitize_recent_searches(raw.get("recent_searches"))
    return state


def sanitize_settings(raw: Any) -> dict[str, str]:
    if not isinstance(raw, dict):
        return {}
    settings: dict[str, str] = {}
    for key, allowed in SETTINGS.items():
        value = text(raw.get(key))
        if value in allowed:
            settings[key] = value
    return settings


def sanitize_bookmarks(raw: Any) -> list[dict[str, Any]]:
    if not isinstance(raw, list):
        return []
    bookmarks: list[dict[str, Any]] = []
    seen: set[str] = set()
    for item in raw[:500]:
        if not isinstance(item, dict):
            continue
        url = canonical_local_url(item.get("url") or item.get("route") or item.get("id"))
        if not url or url in seen:
            continue
        seen.add(url)
        bookmarks.append(
            {
                "id": url,
                "url": url,
                "title": text(item.get("title"))[:240] or url,
                "description": text(item.get("description"))[:500],
                "thumbnail": safe_asset_url(item.get("thumbnail")),
                "addedAt": text(item.get("addedAt"))[:80],
            }
        )
    return bookmarks


def sanitize_chat(raw: Any) -> dict[str, Any]:
    if not isinstance(raw, dict):
        return {}
    chat: dict[str, Any] = {}
    ai_display = text(raw.get("ai_display"))
    if ai_display in {"bubble", "panel"}:
        chat["ai_display"] = ai_display
    try:
        width = int(raw.get("ai_panel_width"))
    except (TypeError, ValueError):
        width = 0
    if width:
        chat["ai_panel_width"] = max(360, min(720, width))
    route = canonical_local_url(raw.get("latest_route"))
    if route:
        chat["latest_route"] = route
    thread = raw.get("latest_thread")
    if isinstance(thread, list):
        chat["latest_thread"] = sanitize_thread(thread)
    return chat


def sanitize_thread(raw: list[Any]) -> list[dict[str, Any]]:
    thread: list[dict[str, Any]] = []
    for item in raw[-50:]:
        if not isinstance(item, dict):
            continue
        role = text(item.get("role"))
        message = text(item.get("text"))
        if role not in {"user", "bot"} or not message:
            continue
        try:
            at = int(item.get("at") or 0)
        except (TypeError, ValueError):
            at = 0
        thread.append({"role": role, "text": message[:12000], "at": at})
    return thread


def sanitize_recent_searches(raw: Any) -> list[str]:
    if not isinstance(raw, list):
        return []
    seen: set[str] = set()
    searches: list[str] = []
    for item in raw[:50]:
        query = text(item)[:120]
        key = query.lower()
        if query and key not in seen:
            seen.add(key)
            searches.append(query)
    return searches


def canonical_local_url(value: Any) -> str:
    raw = text(value)
    if not raw or raw.startswith("//"):
        return ""
    if raw.startswith("http://127.0.0.1") or raw.startswith("http://localhost"):
        raw = "/" + raw.split("/", 3)[3] if "/" in raw[8:] else "/"
    if not raw.startswith("/"):
        return ""
    return raw.rstrip("/") if raw != "/" else raw


def safe_asset_url(value: Any) -> str:
    raw = text(value)
    if not raw:
        return ""
    if raw.startswith("/") or raw.startswith("http://127.0.0.1") or raw.startswith("http://localhost"):
        return raw[:1000]
    return ""


def text(value: Any) -> str:
    return str(value or "").strip()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
