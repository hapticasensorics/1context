from __future__ import annotations

import json
import os
import shlex
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem, compile_system_map, optional_str, resolve_path
from onectx.io_utils import atomic_write_json, atomic_write_text
from onectx.storage import stable_id, utc_now


class AgentLaunchPlanError(RuntimeError):
    """Raised when a job cannot be resolved into a runnable agent session."""


@dataclass(frozen=True)
class AgentLaunchPlan:
    run_id: str
    provider: str
    job_id: str
    agent_id: str
    harness_id: str
    model: str
    model_policy: dict[str, Any]
    status: str
    run_dir: Path
    workspace_dir: Path
    prompt_path: Path
    launch_path: Path
    script_path: Path
    command: dict[str, Any]
    job: dict[str, Any]
    agent: dict[str, Any]
    harness: dict[str, Any]
    prompt_paths: tuple[str, ...]
    reference_paths: tuple[str, ...]
    params: dict[str, str]
    missing: dict[str, Any]

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        return {
            "schema_version": 1,
            "operation": "agent.launch.plan",
            "status": self.status,
            "run_id": self.run_id,
            "provider": self.provider,
            "job": {
                "id": self.job_id,
                "label": self.job.get("label", self.job_id),
                "source_path": self.job.get("source_path", ""),
            },
            "agent": {
                "id": self.agent_id,
                "label": self.agent.get("label", self.agent_id),
                "source_path": self.agent.get("source_path", ""),
            },
            "harness": {
                "id": self.harness_id,
                "label": self.harness.get("label", self.harness_id),
                "source_path": self.harness.get("source_path", ""),
            },
            "model": self.model,
            "model_policy": dict(self.model_policy),
            "paths": {
                "run_dir": format_path(self.run_dir, root),
                "workspace_dir": format_path(self.workspace_dir, root),
                "prompt": format_path(self.prompt_path, root),
                "launch": format_path(self.launch_path, root),
                "run_script": format_path(self.script_path, root),
            },
            "prompt_paths": list(self.prompt_paths),
            "reference_paths": list(self.reference_paths),
            "params": dict(self.params),
            "missing": self.missing,
            "command": self.command,
            "agent_harness_call": agent_harness_call_request(self),
        }


def build_agent_launch_plan(
    system: MemorySystem,
    *,
    job_id: str,
    provider: str = "declared",
    model: str = "",
    run_id: str = "",
    workspace: Path | str | None = None,
    params: dict[str, str] | None = None,
    materialize: bool = True,
) -> AgentLaunchPlan:
    job = require_record(system.jobs, job_id, "job")
    agent_id = optional_str(job.get("agent"))
    if not agent_id:
        raise AgentLaunchPlanError(f"job {job_id!r} is missing an agent")
    agent = require_record(system.agents, agent_id, "agent")

    requested_provider = normalize_provider(provider)
    harness_id = resolve_harness_id(agent, requested_provider)
    harness = require_record(system.harnesses, harness_id, "harness")
    resolved_provider = provider_for_harness(harness_id, agent, requested_provider)
    resolved_model = resolve_model(agent, resolved_provider, model, requested_provider=requested_provider)
    model_policy = resolve_model_policy(agent)
    timestamp = utc_now()
    resolved_run_id = run_id or stable_id("agent_run", job_id, resolved_provider, timestamp)
    run_dir = system.runtime_dir / "agent-sessions" / resolved_run_id
    workspace_dir = Path(workspace).expanduser().resolve() if workspace else run_dir / "workspace"
    prompt_path = run_dir / "prompt.md"
    launch_path = run_dir / "launch.json"
    script_path = run_dir / "run.sh"
    params = dict(params or {})
    prompt_paths = resolve_prompt_paths(system, agent, job)
    reference_paths = resolve_reference_paths(system, agent, job)
    system_map = compile_system_map(system)
    compiled_job = system_map.get("jobs", {}).get(job_id, {})
    missing = dict(compiled_job.get("missing", {}))

    command = command_for_harness(
        system=system,
        harness_id=harness_id,
        model=resolved_model,
        run_dir=run_dir,
        workspace_dir=workspace_dir,
        prompt_path=prompt_path,
        model_policy=model_policy,
    )
    plan = AgentLaunchPlan(
        run_id=resolved_run_id,
        provider=resolved_provider,
        job_id=job_id,
        agent_id=agent_id,
        harness_id=harness_id,
        model=resolved_model,
        model_policy=model_policy,
        status=compiled_job.get("status", "unknown"),
        run_dir=run_dir,
        workspace_dir=workspace_dir,
        prompt_path=prompt_path,
        launch_path=launch_path,
        script_path=script_path,
        command=command,
        job=job,
        agent=agent,
        harness=harness,
        prompt_paths=tuple(str(path) for path in prompt_paths),
        reference_paths=tuple(str(path) for path in reference_paths),
        params=params,
        missing=missing,
    )
    if materialize:
        materialize_launch_plan(plan, system=system, prompt_paths=prompt_paths, reference_paths=reference_paths)
    return plan


