from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .state_machines import (
    LanguageRuntime,
    StateMachineError,
    available_language_runtimes,
    load_state_machine_dir,
    machine_source_files,
    select_language_runtime,
)


class ConfigError(RuntimeError):
    """Raised when the declarative memory config cannot be loaded."""


@dataclass(frozen=True)
class MemorySystem:
    root: Path
    config_path: Path
    active_plugin: str
    plugin_dirs: tuple[Path, ...]
    runtime_dir: Path
    storage_dir: Path
    runtime_policy: dict[str, Any]
    plugin_path: Path
    accounts: dict[str, dict[str, Any]]
    host: dict[str, Any]
    plugin: dict[str, Any]
    linking: dict[str, Any]
    agents: dict[str, dict[str, Any]]
    harnesses: dict[str, dict[str, Any]]
    jobs: dict[str, dict[str, Any]]
    state_machines: dict[str, dict[str, Any]]
    custom_tools: dict[str, dict[str, Any]]
    dependencies: dict[str, dict[str, Any]]
    native_memory_formats: dict[str, dict[str, Any]]
    providers: dict[str, dict[str, Any]]
    lived_experience: dict[str, dict[str, Any]]
    state_machine_language: dict[str, Any]


def load_system(root: Path | str | None = None, active_plugin: str | None = None) -> MemorySystem:
    resolved_root = find_root(Path(root).expanduser() if root else Path.cwd())
    config_path = find_config_path(resolved_root)
    if not config_path.exists():
        raise ConfigError(f"missing {resolved_root / '1context.toml'}")

    config = read_toml(config_path)
    selected_plugin = str(active_plugin or config.get("active_plugin", "")).strip()
    if not selected_plugin:
        raise ConfigError(f"{config_path.name} must set active_plugin")

    plugin_dirs = tuple(resolve_path(resolved_root, path) for path in config.get("plugin_dirs", ["memory/plugins"]))
    runtime_dir = resolve_path(resolved_root, config.get("runtime_dir", "memory/runtime"))
    storage_dir = resolve_path(resolved_root, config.get("storage_dir", "storage/lakestore"))
    runtime_policy = load_runtime_policy(config)
    plugin_path = find_plugin_path(selected_plugin, plugin_dirs)
    plugin = read_toml(plugin_path / "plugin.toml")
    plugin.setdefault("id", selected_plugin)
    plugin["path"] = str(plugin_path)
    custom_tools_dir = plugin_path / "custom-tools"
    dependencies_dir = plugin_path / "dependencies"
    native_memory_path = plugin_path / "native-memory.toml"
    providers_path = plugin_path / "providers.toml"
    dependencies = load_dependencies(dependencies_dir)
    state_machine_runtime = validate_state_machine_language(plugin_path, dependencies)

    return MemorySystem(
        root=resolved_root,
        config_path=config_path,
        active_plugin=selected_plugin,
        plugin_dirs=plugin_dirs,
        runtime_dir=runtime_dir,
        storage_dir=storage_dir,
        runtime_policy=runtime_policy,
        plugin_path=plugin_path,
        accounts=load_accounts(resolved_root, config),
        host=load_host(resolved_root),
        plugin=plugin,
        linking=load_linking_policy(plugin_path),
        agents=load_manifest_dir(plugin_path / "agents"),
        harnesses=load_manifest_dir(plugin_path / "harnesses"),
        jobs=load_manifest_dir(plugin_path / "jobs"),
        state_machines=load_plugin_state_machines(plugin_path, state_machine_runtime),
        custom_tools=load_manifest_collection(custom_tools_dir / "custom-tools.toml", "custom_tools"),
        dependencies=dependencies,
        native_memory_formats=load_manifest_collection(native_memory_path, "native_memory_formats"),
        providers=load_manifest_collection(providers_path, "providers"),
        lived_experience=load_lived_experience(plugin_path / "lived-experiences"),
        state_machine_language=state_machine_language_payload(dependencies, state_machine_runtime),
    )


