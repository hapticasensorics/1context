from __future__ import annotations

import hashlib
import json
import re
import tomllib
import uuid
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem

from .ledger import Ledger, exclusive_file_lock, ledger_events_path, ledger_lock_path, utc_now


ATTACH_MODES = {"new", "last_for_job", "manual", "none"}
LINKER_ID = "onectx.memory.default"
LINKER_VERSION = "0.1"
LEDGER_SCHEMA_VERSION = "0.1"
HIRED_AGENT_CREATED_EVENT = "hired_agent.created"


class HireError(RuntimeError):
    """Raised when an agent cannot be hired."""


def new_uuid_urn() -> str:
    return f"urn:uuid:{uuid.uuid4()}"


@dataclass(frozen=True)
class HireResult:
    mode: str
    hired_agent_uuid: str
    experience_id: str | None
    path: Path | None
    created: bool
    event: dict[str, Any]

    def asdict(self) -> dict[str, Any]:
        return {
            "mode": self.mode,
            "hired_agent_uuid": self.hired_agent_uuid,
            "experience_id": self.experience_id,
            "path": str(self.path) if self.path else None,
            "created": self.created,
            "event": self.event,
        }


def hire_agent(
    system: MemorySystem,
    *,
    job_ids: Sequence[str] | None = None,
    agent_id: str = "",
    harness_id: str = "",
    provider_id: str = "",
    model: str = "",
    mode: str | None = None,
    experience_id: str | None = None,
    run_id: str | None = None,
    job_params: dict[str, Any] | None = None,
    experience_packet: dict[str, Any] | None = None,
    harness_launch: dict[str, Any] | None = None,
    prompt_stack: dict[str, Any] | None = None,
) -> HireResult:
    policy = system.linking
    validate_linker_policy(policy)
    ledger = Ledger(ledger_events_path(system.runtime_dir), storage_path=system.storage_dir)
    with exclusive_file_lock(ledger_lock_path(system.runtime_dir)):
        return _hire_agent_locked(
            system,
            policy=policy,
            ledger=ledger,
            job_ids=job_ids,
            agent_id=agent_id,
            harness_id=harness_id,
            provider_id=provider_id,
            model=model,
            mode=mode,
            experience_id=experience_id,
            run_id=run_id,
            job_params=job_params or {},
            experience_packet=experience_packet or {},
            harness_launch=harness_launch or {},
            prompt_stack=prompt_stack or {},
        )


