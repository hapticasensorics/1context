from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.storage import LakeStore, stable_id, utc_now


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

    expected: list[dict[str, Any]] = []
    produced: list[dict[str, Any]] = []
    explicit_outcomes: list[dict[str, Any]] = []
    missing: list[dict[str, Any]] = []

    classify_source_freshness(preflight_payload, explicit_outcomes, missing)
    classify_steps(step_rows, explicit_outcomes, missing)
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
            "reason": "execute_render=true should produce a Swift wiki.refresh request",
        }
    )
    if render_count > 0:
        produced.append(
            {
                "kind": "wiki_refresh_request",
                "render_count": render_count,
                "manifest_count": manifest_count,
                "route_count": route_count,
            }
        )
    else:
        missing.append(
            {
                "kind": "wiki_refresh_request",
                "reason": "render requested but no Swift wiki.refresh request was recorded",
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
    record_storage: bool = False,
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
    if not record_storage:
        evidence_id = stable_id("evidence", "runtime_invariants.passed", artifact_id, passed)
        event_id = stable_id("event", "runtime_invariants.report_written", artifact_id, passed)
        return RuntimeInvariantReportArtifact(
            artifact_id=artifact_id,
            evidence_id=evidence_id,
            event_id=event_id,
            path=resolved_path,
            content_hash=content_hash,
            bytes=len(text.encode("utf-8")),
            passed=passed,
            silent_noops=silent_noops,
        )

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