def materialize_launch_plan(
    plan: AgentLaunchPlan,
    *,
    system: MemorySystem,
    prompt_paths: tuple[Path, ...],
    reference_paths: tuple[Path, ...],
) -> None:
    plan.workspace_dir.mkdir(parents=True, exist_ok=True)
    prompt = render_prompt_packet(plan, system=system, prompt_paths=prompt_paths, reference_paths=reference_paths)
    atomic_write_text(plan.prompt_path, prompt)
    atomic_write_json(plan.launch_path, plan.to_payload(root=system.root))
    atomic_write_text(plan.script_path, render_run_script(plan))
    os.chmod(plan.script_path, 0o755)
    maybe_link_codex_auth(plan)


def render_prompt_packet(
    plan: AgentLaunchPlan,
    *,
    system: MemorySystem,
    prompt_paths: tuple[Path, ...],
    reference_paths: tuple[Path, ...],
) -> str:
    sections = [
        "# 1Context Hired Agent Session",
        "",
        f"Run ID: `{plan.run_id}`",
        f"Job: `{plan.job_id}`",
        f"Agent: `{plan.agent_id}`",
        f"Harness: `{plan.harness_id}`",
        f"Provider: `{plan.provider}`",
        f"Model: `{plan.model}`",
        f"Model policy: `{json.dumps(plan.model_policy, sort_keys=True)}`",
        "",
        "## Job Contract",
        fenced_json(redact_source_paths(plan.job)),
        "",
        "## Agent Profile",
        fenced_json(redact_source_paths(plan.agent)),
        "",
        "## Runtime Params",
        fenced_json(plan.params),
        "",
        "## Agent Harness Birth Request",
        fenced_json(agent_harness_call_request(plan)),
        "",
        *injected_context_sections(plan),
        "",
        "## References",
        "\n".join(f"- `{path}`" for path in plan.reference_paths) or "- none",
        "",
        "## Instructions",
    ]
    for path in prompt_paths:
        sections.extend(
            [
                "",
                f"### {format_path(path, system.root)}",
                "",
                safe_read_text(path),
            ]
        )
    sections.extend(
        [
            "",
            "## Operating Contract",
            "",
            "- Stay inside the job permissions and output contract.",
            "- Prefer durable artifacts and concise proof over chatty status.",
            "- Your required job receipt is the final message written by the harness adapter. Include these fields: status, evidence, proposed_wiki_talk, page_change_summary (added, updated, removed, merged, left_unchanged), next_agent_requests, next_state_machine_event.",
            "- If the job is underspecified, produce a clear blocked/needs_approval result instead of guessing.",
            "- End with the artifact paths, evidence checks, and next state-machine event you believe should fire.",
            "",
        ]
    )
    return "\n".join(sections)