def _hire_agent_locked(
    system: MemorySystem,
    *,
    policy: dict[str, Any],
    ledger: Ledger,
    job_ids: Sequence[str] | None,
    agent_id: str,
    harness_id: str,
    provider_id: str,
    model: str,
    mode: str | None,
    experience_id: str | None,
    run_id: str | None,
    job_params: dict[str, Any],
    experience_packet: dict[str, Any],
    harness_launch: dict[str, Any],
    prompt_stack: dict[str, Any],
) -> HireResult:
    resolved_job_ids = normalize_ids(job_ids or [])
    name_hint = resolved_job_ids[0] if resolved_job_ids else ""
    chosen_mode = mode or str(policy.get("default_attach", "last_for_job"))
    if chosen_mode not in ATTACH_MODES:
        raise HireError(f"unknown attach mode {chosen_mode!r}")

    resolved_agent_id = agent_id or agent_from_jobs(system, resolved_job_ids)
    hired_agent_uuid = new_uuid_urn()
    config_snapshot = hired_agent_config_snapshot(
        system,
        policy=policy,
        job_ids=resolved_job_ids,
        agent_id=resolved_agent_id,
        harness_id=harness_id,
        provider_id=provider_id,
        model=model,
        mode=chosen_mode,
        run_id=run_id,
        job_params=job_params,
        experience_packet=experience_packet,
        harness_launch=harness_launch,
        prompt_stack=prompt_stack,
    )
    versions = definition_versions(system, policy)
    config_hash = stable_hash(config_snapshot)
    linking_policy_hash = stable_hash(public_policy(policy))

    if chosen_mode == "none":
        attachment = attachment_record(
            mode=chosen_mode,
            created=False,
            experience_id=None,
            path=None,
            experience_packet=experience_packet,
        )
        event = append_hired_agent_created(
            ledger,
            hired_agent_uuid=hired_agent_uuid,
            job_ids=resolved_job_ids,
            agent_id=resolved_agent_id,
            config_snapshot=config_snapshot,
            versions=versions,
            attachment=attachment,
            config_hash=config_hash,
            linking_policy_hash=linking_policy_hash,
            run_id=run_id,
            job_params=job_params,
        )
        return HireResult(chosen_mode, hired_agent_uuid, None, None, False, event)

    created = False
    resolved_id = experience_id

    if chosen_mode == "manual":
        if not resolved_id:
            raise HireError("manual attach requires --experience-id")
        if not runtime_experience_dir(system, resolved_id).is_dir():
            raise HireError(f"runtime experience {resolved_id!r} does not exist")

    elif chosen_mode == "new":
        resolved_id = new_experience_id(hired_agent_uuid=hired_agent_uuid, name_hint=name_hint, agent_id=resolved_agent_id)
        create_runtime_experience(
            system,
            resolved_id,
            hired_agent_uuid,
            job_ids=resolved_job_ids,
            agent_id=resolved_agent_id,
            config_snapshot=config_snapshot,
            lived_experience=config_snapshot.get("lived_experience", []),
        )
        created = True

    elif chosen_mode == "last_for_job":
        resolved_id = find_last_experience(
            ledger,
            job_ids=resolved_job_ids,
            agent_id=resolved_agent_id,
            plugin=system.plugin,
            policy=policy,
        )
        if not resolved_id:
            if not bool(policy.get("create_if_missing", True)):
                raise HireError("no previous runtime experience found for job and create_if_missing is false")
            resolved_id = new_experience_id(hired_agent_uuid=hired_agent_uuid, name_hint=name_hint, agent_id=resolved_agent_id)
            create_runtime_experience(
                system,
                resolved_id,
                hired_agent_uuid,
                job_ids=resolved_job_ids,
                agent_id=resolved_agent_id,
                config_snapshot=config_snapshot,
                lived_experience=config_snapshot.get("lived_experience", []),
            )
            created = True

    if not resolved_id:
        raise HireError("linker did not resolve a runtime experience id")

    path = runtime_experience_dir(system, resolved_id)
    runtime_native_homes = read_runtime_native_homes(path)
    attachment = attachment_record(
        mode=chosen_mode,
        created=created,
        experience_id=resolved_id,
        path=path,
        runtime_native_homes=runtime_native_homes,
        experience_packet=experience_packet,
    )
    event = append_hired_agent_created(
        ledger,
        hired_agent_uuid=hired_agent_uuid,
        job_ids=resolved_job_ids,
        agent_id=resolved_agent_id,
        config_snapshot=config_snapshot,
        versions=versions,
        attachment=attachment,
        config_hash=config_hash,
        linking_policy_hash=linking_policy_hash,
        run_id=run_id,
        job_params=job_params,
    )
    return HireResult(chosen_mode, hired_agent_uuid, resolved_id, path, created, event)


def validate_linker_policy(policy: dict[str, Any]) -> None:
    policy_linker = str(policy.get("linker", ""))
    policy_version = str(policy.get("linker_version", ""))
    policy_ledger_schema_version = str(policy.get("ledger_schema_version", LEDGER_SCHEMA_VERSION))
    if policy_linker != LINKER_ID:
        raise HireError(f"linking policy requires linker {policy_linker!r}, but this runtime provides {LINKER_ID!r}")
    if policy_version != LINKER_VERSION:
        raise HireError(
            f"linking policy requires {policy_linker} {policy_version}, "
            f"but this runtime provides {LINKER_ID} {LINKER_VERSION}"
        )
    if policy_ledger_schema_version != LEDGER_SCHEMA_VERSION:
        raise HireError(
            f"linking policy requires ledger schema {policy_ledger_schema_version}, "
            f"but this runtime writes {LEDGER_SCHEMA_VERSION}"
        )


