from __future__ import annotations

from typing import Any

from .linker import HIRED_AGENT_CREATED_EVENT


class BirthCertificateError(RuntimeError):
    """Raised when a hired-agent birth certificate cannot be found."""


def select_birth_event(
    events: list[dict[str, Any]],
    *,
    hired_agent_uuid: str | None = None,
) -> dict[str, Any]:
    births = [event for event in events if event.get("event") == HIRED_AGENT_CREATED_EVENT]
    if hired_agent_uuid:
        needle = hired_agent_uuid.strip()
        births = [
            event
            for event in births
            if str(event.get("hired_agent_uuid", "")) == needle
            or str(event.get("hired_agent_uuid", "")).endswith(needle)
        ]
    if not births:
        target = f" for {hired_agent_uuid}" if hired_agent_uuid else ""
        raise BirthCertificateError(f"no hired-agent birth certificate found{target}")
    return births[-1]


def render_birth_certificate(event: dict[str, Any]) -> str:
    certificate = event.get("birth_certificate", {})
    snapshot = certificate.get("config_snapshot", {}) if isinstance(certificate, dict) else {}
    attachment = certificate.get("attachment", event.get("attachment", {})) if isinstance(certificate, dict) else {}
    versions = certificate.get("definition_versions", event.get("definition_versions", {}))
    fingerprints = event.get("fingerprints", {})

    agent = manifest_id(snapshot.get("agent"))
    harness = manifest_id(snapshot.get("harness"))
    provider = manifest_id(snapshot.get("provider"))
    model = value_or_dash(snapshot.get("model"))
    created_state = runtime_experience_state(attachment)

    lines = [
        "Hired Agent Birth Certificate",
        f"  born: {value_or_dash(event.get('ts'))}",
        f"  uuid: {value_or_dash(event.get('hired_agent_uuid'))}",
        f"  jobs: {join_values(event.get('job_ids', []))}",
        f"  job params: {format_params(event.get('job_params', {}))}",
        f"  agent: {agent}",
        f"  harness: {harness}",
        f"  model: {provider}/{model}" if provider != "-" or model != "-" else "  model: -",
        f"  runtime experience: {value_or_dash(attachment.get('experience_id'))} ({created_state})",
        f"  experience path: {value_or_dash(attachment.get('experience_path'))}",
    ]

    native_homes = attachment.get("runtime_native_homes") or []
    lines.append("  native homes:")
    if native_homes:
        for home in native_homes:
            auth = home.get("auth_link_status") or home.get("auth_mode") or "-"
            lines.append(
                "    "
                f"{value_or_dash(home.get('harness'))} "
                f"{value_or_dash(home.get('format'))} -> {value_or_dash(home.get('home'))} "
                f"auth={auth}"
            )
    else:
        lines.append("    -")

    lines.append("  prompts:")
    add_file_list(lines, snapshot.get("prompts", []))
    lines.append("  references:")
    add_file_list(lines, snapshot.get("references", []))
    lines.append("  lived experience:")
    add_manifest_list(lines, snapshot.get("lived_experience", []))

    harness_tools = snapshot.get("harness_tools", {})
    lines.append("  tools:")
    if isinstance(harness_tools, dict):
        lines.append(f"    harness default: {join_values(harness_tools.get('default', []))}")
        lines.append(f"    harness optional: {join_values(harness_tools.get('optional', []))}")
    else:
        lines.append("    harness: -")
    custom_tools = [manifest_id(item) for item in snapshot.get("custom_tools", [])]
    lines.append(f"    custom: {join_values(custom_tools)}")

    lines.append("  accounts:")
    add_account_list(lines, snapshot.get("accounts", []))
    lines.append("  versions:")
    for key in (
        "ledger_schema_version",
        "plugin_id",
        "plugin_version",
        "plugin_schema_version",
        "linking_version",
        "linker",
        "linker_version",
    ):
        lines.append(f"    {key}: {value_or_dash(versions.get(key))}")
    lines.append("  fingerprints:")
    for key in ("birth_certificate_sha256", "config_snapshot_sha256", "linking_policy_sha256"):
        lines.append(f"    {key}: {value_or_dash(fingerprints.get(key))}")

    return "\n".join(lines)


def add_file_list(lines: list[str], items: Any) -> None:
    if not isinstance(items, list) or not items:
        lines.append("    -")
        return
    for item in items:
        if not isinstance(item, dict):
            lines.append(f"    {item}")
            continue
        exists = "ok" if item.get("exists") else "missing"
        lines.append(f"    {value_or_dash(item.get('path'))} ({exists})")


def add_manifest_list(lines: list[str], items: Any) -> None:
    if not isinstance(items, list) or not items:
        lines.append("    -")
        return
    for item in items:
        lines.append(f"    {manifest_id(item)}")


def add_account_list(lines: list[str], items: Any) -> None:
    if not isinstance(items, list) or not items:
        lines.append("    -")
        return
    for item in items:
        if not isinstance(item, dict):
            lines.append(f"    {item}")
            continue
        selected = item.get("selected_mode") or item.get("default_mode") or "-"
        status = item.get("selected_mode_status") or "-"
        lines.append(f"    {manifest_id(item)} selected={selected} status={status}")


def format_params(params: Any) -> str:
    if not isinstance(params, dict) or not params:
        return "-"
    return ", ".join(f"{key}={value}" for key, value in sorted(params.items()))


def runtime_experience_state(attachment: dict[str, Any]) -> str:
    if attachment.get("mode") == "none" or not attachment.get("experience_id"):
        return "none"
    return "created" if attachment.get("created_runtime_experience") else "reused"


def manifest_id(item: Any) -> str:
    if not isinstance(item, dict):
        return "-"
    item_id = item.get("id")
    if item_id:
        return str(item_id)
    return "-"


def join_values(values: Any) -> str:
    if not values:
        return "-"
    if isinstance(values, str):
        return values
    return ", ".join(str(value) for value in values) or "-"


def value_or_dash(value: Any) -> str:
    if value in (None, ""):
        return "-"
    return str(value)