def injected_context_sections(plan: AgentLaunchPlan) -> list[str]:
    source_packet = str(plan.params.get("source_packet_path") or "").strip()
    if not source_packet:
        return []
    path = Path(source_packet).expanduser()
    if not path.is_file():
        return [
            "## Injected Session History",
            "",
            f"Source packet was declared but not found: `{source_packet}`",
        ]
    return [
        "## Injected Session History",
        "",
        safe_read_text(path),
    ]


def agent_harness_call_request(plan: AgentLaunchPlan) -> dict[str, Any]:
    source_packet_path = str(plan.params.get("source_packet_path") or "")
    source_packet_kind = str(plan.params.get("source_packet_kind") or "")
    source_status = str(plan.params.get("source_status") or "")
    source_event_count = str(plan.params.get("source_event_count") or "0")
    source_session_count = str(plan.params.get("source_session_count") or "0")
    source_window_days = str(plan.params.get("source_window_days") or "0")
    runtime = {
        "run_id": plan.run_id,
        "run_dir": str(plan.run_dir),
        "workspace_dir": str(plan.workspace_dir),
        "prompt_path": str(plan.prompt_path),
        "launch_path": str(plan.launch_path),
        "run_script": str(plan.script_path),
        "command_kind": str(plan.command.get("kind") or ""),
        "provider": plan.provider,
        "harness_id": plan.harness_id,
    }
    capabilities = [
        {
            "id": "prompt_packet",
            "transport": harness_transport(plan),
            "tool_names": ["agent.prompt_packet"],
            "config": {"prompt_path": str(plan.prompt_path)},
            "policy": {"body_persisted_in_harness": False},
            "proof_required": [],
        },
        {
            "id": "context_injection",
            "transport": "host_hook",
            "tool_names": ["source_packet.perception_db_session_history"],
            "config": {
                "source_packet_path": source_packet_path,
                "source_packet_kind": source_packet_kind,
                "source_store": str(plan.params.get("source_store") or ""),
                "source_status": source_status,
                "source_window_days": source_window_days,
                "source_event_count": source_event_count,
                "source_session_count": source_session_count,
            },
            "policy": {"raw_session_body_redacted_from_birth_certificate": True},
            "proof_required": ["context_injection"],
        },
    ]
    if plan.job_id.startswith("memory.wiki."):
        capabilities.append(
            {
                "id": "wiki_core",
                "transport": "host_hook",
                "tool_names": wiki_core_tool_names_for_job(plan.job_id),
                "config": {"surface": "onecontext-wiki-core"},
                "policy": {"page_lifecycle_owner": "onecontext-wiki-core"},
                "proof_required": [],
            }
        )
    return {
        "schema_version": 1,
        "operation": "agent.harness.call",
        "request": {
            "unit_id": plan.run_id,
            "role": plan.agent_id,
            "model": plan.model,
            "model_policy": dict(plan.model_policy),
            "identity": {
                "agent_id": plan.agent_id,
                "agent_label": str(plan.agent.get("label") or plan.agent_id),
                "job_id": plan.job_id,
                "job_label": str(plan.job.get("label") or plan.job_id),
                "provider": plan.provider,
                "harness_id": plan.harness_id,
            },
            "instructions": {
                "prompt_packet_path": str(plan.prompt_path),
                "source_packet_path": source_packet_path,
                "output_contract": "Use the prompt packet and write durable artifacts through the declared 1Context surfaces.",
            },
            "runtime": runtime,
            "capabilities": capabilities,
            "visibility": "private",
            "metadata": {
                "source": "memory.update_wiki",
                "job_status": plan.status,
                "reference_paths": list(plan.reference_paths),
                "prompt_paths": list(plan.prompt_paths),
                "source_status": source_status,
                "source_window_days": source_window_days,
                "source_event_count": source_event_count,
                "source_session_count": source_session_count,
                "model_policy": dict(plan.model_policy),
            },
        },
    }