def find_last_experience(
    ledger: Ledger,
    *,
    job_ids: Sequence[str],
    agent_id: str,
    plugin: dict[str, Any],
    policy: dict[str, Any],
) -> str | None:
    scope = policy.get("scope", {})
    match_job = bool(scope.get("job", True))
    match_agent = bool(scope.get("agent", False))
    match_plugin = bool(scope.get("plugin", False))
    desired_job_ids = set(job_ids)
    for event in reversed(ledger.read()):
        if event.get("event") != HIRED_AGENT_CREATED_EVENT:
            continue
        experience_id = event_experience_id(event)
        if not experience_id:
            continue
        if match_plugin and event.get("plugin_id") != plugin.get("id"):
            continue
        if match_job:
            event_job_ids = normalize_ids(event.get("job_ids", []))
            if desired_job_ids and not desired_job_ids.intersection(event_job_ids):
                continue
            if not desired_job_ids and event_job_ids:
                continue
        if match_agent and event.get("agent_id") != (agent_id or None):
            continue
        if experience_id:
            return str(experience_id)
    return None


def event_experience_id(event: dict[str, Any]) -> str | None:
    attachment = event.get("attachment", {})
    if isinstance(attachment, dict) and attachment.get("experience_id"):
        return str(attachment["experience_id"])
    return None


def attachment_record(
    *,
    mode: str,
    created: bool,
    experience_id: str | None,
    path: Path | None,
    runtime_native_homes: Sequence[dict[str, Any]] | None = None,
    experience_packet: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "mode": mode,
        "created_runtime_experience": created,
        "experience_id": experience_id,
        "experience_path": str(path) if path else None,
        "runtime_native_homes": list(runtime_native_homes or []),
        "experience_packet": scrub_manifest(experience_packet or {}),
    }


def definition_versions(system: MemorySystem, policy: dict[str, Any]) -> dict[str, Any]:
    return {
        "ledger_schema_version": LEDGER_SCHEMA_VERSION,
        "plugin_id": system.plugin.get("id"),
        "plugin_version": system.plugin.get("version"),
        "plugin_schema_version": system.plugin.get("schema_version"),
        "linking_version": policy.get("version"),
        "linker": policy.get("linker"),
        "linker_version": policy.get("linker_version"),
    }


def append_hired_agent_created(
    ledger: Ledger,
    *,
    hired_agent_uuid: str,
    job_ids: Sequence[str],
    job_params: dict[str, Any],
    agent_id: str,
    config_snapshot: dict[str, Any],
    versions: dict[str, Any],
    attachment: dict[str, Any],
    config_hash: str,
    linking_policy_hash: str,
    run_id: str | None,
) -> dict[str, Any]:
    birth_certificate = {
        "kind": "hired_agent_birth_certificate",
        "schema_version": LEDGER_SCHEMA_VERSION,
        "hired_agent_uuid": hired_agent_uuid,
        "job_ids": list(job_ids),
        "job_params": dict(job_params),
        "agent_id": agent_id or None,
        "run_id": run_id,
        "definition_versions": versions,
        "attachment": attachment,
        "experience_packet": config_snapshot.get("experience_packet", {}),
        "prompt_stack": config_snapshot.get("prompt_stack", {}),
        "harness_launch": config_snapshot.get("harness_launch", {}),
        "config_snapshot": config_snapshot,
    }
    birth_certificate_hash = stable_hash(birth_certificate)
    fingerprints = {
        "birth_certificate_sha256": birth_certificate_hash,
        "config_snapshot_sha256": config_hash,
        "linking_policy_sha256": linking_policy_hash,
    }

    harness = config_snapshot.get("harness") if isinstance(config_snapshot.get("harness"), dict) else None
    provider = config_snapshot.get("provider") if isinstance(config_snapshot.get("provider"), dict) else None
    return ledger.append(
        HIRED_AGENT_CREATED_EVENT,
        ledger_schema_version=LEDGER_SCHEMA_VERSION,
        hired_agent_uuid=hired_agent_uuid,
        job_ids=list(job_ids),
        job_params=dict(job_params),
        agent_id=agent_id or None,
        harness_id=harness.get("id") if harness else None,
        provider_id=provider.get("id") if provider else None,
        model=config_snapshot.get("model") or None,
        run_id=run_id,
        plugin_id=versions.get("plugin_id"),
        plugin_version=versions.get("plugin_version"),
        plugin_schema_version=versions.get("plugin_schema_version"),
        linking_version=versions.get("linking_version"),
        linker=versions.get("linker"),
        linker_version=versions.get("linker_version"),
        attachment=attachment,
        experience_packet=config_snapshot.get("experience_packet", {}),
        prompt_stack=config_snapshot.get("prompt_stack", {}),
        harness_launch=config_snapshot.get("harness_launch", {}),
        definition_versions=versions,
        fingerprints=fingerprints,
        birth_certificate=birth_certificate,
    )


