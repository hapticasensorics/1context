from __future__ import annotations

import subprocess
import time
import re
from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, replace
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem

from .ledger import Ledger, ledger_events_path
from .linker import hire_agent


class HiredAgentRunnerError(RuntimeError):
    """Raised when the hired-agent execution lifecycle cannot complete."""


Validator = Callable[[Path], dict[str, Any]]


@dataclass(frozen=True)
class ArtifactSpec:
    kind: str
    path: Path

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": self.kind,
            "path": str(self.path),
        }


@dataclass(frozen=True)
class HarnessLaunchSpec:
    harness: str
    isolation_mode: str
    argv: list[str]
    cwd: Path
    stdin_label: str = "prompt-input.md"

    def to_payload(self) -> dict[str, Any]:
        return {
            "harness": self.harness,
            "isolation_mode": self.isolation_mode,
            "argv": list(self.argv),
            "cwd": str(self.cwd),
            "stdin": self.stdin_label,
        }


@dataclass(frozen=True)
class HiredAgentExecutionSpec:
    run_id: str
    job_ids: Sequence[str]
    job_params: dict[str, Any]
    experience_packet: dict[str, Any]
    prompt: str
    workspace: Path
    artifact: ArtifactSpec
    harness_launch: HarnessLaunchSpec
    prompt_stack: dict[str, Any] | None = None
    agent_id: str = ""
    harness_id: str = ""
    provider_id: str = ""
    model: str = ""
    mode: str = "new"
    run_harness: bool = False
    prompt_filename: str = "prompt-input.md"
    completed_event: str = "hired_agent.execution_completed"
    validator: Validator | None = None
    max_concurrent_agents: int | None = None
    harness_timeout_seconds: int | None = None


@dataclass(frozen=True)
class HiredAgentExecutionResult:
    dry_run: bool
    workspace: Path
    output_path: Path
    prompt_path: Path
    experience_packet: dict[str, Any]
    hire: dict[str, Any]
    validation: dict[str, Any]
    harness_launch: dict[str, Any]
    prompt_stack: dict[str, Any] | None = None
    returncode: int | None = None
    stdout_path: Path | None = None
    stderr_path: Path | None = None
    started_at: str = ""
    completed_at: str = ""
    duration_ms: int = 0

    def to_payload(self) -> dict[str, Any]:
        return {
            "dry_run": self.dry_run,
            "workspace": str(self.workspace),
            "output_path": str(self.output_path),
            "prompt_path": str(self.prompt_path),
            "experience_packet": self.experience_packet,
            "hire": self.hire,
            "validation": self.validation,
            "harness_launch": self.harness_launch,
            "prompt_stack": self.prompt_stack,
            "returncode": self.returncode,
            "stdout_path": str(self.stdout_path) if self.stdout_path else None,
            "stderr_path": str(self.stderr_path) if self.stderr_path else None,
            "started_at": self.started_at,
            "completed_at": self.completed_at,
            "duration_ms": self.duration_ms,
        }


@dataclass(frozen=True)
class HiredAgentBatchResult:
    max_concurrent: int
    results: tuple[HiredAgentExecutionResult | None, ...]
    errors: tuple[dict[str, Any], ...]
    started_at: str = ""
    completed_at: str = ""
    duration_ms: int = 0

    @property
    def ok(self) -> bool:
        return not self.errors

    def to_payload(self) -> dict[str, Any]:
        return {
            "ok": self.ok,
            "max_concurrent": self.max_concurrent,
            "result_count": len([result for result in self.results if result is not None]),
            "error_count": len(self.errors),
            "validation_failure_count": len(
                [
                    result
                    for result in self.results
                    if result is not None and not result.dry_run and not result.validation.get("ok")
                ]
            ),
            "started_at": self.started_at,
            "completed_at": self.completed_at,
            "duration_ms": self.duration_ms,
            "results": [result.to_payload() if result else None for result in self.results],
            "errors": list(self.errors),
        }


