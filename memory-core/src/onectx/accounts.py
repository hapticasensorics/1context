from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .config import ConfigError, find_config_path, find_root, load_system, read_toml, resolve_path, unique


BUILTIN_ACCOUNTS: dict[str, dict[str, Any]] = {
    "openai": {
        "label": "OpenAI / ChatGPT",
        "kind": "model_provider",
        "provider": "openai",
        "default_mode": "chatgpt_subscription",
        "modes": ["chatgpt_subscription", "api_key"],
        "subscription": "Codex can use ChatGPT subscription auth through its native login/cache.",
        "api_key_env": "OPENAI_API_KEY",
        "secret_storage": "environment_or_codex_login",
        "agent_visible": False,
    },
    "anthropic": {
        "label": "Anthropic / Claude",
        "kind": "model_provider",
        "provider": "anthropic",
        "default_mode": "account",
        "modes": ["account", "api_key"],
        "subscription": "Claude Code can use a paid/Console-capable account through its native login/cache.",
        "api_key_env": "ANTHROPIC_API_KEY",
        "secret_storage": "environment_or_claude_login",
        "agent_visible": False,
    },
    "gemini": {
        "label": "Google Gemini",
        "kind": "model_provider",
        "provider": "google",
        "default_mode": "api_key",
        "modes": ["api_key"],
        "api_key_env": "GEMINI_API_KEY",
        "alternate_env": ["GOOGLE_API_KEY"],
        "secret_storage": "environment",
        "agent_visible": False,
    },
    "deepseek": {
        "label": "DeepSeek",
        "kind": "model_provider",
        "provider": "deepseek",
        "default_mode": "api_key",
        "modes": ["api_key"],
        "api_key_env": "DEEPSEEK_API_KEY",
        "secret_storage": "environment",
        "agent_visible": False,
    },
    "kimi": {
        "label": "Kimi / Moonshot",
        "kind": "model_provider",
        "provider": "moonshot",
        "default_mode": "api_key",
        "modes": ["api_key"],
        "api_key_env": "MOONSHOT_API_KEY",
        "secret_storage": "environment",
        "agent_visible": False,
    },
    "cloudflare": {
        "label": "Cloudflare",
        "kind": "infrastructure",
        "provider": "cloudflare",
        "default_mode": "api_token",
        "modes": ["api_token"],
        "token_env": "CLOUDFLARE_API_TOKEN",
        "secret_storage": "environment",
        "agent_visible": False,
    },
    "1context": {
        "label": "1Context",
        "kind": "product_subscription",
        "provider": "1context",
        "default_mode": "subscription",
        "modes": ["subscription", "api_key"],
        "api_key_env": "ONECONTEXT_API_KEY",
        "secret_storage": "environment_or_native_login",
        "agent_visible": False,
    },
}

USER_FIELDS = {
    "selected_mode",
    "api_key_env",
    "token_env",
    "alternate_env",
    "secret_storage",
    "agent_visible",
    "note",
    "notes",
    "account_ref",
    "login_hint",
}


@dataclass(frozen=True)
class AccountsLinkResult:
    path: Path
    accounts: dict[str, dict[str, Any]]
    changed: bool


def link_accounts(
    root: Path | str | None = None,
    active_plugin: str | None = None,
    *,
    write: bool = True,
) -> AccountsLinkResult:
    resolved_root = find_root(Path(root).expanduser() if root else Path.cwd())
    config_path = find_config_path(resolved_root)
    config = read_toml(config_path) if config_path.exists() else {}
    accounts_path = resolve_path(resolved_root, config.get("accounts_file", "accounts.toml"))
    existing = load_existing_accounts(accounts_path)
    system = load_system(resolved_root, active_plugin)

    required_by_account = account_requirements(system)
    account_ids = unique(list(BUILTIN_ACCOUNTS) + list(required_by_account) + list(existing))
    linked: dict[str, dict[str, Any]] = {}
    for account_id in account_ids:
        linked[account_id] = linked_account(account_id, existing.get(account_id, {}), required_by_account.get(account_id, []))

    text = render_accounts_toml(linked)
    old_text = accounts_path.read_text(encoding="utf-8") if accounts_path.exists() else ""
    changed = text != old_text
    if write and changed:
        accounts_path.parent.mkdir(parents=True, exist_ok=True)
        accounts_path.write_text(text, encoding="utf-8")
    return AccountsLinkResult(path=accounts_path, accounts=linked, changed=changed)


def load_existing_accounts(path: Path) -> dict[str, dict[str, Any]]:
    if not path.exists():
        return {}
    raw = read_toml(path)
    result: dict[str, dict[str, Any]] = {}
    for index, record in enumerate(raw.get("accounts", [])):
        if not isinstance(record, dict):
            raise ConfigError(f"{path} accounts item {index} must be a table")
        account_id = str(record.get("id", "")).strip()
        if not account_id:
            raise ConfigError(f"{path} accounts item {index} is missing id")
        result[account_id] = dict(record)
    return result