def harness_transport(plan: AgentLaunchPlan) -> str:
    if plan.harness_id == "codex-harness":
        return "codex_skill"
    return "host_hook"


def command_for_harness(
    *,
    system: MemorySystem,
    harness_id: str,
    model: str,
    run_dir: Path,
    workspace_dir: Path,
    prompt_path: Path,
    model_policy: dict[str, Any] | None = None,
) -> dict[str, Any]:
    if harness_id == "codex-harness":
        codex_home = run_dir / "CODEX_HOME"
        final_message = run_dir / "final-message.md"
        env = developer_tool_env({"CODEX_HOME": str(codex_home)})
        argv = [
            "codex",
            "exec",
            "--ignore-user-config",
            "--ignore-rules",
            "--dangerously-bypass-approvals-and-sandbox",
            "--json",
            "--output-last-message",
            str(final_message),
            "-C",
            str(workspace_dir),
            "-m",
            model,
            "-c",
            "project_doc_max_bytes=0",
            "-c",
            'approval_policy="never"',
            "-c",
            'sandbox_mode="danger-full-access"',
            "-c",
            f'model_instructions_file="{system.plugin_path / "prompts/codex-harness.instructions.md"}"',
            "-",
        ]
        reasoning_effort = optional_str((model_policy or {}).get("reasoning_effort"))
        if reasoning_effort:
            argv[-1:-1] = [
                "-c",
                f'model_reasoning_effort="{reasoning_effort}"',
            ]
        return {
            "kind": "codex.exec",
            "available": bool(shutil.which("codex", path=env.get("PATH"))),
            "cwd": str(workspace_dir),
            "env": env,
            "stdin_path": str(prompt_path),
            "final_message_path": str(final_message),
            "model_policy": dict(model_policy or {}),
            "argv": argv,
            "shell": shell_join(argv, env=env) + f" < {shlex.quote(str(prompt_path))}",
        }
    if harness_id == "claude-code":
        env = developer_tool_env()
        argv = [
            "claude",
            "--print",
            "--output-format",
            "stream-json",
            "--model",
            model,
            "--permission-mode",
            "acceptEdits",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--mcp-config",
            '{"mcpServers":{}}',
            "--disable-slash-commands",
            "--setting-sources",
            "project,local",
            "--add-dir",
            str(workspace_dir),
        ]
        return {
            "kind": "claude.print",
            "available": bool(shutil.which("claude", path=env.get("PATH"))),
            "cwd": str(workspace_dir),
            "env": env,
            "stdin_path": str(prompt_path),
            "model_policy": dict(model_policy or {}),
            "argv": argv,
            "shell": shell_join(argv, env=env) + f" \"$(cat {shlex.quote(str(prompt_path))})\"",
        }
    raise AgentLaunchPlanError(f"unsupported harness for launch planning: {harness_id}")


def render_run_script(plan: AgentLaunchPlan) -> str:
    lines = [
        "#!/usr/bin/env bash",
        "set -euo pipefail",
        f"cd {shlex.quote(str(plan.workspace_dir))}",
    ]
    env = plan.command.get("env", {})
    for key, value in sorted(env.items()):
        lines.append(f"export {key}={shlex.quote(str(value))}")
    if plan.harness_id == "codex-harness":
        lines.append(shell_join(plan.command["argv"]) + f" < {shlex.quote(str(plan.prompt_path))}")
    elif plan.harness_id == "claude-code":
        lines.append(shell_join(plan.command["argv"]) + f" \"$(cat {shlex.quote(str(plan.prompt_path))})\"")
    else:
        raise AgentLaunchPlanError(f"unsupported harness for run script: {plan.harness_id}")
    lines.append("")
    return "\n".join(lines)