def execute_hired_agents(
    system: MemorySystem,
    specs: Sequence[HiredAgentExecutionSpec],
    *,
    max_concurrent: int | None = None,
    run_id: str | None = None,
) -> HiredAgentBatchResult:
    """Run several hired-agent executions while enforcing the configured concurrency cap."""
    spec_list = list(specs)
    effective_max = int(max_concurrent or system.runtime_policy["max_concurrent_agents"])
    if effective_max < 1:
        raise HiredAgentRunnerError("max_concurrent must be >= 1")

    started_at = now_iso()
    start = time.perf_counter()
    batch_run_id = run_id or shared_run_id(spec_list) or "hired-agent-batch"
    ledger = Ledger(ledger_events_path(system.runtime_dir), storage_path=system.storage_dir)
    ledger.append(
        "hired_agent.batch_started",
        ledger_schema_version="0.1",
        plugin_id=system.active_plugin,
        run_id=batch_run_id,
        summary=f"Starting hired-agent batch with {len(spec_list)} specs and max concurrency {effective_max}.",
        spec_count=len(spec_list),
        max_concurrent_agents=effective_max,
        started_at=started_at,
        outcome="started",
    )

    results: list[HiredAgentExecutionResult | None] = [None] * len(spec_list)
    errors: list[dict[str, Any]] = []
    if spec_list:
        with ThreadPoolExecutor(max_workers=min(effective_max, len(spec_list))) as executor:
            futures = {
                executor.submit(execute_hired_agent, system, replace(spec, max_concurrent_agents=effective_max)): index
                for index, spec in enumerate(spec_list)
            }
            for future in as_completed(futures):
                index = futures[future]
                try:
                    results[index] = future.result()
                except Exception as exc:  # noqa: BLE001 - batch mode intentionally collects job failures.
                    spec = spec_list[index]
                    errors.append(
                        {
                            "index": index,
                            "job_ids": list(spec.job_ids),
                            "run_id": spec.run_id,
                            "error_type": type(exc).__name__,
                            "message": str(exc),
                        }
                    )

    completed_at = now_iso()
    duration_ms = elapsed_ms(start)
    outcome = "done" if not errors else "failure"
    ledger.append(
        "hired_agent.batch_completed",
        ledger_schema_version="0.1",
        plugin_id=system.active_plugin,
        run_id=batch_run_id,
        summary=f"Completed hired-agent batch with {len(spec_list) - len(errors)} successes and {len(errors)} failures.",
        spec_count=len(spec_list),
        success_count=len(spec_list) - len(errors),
        error_count=len(errors),
        max_concurrent_agents=effective_max,
        started_at=started_at,
        completed_at=completed_at,
        duration_ms=duration_ms,
        errors=errors,
        outcome=outcome,
    )
    return HiredAgentBatchResult(
        max_concurrent=effective_max,
        results=tuple(results),
        errors=tuple(errors),
        started_at=started_at,
        completed_at=completed_at,
        duration_ms=duration_ms,
    )


