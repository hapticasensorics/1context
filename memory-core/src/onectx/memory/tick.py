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
from onectx.memory.wiki import (
    WikiError as MemoryWikiError,
    evaluate_wiki_route_source_freshness,
    plan_wiki_roles,
    preview_wiki_route_execution,
    write_wiki_route_execution_artifact,
)
from onectx.memory.wiki_executor import execute_wiki_route_hires, promote_wiki_route_outputs
from onectx.state_machines.runtime import (
    StateMachineRuntimeError,
    persist_scope_state,
    record_transition_execution,
    select_transition,
)
from onectx.storage import LakeStore, stable_id, utc_now
from onectx.storage.hour_events import normalize_source
from onectx.wiki.evidence import record_render_evidence
from onectx.wiki.families import WikiError as WikiEngineError
from onectx.wiki.families import discover_families
from onectx.wiki.render import render_family
from onectx.wiki.routes import load_route_table
from onectx.wiki.site import write_site_files


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
class MemoryTickResult:
    cycle_id: str
    path: Path
    mode: str
    status: str
    dry_run: bool
    planned_hire_count: int
    non_hire_count: int
    route_hire_count: int
    route_hire_error_count: int
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
            "planned_hire_count": self.planned_hire_count,
            "non_hire_count": self.non_hire_count,
            "route_hire_count": self.route_hire_count,
            "route_hire_error_count": self.route_hire_error_count,
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
    planned_hire_count: int
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
            "planned_hire_count": self.planned_hire_count,
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
    workspace: Path | None = None,
    concept_dir: Path | None = None,
    audience: str = "private",
    sources: tuple[str, ...] = ("codex", "claude-code"),
    max_source_age_hours: int | None = None,
    require_fresh: bool = False,
    freshness_check: str = "auto",
    execute_render: bool = False,
    execute_route_hires: bool = False,
    route_hire_limit: int = 0,
    route_hire_run_harness: bool = False,
    promote_route_outputs: bool = False,
    route_promotion_operator_approval: str = "",
    render_family_ids: tuple[str, ...] = (),
    include_talk: bool = True,
    record_evidence: bool = True,
    retry_budget: int = 0,
    execute_migrations: bool = False,
    cycle_id: str = "",
) -> MemoryTickResult:
    if not wiki_only:
        raise MemoryTickError("only --wiki-only memory ticks are implemented")
    if (workspace is None) != (concept_dir is None):
        raise MemoryTickError("--workspace and --concept-dir must be supplied together")

    store = LakeStore(system.storage_dir)
    store.ensure()
    normalized_sources = tuple(normalize_source(source) for source in sources if str(source).strip())
    if freshness_check not in FRESHNESS_CHECK_MODES:
        raise MemoryTickError(f"freshness_check must be one of {sorted(FRESHNESS_CHECK_MODES)}")
    cycle = cycle_id or stable_id("cycle", "wiki-only", utc_now())
    out_dir = system.runtime_dir / "cycles" / cycle
    out_dir.mkdir(parents=True, exist_ok=True)

    steps: list[dict[str, Any]] = []
    route_preview_payload: dict[str, Any] = {}
    route_artifact_payload: dict[str, Any] = {}
    route_hire_execution_payload: dict[str, Any] = {}
    route_promotion_payload: dict[str, Any] = {}
    migration_payload: dict[str, Any] = {}
    freshness: dict[str, Any] = {}
    planned_hire_count = 0
    non_hire_count = 0
    migration_failures: list[dict[str, Any]] = []
    render_failures: list[dict[str, Any]] = []
    route_hire_failures: list[dict[str, Any]] = []
    route_promotion_failures: list[dict[str, Any]] = []
    route_hire_execution = None
    source_derived = bool(workspace and concept_dir)
    should_check_freshness = freshness_check == "always" or (freshness_check == "auto" and source_derived)
    max_age_hours = int(
        max_source_age_hours
        if max_source_age_hours is not None
        else system.runtime_policy.get("max_importer_staleness_hours", 24)
    )

    if should_check_freshness:
        freshness = evaluate_wiki_route_source_freshness(
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
    if workspace and concept_dir:
        try:
            plan = plan_wiki_roles(workspace=workspace, concept_dir=concept_dir, audience=audience)
            preview = preview_wiki_route_execution(plan, freshness=freshness)
            route_artifact = write_wiki_route_execution_artifact(system, preview)
        except MemoryWikiError as exc:
            raise MemoryTickError(str(exc)) from exc

        route_preview_payload = preview.to_payload()
        route_artifact_payload = route_artifact.to_payload()
        planned_hire_count = preview.planned_hire_count
        non_hire_count = preview.non_hire_count
        steps.append(
            {
                "id": "wiki_route_dry_run",
                "status": "passed",
                "planned_hire_count": planned_hire_count,
                "non_hire_count": non_hire_count,
                "artifact_id": route_artifact.artifact_id,
            }
        )
    else:
        steps.append(
            {
                "id": "wiki_route_dry_run",
                "status": "skipped",
                "reason": "workspace and concept_dir were not supplied",
            }
        )

    freshness_failed = preflight["source_freshness"]["status"] == "failed"
    freshness_required = bool(require_fresh and should_check_freshness)
    blocked = bool(freshness_required and freshness_failed)
    if execute_route_hires:
        if blocked:
            steps.append(
                {
                    "id": "wiki_route_hires",
                    "status": "blocked",
                    "reason": "source import freshness failed",
                }
            )
        elif not (workspace and concept_dir):
            steps.append(
                {
                    "id": "wiki_route_hires",
                    "status": "skipped",
                    "reason": "workspace and concept_dir were not supplied",
                }
            )
        else:
            try:
                route_hire_execution = execute_wiki_route_hires(
                    system,
                    plan,
                    run_id=cycle,
                    run_harness=route_hire_run_harness,
                    limit=max(0, int(route_hire_limit)),
                )
                route_hire_execution_payload = route_hire_execution.to_payload()
            except Exception as exc:  # noqa: BLE001 - tick records route-hire failures as explicit outcomes.
                route_hire_failures.append(
                    {
                        "step": "wiki_route_hires",
                        "error": str(exc),
                        "retryable": True,
                    }
                )
                steps.append(
                    {
                        "id": "wiki_route_hires",
                        "status": "retryable" if retry_budget > 0 else "failed",
                        "reason": str(exc),
                    }
                )
            else:
                if route_hire_execution.ok:
                    steps.append(
                        {
                            "id": "wiki_route_hires",
                            "status": "passed",
                            "dry_run": route_hire_execution.dry_run,
                            "spec_count": route_hire_execution.spec_count,
                            "completed_count": route_hire_execution.completed_count,
                            "error_count": route_hire_execution.error_count,
                            "validation_failure_count": route_hire_execution.validation_failure_count,
                            "max_concurrent_agents": route_hire_execution.max_concurrent,
                        }
                    )
                    if promote_route_outputs:
                        route_promotion = promote_wiki_route_outputs(
                            system,
                            plan,
                            route_hire_execution,
                            run_id=cycle,
                            operator_approval=route_promotion_operator_approval,
                        )
                        route_promotion_payload = route_promotion.to_payload()
                        if route_promotion.ok:
                            steps.append(
                                {
                                    "id": "wiki_route_promotions",
                                    "status": "passed",
                                    "item_count": route_promotion.item_count,
                                    "promoted_count": route_promotion.promoted_count,
                                    "skipped_count": route_promotion.skipped_count,
                                }
                            )
                        else:
                            failed_items = [item for item in route_promotion.items if not item.ok]
                            reason = (
                                failed_items[0].failures[0]
                                if failed_items and failed_items[0].failures
                                else "route output promotion failed"
                            )
                            route_promotion_failures.append(
                                {
                                    "step": "wiki_route_promotions",
                                    "error": reason,
                                    "retryable": False,
                                }
                            )
                            steps.append(
                                {
                                    "id": "wiki_route_promotions",
                                    "status": "failed",
                                    "reason": reason,
                                    "item_count": route_promotion.item_count,
                                    "promoted_count": route_promotion.promoted_count,
                                    "blocked_count": route_promotion.blocked_count,
                                    "failed_count": route_promotion.failed_count,
                                }
                            )
                else:
                    first_error = route_hire_execution.batch.errors[0] if route_hire_execution.batch.errors else {}
                    reason = str(first_error.get("message") or "route hire output validation failed")
                    route_hire_failures.append(
                        {
                            "step": "wiki_route_hires",
                            "error": reason,
                            "retryable": True,
                        }
                    )
                    steps.append(
                        {
                            "id": "wiki_route_hires",
                            "status": "retryable" if retry_budget > 0 else "failed",
                            "reason": reason,
                            "spec_count": route_hire_execution.spec_count,
                            "completed_count": route_hire_execution.completed_count,
                            "error_count": route_hire_execution.error_count,
                            "validation_failure_count": route_hire_execution.validation_failure_count,
                        }
                    )
    elif promote_route_outputs:
        route_promotion_failures.append(
            {
                "step": "wiki_route_promotions",
                "error": "--promote-route-outputs requires --execute-route-hires",
                "retryable": False,
            }
        )
        steps.append(
            {
                "id": "wiki_route_promotions",
                "status": "failed",
                "reason": "--promote-route-outputs requires --execute-route-hires",
            }
        )
    render_payloads: list[dict[str, Any]] = []
    render_evidence_payloads: list[dict[str, Any]] = []
    if blocked:
        steps.append({"id": "wiki_render", "status": "blocked", "reason": "source import freshness failed"})
    elif execute_render:
        family_ids = render_family_ids or tuple(family.id for family in discover_families(system.root))
        if not family_ids:
            render_failures.append(
                {
                    "step": "wiki_render",
                    "family_id": "",
                    "error": "no wiki families discovered to render",
                    "retryable": False,
                }
            )
        for family_id in family_ids:
            try:
                render_result = render_family(system.root, family_id, include_talk=include_talk)
            except WikiEngineError as exc:
                render_failures.append(
                    {
                        "step": "wiki_render",
                        "family_id": family_id,
                        "error": str(exc),
                        "retryable": True,
                    }
                )
                break
            render_payloads.append(render_result.to_payload(system.root))
            if record_evidence:
                render_evidence_payloads.append(record_render_evidence(system, render_result).to_payload())
        if render_failures:
            can_retry = retry_budget > 0 and any(bool(item.get("retryable")) for item in render_failures)
            steps.append(
                {
                    "id": "wiki_render",
                    "status": "retryable" if can_retry else "failed",
                    "family_count": len(render_payloads),
                    "failure_count": len(render_failures),
                    "reason": render_failures[0]["error"],
                }
            )
        else:
            site_paths = write_site_files(system.root)
            steps.append(
                {
                    "id": "wiki_render",
                    "status": "passed",
                    "family_count": len(family_ids),
                    "site_files": [format_path(path, system.root) for path in site_paths],
                }
            )
    else:
        steps.append({"id": "wiki_render", "status": "skipped", "reason": "execute_render=false"})

    route_table = load_route_table(system.root)
    retryable = bool(
        retry_budget > 0
        and any(
            bool(item.get("retryable"))
            for item in [*migration_failures, *render_failures, *route_hire_failures, *route_promotion_failures]
        )
    )
    if blocked:
        status = "blocked"
    elif retryable:
        status = "retryable"
    elif migration_failures or render_failures or route_hire_failures or route_promotion_failures:
        status = "failed"
    else:
        status = "completed"
    source_promoted = bool(route_promotion_payload.get("promoted_count"))
    dry_run = (
        (not execute_render and not source_promoted)
        or blocked
        or bool(migration_failures)
        or bool(render_failures)
        or bool(route_hire_failures)
        or bool(route_promotion_failures)
    )
    recovery = {
        "status": status,
        "retry_budget": max(0, int(retry_budget)),
        "retryable": retryable,
        "failure_count": (
            len(migration_failures)
            + len(render_failures)
            + len(route_hire_failures)
            + len(route_promotion_failures)
        ),
        "failures": [
            *migration_failures,
            *route_hire_failures,
            *route_promotion_failures,
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
        route_plan_ready=bool(route_preview_payload) and not blocked and not migration_failures,
        route_agent_layer_closed=not route_hire_failures and not route_promotion_failures,
    )
    invariant_report = build_runtime_invariant_report(
        run_id=cycle,
        mode="wiki_only",
        status=status,
        dry_run=dry_run,
        preflight=preflight,
        steps=steps,
        route_preview=route_preview_payload,
        route_artifact=route_artifact_payload,
        route_hire_execution=route_hire_execution_payload,
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
            route_preview=route_preview_payload,
            route_artifact=route_artifact_payload,
            route_hire_execution=route_hire_execution_payload,
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
            "workspace": str(workspace.resolve()) if workspace else "",
            "concept_dir": str(concept_dir.resolve()) if concept_dir else "",
            "audience": audience,
            "sources": list(normalized_sources),
            "freshness_check": freshness_check,
            "execute_migrations": execute_migrations,
            "execute_render": execute_render,
            "execute_route_hires": execute_route_hires,
            "route_hire_limit": max(0, int(route_hire_limit)),
            "route_hire_run_harness": route_hire_run_harness,
            "promote_route_outputs": promote_route_outputs,
            "route_promotion_operator_approval_supplied": bool(route_promotion_operator_approval),
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
        "route_preview": route_preview_payload,
        "route_artifact": route_artifact_payload,
        "route_hire_execution": route_hire_execution_payload,
        "route_promotion_execution": route_promotion_payload,
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
                "wiki.render.succeeded",
                "wiki.manifest.recorded",
                "wiki.generated.available",
                "reader_surface.ready",
            ],
            "route_evidence": [
                "wiki_route_execution.preview_written",
                "wiki_route_plan.ready",
                "source_import.fresh",
                "contract_migrations.closed",
                "runtime_invariants.passed",
                "wiki_apply.source_promotion",
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
            "planned_hire_count": planned_hire_count,
            "non_hire_count": non_hire_count,
            "route_hire_count": int(route_hire_execution_payload.get("completed_count") or 0),
            "route_hire_error_count": int(route_hire_execution_payload.get("error_count") or 0)
            + int(route_hire_execution_payload.get("validation_failure_count") or 0),
            "route_promotion_count": int(route_promotion_payload.get("promoted_count") or 0),
            "render_count": len(render_payloads),
            "route_count": len(route_table.routes),
            "manifest_count": len(route_table.manifests),
            "source_freshness_status": preflight["source_freshness"]["status"],
            "migration_status": migration_payload.get("status", "skipped"),
            "retry_budget": max(0, int(retry_budget)),
            "failure_count": len(migration_failures)
            + len(render_failures)
            + len(route_hire_failures)
            + len(route_promotion_failures),
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
                "reader_surface.ready",
                artifact_id=artifact_id,
                status="passed" if route_table.manifests and route_table.routes else "failed",
                checker="memory.tick",
                text="wiki reader surface has rendered manifests and routes",
                checks=[
                    "render command completed for requested families",
                    "site manifest/content index written",
                    "route table has at least one manifest and route",
                ],
                payload={
                    "cycle_id": cycle,
                    "manifest_count": len(route_table.manifests),
                    "route_count": len(route_table.routes),
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
        dry_run_text = "wiki-only memory tick planned without executing renderer"
        if route_hire_execution_payload:
            dry_run_checks.append("route hired-agent dry-run births completed")
            dry_run_text = "wiki-only memory tick planned and dry-ran route hired-agent births"
        else:
            dry_run_checks.append("no hired agents launched")
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
            "planned_hire_count": planned_hire_count,
            "non_hire_count": non_hire_count,
            "route_hire_count": int(route_hire_execution_payload.get("completed_count") or 0),
            "route_hire_error_count": int(route_hire_execution_payload.get("error_count") or 0)
            + int(route_hire_execution_payload.get("validation_failure_count") or 0),
            "render_count": len(render_payloads),
            "route_count": len(route_table.routes),
            "manifest_count": len(route_table.manifests),
            "retryable": retryable,
            "failure_count": len(migration_failures) + len(render_failures) + len(route_hire_failures),
            "migration_status": migration_payload.get("status", "skipped"),
        },
    )

    return MemoryTickResult(
        cycle_id=cycle,
        path=out_dir,
        mode="wiki_only",
        status=status,
        dry_run=dry_run,
        planned_hire_count=planned_hire_count,
        non_hire_count=non_hire_count,
        route_hire_count=int(route_hire_execution_payload.get("completed_count") or 0),
        route_hire_error_count=int(route_hire_execution_payload.get("error_count") or 0)
        + int(route_hire_execution_payload.get("validation_failure_count") or 0),
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
            "evidence.reader_surface_ready",
            "reader_surface.ready" in evidence_ids,
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
            and "memory_system_fabric.cycle.building_reader_surface--memory.reader_surface.ready--complete"
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
        planned_hire_count=int(payload.get("route_preview", {}).get("planned_hire_count") or 0)
        if isinstance(payload.get("route_preview"), dict)
        else 0,
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
    route_plan_ready: bool = False,
    route_agent_layer_closed: bool = True,
) -> dict[str, Any]:
    executions: list[Any] = []
    terminal_state = terminal_state_for_status(status)
    note = ""
    initial_state = "validating" if route_plan_ready else READER_SURFACE_CONTRACT["source_state"]
    if route_plan_ready:
        try:
            route_transition = record_transition_execution(
                system,
                machine_id=READER_SURFACE_CONTRACT["machine"],
                scope=READER_SURFACE_CONTRACT["scope"],
                source_state="validating",
                event_name="memory.agent_outputs.closed",
                target_state=READER_SURFACE_CONTRACT["source_state"],
                status="passed",
                produced_evidence=("wiki_route_plan.ready",),
                completed_steps=("run_wiki_growth_fabric",),
                emitted_events=("wiki.fabric.tick",),
            )
            executions.append(route_transition)
            terminal_state = READER_SURFACE_CONTRACT["source_state"]
        except StateMachineRuntimeError as exc:
            note = str(exc)
            terminal_state = "failed"

    should_trace_reader = execute_render and route_agent_layer_closed and status in {"completed", "failed", "retryable"}
    if should_trace_reader:
        reader_ready = bool(render_count and manifest_count and route_count)
        produced_evidence = ("reader_surface.ready",) if reader_ready else ()
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
                completed_steps=("run_wiki_reader_loop", "render_wiki_engine_families"),
                emitted_events=("memory.reader_surface.ready",) if reader_ready else (),
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
                    event_name="memory.reader_surface.ready",
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


def find_transition_contract(
    system: MemorySystem,
    *,
    machine_id: str,
    scope: str,
    source_state: str,
    event_name: str,
    target_state: str,
) -> dict[str, Any]:
    return _legacy_find_transition_contract(
        system,
        machine_id=machine_id,
        scope=scope,
        source_state=source_state,
        event_name=event_name,
        target_state=target_state,
    )


def _legacy_find_transition_contract(
    system: MemorySystem,
    *,
    machine_id: str,
    scope: str,
    source_state: str,
    event_name: str,
    target_state: str,
) -> dict[str, Any]:
    machine = compile_system_map(system)["state_machines"].get(machine_id)
    if not isinstance(machine, dict):
        raise MemoryTickError(f"state-machine {machine_id!r} is not available")

    for index, transition in enumerate(machine.get("transitions", []), start=1):
        if not isinstance(transition, dict):
            continue
        source = transition.get("source") if isinstance(transition.get("source"), dict) else {}
        target_payload = transition.get("target") if isinstance(transition.get("target"), dict) else {}
        event_payload = transition.get("event") if isinstance(transition.get("event"), dict) else {}
        if (
            source.get("scope") == scope
            and source.get("state") == source_state
            and target_payload.get("scope") == scope
            and target_payload.get("state") == target_state
            and event_payload.get("name") == event_name
        ):
            collected = collect_contract_actions(transition.get("actions", []))
            transition_id = f"{machine_id}.{scope}.{source_state}--{event_name}--{target_state}"
            return {
                "machine": machine_id,
                "scope": scope,
                "transition": transition_id,
                "transition_index": index,
                "event": event_name,
                "source": {"scope": scope, "state": source_state},
                "target": {"scope": scope, "state": target_state},
                "steps": collected["steps"],
                "expects": collected["expects"],
                "emits": collected["emits"],
            }

    raise MemoryTickError(
        "reader-surface IR contract transition not found: "
        f"{machine_id}.{scope}.{source_state} --{event_name}--> {target_state}"
    )


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
        and "run_wiki_reader_loop" in set(contract.get("steps") or [])
        and "render_wiki_engine_families" in set(contract.get("steps") or [])
        and "reader_surface.ready" in set(contract.get("expects") or [])
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