def compile_system_map(system: MemorySystem) -> dict[str, Any]:
    host_allow = set(system.host.get("allow", []))
    jobs: dict[str, Any] = {}

    for job_id, job in sorted(system.jobs.items()):
        agent_id = optional_str(job.get("agent"))
        agent = system.agents.get(agent_id or "")
        harness_id = optional_str(agent.get("harness")) if agent else None
        harness = system.harnesses.get(harness_id or "")
        provider_id = optional_str(agent.get("provider")) if agent else None
        provider = system.providers.get(provider_id or "")
        provider_account_id = optional_str(provider.get("account")) if provider else None
        harness_tools = list(harness.get("default_tools", [])) if harness else []
        custom_tools = []
        if agent:
            custom_tools.extend(agent.get("tools", []))
        custom_tools.extend(job.get("tools", []))
        custom_tools = unique(custom_tools)
        host_grants = unique(harness_tools + custom_tools)

        required_accounts = []
        if provider_account_id:
            required_accounts.append(provider_account_id)
        required_dependencies = []
        for tool_id in custom_tools:
            tool = system.custom_tools.get(tool_id)
            if tool:
                required_dependencies.extend(tool.get("dependencies", []))
                required_accounts.extend(tool.get("accounts", []))
        required_accounts = unique(required_accounts)
        required_dependencies = unique(required_dependencies)

        experience_ids = static_experience_ids(job.get("experience", []))
        if agent and not experience_ids:
            experience_ids = static_experience_ids(agent.get("experience", []))

        missing = {
            "agent": [agent_id] if agent_id and not agent else [],
            "agent_paths": missing_agent_paths(system, agent) if agent else [],
            "harness": [harness_id] if harness_id and harness_id not in system.harnesses else [],
            "harness_paths": missing_harness_paths(system, harness) if harness else [],
            "provider": [provider_id] if provider_id and provider_id not in system.providers else [],
            "accounts": [account for account in required_accounts if account not in system.accounts],
            "dependencies": [dependency for dependency in required_dependencies if dependency not in system.dependencies],
            "custom_tools": [tool for tool in custom_tools if tool not in system.custom_tools],
            "host_grants": [tool for tool in host_grants if tool not in host_allow and "*" not in host_allow],
            "experience": [item for item in experience_ids if item not in system.lived_experience],
        }
        status = "ready" if not any(missing.values()) else "incomplete"

        jobs[job_id] = {
            "id": job_id,
            "label": job.get("label", job_id),
            "agent": agent_id,
            "harness": harness_id,
            "provider": provider_id,
            "model": agent.get("model") if agent else None,
            "harness_tools": harness_tools,
            "custom_tools": custom_tools,
            "required_accounts": required_accounts,
            "required_dependencies": required_dependencies,
            "experience": experience_ids,
            "experience_config": job.get("experience", {}) if isinstance(job.get("experience"), dict) else {},
            "permissions": job.get("permissions", {}),
            "missing": missing,
            "missing_host_grants": missing["host_grants"],
            "status": status,
        }

    return {
        "root": str(system.root),
        "active_plugin": system.active_plugin,
        "runtime": {
            "dir": str(system.runtime_dir),
            "ledger": str(system.runtime_dir / "ledger" / "events.jsonl"),
            "lakestore": str(system.storage_dir),
            "runs": str(system.runtime_dir / "runs"),
            "experiences": str(system.runtime_dir / "experiences"),
            "proposals": str(system.runtime_dir / "proposals"),
            "policy": system.runtime_policy,
        },
        "plugin": system.plugin,
        "linking": system.linking,
        "accounts": system.accounts,
        "host": system.host,
        "runtime_policy": system.runtime_policy,
        "agents": system.agents,
        "harnesses": system.harnesses,
        "jobs": jobs,
        "state_machines": system.state_machines,
        "state_machine_language": system.state_machine_language,
        "custom_tools": system.custom_tools,
        "dependencies": system.dependencies,
        "native_memory_formats": system.native_memory_formats,
        "providers": system.providers,
        "lived_experience": system.lived_experience,
    }


def list_plugins(root: Path | str | None = None) -> list[dict[str, Any]]:
    resolved_root = find_root(Path(root).expanduser() if root else Path.cwd())
    config = read_toml(find_config_path(resolved_root))
    plugin_dirs = tuple(resolve_path(resolved_root, path) for path in config.get("plugin_dirs", ["memory/plugins"]))
    plugins: list[dict[str, Any]] = []
    for plugin_dir in plugin_dirs:
        if not plugin_dir.is_dir():
            continue
        for child in sorted(plugin_dir.iterdir()):
            manifest = child / "plugin.toml"
            if not manifest.exists():
                continue
            plugin = read_toml(manifest)
            plugin.setdefault("id", child.name)
            plugin["path"] = str(child)
            plugins.append(plugin)
    return plugins


def load_host(root: Path) -> dict[str, Any]:
    path = root / "host.toml"
    if not path.exists():
        return {
            "id": "unspecified-host",
            "trust_mode": "unspecified",
            "allow": [],
            "deny": [],
            "audit": False,
            "confirm_destructive": True,
        }
    host = read_toml(path)
    host["path"] = str(path)
    host.setdefault("allow", [])
    host.setdefault("deny", [])
    return host