def execute_hired_agent(system: MemorySystem, spec: HiredAgentExecutionSpec) -> HiredAgentExecutionResult:
    """Run the shared hired-agent lifecycle for one bounded job invocation."""
    started_at = now_iso()
    start = time.perf_counter()
    ledger = Ledger(ledger_events_path(system.runtime_dir), storage_path=system.storage_dir)
    spec.workspace.mkdir(parents=True, exist_ok=True)

    job_params = {
        **spec.job_params,
        "harness_isolation": spec.harness_launch.isolation_mode,
        "max_concurrent_agents": spec.max_concurrent_agents or system.runtime_policy["max_concurrent_agents"],
    }
    launch_payload = spec.harness_launch.to_payload()
    hire = hire_agent(
        system,
        job_ids=spec.job_ids,
        agent_id=spec.agent_id,
        harness_id=spec.harness_id,
        provider_id=spec.provider_id,
        model=spec.model,
        mode=spec.mode,
        run_id=spec.run_id,
        job_params=job_params,
        experience_packet=spec.experience_packet,
        harness_launch=launch_payload,
        prompt_stack=spec.prompt_stack or {},
    )

    prompt_path = hire.path / spec.prompt_filename if hire.path else spec.workspace / spec.prompt_filename
    prompt_path.parent.mkdir(parents=True, exist_ok=True)
    prompt_path.write_text(spec.prompt, encoding="utf-8")

    returncode = None
    stdout_path = None
    stderr_path = None
    if spec.run_harness:
        stdout_path = prompt_path.parent / "harness.stdout.md"
        stderr_path = prompt_path.parent / "harness.stderr.log"
        timeout_seconds = resolve_harness_timeout_seconds(system, spec)
        job_params["harness_timeout_seconds"] = timeout_seconds
        ledger.append(
            "harness.launch_started",
            ledger_schema_version="0.1",
            hired_agent_uuid=hire.hired_agent_uuid,
            job_ids=list(spec.job_ids),
            job_params=job_params,
            plugin_id=system.active_plugin,
            run_id=spec.run_id,
            summary=f"Started {spec.harness_launch.harness} for {', '.join(spec.job_ids)}.",
            harness_launch=launch_payload,
            timeout_seconds=timeout_seconds,
            started_at=now_iso(),
            outcome="started",
        )
        harness_start = time.perf_counter()
        try:
            completed = subprocess.run(
                spec.harness_launch.argv,
                cwd=spec.harness_launch.cwd,
                input=spec.prompt,
                text=True,
                capture_output=True,
                check=False,
                timeout=timeout_seconds,
            )
        except subprocess.TimeoutExpired as exc:
            harness_completed_at = now_iso()
            harness_duration_ms = elapsed_ms(harness_start)
            stdout_path.write_text(coerce_subprocess_output(exc.stdout), encoding="utf-8")
            stderr_text = coerce_subprocess_output(exc.stderr)
            if stderr_text:
                stderr_text += "\n"
            stderr_text += f"{spec.harness_launch.harness} timed out after {timeout_seconds} seconds\n"
            stderr_path.write_text(stderr_text, encoding="utf-8")
            ledger.append(
                "harness.launch_timed_out",
                ledger_schema_version="0.1",
                hired_agent_uuid=hire.hired_agent_uuid,
                job_ids=list(spec.job_ids),
                job_params=job_params,
                plugin_id=system.active_plugin,
                run_id=spec.run_id,
                summary=f"{spec.harness_launch.harness} timed out after {timeout_seconds} seconds.",
                harness_launch=launch_payload,
                timeout_seconds=timeout_seconds,
                stdout_path=str(stdout_path),
                stderr_path=str(stderr_path),
                completed_at=harness_completed_at,
                duration_ms=harness_duration_ms,
                outcome="timeout",
            )
            raise HiredAgentRunnerError(
                f"{spec.harness_launch.harness} timed out after {timeout_seconds} seconds; see {stderr_path}"
            ) from exc
        returncode = completed.returncode
        harness_completed_at = now_iso()
        harness_duration_ms = elapsed_ms(harness_start)
        stdout_path.write_text(completed.stdout, encoding="utf-8")
        stderr_path.write_text(completed.stderr, encoding="utf-8")
        ledger.append(
            "harness.launch_completed",
            ledger_schema_version="0.1",
            hired_agent_uuid=hire.hired_agent_uuid,
            job_ids=list(spec.job_ids),
            job_params=job_params,
            plugin_id=system.active_plugin,
            run_id=spec.run_id,
            summary=f"Completed {spec.harness_launch.harness} with return code {completed.returncode}.",
            harness_launch=launch_payload,
            timeout_seconds=timeout_seconds,
            returncode=completed.returncode,
            stdout_path=str(stdout_path),
            stderr_path=str(stderr_path),
            completed_at=harness_completed_at,
            duration_ms=harness_duration_ms,
            outcome="done" if completed.returncode == 0 else "failure",
        )
        if completed.returncode != 0:
            raise HiredAgentRunnerError(f"{spec.harness_launch.harness} exited {completed.returncode}; see {stderr_path}")

    if spec.run_harness:
        validation = (
            spec.validator(spec.artifact.path)
            if spec.validator
            else {"ok": spec.artifact.path.is_file(), "checks": [], "failures": []}
        )
    else:
        validation = {
            "ok": False,
            "checks": ["prompt assembled"],
            "failures": ["dry run did not launch harness or validate an output artifact"],
        }
    outcome = "dry_run" if not spec.run_harness else ("done" if validation.get("ok") else "failure")
    if spec.run_harness and spec.artifact.path.exists():
        ledger.append(
            "artifact.produced",
            ledger_schema_version="0.1",
            hired_agent_uuid=hire.hired_agent_uuid,
            job_ids=list(spec.job_ids),
            job_params=job_params,
            plugin_id=system.active_plugin,
            run_id=spec.run_id,
            summary=f"Produced {spec.artifact.kind} artifact.",
            artifact=spec.artifact.to_payload(),
            outcome="done",
        )
    ledger.append(
        "artifact.validated",
        ledger_schema_version="0.1",
        hired_agent_uuid=hire.hired_agent_uuid,
        job_ids=list(spec.job_ids),
        job_params=job_params,
        plugin_id=system.active_plugin,
        run_id=spec.run_id,
        summary=(
            f"Validated {spec.artifact.kind} artifact."
            if spec.run_harness
            else f"Skipped {spec.artifact.kind} artifact validation for dry run."
        ),
        artifact=spec.artifact.to_payload(),
        evidence=validation,
        outcome="skipped" if not spec.run_harness else ("done" if validation.get("ok") else "failure"),
    )
    completed_at = now_iso()
    duration_ms = elapsed_ms(start)
    ledger.append(
        spec.completed_event,
        ledger_schema_version="0.1",
        hired_agent_uuid=hire.hired_agent_uuid,
        job_ids=list(spec.job_ids),
        job_params=job_params,
        plugin_id=system.active_plugin,
        run_id=spec.run_id,
        summary=f"Hired-agent execution {'dry run' if not spec.run_harness else 'run'} for {', '.join(spec.job_ids)}.",
        experience_packet=spec.experience_packet,
        prompt_stack=spec.prompt_stack or {},
        harness_launch=launch_payload,
        artifact=spec.artifact.to_payload(),
        evidence=validation,
        started_at=started_at,
        completed_at=completed_at,
        duration_ms=duration_ms,
        outcome=outcome,
    )

    return HiredAgentExecutionResult(
        dry_run=not spec.run_harness,
        workspace=spec.workspace,
        output_path=spec.artifact.path,
        prompt_path=prompt_path,
        experience_packet=spec.experience_packet,
        hire=hire.asdict(),
        validation=validation,
        harness_launch=launch_payload,
        prompt_stack=spec.prompt_stack or {},
        returncode=returncode,
        stdout_path=stdout_path,
        stderr_path=stderr_path,
        started_at=started_at,
        completed_at=completed_at,
        duration_ms=duration_ms,
    )


