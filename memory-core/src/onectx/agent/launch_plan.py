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
    resolved_model = resolve_model(agent, resolved_provider, model)
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
    )
    plan = AgentLaunchPlan(
        run_id=resolved_run_id,
        provider=resolved_provider,
        job_id=job_id,
        agent_id=agent_id,
        harness_id=harness_id,
        model=resolved_model,
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
            "- If the job is underspecified, produce a clear blocked/needs_approval result instead of guessing.",
            "- End with the artifact paths, evidence checks, and next state-machine event you believe should fire.",
            "",
        ]
    )
    return "\n".join(sections)


def command_for_harness(
    *,
    system: MemorySystem,
    harness_id: str,
    model: str,
    run_dir: Path,
    workspace_dir: Path,
    prompt_path: Path,
) -> dict[str, Any]:
    if harness_id == "codex-harness":
        codex_home = run_dir / "CODEX_HOME"
        final_message = run_dir / "final-message.md"
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
        return {
            "kind": "codex.exec",
            "available": bool(shutil.which("codex")),
            "cwd": str(workspace_dir),
            "env": {"CODEX_HOME": str(codex_home)},
            "stdin_path": str(prompt_path),
            "argv": argv,
            "shell": shell_join(argv, env={"CODEX_HOME": str(codex_home)}) + f" < {shlex.quote(str(prompt_path))}",
        }
    if harness_id == "claude-code":
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
            "available": bool(shutil.which("claude")),
            "cwd": str(workspace_dir),
            "env": {},
            "stdin_path": str(prompt_path),
            "argv": argv,
            "shell": shell_join(argv) + f" \"$(cat {shlex.quote(str(prompt_path))})\"",
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


def resolve_model(agent: dict[str, Any], provider: str, override: str) -> str:
    if override:
        return override
    if provider == "codex":
        return os.environ.get("ONECONTEXT_CODEX_MODEL", "gpt-5.5")
    if provider == "claude":
        return optional_str(agent.get("model")) or os.environ.get("ONECONTEXT_CLAUDE_MODEL", "opus")
    return optional_str(agent.get("model")) or ""


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
