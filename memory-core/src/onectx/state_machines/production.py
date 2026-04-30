from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem, compile_system_map
from onectx.state_machines.mermaid import StateMachineDiagramError, state_machine_to_mermaid
from onectx.storage import stable_id, utc_now


class StateMachineProductionError(RuntimeError):
    """Raised when state-machine production artifacts cannot be written."""


@dataclass(frozen=True)
class StateMachineProductionResult:
    run_id: str
    path: Path
    machines: tuple[str, ...]
    files: tuple[Path, ...]

    def to_payload(self, root: Path | None = None) -> dict[str, Any]:
        return {
            "run_id": self.run_id,
            "path": format_path(self.path, root),
            "machines": list(self.machines),
            "files": [format_path(path, root) for path in self.files],
        }


@dataclass(frozen=True)
class StateMachineVerificationResult:
    run_id: str
    path: Path
    passed: bool
    checks: tuple[dict[str, Any], ...]
    production: StateMachineProductionResult

    def to_payload(self, root: Path | None = None) -> dict[str, Any]:
        return {
            "run_id": self.run_id,
            "path": format_path(self.path, root),
            "passed": self.passed,
            "checks": list(self.checks),
            "production": self.production.to_payload(root),
        }


REQUIRED_MEMORY_SYSTEM_EVIDENCE = {
    "memory_cycle.artifact_written",
    "source_import.fresh",
    "reader_surface.ready",
    "memory_tick.recovery_recorded",
}
REQUIRED_MEMORY_SYSTEM_CYCLE_TERMINALS = {"complete", "blocked", "retryable", "failed"}


def compile_state_machine_artifacts(
    system: MemorySystem,
    *,
    output_dir: Path | None = None,
    run_id: str = "",
) -> StateMachineProductionResult:
    system_map = compile_system_map(system)
    machines = system_map.get("state_machines", {})
    production_id = run_id or stable_id("state-machine-production", utc_now())
    out_dir = (output_dir or system.runtime_dir / "state-machines" / "production" / production_id).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    files: list[Path] = []

    manifest = {
        "run_id": production_id,
        "created_at": utc_now(),
        "root": str(system.root),
        "active_plugin": system.active_plugin,
        "state_machine_language": system.state_machine_language,
        "machines": {},
    }
    for machine_id, machine in sorted(machines.items()):
        machine_dir = out_dir / machine_id
        machine_dir.mkdir(parents=True, exist_ok=True)
        ir_path = machine_dir / f"{machine_id}.ir.json"
        write_json(ir_path, machine)
        files.append(ir_path)
        scope_files: list[str] = []
        for scope in machine.get("scopes", []):
            scope_name = str(scope.get("name") or "")
            if not scope_name:
                continue
            try:
                mermaid = state_machine_to_mermaid(machine, scope_name=scope_name)
            except StateMachineDiagramError:
                continue
            diagram_path = machine_dir / f"{machine_id}.{scope_name}.mmd"
            diagram_path.write_text(mermaid, encoding="utf-8")
            files.append(diagram_path)
            scope_files.append(format_path(diagram_path, out_dir))
        manifest["machines"][machine_id] = {
            "version": machine.get("version", ""),
            "language": machine.get("language", {}),
            "source_path": machine.get("source_path", ""),
            "ir": format_path(ir_path, out_dir),
            "diagrams": scope_files,
            "scope_count": len(machine.get("scopes", [])),
            "transition_count": len(machine.get("transitions", [])),
            "artifact_count": len(machine.get("artifacts", [])),
            "evidence_count": len(machine.get("evidence", [])),
        }

    manifest_path = out_dir / "manifest.json"
    write_json(manifest_path, manifest)
    files.insert(0, manifest_path)
    return StateMachineProductionResult(
        run_id=production_id,
        path=out_dir,
        machines=tuple(sorted(machines)),
        files=tuple(files),
    )