def account_requirements(system: Any) -> dict[str, list[dict[str, Any]]]:
    requirements: dict[str, list[dict[str, Any]]] = {}
    plugin_id = str(system.plugin.get("id", system.active_plugin))
    for dependency_id, dependency in system.dependencies.items():
        account_ref = str(dependency.get("account", "")).strip()
        if not account_ref:
            continue
        account_id = account_ref.split(".", 1)[1] if account_ref.startswith("accounts.") else account_ref
        requirement = {
            "plugin": plugin_id,
            "dependency": dependency_id,
            "auth_modes": list(dependency.get("auth_modes", [])),
            "models": list(dependency.get("models", [])),
            "model_patterns": list(dependency.get("model_patterns", [])),
            "required": bool(dependency.get("required", False)),
        }
        requirements.setdefault(account_id, []).append(requirement)

    for tool in system.custom_tools.values():
        for account_id in tool.get("accounts", []):
            requirements.setdefault(str(account_id), []).append(
                {
                    "plugin": plugin_id,
                    "dependency": str(tool.get("id", "custom-tool")),
                    "auth_modes": [],
                    "models": [],
                    "model_patterns": [],
                    "required": False,
                }
            )
    return requirements


def linked_account(
    account_id: str,
    existing: dict[str, Any],
    requirements: list[dict[str, Any]],
) -> dict[str, Any]:
    builtin = dict(BUILTIN_ACCOUNTS.get(account_id, {}))
    account = {
        "id": account_id,
        "label": existing.get("label", builtin.get("label", account_id)),
        "kind": existing.get("kind", builtin.get("kind", "external_account")),
        "provider": existing.get("provider", builtin.get("provider", account_id)),
    }

    modes = unique(list(builtin.get("modes", [])) + list(existing.get("modes", [])) + requirement_modes(requirements))
    if not modes:
        modes = ["manual"]
    default_mode = str(existing.get("default_mode") or builtin.get("default_mode") or modes[0])
    if default_mode not in modes:
        modes.insert(0, default_mode)
    selected_mode = str(existing.get("selected_mode") or default_mode)
    if selected_mode not in modes:
        selected_mode = default_mode

    account["default_mode"] = default_mode
    account["selected_mode"] = selected_mode
    account["modes"] = modes

    for key, value in builtin.items():
        if key not in account and key != "id":
            account[key] = value
    for key in USER_FIELDS:
        if key in existing and key != "selected_mode":
            account[key] = existing[key]
    account["required_by"] = requirements
    unmet = unmet_dependencies(selected_mode, requirements)
    account["selected_mode_status"] = "ok" if not unmet else "attention"
    if unmet:
        account["unmet_dependencies"] = unmet
    return account


def requirement_modes(requirements: list[dict[str, Any]]) -> list[str]:
    modes: list[str] = []
    for requirement in requirements:
        modes.extend(requirement.get("auth_modes", []))
    return unique(modes)


def unmet_dependencies(selected_mode: str, requirements: list[dict[str, Any]]) -> list[str]:
    unmet = []
    for requirement in requirements:
        modes = requirement.get("auth_modes", [])
        if modes and selected_mode not in modes:
            unmet.append(str(requirement.get("dependency", "")))
    return [item for item in unmet if item]


def render_accounts_toml(accounts: dict[str, dict[str, Any]]) -> str:
    lines = [
        "# Generated by `1context accounts link`.",
        "# Edit selected_mode, env var names, and notes here. Do not store secret values in this file.",
        "# Real secrets belong in environment variables, OS keychain, native harness login caches, or a daemon secret store.",
        "",
    ]
    for account in accounts.values():
        requirements = list(account.get("required_by", []))
        scalar = {key: value for key, value in account.items() if key != "required_by"}
        lines.append("[[accounts]]")
        for key in ordered_account_keys(scalar):
            if key in scalar:
                lines.append(f"{key} = {toml_value(scalar[key])}")
        for requirement in requirements:
            lines.append("")
            lines.append("[[accounts.required_by]]")
            for key in ("plugin", "dependency", "required", "auth_modes", "models", "model_patterns"):
                value = requirement.get(key)
                if value not in (None, [], ""):
                    lines.append(f"{key} = {toml_value(value)}")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def ordered_account_keys(account: dict[str, Any]) -> list[str]:
    preferred = [
        "id",
        "label",
        "kind",
        "provider",
        "default_mode",
        "selected_mode",
        "selected_mode_status",
        "modes",
        "api_key_env",
        "token_env",
        "alternate_env",
        "subscription",
        "secret_storage",
        "agent_visible",
        "account_ref",
        "login_hint",
        "note",
        "notes",
        "unmet_dependencies",
    ]
    return preferred + sorted(key for key in account if key not in preferred)


def toml_value(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, int | float):
        return str(value)
    raise ConfigError(f"cannot render TOML value {value!r}")
