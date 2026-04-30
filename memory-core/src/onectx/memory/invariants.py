from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.storage import LakeStore, stable_id, utc_now


EXPLICIT_OUTCOMES = {
    "already_current",
    "blocked",
    "defer",
    "deferred",
    "failed",
    "forget",
    "hire",
    "needs_approval",
    "needs_fresh_events",
    "no_change",
    "retryable",
    "skip",
    "skipped",
    "split_parent",
}


@dataclass(frozen=True)
class RuntimeInvariantReportArtifact:
    artifact_id: str
    evidence_id: str
    event_id: str
    path: Path
    content_hash: str
    bytes: int
    passed: bool
    silent_noops: int

    def to_payload(self) -> dict[str, Any]:
        return {
            "artifact_id": self.artifact_id,
            "evidence_id": self.evidence_id,
            "event_id": self.event_id,
            "path": str(self.path),
            "content_hash": self.content_hash,
            "bytes": self.bytes,
            "passed": self.passed,
            "silent_noops": self.silent_noops,
        }


def build_runtime_invariant_report(
    *,
    run_id: str,
    mode: str,
    status: str,
    dry_run: bool,
    preflight: dict[str, Any] | None = None,
    steps: list[dict[str, Any]] | None = None,
    route_preview: dict[str, Any] | None = None,
    route_artifact: dict[str, Any] | None = None,
    route_hire_execution: dict[str, Any] | None = None,
    render_count: int = 0,
    manifest_count: int = 0,
    route_count: int = 0,
    execute_render: bool = False,
) -> dict[str, Any]:
    """Build the no-silent-noop report for one concrete run.

    This report deliberately validates execution shape, not prose quality. It
    asks whether expected work was planned, produced, skipped, deferred, failed,
    or otherwise explained before the state machine advances.
    """
    preflight_payload = preflight or {}
    step_rows = list(steps or [])
    route_payload = route_preview or {}
    route_artifact_payload = route_artifact or {}
    route_hire_payload = route_hire_execution or {}

    expected: list[dict[str, Any]] = []
    produced: list[dict[str, Any]] = []
    explicit_outcomes: list[dict[str, Any]] = []
    missing: list[dict[str, Any]] = []

    classify_source_freshness(preflight_payload, explicit_outcomes, missing)
    classify_route_preview(
        route_payload,
        route_artifact_payload,
        expected,
        produced,
        explicit_outcomes,
        missing,
    )
    classify_steps(step_rows, explicit_outcomes, missing)
    classify_route_hire_execution(
        route_hire_payload,
        expected,
        produced,
        explicit_outcomes,
        missing,
    )
    classify_render_surface(
        execute_render=execute_render,
        status=status,
        dry_run=dry_run,
        render_count=render_count,
        manifest_count=manifest_count,
        route_count=route_count,
        expected=expected,
        produced=produced,
        explicit_outcomes=explicit_outcomes,
        missing=missing,
    )

    silent = [item for item in missing if not item.get("explained")]
    return {
        "kind": "runtime_invariant_report",
        "version": "0.1",
        "run_id": run_id,
        "mode": mode,
        "status": status,
        "dry_run": dry_run,
        "created_at": utc_now(),
        "preflight_inventory": {
            "expected": expected,
        },
        "postflight_diff": {
            "produced": produced,
            "explicit_outcomes": explicit_outcomes,
            "missing": missing,
        },
        "summary": {
            "expected_count": len(expected),
            "produced_count": len(produced),
            "explicit_outcome_count": len(explicit_outcomes),
            "missing_count": len(missing),
            "silent_noops": len(silent),
            "passed": len(silent) == 0,
        },
    }