def create_runtime_experience(
    system: MemorySystem,
    experience_id: str,
    hired_agent_uuid: str,
    *,
    job_ids: Sequence[str],
    agent_id: str,
    config_snapshot: dict[str, Any],
    lived_experience: Sequence[dict[str, Any]] | None = None,
) -> Path:
    path = runtime_experience_dir(system, experience_id)
    path.mkdir(parents=True, exist_ok=False)
    copied_lived_experience = [item for item in (lived_experience or []) if item.get("defined", True)]
    copied_lived_experience_ids = normalize_ids(item.get("id", "") for item in copied_lived_experience)
    native_homes = create_native_harness_homes(path, config_snapshot)
    files = ["manifest.toml"]
    if native_homes:
        (path / "native-homes.toml").write_text(toml_records("native_homes", native_homes), encoding="utf-8")
        files.append("native-homes.toml")
    if copied_lived_experience:
        seed_path = path / "seeds" / "lived-experience.md"
        seed_path.parent.mkdir(parents=True, exist_ok=True)
        seed_path.write_text(lived_experience_seed(copied_lived_experience), encoding="utf-8")
        files.append("seeds/lived-experience.md")
    (path / "artifacts").mkdir()
    (path / "views").mkdir()
    profile = {
        "id": experience_id,
        "version": "0.0.0-runtime",
        "kind": "runtime_experience",
        "title": f"Runtime Experience {experience_id}",
        "summary": "Local runtime experience that owns harness-native state; readable views are derived.",
        "created": utc_now(),
        "created_by_hired_agent_uuid": hired_agent_uuid,
        "created_by_ledger_schema_version": LEDGER_SCHEMA_VERSION,
        "created_by_linker": LINKER_ID,
        "created_by_linker_version": LINKER_VERSION,
        "created_by_plugin": system.active_plugin,
        "created_by_job_ids": list(job_ids),
        "created_by_agent": agent_id or "",
        "primary_memory": "harness-native",
        "native_harnesses": [record["harness"] for record in native_homes],
        "copied_lived_experience": copied_lived_experience_ids,
        "files": files,
    }
    (path / "manifest.toml").write_text(toml_from_dict(profile), encoding="utf-8")
    return path


def create_native_harness_homes(experience_path: Path, config_snapshot: dict[str, Any]) -> list[dict[str, str]]:
    harness = config_snapshot.get("harness") or {}
    if not isinstance(harness, dict) or not harness.get("defined", True):
        return []
    native_memory = harness.get("native_memory") or {}
    if not isinstance(native_memory, dict):
        return []
    home = str(native_memory.get("home", "")).strip()
    if not home:
        return []

    home_rel = safe_relative_path(home)
    home_path = experience_path / home_rel
    home_path.mkdir(parents=True, exist_ok=False)
    for subdir in native_memory.get("subdirs", []):
        (home_path / safe_relative_path(str(subdir))).mkdir(parents=True, exist_ok=True)

    auth_status = provision_native_auth(home_path, home_rel, harness)
    return [
        {
            "harness": str(harness.get("id", "")),
            "format": str(harness.get("primary_memory_format", "")),
            "home": str(home_rel),
            "source_of_truth": str(native_memory.get("source_of_truth", True)).lower(),
            **auth_status,
        }
    ]


def provision_native_auth(home_path: Path, home_rel: Path, harness: dict[str, Any]) -> dict[str, str]:
    auth = harness.get("auth") or {}
    if not isinstance(auth, dict):
        return {}

    mode = str(auth.get("default_mode", "")).strip()
    if mode != "existing_codex_login":
        return {
            "auth_mode": mode,
            "auth_secret_recorded": "false",
        }

    source_label = str(auth.get("existing_auth_source", "~/.codex/auth.json")).strip()
    link_label = str(auth.get("link_auth_into_home", "auth.json")).strip()
    strategy = str(auth.get("link_strategy", "symlink")).strip() or "symlink"
    source = Path(source_label).expanduser()
    target_rel = safe_relative_path(link_label)
    target = home_path / target_rel
    target.parent.mkdir(parents=True, exist_ok=True)

    status = "linked"
    if not source.is_file():
        status = "missing_source"
    elif target.exists() or target.is_symlink():
        try:
            if target.is_symlink() and target.resolve() == source.resolve():
                status = "already_linked"
            else:
                status = "already_present"
        except OSError:
            status = "already_present"
    elif strategy == "symlink":
        target.symlink_to(source)
    else:
        raise HireError(f"unsupported native auth link strategy {strategy!r}")

    return {
        "auth_mode": mode,
        "auth_source": "codex_global_login",
        "auth_source_path": str(source),
        "auth_link": str(home_rel / target_rel),
        "auth_link_strategy": strategy,
        "auth_link_status": status,
        "auth_secret_recorded": "false",
    }