def shared_run_id(specs: Sequence[HiredAgentExecutionSpec]) -> str | None:
    run_ids = {spec.run_id for spec in specs if spec.run_id}
    return next(iter(run_ids)) if len(run_ids) == 1 else None


def now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def elapsed_ms(start: float) -> int:
    return int((time.perf_counter() - start) * 1000)


def resolve_harness_timeout_seconds(system: MemorySystem, spec: HiredAgentExecutionSpec) -> int:
    if spec.harness_timeout_seconds is not None:
        return max(1, int(spec.harness_timeout_seconds))
    return parse_duration_seconds(system.runtime_policy.get("agent_timeout", "30m"))


def parse_duration_seconds(value: Any) -> int:
    if isinstance(value, (int, float)):
        return max(1, int(value))
    text = str(value or "").strip().lower()
    if not text:
        return 30 * 60
    match = re.fullmatch(r"(\d+(?:\.\d+)?)\s*([smhd]?)", text)
    if not match:
        raise HiredAgentRunnerError(f"unsupported duration {value!r}")
    amount = float(match.group(1))
    unit = match.group(2) or "s"
    multiplier = {"s": 1, "m": 60, "h": 3600, "d": 86400}[unit]
    return max(1, int(amount * multiplier))


def coerce_subprocess_output(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return str(value)
