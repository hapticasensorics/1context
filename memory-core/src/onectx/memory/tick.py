from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem, compile_system_map
from onectx.memory.invariants import (
    build_runtime_invariant_report,
    write_runtime_invariant_report_artifact,
)
from onectx.memory.migrations import MigrationError, run_contract_migrations
from onectx.memory.source_freshness import evaluate_source_freshness
from onectx.state_machines.runtime import (
    StateMachineRuntimeError,
    persist_scope_state,
    record_transition_execution,
    select_transition,
)
from onectx.storage import LakeStore, stable_id, utc_now
from onectx.storage.hour_events import normalize_source


FRESHNESS_CHECK_MODES = {"auto", "always", "skip"}
READER_SURFACE_CONTRACT = {
    "machine": "memory_system_fabric",
    "scope": "cycle",
    "source_state": "routing_wiki",
    "event": "wiki.agent_layer.closed",
    "target_state": "building_reader_surface",
}


class MemoryTickError(RuntimeError):
    """Raised when a concrete memory tick cannot be executed."""


@dataclass(frozen=True)
class RouteTableSummary:
    manifests: tuple[dict[str, Any], ...] = ()
    routes: tuple[dict[str, Any], ...] = ()


@dataclass(frozen=True)
class MemoryTickResult:
    cycle_id: str
    path: Path
    mode: str
    status: str
    dry_run: bool
    render_count: int
    route_count: int
    manifest_count: int
    artifact_id: str
    content_hash: str
    evidence_ids: tuple[str, ...]
    event_id: str

    def to_payload(self) -> dict[str, Any]:
        return {
            "cycle_id": self.cycle_id,
            "path": str(self.path),
            "mode": self.mode,
            "status": self.status,
            "dry_run": self.dry_run,
            "render_count": self.render_count,
            "route_count": self.route_count,
            "manifest_count": self.manifest_count,
            "artifact_id": self.artifact_id,
            "content_hash": self.content_hash,
            "evidence_ids": list(self.evidence_ids),
            "event_id": self.event_id,
            "files": {
                "cycle": str(self.path / "cycle.json"),
            },
        }


@dataclass(frozen=True)
class MemoryCycleSummary:
    cycle_id: str
    path: Path
    status: str
    mode: str
    dry_run: bool
    created_at: str
    render_count: int
    manifest_count: int
    route_count: int

    def to_payload(self) -> dict[str, Any]:
        return {
            "cycle_id": self.cycle_id,
            "path": str(self.path),
            "status": self.status,
            "mode": self.mode,
            "dry_run": self.dry_run,
            "created_at": self.created_at,
            "render_count": self.render_count,
            "manifest_count": self.manifest_count,
            "route_count": self.route_count,
        }


@dataclass(frozen=True)
class MemoryCycleValidation:
    cycle_id: str
    path: Path
    passed: bool
    checks: tuple[dict[str, Any], ...]
    artifact_id: str = ""
    event_id: str = ""

    def to_payload(self) -> dict[str, Any]:
        return {
            "cycle_id": self.cycle_id,
            "path": str(self.path),
            "passed": self.passed,
            "artifact_id": self.artifact_id,
            "event_id": self.event_id,
            "checks": list(self.checks),
        }