def load_accounts(root: Path, config: dict[str, Any]) -> dict[str, dict[str, Any]]:
    path = resolve_path(root, config.get("accounts_file", "accounts.toml"))
    return load_manifest_collection(path, "accounts")


def load_runtime_policy(config: dict[str, Any]) -> dict[str, Any]:
    raw = config.get("runtime_policy", {})
    if not isinstance(raw, dict):
        raise ConfigError("1context.toml [runtime_policy] must be a table")
    max_concurrent = int(raw.get("max_concurrent_agents", 8))
    if max_concurrent < 1:
        raise ConfigError("runtime_policy.max_concurrent_agents must be >= 1")
    return {
        "max_concurrent_agents": max_concurrent,
        "max_importer_staleness_hours": int(raw.get("max_importer_staleness_hours", 24)),
        "max_concurrent_renderers": int(raw.get("max_concurrent_renderers", max(1, min(max_concurrent, 4)))),
        "max_prompt_tokens": int(raw.get("max_prompt_tokens", 128000)),
        "agent_timeout": str(raw.get("agent_timeout", "30m")),
        "default_harness_isolation": str(raw.get("default_harness_isolation", "account_clean")),
    }


def load_dependencies(dependencies_dir: Path) -> dict[str, dict[str, Any]]:
    return load_manifest_collection(dependencies_dir / "dependencies.toml", "dependencies")


def load_linking_policy(plugin_path: Path) -> dict[str, Any]:
    path = plugin_path / "linking.toml"
    if path.exists():
        policy = read_toml(path)
    else:
        policy = {}
    policy.setdefault("linker", "onectx.memory.default")
    policy.setdefault("linker_version", "0.1")
    policy.setdefault("ledger_schema_version", "0.1")
    policy.setdefault("default_attach", "last_for_job")
    policy.setdefault("create_if_missing", True)
    policy.setdefault("lived_experience_start", "2026-04-24")
    policy.setdefault("inject_order", ["lived_experience_seed", "harness_native_experience"])
    scope = policy.setdefault("scope", {})
    scope.setdefault("job", True)
    scope.setdefault("agent", False)
    scope.setdefault("plugin", False)
    policy["source_path"] = str(path)
    return policy


def load_plugin_state_machines(
    plugin_path: Path,
    language_runtime: LanguageRuntime | None,
) -> dict[str, dict[str, Any]]:
    try:
        return load_state_machine_dir(plugin_path / "state_machines", language_runtime=language_runtime)
    except StateMachineError as exc:
        raise ConfigError(str(exc)) from exc


def validate_state_machine_language(
    plugin_path: Path,
    dependencies: dict[str, dict[str, Any]],
) -> LanguageRuntime | None:
    sources = machine_source_files(plugin_path / "state_machines")
    requirements = state_machine_language_requirements(dependencies)
    if sources and not requirements:
        raise ConfigError(
            f"{plugin_path / 'state_machines'} contains machine definitions but "
            "dependencies/dependencies.toml does not declare kind='state_machine_language'"
        )
    selected: LanguageRuntime | None = None
    for requirement in requirements:
        language_id = optional_str(requirement.get("language")) or optional_str(requirement.get("language_id"))
        if not language_id:
            raise ConfigError(f"state-machine dependency {requirement['id']!r} is missing language")
        version_spec = optional_str(requirement.get("version_spec")) or optional_str(requirement.get("version")) or ""
        try:
            runtime = select_language_runtime(language_id, version_spec=version_spec)
        except StateMachineError as exc:
            raise ConfigError(str(exc)) from exc
        if selected and selected != runtime:
            raise ConfigError(
                "plugin declares multiple state-machine language requirements "
                f"that select different runtimes: {selected.id} {selected.version} and "
                f"{runtime.id} {runtime.version}"
            )
        selected = runtime
    return selected


def state_machine_language_requirements(dependencies: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        dependency
        for dependency in dependencies.values()
        if dependency.get("kind") == "state_machine_language"
    ]


def state_machine_language_payload(
    dependencies: dict[str, dict[str, Any]],
    selected: LanguageRuntime | None,
) -> dict[str, Any]:
    return {
        "selected_runtime": selected.to_ir() if selected else None,
        "available_runtimes": available_language_runtimes(),
        "requirements": state_machine_language_requirements(dependencies),
    }


