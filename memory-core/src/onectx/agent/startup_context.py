from __future__ import annotations

import json
import os
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import quote


DEFAULT_WIKI_URL = "http://127.0.0.1:17319/for-you"
DEFAULT_TEMPLATE = """1Context is available for this session.

Local wiki: {wiki_url}
Use it as the source of durable personal and project context. Prefer reading or searching the wiki before guessing. The browser chat is the 1Context Librarian; durable memory jobs are separate."""
USER_CONFIG_PATH = Path.home() / ".1context" / "agent-startup.json"


@dataclass(frozen=True)
class StartupContext:
    provider: str
    cwd: Path
    repo_root: Path | None
    wiki_url: str
    message: str
    enabled: bool
    config_paths: tuple[Path, ...]

    def hook_payload(self, *, hook_event_name: str = "SessionStart") -> dict[str, Any]:
        if not self.enabled or not self.message.strip():
            return {}
        return {
            "hookSpecificOutput": {
                "hookEventName": hook_event_name,
                "additionalContext": self.message.strip(),
            }
        }

    def diagnostic_payload(self) -> dict[str, Any]:
        return {
            "provider": self.provider,
            "cwd": str(self.cwd),
            "repo_root": str(self.repo_root) if self.repo_root else "",
            "wiki_url": self.wiki_url,
            "enabled": self.enabled,
            "message": self.message,
            "config_paths": [str(path) for path in self.config_paths],
        }


def build_startup_context(
    *,
    provider: str = "generic",
    cwd: Path | str | None = None,
    hook_input: dict[str, Any] | None = None,
    wiki_url: str | None = None,
    template: str | None = None,
) -> StartupContext:
    hook_input = hook_input or {}
    resolved_cwd = resolve_cwd(cwd, hook_input)
    repo_root = find_repo_root(resolved_cwd)
    config, config_paths = load_startup_config(repo_root)
    resolved_provider = normalize_provider(provider or text(hook_input.get("provider")) or "generic")
    resolved_wiki_url = (
        text(wiki_url)
        or text(os.environ.get("ONECTX_WIKI_URL"))
        or text(config.get("wiki_url"))
        or DEFAULT_WIKI_URL
    )
    message_template = template or text(os.environ.get("ONECTX_STARTUP_CONTEXT_TEMPLATE")) or text(
        config.get("message_template")
    ) or DEFAULT_TEMPLATE
    enabled = bool(config.get("enabled", True))
    values = template_values(
        provider=resolved_provider,
        cwd=resolved_cwd,
        repo_root=repo_root,
        wiki_url=resolved_wiki_url,
    )
    message = render_template(message_template, values)
    return StartupContext(
        provider=resolved_provider,
        cwd=resolved_cwd,
        repo_root=repo_root,
        wiki_url=resolved_wiki_url,
        message=message,
        enabled=enabled,
        config_paths=config_paths,
    )


def resolve_cwd(cwd: Path | str | None, hook_input: dict[str, Any]) -> Path:
    raw = cwd or hook_input.get("cwd") or hook_input.get("current_dir") or os.getcwd()
    return Path(str(raw)).expanduser().resolve()


def find_repo_root(cwd: Path) -> Path | None:
    for path in (cwd, *cwd.parents):
        if (path / "1context.toml").exists() or (path / "AGENTS.md").exists():
            return path
    return None


def load_startup_config(repo_root: Path | None) -> tuple[dict[str, Any], tuple[Path, ...]]:
    paths: list[Path] = []
    env_path = text(os.environ.get("ONECTX_STARTUP_CONTEXT_CONFIG"))
    if env_path:
        paths.append(Path(env_path).expanduser())
    else:
        paths.append(USER_CONFIG_PATH)
    if repo_root is not None:
        paths.append(repo_root / "memory" / "runtime" / "agent" / "startup-context.json")

    config: dict[str, Any] = {}
    loaded: list[Path] = []
    for path in paths:
        payload = read_json_object(path)
        if payload is None:
            continue
        loaded.append(path)
        config.update(payload)
    return config, tuple(loaded)


def read_json_object(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return payload if isinstance(payload, dict) else None


def template_values(*, provider: str, cwd: Path, repo_root: Path | None, wiki_url: str) -> dict[str, str]:
    base_url = wiki_url.removesuffix("/for-you")
    return {
        "provider": provider,
        "cwd": str(cwd),
        "repo_root": str(repo_root or ""),
        "wiki_url": wiki_url,
        "routes_url": base_url + "/_routes",
        "search_url": base_url + "/api/wiki/search?q=" + quote("wiki"),
        "date": datetime.now(timezone.utc).date().isoformat(),
        "ui_name": "1Context Librarian",
        "chat_role": "wiki.chat_librarian",
        "memory_role": "memory.wiki.librarian",
    }


def render_template(template: str, values: dict[str, str]) -> str:
    return template.format_map(DefaultingDict(values)).strip()


def read_hook_input() -> dict[str, Any]:
    raw = sys.stdin.read().strip()
    if not raw:
        return {}
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        return {}
    return payload if isinstance(payload, dict) else {}


def normalize_provider(value: str) -> str:
    provider = value.strip().lower()
    if provider in {"claude-code", "claude_code"}:
        return "claude"
    return provider or "generic"


def text(value: Any) -> str:
    return str(value or "").strip()


class DefaultingDict(dict[str, str]):
    def __missing__(self, key: str) -> str:
        return "{" + key + "}"