def maybe_link_codex_auth(plan: AgentLaunchPlan) -> None:
    if plan.harness_id != "codex-harness":
        return
    source = Path.home() / ".codex" / "auth.json"
    if not source.exists():
        return
    codex_home = Path(plan.command["env"]["CODEX_HOME"])
    codex_home.mkdir(parents=True, exist_ok=True)
    target = codex_home / "auth.json"
    if target.exists() or target.is_symlink():
        return
    target.symlink_to(source)


def developer_tool_env(extra: dict[str, str] | None = None) -> dict[str, str]:
    env = dict(extra or {})
    existing = os.environ.get("PATH", "")
    home = Path.home()
    path_parts: list[str] = []
    for item in (
        existing,
        *nvm_node_bin_paths(home),
        str(home / ".local/bin"),
        str(home / ".cargo/bin"),
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ):
        for part in item.split(":"):
            if part and part not in path_parts:
                path_parts.append(part)
    env["PATH"] = ":".join(path_parts)
    return env


def nvm_node_bin_paths(home: Path) -> list[str]:
    versions_root = home / ".nvm" / "versions" / "node"
    try:
        versions = sorted((path for path in versions_root.iterdir() if path.is_dir()), key=lambda path: path.name, reverse=True)
    except OSError:
        return []
    return [str(path / "bin") for path in versions]


def resolve_harness_id(agent: dict[str, Any], provider: str) -> str:
    if provider == "declared":
        harness_id = optional_str(agent.get("harness"))
        if harness_id:
            return harness_id
        raise AgentLaunchPlanError(f"agent {agent.get('id')!r} is missing harness")
    if provider == "codex":
        return "codex-harness"
    if provider == "claude":
        return "claude-code"
    raise AgentLaunchPlanError(f"unsupported launch provider {provider!r}; expected declared, codex, or claude")


def provider_for_harness(harness_id: str, agent: dict[str, Any], requested_provider: str) -> str:
    if requested_provider in {"codex", "claude"}:
        return requested_provider
    provider = optional_str(agent.get("provider"))
    if provider:
        return provider
    if harness_id == "codex-harness":
        return "codex"
    if harness_id == "claude-code":
        return "claude"
    return "unknown"


def wiki_core_tool_names_for_job(job_id: str) -> list[str]:
    page_write_jobs = {
        "memory.wiki.for_you_curator",
        "memory.wiki.context_curator",
        "memory.wiki.redactor",
    }
    if job_id in page_write_jobs:
        return ["wiki.page.write_body", "wiki.page.patch_body", "wiki.publish", "wiki.talk.append"]
    return ["wiki.talk.append"]


def resolve_model(agent: dict[str, Any], provider: str, override: str, *, requested_provider: str) -> str:
    if override:
        return override
    if provider == "codex":
        return os.environ.get("ONECONTEXT_CODEX_MODEL", "gpt-5.5")
    if provider == "claude":
        if requested_provider == "declared":
            return optional_str(agent.get("model")) or os.environ.get("ONECONTEXT_CLAUDE_MODEL", "opus")
        return os.environ.get("ONECONTEXT_CLAUDE_MODEL", "opus")
    return optional_str(agent.get("model")) or ""


def resolve_model_policy(agent: dict[str, Any]) -> dict[str, Any]:
    raw = agent.get("model_policy", {})
    if raw is None:
        raw = {}
    if not isinstance(raw, dict):
        raise AgentLaunchPlanError(f"agent {agent.get('id')!r} model_policy must be a table")

    policy: dict[str, Any] = {}
    reasoning_effort = optional_str(raw.get("reasoning_effort"))
    if reasoning_effort:
        if reasoning_effort not in {"none", "minimal", "low", "medium", "high", "xhigh"}:
            raise AgentLaunchPlanError(
                f"agent {agent.get('id')!r} model_policy.reasoning_effort must be one of none, minimal, low, medium, high, xhigh"
            )
        policy["reasoning_effort"] = reasoning_effort

    usable_context_tokens = optional_positive_int(raw.get("usable_context_tokens"))
    if usable_context_tokens is not None:
        policy["usable_context_tokens"] = usable_context_tokens

    context_fraction = optional_positive_float(raw.get("context_fraction"))
    if context_fraction is not None:
        policy["context_fraction"] = context_fraction

    return policy