def load_manifest_dir(path: Path) -> dict[str, dict[str, Any]]:
    items: dict[str, dict[str, Any]] = {}
    if not path.is_dir():
        return items
    for manifest in sorted(path.glob("*.toml")):
        raw = read_toml(manifest)
        item_id = str(raw.get("id", manifest.stem.replace("-", "_")))
        if item_id in items:
            raise ConfigError(f"duplicate id {item_id!r} in {manifest}")
        raw["id"] = item_id
        raw["source_path"] = str(manifest)
        items[item_id] = raw
    return items


def load_manifest_collection(path: Path, section: str) -> dict[str, dict[str, Any]]:
    items: dict[str, dict[str, Any]] = {}
    if not path.exists():
        return items
    raw = read_toml(path)
    records = raw.get(section, [])
    if not isinstance(records, list):
        raise ConfigError(f"{path} section {section!r} must be a list")
    for index, record in enumerate(records):
        if not isinstance(record, dict):
            raise ConfigError(f"{path} item {index} in {section!r} must be a table")
        item_id = str(record.get("id", "")).strip()
        if not item_id:
            raise ConfigError(f"{path} item {index} in {section!r} is missing id")
        if item_id in items:
            raise ConfigError(f"duplicate id {item_id!r} in {path}")
        item = dict(record)
        item["id"] = item_id
        item["source_path"] = str(path)
        item["source_index"] = index
        items[item_id] = item
    return items


def load_lived_experience(path: Path) -> dict[str, dict[str, Any]]:
    items: dict[str, dict[str, Any]] = {}
    if not path.is_dir():
        return items
    for manifest in sorted(path.glob("*/manifest.toml")):
        raw = read_toml(manifest)
        item_id = str(raw.get("id", manifest.parent.name.replace("-", "_")))
        if item_id in items:
            raise ConfigError(f"duplicate lived-experience id {item_id!r} in {manifest}")
        raw["id"] = item_id
        raw["source_path"] = str(manifest)
        raw["path"] = str(manifest.parent)
        items[item_id] = raw
    return items


def find_root(start: Path) -> Path:
    current = start.resolve()
    if current.is_file():
        current = current.parent
    for candidate in (current, *current.parents):
        if find_config_path(candidate).exists():
            return candidate
    return current


def find_config_path(root: Path) -> Path:
    return root / "1context.toml"


def find_plugin_path(active_plugin: str, plugin_dirs: tuple[Path, ...]) -> Path:
    for plugin_dir in plugin_dirs:
        direct = plugin_dir / active_plugin
        if (direct / "plugin.toml").exists():
            return direct.resolve()
        if not plugin_dir.is_dir():
            continue
        for child in sorted(plugin_dir.iterdir()):
            manifest = child / "plugin.toml"
            if not manifest.exists():
                continue
            plugin = read_toml(manifest)
            if plugin.get("id") == active_plugin:
                return child.resolve()
    searched = ", ".join(str(path) for path in plugin_dirs)
    raise ConfigError(f"active plugin {active_plugin!r} not found in {searched}")


def read_toml(path: Path) -> dict[str, Any]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def resolve_path(root: Path, value: str | Path) -> Path:
    path = Path(value).expanduser()
    return path if path.is_absolute() else (root / path).resolve()


def optional_str(value: Any) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def missing_agent_paths(system: MemorySystem, agent: dict[str, Any]) -> list[str]:
    missing: list[str] = []
    for key in ("prompt_paths", "reference_paths"):
        for value in agent.get(key, []):
            path = resolve_path(system.plugin_path, value)
            if not path.exists():
                missing.append(f"{key}:{value}")
    return missing


def missing_harness_paths(system: MemorySystem, harness: dict[str, Any]) -> list[str]:
    missing: list[str] = []
    prompt_control = harness.get("prompt_control", {})
    if not isinstance(prompt_control, dict):
        return missing
    for key in ("model_instructions_file", "memory_file"):
        value = optional_str(prompt_control.get(key))
        if value and not resolve_path(system.plugin_path, value).exists():
            missing.append(f"{key}:{value}")
    for value in prompt_control.get("prompt_paths", []):
        path = str(value).strip()
        if path and not resolve_path(system.plugin_path, path).exists():
            missing.append(f"prompt_paths:{path}")
    return missing


def unique(values: list[Any]) -> list[Any]:
    result = []
    seen = set()
    for value in values:
        if value not in seen:
            result.append(value)
            seen.add(value)
    return result


def static_experience_ids(value: Any) -> list[str]:
    if isinstance(value, dict):
        return []
    if isinstance(value, str):
        raw_values = [value]
    else:
        raw_values = list(value or [])
    result = []
    seen = set()
    for raw in raw_values:
        for item in str(raw).split(","):
            clean = item.strip()
            if clean and clean not in seen:
                result.append(clean)
                seen.add(clean)
    return result