def read_runtime_native_homes(experience_path: Path) -> list[dict[str, Any]]:
    path = experience_path / "native-homes.toml"
    if not path.exists():
        return []
    try:
        raw = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        raise HireError(f"invalid native homes file {path}: {exc}") from exc
    records = raw.get("native_homes", [])
    if not isinstance(records, list):
        return []
    return [record for record in records if isinstance(record, dict)]


def lived_experience_seed(lived_experience: Sequence[dict[str, Any]]) -> str:
    parts = [
        "# Lived Experience Seed",
        "",
        "This file is prompt seed material copied from the active plugin when this runtime experience was created.",
        "It is not the harness-native session store. The selected harness owns memory under `harnesses/`.",
        "",
    ]
    for item in lived_experience:
        parts.append(format_lived_experience_seed(item))
    return "\n".join(parts).rstrip() + "\n"


def format_lived_experience_seed(item: dict[str, Any]) -> str:
    title = str(item.get("title") or item.get("id") or "lived-experience")
    lines = [f"### {title}", ""]
    for key in ("id", "version", "kind", "created", "summary", "source_path"):
        value = item.get(key)
        if value not in (None, ""):
            lines.append(f"{key}: {value}")
    if any(item.get(key) not in (None, "") for key in ("id", "version", "kind", "created", "summary", "source_path")):
        lines.append("")
    for file_item in item.get("files", []):
        file_path = str(file_item.get("path", "experience.md"))
        lines.extend([f"#### File: {file_path}", ""])
        if not file_item.get("exists", False):
            lines.extend(["[missing]", ""])
            continue
        text = file_item.get("text")
        if text is None:
            lines.extend(["[binary or unreadable]", ""])
            continue
        lines.append(str(text).rstrip())
        lines.append("")
    return "\n".join(lines)


def runtime_experience_dir(system: MemorySystem, experience_id: str) -> Path:
    return system.runtime_dir / "experiences" / safe_id(experience_id)


def safe_relative_path(value: str) -> Path:
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise HireError(f"native memory path must stay inside the runtime experience: {value!r}")
    if not path.parts:
        raise HireError("native memory path is empty")
    return path


def new_experience_id(*, hired_agent_uuid: str, name_hint: str, agent_id: str) -> str:
    base = safe_id(name_hint or agent_id or "runtime")
    suffix = hired_agent_uuid.removeprefix("urn:uuid:").replace("-", "")[:12]
    return f"exp_{base}_{suffix}"


def agent_from_jobs(system: MemorySystem, job_ids: Sequence[str]) -> str:
    agent_ids = []
    for job_id in job_ids:
        job = system.jobs.get(job_id)
        if job and job.get("agent"):
            agent_ids.append(str(job["agent"]))
    agent_ids = normalize_ids(agent_ids)
    return agent_ids[0] if len(agent_ids) == 1 else ""