def verify_state_machine_artifacts(
    system: MemorySystem,
    *,
    output_dir: Path | None = None,
    run_id: str = "",
) -> StateMachineVerificationResult:
    production = compile_state_machine_artifacts(system, output_dir=output_dir, run_id=run_id)
    system_map = compile_system_map(system)
    machines = system_map.get("state_machines", {})
    jobs = system_map.get("jobs", {})
    checks: list[dict[str, Any]] = []

    add_check(checks, "machines.present", bool(machines), f"{len(machines)} machine(s)")
    for machine_id, machine in sorted(machines.items()):
        add_check(checks, f"{machine_id}.scopes.present", bool(machine.get("scopes")), f"{len(machine.get('scopes', []))} scope(s)")
        add_check(
            checks,
            f"{machine_id}.transitions.present",
            bool(machine.get("transitions")),
            f"{len(machine.get('transitions', []))} transition(s)",
        )
        add_check(
            checks,
            f"{machine_id}.ir.written",
            (production.path / machine_id / f"{machine_id}.ir.json").is_file(),
            format_path(production.path / machine_id / f"{machine_id}.ir.json", system.root),
        )
        verify_spawn_jobs(checks, machine_id, machine, jobs)
        verify_explicit_sources(checks, machine_id, machine)

    memory = machines.get("memory_system_fabric")
    if memory:
        evidence_names = {str(item.get("name") or "") for item in memory.get("evidence", [])}
        missing = sorted(REQUIRED_MEMORY_SYSTEM_EVIDENCE - evidence_names)
        add_check(
            checks,
            "memory_system_fabric.runner_evidence_declared",
            not missing,
            "missing: " + ", ".join(missing) if missing else "all runner evidence declared",
        )
        cycle = next((scope for scope in memory.get("scopes", []) if scope.get("name") == "cycle"), {})
        cycle_states = {str(item) for item in cycle.get("states", [])}
        missing_states = sorted(REQUIRED_MEMORY_SYSTEM_CYCLE_TERMINALS - cycle_states)
        add_check(
            checks,
            "memory_system_fabric.cycle_terminal_states",
            not missing_states,
            "missing: " + ", ".join(missing_states) if missing_states else "terminal states declared",
        )
    else:
        add_check(checks, "memory_system_fabric.present", False, "missing")

    checks_path = production.path / "checks.json"
    summary_path = production.path / "summary.md"
    passed = all(check["status"] != "failed" for check in checks)
    write_json(checks_path, {"run_id": production.run_id, "passed": passed, "checks": checks})
    summary_path.write_text(render_summary(production.run_id, passed, checks), encoding="utf-8")
    return StateMachineVerificationResult(
        run_id=production.run_id,
        path=production.path,
        passed=passed,
        checks=tuple(checks),
        production=production,
    )


def verify_spawn_jobs(checks: list[dict[str, Any]], machine_id: str, machine: dict[str, Any], jobs: dict[str, Any]) -> None:
    spawns = [action for action in iter_actions(machine) if action.get("kind") == "spawn"]
    missing = sorted({str(action.get("job") or "") for action in spawns if str(action.get("job") or "") not in jobs})
    add_check(
        checks,
        f"{machine_id}.spawn_jobs_exist",
        not missing,
        "missing: " + ", ".join(missing) if missing else f"{len(spawns)} spawn action(s)",
    )


def verify_explicit_sources(checks: list[dict[str, Any]], machine_id: str, machine: dict[str, Any]) -> None:
    missing = []
    for index, transition in enumerate(machine.get("transitions", []), start=1):
        target = transition.get("target") or {}
        if target.get("scope") and not transition.get("source"):
            event = transition.get("event") or {}
            missing.append(f"{index}:{event.get('name') or event.get('kind')}")
    add_check(
        checks,
        f"{machine_id}.explicit_transition_sources",
        not missing,
        "missing: " + ", ".join(missing) if missing else "all scoped transitions declare source",
    )


def iter_actions(machine: dict[str, Any]):
    for transition in machine.get("transitions", []):
        yield from iter_action_list(transition.get("actions", []))


def iter_action_list(actions: Any):
    if isinstance(actions, dict):
        yield actions
        for child in actions.get("actions", []):
            yield from iter_action_list(child)
    elif isinstance(actions, list):
        for action in actions:
            yield from iter_action_list(action)


def add_check(checks: list[dict[str, Any]], check_id: str, passed: bool, detail: str, *, severity: str = "error") -> None:
    checks.append(
        {
            "id": check_id,
            "status": "passed" if passed else ("failed" if severity == "error" else "warning"),
            "severity": severity,
            "detail": detail,
        }
    )


def render_summary(run_id: str, passed: bool, checks: list[dict[str, Any]]) -> str:
    failed = [check for check in checks if check["status"] == "failed"]
    warnings = [check for check in checks if check["status"] == "warning"]
    lines = [
        "# State Machine Verification",
        "",
        f"run_id: `{run_id}`",
        f"passed: `{str(passed).lower()}`",
        f"checks: `{len(checks)}`",
        f"failed: `{len(failed)}`",
        f"warnings: `{len(warnings)}`",
        "",
        "## Checks",
        "",
    ]
    for check in checks:
        lines.append(f"- `{check['status']}` `{check['id']}` - {check['detail']}")
    return "\n".join(lines) + "\n"


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n", encoding="utf-8")


def format_path(path: Path, root: Path | None) -> str:
    if root:
        try:
            return str(path.relative_to(root))
        except ValueError:
            pass
    return str(path)