def classify_route_hire_execution(
    route_hire_execution: dict[str, Any],
    expected: list[dict[str, Any]],
    produced: list[dict[str, Any]],
    explicit_outcomes: list[dict[str, Any]],
    missing: list[dict[str, Any]],
) -> None:
    if not route_hire_execution:
        return

    spec_count = int(route_hire_execution.get("spec_count") or 0)
    completed_count = int(route_hire_execution.get("completed_count") or 0)
    error_count = int(route_hire_execution.get("error_count") or 0)
    dry_run = bool(route_hire_execution.get("dry_run"))
    batch = route_hire_execution.get("batch") if isinstance(route_hire_execution.get("batch"), dict) else {}
    results = list(batch.get("results") or [])
    errors = list(batch.get("errors") or [])
    expected.append(
        {
            "kind": "wiki_route_hired_agent_execution",
            "reason": "opt-in route hire execution should birth every selected hired-agent spec or record an explicit error",
            "spec_count": spec_count,
            "dry_run": dry_run,
        }
    )

    if spec_count == 0:
        explicit_outcomes.append(
            {
                "kind": "wiki_route_hired_agent_execution",
                "outcome": "no_change",
                "reason": "route hire execution was requested but no hire rows were selected",
            }
        )
        return

    if completed_count + error_count >= spec_count:
        produced.append(
            {
                "kind": "wiki_route_hired_agent_execution_batch",
                "spec_count": spec_count,
                "completed_count": completed_count,
                "error_count": error_count,
                "dry_run": dry_run,
            }
        )
    else:
        missing.append(
            {
                "kind": "wiki_route_hired_agent_execution_batch",
                "reason": "fewer hired-agent executions completed or errored than were scheduled",
                "spec_count": spec_count,
                "completed_count": completed_count,
                "error_count": error_count,
                "explained": False,
            }
        )

    for index, result in enumerate(results):
        if not isinstance(result, dict):
            missing.append(
                {
                    "kind": "wiki_route_hired_agent_execution",
                    "index": index,
                    "reason": "batch result slot was empty without an associated error",
                    "explained": False,
                }
            )
            continue
        hire = result.get("hire") if isinstance(result.get("hire"), dict) else {}
        prompt_path = str(result.get("prompt_path") or "")
        hired_agent_uuid = str(hire.get("hired_agent_uuid") or "")
        if hired_agent_uuid and prompt_path:
            produced.append(
                {
                    "kind": "wiki_route_hired_agent_birth",
                    "index": index,
                    "hired_agent_uuid": hired_agent_uuid,
                    "prompt_path": prompt_path,
                    "dry_run": bool(result.get("dry_run")),
                }
            )
        else:
            missing.append(
                {
                    "kind": "wiki_route_hired_agent_birth",
                    "index": index,
                    "reason": "hired-agent result missing uuid or prompt path",
                    "explained": False,
                }
            )

    for error in errors:
        explicit_outcomes.append(
            {
                "kind": "wiki_route_hired_agent_execution",
                "outcome": "failed",
                "index": error.get("index", ""),
                "job_ids": list(error.get("job_ids") or []),
                "reason": str(error.get("message") or error.get("error_type") or "hired-agent execution failed"),
            }
        )


def classify_source_freshness(
    preflight: dict[str, Any],
    explicit_outcomes: list[dict[str, Any]],
    missing: list[dict[str, Any]],
) -> None:
    source = preflight.get("source_freshness") if isinstance(preflight.get("source_freshness"), dict) else {}
    status = str(source.get("status") or "")
    if not status:
        missing.append(
            {
                "kind": "preflight.source_freshness",
                "reason": "source freshness preflight missing",
                "explained": False,
            }
        )
        return
    if status == "skipped":
        explicit_outcomes.append(
            {
                "kind": "preflight.source_freshness",
                "outcome": "skipped",
                "reason": str(source.get("reason") or "freshness check intentionally skipped"),
            }
        )
    elif status == "failed":
        explicit_outcomes.append(
            {
                "kind": "preflight.source_freshness",
                "outcome": "blocked" if source.get("required") else "failed",
                "reason": "required source importer was stale or missing"
                if source.get("required")
                else "source importer was stale or missing",
            }
        )
    else:
        explicit_outcomes.append(
            {
                "kind": "preflight.source_freshness",
                "outcome": "passed",
                "reason": "required sources were fresh enough",
            }
        )