def hired_agent_config_snapshot(
    system: MemorySystem,
    *,
    policy: dict[str, Any],
    job_ids: Sequence[str],
    agent_id: str,
    harness_id: str,
    provider_id: str,
    model: str,
    mode: str,
    run_id: str | None,
    job_params: dict[str, Any],
    experience_packet: dict[str, Any] | None = None,
    harness_launch: dict[str, Any] | None = None,
    prompt_stack: dict[str, Any] | None = None,
) -> dict[str, Any]:
    agent = system.agents.get(agent_id or "")
    jobs = [snapshot_manifest(system.jobs.get(job_id), fallback_id=job_id) for job_id in job_ids]
    resolved_harness_id = str((agent or {}).get("harness") or harness_id or "")
    resolved_provider_id = str((agent or {}).get("provider") or provider_id or "")
    resolved_model = str((agent or {}).get("model") or model or "")
    harness = system.harnesses.get(resolved_harness_id)
    provider = system.providers.get(resolved_provider_id)

    custom_tool_ids = []
    if agent:
        custom_tool_ids.extend(agent.get("tools", []))
    for job_id in job_ids:
        job = system.jobs.get(job_id)
        if job:
            custom_tool_ids.extend(job.get("tools", []))
    custom_tool_ids = normalize_ids(custom_tool_ids)

    experience_ids = []
    if agent:
        experience_ids.extend(static_experience_ids(agent.get("experience", [])))
    for job_id in job_ids:
        job = system.jobs.get(job_id)
        if job:
            experience_ids.extend(static_experience_ids(job.get("experience", [])))
    experience_ids = normalize_ids(experience_ids)

    account_ids = []
    if provider and provider.get("account"):
        account_ids.append(str(provider["account"]))
    dependency_ids = []
    for tool_id in custom_tool_ids:
        tool = system.custom_tools.get(tool_id)
        if tool:
            account_ids.extend(tool.get("accounts", []))
            dependency_ids.extend(tool.get("dependencies", []))
    account_ids = normalize_ids(account_ids)
    dependency_ids = normalize_ids(dependency_ids)
    prompt_paths = []
    prompt_paths.extend(harness_prompt_paths(harness))
    prompt_paths.extend((agent or {}).get("prompt_paths", []))
    reference_paths = []
    reference_paths.extend((agent or {}).get("reference_paths", []))
    for job_id in job_ids:
        job = system.jobs.get(job_id)
        if job:
            prompt_paths.extend(job.get("prompt_paths", []))
            reference_paths.extend(job.get("reference_paths", []))

    return {
        "mode": mode,
        "run_id": run_id,
        "job_invocation": {
            "job_ids": list(job_ids),
            "params": dict(job_params),
        },
        "experience_packet": scrub_manifest(experience_packet or {}),
        "prompt_stack": scrub_manifest(prompt_stack or {}),
        "harness_launch": scrub_manifest(harness_launch or {}),
        "plugin": scrub_manifest(system.plugin),
        "host": scrub_manifest(system.host),
        "runtime_policy": scrub_manifest(system.runtime_policy),
        "linking": scrub_manifest(public_policy(policy)),
        "jobs": jobs,
        "agent": snapshot_manifest(agent, fallback_id=agent_id) if agent_id else ad_hoc_agent_snapshot(
            harness_id=resolved_harness_id,
            provider_id=resolved_provider_id,
            model=resolved_model,
        ),
        "harness": snapshot_manifest(harness, fallback_id=resolved_harness_id) if resolved_harness_id else None,
        "provider": snapshot_manifest(provider, fallback_id=resolved_provider_id) if resolved_provider_id else None,
        "model": resolved_model,
        "harness_tools": {
            "default": list((harness or {}).get("default_tools", [])),
            "optional": list((harness or {}).get("optional_tools", [])),
        },
        "custom_tools": [
            snapshot_manifest(system.custom_tools.get(tool_id), fallback_id=tool_id) for tool_id in custom_tool_ids
        ],
        "accounts": [
            snapshot_manifest(system.accounts.get(account_id), fallback_id=account_id) for account_id in account_ids
        ],
        "dependencies": [
            snapshot_manifest(system.dependencies.get(dependency_id), fallback_id=dependency_id)
            for dependency_id in dependency_ids
        ],
        "prompts": plugin_file_snapshots(system, unique_values(prompt_paths)),
        "references": plugin_file_snapshots(system, unique_values(reference_paths)),
        "lived_experience": [lived_experience_snapshot(system, experience_id) for experience_id in experience_ids],
    }


def snapshot_manifest(manifest: dict[str, Any] | None, *, fallback_id: str) -> dict[str, Any]:
    if not manifest:
        return {
            "id": fallback_id,
            "defined": False,
        }
    snapshot = scrub_manifest(manifest)
    snapshot["defined"] = True
    return snapshot