def run_memory_tick(
    system: MemorySystem,
    *,
    wiki_only: bool,
    sources: tuple[str, ...] = ("codex", "claude-code"),
    max_source_age_hours: int | None = None,
    require_fresh: bool = False,
    freshness_check: str = "auto",
    execute_render: bool = False,
    render_family_ids: tuple[str, ...] = (),
    include_talk: bool = True,
    record_evidence: bool = True,
    retry_budget: int = 0,
    execute_migrations: bool = False,
    cycle_id: str = "",
) -> MemoryTickResult:
    if not wiki_only:
        raise MemoryTickError("only --wiki-only memory ticks are implemented")

    store = LakeStore(system.storage_dir)
    store.ensure()
    normalized_sources = tuple(normalize_source(source) for source in sources if str(source).strip())
    if freshness_check not in FRESHNESS_CHECK_MODES:
        raise MemoryTickError(f"freshness_check must be one of {sorted(FRESHNESS_CHECK_MODES)}")
    cycle = cycle_id or stable_id("cycle", "wiki-only", utc_now())
    out_dir = system.runtime_dir / "cycles" / cycle
    out_dir.mkdir(parents=True, exist_ok=True)

    steps: list[dict[str, Any]] = []
    migration_payload: dict[str, Any] = {}
    freshness: dict[str, Any] = {}
    migration_failures: list[dict[str, Any]] = []
    render_failures: list[dict[str, Any]] = []
    should_check_freshness = freshness_check == "always"
    max_age_hours = int(
        max_source_age_hours
        if max_source_age_hours is not None
        else system.runtime_policy.get("max_importer_staleness_hours", 24)
    )

    if should_check_freshness:
        freshness = evaluate_source_freshness(
            store,
            required_sources=normalized_sources,
            max_age_hours=max_age_hours,
        )
        preflight = {
            "source_freshness": {
                "status": "passed" if freshness.get("passed") else "failed",
                "mode": freshness_check,
                "required": bool(require_fresh),
                "reason": "checked source importer freshness",
                "freshness": freshness,
            }
        }
    else:
        reason = "freshness_check=skip" if freshness_check == "skip" else "no source-derived route planning requested"
        preflight = {
            "source_freshness": {
                "status": "skipped",
                "mode": freshness_check,
                "required": bool(require_fresh),
                "reason": reason,
                "freshness": {},
            }
        }

    if execute_migrations:
        try:
            migration_result = run_contract_migrations(system, run_id=cycle)
        except MigrationError as exc:
            migration_failures.append(
                {
                    "step": "contract_migrations",
                    "error": str(exc),
                    "retryable": False,
                }
            )
            steps.append({"id": "contract_migrations", "status": "failed", "reason": str(exc)})
        else:
            migration_payload = migration_result.to_payload()
            step_status = "passed" if migration_result.status == "passed" else "failed"
            steps.append(
                {
                    "id": "contract_migrations",
                    "status": step_status,
                    "applied_count": migration_result.applied_count,
                    "already_current_count": migration_result.already_current_count,
                    "failed_count": migration_result.failed_count,
                    "artifact_id": migration_result.artifact_id,
                    "evidence_id": migration_result.evidence_id,
                }
            )
            if migration_result.status != "passed":
                migration_failures.append(
                    {
                        "step": "contract_migrations",
                        "error": "contract migration verification failed",
                        "retryable": False,
                    }
                )
    steps.append(
        {
            "id": "wiki_interface",
            "status": "ready",
            "reason": "Wiki semantics are delegated to the portable Rust core; Python keeps transitional authoring receipts and Swift wiki.refresh requests",
        }
    )

    freshness_failed = preflight["source_freshness"]["status"] == "failed"
    freshness_required = bool(require_fresh and should_check_freshness)
    blocked = bool(freshness_required and freshness_failed)
    render_payloads: list[dict[str, Any]] = []
    render_evidence_payloads: list[dict[str, Any]] = []
    if blocked:
        steps.append({"id": "wiki_render", "status": "blocked", "reason": "source import freshness failed"})
    elif execute_render:
        render_request = {
            "schema_version": 1,
            "kind": "wiki.render_request",
            "method": "wiki.refresh",
            "transport": "daemon-jsonrpc",
            "socket": "app-support://run/1context.sock",
            "requested_at": utc_now(),
            "render_family_ids": list(render_family_ids),
            "include_talk": include_talk,
            "record_evidence": record_evidence,
        }
        render_request_path = out_dir / "wiki-render-request.json"
        render_request_path.write_text(json.dumps(render_request, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        render_payloads.append(
            {
                **render_request,
                "path": format_path(render_request_path, system.root),
            }
        )
        steps.append(
            {
                "id": "wiki_render",
                "status": "requested",
                "method": "wiki.refresh",
                "path": format_path(render_request_path, system.root),
            }
        )
    else:
        steps.append({"id": "wiki_render", "status": "skipped", "reason": "execute_render=false"})

    route_table = RouteTableSummary()
    retryable = bool(
        retry_budget > 0
        and any(
            bool(item.get("retryable"))
            for item in [*migration_failures, *render_failures]
        )
    )
    if blocked:
        status = "blocked"
    elif retryable:
        status = "retryable"
    elif migration_failures or render_failures:
        status = "failed"
    else:
        status = "completed"
    dry_run = (
        not execute_render
        or blocked
        or bool(migration_failures)
        or bool(render_failures)
    )
    recovery = {
        "status": status,
        "retry_budget": max(0, int(retry_budget)),
        "retryable": retryable,
        "failure_count": (
            len(migration_failures)
            + len(render_failures)
        ),
        "failures": [
            *migration_failures,
            *render_failures,
        ],
        "next_action": recovery_next_action(status, retry_budget),
    }
    ir_contract = reader_surface_ir_contract(system)
    state_machine_execution = build_state_machine_execution(
        system,
        cycle_id=cycle,
        status=status,
        dry_run=dry_run,
        execute_render=execute_render,
        render_count=len(render_payloads),
        manifest_count=len(route_table.manifests),
        route_count=len(route_table.routes),
    )
    invariant_report = build_runtime_invariant_report(
        run_id=cycle,
        mode="wiki_only",
        status=status,
        dry_run=dry_run,
        preflight=preflight,
        steps=steps,
        render_count=len(render_payloads),
        manifest_count=len(route_table.manifests),
        route_count=len(route_table.routes),
        execute_render=execute_render,
    )
    invariant_summary = invariant_report.get("summary") if isinstance(invariant_report.get("summary"), dict) else {}
    if not invariant_summary.get("passed") and status == "completed":
        status = "blocked"
        dry_run = True
        recovery = {
            **recovery,
            "status": status,
            "retryable": False,
            "failure_count": max(1, int(recovery.get("failure_count") or 0)),
            "next_action": "operator_review",
            "failures": [
                *list(recovery.get("failures") or []),
                {
                    "step": "runtime_invariants",
                    "error": "runtime invariant report found silent no-ops",
                    "retryable": False,
                },
            ],
        }
        steps.append(
            {
                "id": "runtime_invariants",
                "status": "blocked",
                "reason": "runtime invariant report found silent no-ops",
            }
        )
        invariant_report = build_runtime_invariant_report(
            run_id=cycle,
            mode="wiki_only",
            status=status,
            dry_run=dry_run,
            preflight=preflight,
            steps=steps,
            render_count=len(render_payloads),
            manifest_count=len(route_table.manifests),
            route_count=len(route_table.routes),
            execute_render=execute_render,
        )
    invariant_artifact = write_runtime_invariant_report_artifact(
        system,
        invariant_report,
        run_id=cycle,
        path=out_dir / "runtime-invariants.json",
        checker="memory.tick",
    )
    payload = {
        "cycle_id": cycle,
        "kind": "memory_tick",
        "mode": "wiki_only",
        "state_machine": "memory_system_fabric",
        "scope": "cycle",
        "status": status,
        "dry_run": dry_run,
        "created_at": utc_now(),
        "runtime_policy": {
            "max_concurrent_agents": system.runtime_policy.get("max_concurrent_agents"),
            "max_concurrent_renderers": system.runtime_policy.get("max_concurrent_renderers"),
            "max_importer_staleness_hours": system.runtime_policy.get("max_importer_staleness_hours"),
        },
        "inputs": {
            "sources": list(normalized_sources),
            "freshness_check": freshness_check,
            "execute_migrations": execute_migrations,
            "execute_render": execute_render,
            "render_family_ids": list(render_family_ids),
            "include_talk": include_talk,
            "record_evidence": record_evidence,
            "require_fresh": require_fresh,
            "retry_budget": max(0, int(retry_budget)),
        },
        "steps": steps,
        "preflight": preflight,
        "recovery": recovery,
        "freshness": freshness,
        "contract_migrations": migration_payload,
        "runtime_invariant_report": {
            **invariant_artifact.to_payload(),
            "summary": invariant_report.get("summary", {}),
        },
        "renders": render_payloads,
        "render_evidence": render_evidence_payloads,
        "route_table": {
            "manifest_count": len(route_table.manifests),
            "route_count": len(route_table.routes),
        },
        "ir_contract": ir_contract,
        "state_machine_execution": state_machine_execution,
        "dsl_contract": {
            "from_ir_contract": ir_contract["transition"],
            "reader_surface_evidence": [
                "wiki.refresh.requested",
                "wiki.render.queued",
                "wiki.render.succeeded",
            "wiki.manifest.recorded",
            ],
            "memory_evidence": [
                "wiki_interface.authoring.available",
                "source_import.fresh",
                "contract_migrations.closed",
                "runtime_invariants.passed",
            ],
        },
    }
    text = stable_json(payload) + "\n"
    cycle_path = out_dir / "cycle.json"
    cycle_path.write_text(text, encoding="utf-8")
    content_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()
    artifact_id = stable_id("artifact", "memory_cycle_tick", cycle)
    artifact = store.artifact_row(
        "memory_cycle_tick",
        artifact_id=artifact_id,
        uri=f"file://{cycle_path}",
        path=str(cycle_path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=len(text.encode("utf-8")),
        source="memory.tick",
        state=status,
        text=f"memory tick {cycle} ({status})",
        metadata={
            "cycle_id": cycle,
            "mode": "wiki_only",
            "dry_run": dry_run,
            "render_count": len(render_payloads),
            "route_count": len(route_table.routes),
            "manifest_count": len(route_table.manifests),
            "source_freshness_status": preflight["source_freshness"]["status"],
            "migration_status": migration_payload.get("status", "skipped"),
            "retry_budget": max(0, int(retry_budget)),
            "failure_count": len(migration_failures) + len(render_failures),
            "ir_contract": ir_contract["transition"],
        },
    )
    store.replace_rows("artifacts", "artifact_id", [artifact])

    evidence_rows = [
        store.append_evidence(
            "memory_cycle.artifact_written",
            artifact_id=artifact_id,
            status="passed",
            checker="memory.tick",
            text="memory tick cycle artifact written",
            checks=["cycle.json exists", "cycle payload records state_machine and scope"],
            payload={"cycle_id": cycle, "path": str(cycle_path)},
        )
    ]
    evidence_rows.append(
        {
            "evidence_id": invariant_artifact.evidence_id,
            "artifact_id": invariant_artifact.artifact_id,
            "check_id": "runtime_invariants.passed",
            "status": "passed" if invariant_artifact.passed else "failed",
        }
    )
    if preflight["source_freshness"]["status"] != "skipped":
        evidence_rows.append(
            store.append_evidence(
                "source_import.fresh",
                artifact_id=artifact_id,
                status="passed" if preflight["source_freshness"]["status"] == "passed" else "failed",
                checker="memory.tick",
                text="source importer freshness checked during memory tick preflight",
                checks=["required source latest_ts within max_age_hours"],
                payload=preflight["source_freshness"],
            )
        )
    if migration_payload:
        evidence_rows.append(
            store.append_evidence(
                "contract_migrations.closed",
                artifact_id=artifact_id,
                status="passed" if migration_payload.get("status") == "passed" else "failed",
                checker="memory.tick",
                text="contract migration receipts were closed during memory tick",
                checks=["migration summary artifact written", "migration receipts written"],
                payload={
                    "cycle_id": cycle,
                    "migration_artifact_id": migration_payload.get("artifact_id", ""),
                    "migration_evidence_id": migration_payload.get("evidence_id", ""),
                    "status": migration_payload.get("status", ""),
                    "applied_count": migration_payload.get("applied_count", 0),
                    "already_current_count": migration_payload.get("already_current_count", 0),
                    "failed_count": migration_payload.get("failed_count", 0),
                },
            )
        )
    if execute_render and status == "completed":
        evidence_rows.append(
            store.append_evidence(
                "wiki.refresh.requested",
                artifact_id=artifact_id,
                status="passed" if render_payloads else "failed",
                checker="memory.tick",
                text="wiki refresh request handed to Swift render queue",
                checks=[
                    "memory-core wrote a wiki.refresh request",
                    "request names the daemon JSON-RPC method",
                    "renderer internals remain outside memory-core",
                ],
                payload={
                    "cycle_id": cycle,
                    "request_count": len(render_payloads),
                    "requests": render_payloads,
                },
            )
        )
    elif status in {"failed", "retryable"}:
        evidence_rows.append(
            store.append_evidence(
                "memory_tick.recovery_recorded",
                artifact_id=artifact_id,
                status="passed",
                checker="memory.tick",
                text="memory tick recorded failure/retry recovery state",
                checks=["cycle artifact written", "recovery.failure_count > 0", "terminal event will be recorded"],
                payload={"cycle_id": cycle, "recovery": recovery},
            )
        )
    else:
        dry_run_checks = ["cycle artifact written"]
        dry_run_text = "wiki-only memory tick planned without requesting wiki refresh"
        dry_run_checks.append("no wiki render requested")
        evidence_rows.append(
            store.append_evidence(
                "memory_tick.dry_run_planned",
                artifact_id=artifact_id,
                status="passed" if not blocked else "failed",
                checker="memory.tick",
                text=dry_run_text,
                checks=dry_run_checks,
                payload={"cycle_id": cycle, "blocked": blocked},
            )
        )

    event_name = {
        "completed": "memory.tick.completed",
        "blocked": "memory.tick.blocked",
        "retryable": "memory.tick.retryable",
        "failed": "memory.tick.failed",
    }.get(status, "memory.tick.failed")
    event = store.append_event(
        event_name,
        source="memory.tick",
        kind="state_machine",
        actor="memory_tick",
        subject=cycle,
        state_machine="memory_system_fabric",
        scope="cycle",
        artifact_id=artifact_id,
        evidence_id=evidence_rows[0]["evidence_id"],
        text=f"Memory wiki-only tick {status}.",
        payload={
            "cycle_id": cycle,
            "status": status,
            "dry_run": dry_run,
            "render_count": len(render_payloads),
            "route_count": len(route_table.routes),
            "manifest_count": len(route_table.manifests),
            "retryable": retryable,
            "failure_count": len(migration_failures) + len(render_failures),
            "migration_status": migration_payload.get("status", "skipped"),
        },
    )

    return MemoryTickResult(
        cycle_id=cycle,
        path=out_dir,
        mode="wiki_only",
        status=status,
        dry_run=dry_run,
        render_count=len(render_payloads),
        route_count=len(route_table.routes),
        manifest_count=len(route_table.manifests),
        artifact_id=artifact_id,
        content_hash=content_hash,
        evidence_ids=tuple(row["evidence_id"] for row in evidence_rows),
        event_id=event["event_id"],
    )


def list_memory_cycles(system: MemorySystem, *, limit: int = 20) -> tuple[MemoryCycleSummary, ...]:
    cycle_root = system.runtime_dir / "cycles"
    if not cycle_root.is_dir():
        return ()
    paths = sorted(
        cycle_root.glob("*/cycle.json"),
        key=lambda path: path.stat().st_mtime,
        reverse=True,
    )
    summaries: list[MemoryCycleSummary] = []
    for path in paths[: max(0, limit)]:
        try:
            payload = load_cycle_payload(path)
        except MemoryTickError:
            continue
        summaries.append(cycle_summary_from_payload(path.parent, payload))
    return tuple(summaries)


def load_memory_cycle(system: MemorySystem, cycle_id: str) -> dict[str, Any]:
    cycle_id = cycle_id.strip()
    if not cycle_id:
        raise MemoryTickError("cycle id is required")
    path = system.runtime_dir / "cycles" / cycle_id / "cycle.json"
    return load_cycle_payload(path)


def validate_memory_cycle(system: MemorySystem, cycle_id: str) -> MemoryCycleValidation:
    payload = load_memory_cycle(system, cycle_id)
    path = system.runtime_dir / "cycles" / cycle_id / "cycle.json"
    checks: list[dict[str, Any]] = []
    add_check(checks, "cycle_json.exists", path.is_file(), str(path))
    actual_hash = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else ""

    store = LakeStore(system.storage_dir)
    store.ensure()
    artifacts = [
        row
        for row in store.rows("artifacts", limit=0)
        if str(row.get("path") or "") == str(path)
        and str(row.get("kind") or "") == "memory_cycle_tick"
    ]
    artifact = next((row for row in artifacts if row.get("content_hash") == actual_hash), None)
    if artifact is None and artifacts:
        artifact = artifacts[-1]
    artifact_id = str((artifact or {}).get("artifact_id") or "")
    add_check(checks, "artifact.row_exists", bool(artifact), artifact_id or "no artifact row matched cycle path")
    add_check(
        checks,
        "artifact.hash_matches_file",
        bool(artifact and artifact.get("content_hash") == actual_hash),
        f"file={actual_hash} artifact={(artifact or {}).get('content_hash', '')}",
    )

    evidence_rows = [
        row
        for row in store.rows("evidence", limit=0)
        if artifact_id and str(row.get("artifact_id") or "") == artifact_id
    ]
    evidence_ids = {str(row.get("check_id") or "") for row in evidence_rows}
    evidence_statuses = {str(row.get("check_id") or ""): str(row.get("status") or "") for row in evidence_rows}
    add_check(
        checks,
        "evidence.memory_cycle_artifact_written",
        "memory_cycle.artifact_written" in evidence_ids,
        ",".join(sorted(evidence_ids)) or "no evidence",
    )
    invariant_payload = (
        payload.get("runtime_invariant_report")
        if isinstance(payload.get("runtime_invariant_report"), dict)
        else {}
    )
    invariant_artifact_id = str(invariant_payload.get("artifact_id") or "")
    invariant_evidence_id = str(invariant_payload.get("evidence_id") or "")
    invariant_path = Path(str(invariant_payload.get("path") or ""))
    invariant_rows = [
        row
        for row in store.rows("evidence", limit=0)
        if invariant_artifact_id and str(row.get("artifact_id") or "") == invariant_artifact_id
    ]
    add_check(
        checks,
        "runtime_invariant_report.exists",
        bool(invariant_path.is_file()),
        str(invariant_path) if str(invariant_path) else "missing",
    )
    add_check(
        checks,
        "evidence.runtime_invariants_passed",
        any(
            str(row.get("evidence_id") or "") == invariant_evidence_id
            and str(row.get("check_id") or "") == "runtime_invariants.passed"
            and str(row.get("status") or "") == "passed"
            for row in invariant_rows
        ),
        invariant_evidence_id or "missing",
    )
    preflight = payload.get("preflight") if isinstance(payload.get("preflight"), dict) else {}
    source_freshness = (
        preflight.get("source_freshness")
        if isinstance(preflight.get("source_freshness"), dict)
        else {}
    )
    freshness_status = str(source_freshness.get("status") or "")
    add_check(
        checks,
        "preflight.source_freshness.present",
        freshness_status in {"passed", "failed", "skipped"},
        freshness_status or "missing",
    )
    if freshness_status != "skipped":
        add_check(
            checks,
            "evidence.source_import_fresh",
            "source_import.fresh" in evidence_ids,
            ",".join(sorted(evidence_ids)) or "no evidence",
        )
    if payload.get("inputs", {}).get("execute_migrations"):
        migrations_payload = (
            payload.get("contract_migrations")
            if isinstance(payload.get("contract_migrations"), dict)
            else {}
        )
        add_check(
            checks,
            "evidence.contract_migrations_closed",
            migrations_payload.get("status") == "passed" and "contract_migrations.closed" in evidence_ids,
            ",".join(sorted(evidence_ids)) or "no evidence",
        )
    if payload.get("inputs", {}).get("execute_render") and payload.get("status") == "completed":
        add_check(
            checks,
            "evidence.wiki_refresh_requested",
            "wiki.refresh.requested" in evidence_ids,
            ",".join(sorted(evidence_ids)) or "no evidence",
        )
        ir_contract = payload.get("ir_contract") if isinstance(payload.get("ir_contract"), dict) else {}
        expected_evidence = tuple(str(item) for item in ir_contract.get("expects", []) if str(item).strip())
        add_check(
            checks,
            "ir_contract.expected_evidence_satisfied",
            bool(expected_evidence)
            and all(evidence_statuses.get(evidence_name) == "passed" for evidence_name in expected_evidence),
            ",".join(
                f"{evidence_name}:{evidence_statuses.get(evidence_name, 'missing')}"
                for evidence_name in expected_evidence
            )
            or "no expected evidence declared",
        )

    event = next(
        (
            row
            for row in store.rows("events", limit=0)
            if str(row.get("subject") or "") == cycle_id
            and str(row.get("event") or "")
            in {"memory.tick.completed", "memory.tick.blocked", "memory.tick.retryable", "memory.tick.failed"}
        ),
        None,
    )
    event_id = str((event or {}).get("event_id") or "")
    add_check(checks, "event.cycle_terminal", bool(event), event_id or "no terminal cycle event")
    if payload.get("status") in {"failed", "retryable"}:
        recovery = payload.get("recovery") if isinstance(payload.get("recovery"), dict) else {}
        add_check(
            checks,
            "recovery.recorded",
            bool(recovery.get("failure_count")),
            f"failure_count={recovery.get('failure_count', 0)} next_action={recovery.get('next_action', '')}",
        )
        add_check(
            checks,
            "evidence.recovery_recorded",
            "memory_tick.recovery_recorded" in evidence_ids,
            ",".join(sorted(evidence_ids)) or "no evidence",
        )
    add_check(
        checks,
        "dsl_contract.present",
        bool(payload.get("dsl_contract", {}).get("reader_surface_evidence")),
        "reader_surface_evidence",
    )
    ir_contract = payload.get("ir_contract") if isinstance(payload.get("ir_contract"), dict) else {}
    add_check(
        checks,
        "ir_contract.present",
        bool(ir_contract),
        str(ir_contract.get("transition") or "missing"),
    )
    add_check(
        checks,
        "ir_contract.reader_surface_transition",
        ir_contract_matches_reader_surface_transition(ir_contract),
        str(ir_contract.get("transition") or "missing"),
    )
    execution = payload.get("state_machine_execution") if isinstance(payload.get("state_machine_execution"), dict) else {}
    add_check(
        checks,
        "state_machine_execution.present",
        execution.get("machine") == "memory_system_fabric" and execution.get("scope") == "cycle",
        str(execution.get("terminal_state") or "missing"),
    )
    scope_state = (
        execution.get("scope_state")
        if isinstance(execution.get("scope_state"), dict)
        else {}
    )
    scope_state_path = Path(str(scope_state.get("path") or ""))
    add_check(
        checks,
        "state_machine_scope_state.persisted",
        bool(scope_state_path.is_file() and scope_state.get("state") == execution.get("terminal_state")),
        str(scope_state_path) if str(scope_state_path) else "missing",
    )
    if payload.get("inputs", {}).get("execute_render") and payload.get("status") == "completed":
        execution_transitions = (
            execution.get("transitions")
            if isinstance(execution.get("transitions"), list)
            else []
        )
        transition_ids = {
            str(item.get("transition") or "")
            for item in execution_transitions
            if isinstance(item, dict)
        }
        add_check(
            checks,
            "state_machine_execution.reader_surface_transition",
            READER_SURFACE_CONTRACT["event"] in {
                str(item.get("event") or "")
                for item in execution_transitions
                if isinstance(item, dict)
            },
            ",".join(sorted(transition_ids)) or "no executed transitions",
        )
        add_check(
            checks,
            "state_machine_execution.terminal_complete",
            execution.get("terminal_state") == "complete"
            and "memory_system_fabric.cycle.building_reader_surface--wiki.refresh.requested--complete"
            in transition_ids,
            str(execution.get("terminal_state") or "missing"),
        )
    passed = all(bool(check["passed"]) for check in checks)
    return MemoryCycleValidation(
        cycle_id=cycle_id,
        path=path.parent,
        passed=passed,
        checks=tuple(checks),
        artifact_id=artifact_id,
        event_id=event_id,
    )


def load_cycle_payload(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise MemoryTickError(f"cycle artifact not found: {path}")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise MemoryTickError(f"invalid cycle JSON {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise MemoryTickError(f"cycle JSON must be an object: {path}")
    return payload


def cycle_summary_from_payload(path: Path, payload: dict[str, Any]) -> MemoryCycleSummary:
    route_table = payload.get("route_table") if isinstance(payload.get("route_table"), dict) else {}
    return MemoryCycleSummary(
        cycle_id=str(payload.get("cycle_id") or path.name),
        path=path,
        status=str(payload.get("status") or ""),
        mode=str(payload.get("mode") or ""),
        dry_run=bool(payload.get("dry_run")),
        created_at=str(payload.get("created_at") or ""),
        render_count=len(payload.get("renders") or []),
        manifest_count=int(route_table.get("manifest_count") or 0),
        route_count=int(route_table.get("route_count") or 0),
    )


def add_check(checks: list[dict[str, Any]], check_id: str, passed: bool, detail: str) -> None:
    checks.append({"id": check_id, "passed": bool(passed), "detail": detail})


def reader_surface_ir_contract(system: MemorySystem) -> dict[str, Any]:
    try:
        plan = select_transition(
            system,
            machine_id=READER_SURFACE_CONTRACT["machine"],
            scope=READER_SURFACE_CONTRACT["scope"],
            source_state=READER_SURFACE_CONTRACT["source_state"],
            event_name=READER_SURFACE_CONTRACT["event"],
            target_state=READER_SURFACE_CONTRACT["target_state"],
        )
    except StateMachineRuntimeError as exc:
        raise MemoryTickError(str(exc)) from exc
    payload = plan.to_payload()
    return {
        "machine": payload["machine"],
        "scope": payload["scope"],
        "transition": payload["transition"],
        "transition_index": payload["transition_index"],
        "event": payload["event"],
        "source": payload["source"],
        "target": payload["target"],
        "steps": payload["steps"],
        "expects": payload["expects"],
        "emits": payload["emits"],
    }


def build_state_machine_execution(
    system: MemorySystem,
    *,
    cycle_id: str,
    status: str,
    dry_run: bool,
    execute_render: bool,
    render_count: int,
    manifest_count: int,
    route_count: int,
) -> dict[str, Any]:
    executions: list[Any] = []
    terminal_state = terminal_state_for_status(status)
    note = ""
    initial_state = READER_SURFACE_CONTRACT["source_state"]
    should_trace_reader = execute_render and status in {"completed", "failed", "retryable"}
    if should_trace_reader:
        reader_ready = bool(render_count)
        produced_evidence = ("wiki.refresh.requested",) if reader_ready else ()
        try:
            reader_transition = record_transition_execution(
                system,
                machine_id=READER_SURFACE_CONTRACT["machine"],
                scope=READER_SURFACE_CONTRACT["scope"],
                source_state=READER_SURFACE_CONTRACT["source_state"],
                event_name=READER_SURFACE_CONTRACT["event"],
                target_state=READER_SURFACE_CONTRACT["target_state"],
                status="passed" if reader_ready else "failed",
                produced_evidence=produced_evidence,
                completed_steps=("write_wiki_refresh_request", "notify_wiki_render_queue"),
                emitted_events=("wiki.refresh.requested",) if reader_ready else (),
            )
            executions.append(reader_transition)
            terminal_state = (
                READER_SURFACE_CONTRACT["target_state"]
                if reader_ready
                else terminal_state_for_status(status)
            )
            if reader_ready:
                complete_transition = record_transition_execution(
                    system,
                    machine_id=READER_SURFACE_CONTRACT["machine"],
                    scope=READER_SURFACE_CONTRACT["scope"],
                    source_state="building_reader_surface",
                    event_name="wiki.refresh.requested",
                    target_state="complete",
                    status="passed",
                    completed_steps=("append_cycle_summary_event",),
                    emitted_events=("memory.cycle.complete",),
                )
                executions.append(complete_transition)
                terminal_state = "complete"
        except StateMachineRuntimeError as exc:
            note = str(exc)
            terminal_state = "failed"
    elif status == "completed" and dry_run:
        terminal_state = READER_SURFACE_CONTRACT["source_state"]
        note = "dry-run tick stopped before the reader-surface transition executed"

    scope_state = persist_scope_state(
        system,
        machine_id=READER_SURFACE_CONTRACT["machine"],
        scope=READER_SURFACE_CONTRACT["scope"],
        key=cycle_id,
        initial_state=initial_state,
        terminal_state=terminal_state,
        transitions=tuple(executions),
        status=status,
        dry_run=dry_run,
        note=note,
    )
    transitions = [execution.to_payload() for execution in executions]
    return {
        "machine": READER_SURFACE_CONTRACT["machine"],
        "scope": READER_SURFACE_CONTRACT["scope"],
        "cycle_id": cycle_id,
        "status": status,
        "dry_run": dry_run,
        "initial_state": initial_state,
        "terminal_state": terminal_state,
        "transition_count": len(transitions),
        "transitions": transitions,
        "scope_state": {
            "path": scope_state.get("path", ""),
            "state": scope_state.get("state", ""),
            "previous_state": scope_state.get("previous_state", ""),
            "updated_at": scope_state.get("updated_at", ""),
            "history_count": len(scope_state.get("history", []))
            if isinstance(scope_state.get("history"), list)
            else 0,
        },
        "note": note,
    }


def terminal_state_for_status(status: str) -> str:
    return {
        "completed": "complete",
        "blocked": "blocked",
        "retryable": "retryable",
        "failed": "failed",
    }.get(status, "failed")


def collect_contract_actions(actions: Any) -> dict[str, list[str]]:
    collected: dict[str, list[str]] = {"steps": [], "expects": [], "emits": []}
    if not isinstance(actions, list):
        return collected
    for action in actions:
        collect_contract_action(action, collected)
    return collected


def collect_contract_action(action: Any, collected: dict[str, list[str]]) -> None:
    if not isinstance(action, dict):
        return
    kind = str(action.get("kind") or "")
    if kind == "step":
        append_unique(collected["steps"], str(action.get("name") or ""))
    elif kind == "expect":
        append_unique(collected["expects"], str(action.get("evidence") or ""))
    elif kind == "emit":
        append_unique(collected["emits"], str(action.get("event") or ""))
    for child in action.get("actions", []) if isinstance(action.get("actions"), list) else []:
        collect_contract_action(child, collected)


def append_unique(items: list[str], value: str) -> None:
    cleaned = value.strip()
    if cleaned and cleaned not in items:
        items.append(cleaned)


def ir_contract_matches_reader_surface_transition(contract: dict[str, Any]) -> bool:
    if not contract:
        return False
    source = contract.get("source") if isinstance(contract.get("source"), dict) else {}
    target_payload = contract.get("target") if isinstance(contract.get("target"), dict) else {}
    return (
        contract.get("machine") == READER_SURFACE_CONTRACT["machine"]
        and contract.get("scope") == READER_SURFACE_CONTRACT["scope"]
        and contract.get("event") == READER_SURFACE_CONTRACT["event"]
        and source.get("state") == READER_SURFACE_CONTRACT["source_state"]
        and target_payload.get("state") == READER_SURFACE_CONTRACT["target_state"]
        and "write_wiki_refresh_request" in set(contract.get("steps") or [])
        and "notify_wiki_render_queue" in set(contract.get("steps") or [])
        and "wiki.refresh.requested" in set(contract.get("expects") or [])
    )


def recovery_next_action(status: str, retry_budget: int) -> str:
    if status == "retryable" and retry_budget > 0:
        return "retry_on_next_tick"
    if status == "failed":
        return "operator_review"
    if status == "blocked":
        return "wait_for_fresh_inputs_or_operator_override"
    return "none"


def stable_json(payload: dict[str, Any]) -> str:
    return json.dumps(payload, indent=2, sort_keys=True, default=str)


def format_path(path: Path, root: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)