def classify_route_preview(
    route_preview: dict[str, Any],
    route_artifact: dict[str, Any],
    expected: list[dict[str, Any]],
    produced: list[dict[str, Any]],
    explicit_outcomes: list[dict[str, Any]],
    missing: list[dict[str, Any]],
) -> None:
    if not route_preview:
        explicit_outcomes.append(
            {
                "kind": "wiki_route_execution",
                "outcome": "skipped",
                "reason": "no workspace/concept route preview requested",
            }
        )
        return

    planned_hires = list(route_preview.get("planned_hires") or [])
    non_hires = list(route_preview.get("non_hire_outcomes") or [])
    expected.append(
        {
            "kind": "wiki_route_execution_preview",
            "reason": "route rows should become planned hires or explicit non-hire outcomes",
            "planned_hire_count": len(planned_hires),
            "non_hire_count": len(non_hires),
        }
    )
    if route_artifact.get("path") or route_preview.get("artifact_path"):
        produced.append(
            {
                "kind": "wiki_route_execution_preview",
                "path": route_artifact.get("path") or route_preview.get("artifact_path"),
                "planned_hire_count": len(planned_hires),
                "non_hire_count": len(non_hires),
            }
        )
    else:
        explicit_outcomes.append(
            {
                "kind": "wiki_route_execution_preview",
                "outcome": "dry_run",
                "reason": "route preview was computed in memory without artifact persistence",
            }
        )

    for hire in planned_hires:
        job_key = str(hire.get("job_key") or hire.get("route_id") or "")
        expected.append(
            {
                "kind": "planned_hire_birth_preview",
                "job_key": job_key,
                "job_ids": list(hire.get("job_ids") or []),
                "reason": "hire route should include birth preview and prompt stack",
            }
        )
        birth = hire.get("birth_certificate_preview") if isinstance(hire.get("birth_certificate_preview"), dict) else {}
        prompt = hire.get("prompt_stack_preview") if isinstance(hire.get("prompt_stack_preview"), dict) else {}
        if birth and prompt:
            produced.append(
                {
                    "kind": "planned_hire_birth_preview",
                    "job_key": job_key,
                    "job_ids": list(hire.get("job_ids") or []),
                }
            )
        else:
            missing.append(
                {
                    "kind": "planned_hire_birth_preview",
                    "job_key": job_key,
                    "reason": "missing birth certificate or prompt stack preview",
                    "explained": False,
                }
            )

    for outcome in non_hires:
        name = str(outcome.get("outcome") or "")
        reason = str(outcome.get("reason") or "")
        if name in EXPLICIT_OUTCOMES and reason:
            explicit_outcomes.append(
                {
                    "kind": "route_non_hire",
                    "job_key": outcome.get("job_key", ""),
                    "job": outcome.get("job", ""),
                    "outcome": name,
                    "reason": reason,
                }
            )
        else:
            missing.append(
                {
                    "kind": "route_non_hire",
                    "job_key": outcome.get("job_key", ""),
                    "outcome": name,
                    "reason": "non-hire outcome missing recognized outcome or reason",
                    "explained": False,
                }
            )


def classify_steps(
    steps: list[dict[str, Any]],
    explicit_outcomes: list[dict[str, Any]],
    missing: list[dict[str, Any]],
) -> None:
    for step in steps:
        step_id = str(step.get("id") or "")
        status = str(step.get("status") or "")
        reason = str(step.get("reason") or "")
        if status in {"skipped", "blocked", "failed", "retryable"}:
            if reason:
                explicit_outcomes.append(
                    {
                        "kind": "step",
                        "step": step_id,
                        "outcome": status,
                        "reason": reason,
                    }
                )
            else:
                missing.append(
                    {
                        "kind": "step",
                        "step": step_id,
                        "outcome": status,
                        "reason": "quiet step outcome missing reason",
                        "explained": False,
                    }
                )