def ad_hoc_agent_snapshot(*, harness_id: str, provider_id: str, model: str) -> dict[str, Any] | None:
    if not (harness_id or provider_id or model):
        return None
    return {
        "id": "ad-hoc",
        "defined": False,
        "harness": harness_id,
        "provider": provider_id,
        "model": model,
    }


def scrub_manifest(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: scrub_manifest(item) for key, item in value.items() if key != "uuid"}
    if isinstance(value, list):
        return [scrub_manifest(item) for item in value]
    return value


def harness_prompt_paths(harness: dict[str, Any] | None) -> list[str]:
    if not harness:
        return []
    prompt_control = harness.get("prompt_control", {})
    if not isinstance(prompt_control, dict):
        return []
    paths = []
    for key in ("model_instructions_file", "memory_file"):
        value = str(prompt_control.get(key, "")).strip()
        if value:
            paths.append(value)
    paths.extend(str(value).strip() for value in prompt_control.get("prompt_paths", []) if str(value).strip())
    return paths


def static_experience_ids(value: Any) -> list[str]:
    """Return old-style static lived-experience ids, not renderer configs."""
    if isinstance(value, dict):
        return []
    return normalize_ids(value)


def plugin_file_snapshots(system: MemorySystem, paths: Sequence[str]) -> list[dict[str, Any]]:
    return [file_snapshot(system.plugin_path, relative_path) for relative_path in paths]


def lived_experience_snapshot(system: MemorySystem, experience_id: str) -> dict[str, Any]:
    manifest = system.lived_experience.get(experience_id)
    if not manifest:
        return {
            "id": experience_id,
            "defined": False,
        }
    snapshot = snapshot_manifest(manifest, fallback_id=experience_id)
    base = Path(str(manifest.get("path", system.plugin_path)))
    snapshot["files"] = [file_snapshot(base, relative_path) for relative_path in manifest.get("files", [])]
    return snapshot


def file_snapshot(base: Path, relative_path: Any) -> dict[str, Any]:
    rel = str(relative_path)
    path = (base / rel).resolve()
    result: dict[str, Any] = {
        "path": rel,
        "exists": path.is_file(),
    }
    if not path.is_file():
        return result
    try:
        result["text"] = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        result["text"] = None
        result["binary"] = True
    return result


def stable_hash(payload: dict[str, Any]) -> str:
    data = json.dumps(payload, sort_keys=True, default=str).encode("utf-8")
    return hashlib.sha256(data).hexdigest()


def public_policy(policy: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in policy.items() if key != "source_path"}


def normalized_scope(policy: dict[str, Any]) -> dict[str, bool]:
    scope = policy.get("scope", {})
    if not isinstance(scope, dict):
        scope = {}
    return {
        "job": bool(scope.get("job", True)),
        "agent": bool(scope.get("agent", False)),
        "plugin": bool(scope.get("plugin", False)),
    }


def safe_id(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9_.-]+", "-", value.strip()).strip("-")
    return cleaned or "runtime"


def normalize_ids(values: Any) -> list[str]:
    if isinstance(values, str):
        raw_values = [values]
    else:
        raw_values = list(values or [])

    ids: list[str] = []
    seen: set[str] = set()
    for raw in raw_values:
        for item in str(raw).split(","):
            clean = item.strip()
            if clean and clean not in seen:
                ids.append(clean)
                seen.add(clean)
    return ids


def unique_values(values: Sequence[Any]) -> list[Any]:
    result = []
    seen = set()
    for value in values:
        if value not in seen:
            result.append(value)
            seen.add(value)
    return result


def toml_from_dict(values: dict[str, Any]) -> str:
    lines = []
    for key, value in values.items():
        if isinstance(value, list):
            items = ", ".join(f'"{toml_escape(item)}"' for item in value)
            lines.append(f"{key} = [{items}]")
        else:
            lines.append(f'{key} = "{toml_escape(value)}"')
    return "\n".join(lines) + "\n"


def toml_records(section: str, records: Sequence[dict[str, Any]]) -> str:
    lines: list[str] = []
    for record in records:
        if lines:
            lines.append("")
        lines.append(f"[[{section}]]")
        for key, value in record.items():
            lines.append(f'{key} = "{toml_escape(value)}"')
    return "\n".join(lines) + "\n"


def toml_escape(value: Any) -> str:
    return str(value).replace("\\", "\\\\").replace('"', '\\"')
