from __future__ import annotations

import json
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .startup_context import DEFAULT_TEMPLATE, DEFAULT_WIKI_URL, USER_CONFIG_PATH


SESSION_START_SOURCES = ("startup", "resume", "clear", "compact")


@dataclass(frozen=True)
class AgentIntegrationPlan:
    command: str
    startup_config_path: Path
    claude: dict[str, Any]
    codex: dict[str, Any]

    def to_payload(self) -> dict[str, Any]:
        return {
            "command": self.command,
            "startup_config_path": str(self.startup_config_path),
            "startup_config": default_startup_config(),
            "claude": self.claude,
            "codex": self.codex,
        }


def build_install_plan(*, command: str = "1context") -> AgentIntegrationPlan:
    command = command.strip() or "1context"
    claude_path = Path.home() / ".claude" / "settings.json"
    codex_path = Path.home() / ".codex" / "config.toml"
    return AgentIntegrationPlan(
        command=command,
        startup_config_path=USER_CONFIG_PATH,
        claude={
            "binary": shutil.which("claude") or "",
            "settings_path": str(claude_path),
            "available": bool(shutil.which("claude")),
            "strategy": "installer-managed global user settings; later this can become a bundled Claude plugin shim",
            "hook_payload": claude_hook_settings(command),
        },
        codex={
            "binary": shutil.which("codex") or "",
            "config_path": str(codex_path),
            "available": bool(shutil.which("codex")),
            "strategy": "installer-managed global ~/.codex/config.toml merge until Codex has plugin-style distribution",
            "toml_snippet": codex_hook_snippet(command),
        },
    )


def default_startup_config() -> dict[str, Any]:
    return {
        "enabled": True,
        "wiki_url": DEFAULT_WIKI_URL,
        "message_template": DEFAULT_TEMPLATE,
    }


def claude_hook_settings(command: str) -> dict[str, Any]:
    return {
        "hooks": {
            "SessionStart": [
                {
                    "matcher": source,
                    "hooks": [
                        {
                            "type": "command",
                            "command": f"{command} agent startup-context --provider claude",
                        }
                    ],
                }
                for source in SESSION_START_SOURCES
            ]
        }
    }


def codex_hook_snippet(command: str) -> str:
    entries = "\n".join(
        [
            "  { matcher = \""
            + source
            + "\", hooks = [\n"
            + "    { type = \"command\", command = \""
            + command
            + " agent startup-context --provider codex\" },\n"
            + "  ] },"
            for source in SESSION_START_SOURCES
        ]
    )
    return "\n".join(
        [
            "[features]",
            "codex_hooks = true",
            "",
            "[hooks]",
            "SessionStart = [",
            entries,
            "]",
        ]
    )


def write_default_startup_config(path: Path = USER_CONFIG_PATH, *, overwrite: bool = False) -> bool:
    if path.exists() and not overwrite:
        return False
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(default_startup_config(), indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return True


def executable_command() -> str:
    argv0 = Path(sys.argv[0]).name
    if argv0 in {"1context", "onectx"}:
        return argv0
    found = shutil.which("1context")
    return found or "1context"