def classify_render_surface(
    *,
    execute_render: bool,
    status: str,
    dry_run: bool,
    render_count: int,
    manifest_count: int,
    route_count: int,
    expected: list[dict[str, Any]],
    produced: list[dict[str, Any]],
    explicit_outcomes: list[dict[str, Any]],
    missing: list[dict[str, Any]],
) -> None:
    if not execute_render:
        explicit_outcomes.append(
            {
                "kind": "reader_surface",
                "outcome": "skipped",
                "reason": "execute_render=false",
            }
        )
        return
    if status in {"blocked", "failed", "retryable"} and dry_run:
        explicit_outcomes.append(
            {
                "kind": "reader_surface",
                "outcome": status,
                "reason": "reader render did not run to completion because cycle status is explicit",
            }
        )
        return
    expected.append(
        {
            "kind": "reader_surface",
            "reason": "execute_render=true should produce rendered manifests and routes",
        }
    )
    if render_count > 0 and manifest_count > 0 and route_count > 0:
        produced.append(
            {
                "kind": "reader_surface",
                "render_count": render_count,
                "manifest_count": manifest_count,
                "route_count": route_count,
            }
        )
    else:
        missing.append(
            {
                "kind": "reader_surface",
                "reason": "render requested but no rendered route table was available",
                "render_count": render_count,
                "manifest_count": manifest_count,
                "route_count": route_count,
                "explained": False,
            }
        )


def write_runtime_invariant_report_artifact(
    system: MemorySystem,
    report: dict[str, Any],
    *,
    run_id: str = "",
    path: Path | None = None,
    checker: str = "memory.runtime_invariants",
) -> RuntimeInvariantReportArtifact:
    resolved_run_id = run_id or str(report.get("run_id") or stable_id("runtime-invariants", utc_now()))
    resolved_path = path or system.runtime_dir / "invariants" / f"{resolved_run_id}.json"
    text = json.dumps(report, indent=2, sort_keys=True, default=str) + "\n"
    resolved_path.parent.mkdir(parents=True, exist_ok=True)
    resolved_path.write_text(text, encoding="utf-8")
    content_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()
    summary = report.get("summary") if isinstance(report.get("summary"), dict) else {}
    passed = bool(summary.get("passed"))
    silent_noops = int(summary.get("silent_noops") or 0)
    artifact_id = stable_id("artifact", "runtime_invariant_report", resolved_run_id, content_hash)

    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact = store.artifact_row(
        "runtime_invariant_report",
        artifact_id=artifact_id,
        uri=f"file://{resolved_path}",
        path=str(resolved_path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=len(text.encode("utf-8")),
        source=checker,
        state="passed" if passed else "failed",
        text=f"runtime invariant report {resolved_run_id}",
        metadata={
            "run_id": resolved_run_id,
            "mode": report.get("mode", ""),
            "status": report.get("status", ""),
            "passed": passed,
            "silent_noops": silent_noops,
            "missing_count": int(summary.get("missing_count") or 0),
        },
    )
    store.replace_rows("artifacts", "artifact_id", [artifact])
    evidence = store.append_evidence(
        "runtime_invariants.passed",
        artifact_id=artifact_id,
        status="passed" if passed else "failed",
        checker=checker,
        text="runtime invariant report checked for silent no-ops",
        checks=[
            "preflight expected work is recorded",
            "postflight produced and explicit quiet outcomes are recorded",
            "silent_noops == 0",
        ],
        payload=report,
    )
    event = store.append_event(
        "runtime_invariants.report_written",
        source=checker,
        actor=checker,
        subject=resolved_run_id,
        artifact_id=artifact_id,
        evidence_id=evidence["evidence_id"],
        text=f"Runtime invariants {'passed' if passed else 'failed'} with {silent_noops} silent no-ops.",
        payload={
            "run_id": resolved_run_id,
            "path": str(resolved_path),
            "passed": passed,
            "silent_noops": silent_noops,
        },
    )
    return RuntimeInvariantReportArtifact(
        artifact_id=artifact_id,
        evidence_id=evidence["evidence_id"],
        event_id=event["event_id"],
        path=resolved_path,
        content_hash=content_hash,
        bytes=len(text.encode("utf-8")),
        passed=passed,
        silent_noops=silent_noops,
    )