def optional_positive_int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    try:
        parsed = int(value)
    except (TypeError, ValueError) as exc:
        raise AgentLaunchPlanError(f"model_policy usable_context_tokens must be an integer, got {value!r}") from exc
    if parsed < 1:
        raise AgentLaunchPlanError("model_policy usable_context_tokens must be >= 1")
    return parsed


def optional_positive_float(value: Any) -> float | None:
    if value is None or value == "":
        return None
    try:
        parsed = float(value)
    except (TypeError, ValueError) as exc:
        raise AgentLaunchPlanError(f"model_policy context_fraction must be a number, got {value!r}") from exc
    if parsed <= 0:
        raise AgentLaunchPlanError("model_policy context_fraction must be > 0")
    return parsed


def resolve_prompt_paths(system: MemorySystem, agent: dict[str, Any], job: dict[str, Any]) -> tuple[Path, ...]:
    paths = []
    for raw in list(agent.get("prompt_paths", [])) + list(job.get("prompt_paths", [])):
        paths.append(resolve_path(system.plugin_path, raw))
    return tuple(unique_paths(paths))


def resolve_reference_paths(system: MemorySystem, agent: dict[str, Any], job: dict[str, Any]) -> tuple[Path, ...]:
    paths = []
    for raw in list(agent.get("reference_paths", [])) + list(job.get("reference_paths", [])):
        paths.append(resolve_path(system.plugin_path, raw))
    return tuple(unique_paths(paths))


def normalize_provider(value: str) -> str:
    provider = str(value or "declared").strip().lower()
    if provider in {"default", "native", ""}:
        return "declared"
    if provider in {"claude-code", "anthropic"}:
        return "claude"
    return provider


def parse_param_pairs(values: list[str]) -> dict[str, str]:
    params: dict[str, str] = {}
    for value in values:
        if "=" not in value:
            raise AgentLaunchPlanError(f"param must be key=value, got {value!r}")
        key, raw = value.split("=", 1)
        clean_key = key.strip()
        if not clean_key:
            raise AgentLaunchPlanError(f"param key cannot be empty: {value!r}")
        params[clean_key] = raw
    return params


def require_record(records: dict[str, dict[str, Any]], record_id: str, label: str) -> dict[str, Any]:
    try:
        return records[record_id]
    except KeyError as exc:
        raise AgentLaunchPlanError(f"unknown {label} {record_id!r}") from exc


def safe_read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        return f"<!-- unable to read {path}: {exc} -->"


def redact_source_paths(value: dict[str, Any]) -> dict[str, Any]:
    return dict(value)


def fenced_json(value: Any) -> str:
    return "```json\n" + json.dumps(value, indent=2, sort_keys=True, default=str) + "\n```"


def shell_join(argv: list[str], *, env: dict[str, str] | None = None) -> str:
    prefix = ""
    if env:
        prefix = " ".join(f"{key}={shlex.quote(str(value))}" for key, value in sorted(env.items())) + " "
    return prefix + " ".join(shlex.quote(str(part)) for part in argv)


def format_path(path: Path, root: Path | None = None) -> str:
    if root is not None:
        try:
            return str(path.resolve().relative_to(root.resolve()))
        except ValueError:
            pass
    return str(path)


def unique_paths(paths: list[Path]) -> list[Path]:
    result: list[Path] = []
    seen: set[Path] = set()
    for path in paths:
        resolved = path.resolve()
        if resolved in seen:
            continue
        seen.add(resolved)
        result.append(resolved)
    return result
