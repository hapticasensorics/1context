from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.jobs import claude_account_clean_args
from onectx.memory.prompt_stack import PromptPart, PromptStack
from onectx.memory.runner import (
    ArtifactSpec,
    HarnessLaunchSpec,
    HiredAgentBatchResult,
    HiredAgentExecutionResult,
    HiredAgentExecutionSpec,
    execute_hired_agents,
)
from onectx.memory.wiki_apply import (
    apply_curator_decision_to_sandbox,
    promote_wiki_apply_result_to_source,
    write_wiki_apply_promotion_result,
    write_wiki_apply_result,
)
from onectx.memory.wiki import (
    WikiRoutePlan,
    build_wiki_route_prompt_stack,
    render_wiki_route_source_packet,
    route_job_params,
)
from onectx.memory.wiki_validators import validate_wiki_route_output


class WikiRouteExecutorError(RuntimeError):
    """Raised when wiki route rows cannot become hired-agent specs."""


@dataclass(frozen=True)
class WikiRouteHireExecution:
    run_id: str
    dry_run: bool
    spec_count: int
    max_concurrent: int
    batch: HiredAgentBatchResult

    @property
    def completed_count(self) -> int:
        return len([result for result in self.batch.results if result is not None])

    @property
    def error_count(self) -> int:
        return len(self.batch.errors)

    @property
    def validation_failure_count(self) -> int:
        return int(self.batch.to_payload().get("validation_failure_count") or 0)

    @property
    def ok(self) -> bool:
        return self.batch.ok and self.validation_failure_count == 0

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": "wiki_route_hire_execution",
            "run_id": self.run_id,
            "dry_run": self.dry_run,
            "spec_count": self.spec_count,
            "completed_count": self.completed_count,
            "error_count": self.error_count,
            "validation_failure_count": self.validation_failure_count,
            "ok": self.ok,
            "max_concurrent": self.max_concurrent,
            "batch": self.batch.to_payload(),
        }


@dataclass(frozen=True)
class WikiRoutePromotionItem:
    route_id: str
    job_key: str
    job: str
    status: str
    apply_result: dict[str, Any]
    promotion_result: dict[str, Any]
    record: dict[str, Any]
    promotion_record: dict[str, Any]
    failures: tuple[str, ...]

    @property
    def ok(self) -> bool:
        return self.status not in {"failed", "blocked"}

    def to_payload(self) -> dict[str, Any]:
        return {
            "route_id": self.route_id,
            "job_key": self.job_key,
            "job": self.job,
            "status": self.status,
            "ok": self.ok,
            "apply_result": self.apply_result,
            "promotion_result": self.promotion_result,
            "record": self.record,
            "promotion_record": self.promotion_record,
            "failures": list(self.failures),
        }


@dataclass(frozen=True)
class WikiRoutePromotionExecution:
    run_id: str
    item_count: int
    promoted_count: int
    skipped_count: int
    blocked_count: int
    failed_count: int
    items: tuple[WikiRoutePromotionItem, ...]

    @property
    def ok(self) -> bool:
        return self.blocked_count == 0 and self.failed_count == 0

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": "wiki_route_promotion_execution",
            "run_id": self.run_id,
            "item_count": self.item_count,
            "promoted_count": self.promoted_count,
            "skipped_count": self.skipped_count,
            "blocked_count": self.blocked_count,
            "failed_count": self.failed_count,
            "ok": self.ok,
            "items": [item.to_payload() for item in self.items],
        }


def execute_wiki_route_hires(
    system: MemorySystem,
    plan: WikiRoutePlan,
    *,
    run_id: str,
    run_harness: bool = False,
    limit: int = 0,
    max_concurrent: int | None = None,
    model: str | None = None,
) -> WikiRouteHireExecution:
    specs = wiki_route_execution_specs(
        system,
        plan,
        run_id=run_id,
        run_harness=run_harness,
        limit=limit,
        model=model,
    )
    batch = execute_hired_agents(
        system,
        specs,
        max_concurrent=max_concurrent,
        run_id=run_id,
    )
    return WikiRouteHireExecution(
        run_id=run_id,
        dry_run=not run_harness,
        spec_count=len(specs),
        max_concurrent=batch.max_concurrent,
        batch=batch,
    )


def wiki_route_execution_specs(
    system: MemorySystem,
    plan: WikiRoutePlan,
    *,
    run_id: str,
    run_harness: bool,
    limit: int = 0,
    model: str | None = None,
) -> tuple[HiredAgentExecutionSpec, ...]:
    specs: list[HiredAgentExecutionSpec] = []
    for row in planned_hire_rows(plan, limit=limit):
        specs.append(
            wiki_route_execution_spec(
                system,
                row,
                run_id=run_id,
                run_harness=run_harness,
                model=model,
            )
        )
    return tuple(specs)


def promote_wiki_route_outputs(
    system: MemorySystem,
    plan: WikiRoutePlan,
    execution: WikiRouteHireExecution,
    *,
    run_id: str,
    operator_approval: str,
) -> WikiRoutePromotionExecution:
    rows = planned_hire_rows(plan, limit=execution.spec_count)
    items: list[WikiRoutePromotionItem] = []
    for index, row in enumerate(rows):
        result = execution.batch.results[index] if index < len(execution.batch.results) else None
        items.append(
            promote_one_wiki_route_output(
                system,
                plan,
                row,
                result,
                run_id=run_id,
                operator_approval=operator_approval,
            )
        )
    promoted_count = len([item for item in items if item.status == "promoted"])
    skipped_count = len([item for item in items if item.status == "skipped"])
    blocked_count = len([item for item in items if item.status == "blocked"])
    failed_count = len([item for item in items if item.status == "failed"])
    return WikiRoutePromotionExecution(
        run_id=run_id,
        item_count=len(items),
        promoted_count=promoted_count,
        skipped_count=skipped_count,
        blocked_count=blocked_count,
        failed_count=failed_count,
        items=tuple(items),
    )


def promote_one_wiki_route_output(
    system: MemorySystem,
    plan: WikiRoutePlan,
    row: dict[str, Any],
    result: HiredAgentExecutionResult | None,
    *,
    run_id: str,
    operator_approval: str,
) -> WikiRoutePromotionItem:
    ownership = row.get("ownership") if isinstance(row.get("ownership"), dict) else {}
    route_id = str(row.get("route_id") or "")
    job_key = str(row.get("job_key") or route_id or row.get("job") or "wiki-route")
    job = str(row.get("job") or "")
    safe_key = safe_filename(job_key)
    route_apply_run_id = f"{run_id}-{safe_key}"
    if not ownership_is_article_section(ownership):
        return route_promotion_item(row, status="skipped", failures=("route ownership is not an article section mutation",))
    if result is None:
        return route_promotion_item(row, status="failed", failures=("route hire result is missing",))
    if result.dry_run:
        return route_promotion_item(row, status="blocked", failures=("route hire was dry-run; no artifact can be promoted",))
    if not result.validation.get("ok"):
        return route_promotion_item(row, status="failed", failures=("route hire artifact validation failed",))

    sandbox_root = system.runtime_dir / "wiki" / "route-promotions" / run_id / "sandboxes" / safe_key
    apply_result = apply_curator_decision_to_sandbox(
        source_workspace=plan.workspace,
        decision_path=result.output_path,
        route_row=row,
        sandbox_root=sandbox_root,
    )
    record = write_wiki_apply_result(system, apply_result, run_id=route_apply_run_id)
    if apply_result.status != "applied":
        status = apply_result.status if apply_result.ok else "failed"
        return route_promotion_item(
            row,
            status=status,
            apply_result=apply_result.to_payload(),
            record=record,
            failures=apply_result.failures,
        )

    promotion = promote_wiki_apply_result_to_source(
        system,
        apply_result,
        run_id=route_apply_run_id,
        operator_approval=operator_approval,
    )
    promotion_record = write_wiki_apply_promotion_result(system, promotion, run_id=route_apply_run_id)
    return route_promotion_item(
        row,
        status=promotion.status,
        apply_result=apply_result.to_payload(),
        promotion_result=promotion.to_payload(),
        record=record,
        promotion_record=promotion_record,
        failures=promotion.failures,
    )


def wiki_route_execution_spec(
    system: MemorySystem,
    row: dict[str, Any],
    *,
    run_id: str,
    run_harness: bool,
    model: str | None = None,
) -> HiredAgentExecutionSpec:
    job_id = str(row.get("job") or "")
    if not job_id:
        raise WikiRouteExecutorError("route row is missing job")
    job = required_manifest(system.jobs, job_id, "job")
    agent_id = str(job.get("agent") or "")
    agent = required_manifest(system.agents, agent_id, "agent")
    harness_id = str(agent.get("harness") or "")
    provider_id = str(agent.get("provider") or "")
    resolved_model = str(model or agent.get("model") or "")
    if not harness_id or not provider_id or not resolved_model:
        raise WikiRouteExecutorError(f"route job {job_id!r} must resolve harness, provider, and model")

    job_key = str(row.get("job_key") or row.get("route_id") or job_id)
    safe_key = safe_filename(job_key)
    workspace = system.runtime_dir / "wiki" / "route-hire-runs" / run_id / safe_key
    output_path = route_output_path(workspace, row, run_harness=run_harness)
    source_packet_text = render_wiki_route_source_packet(row)
    workspace.mkdir(parents=True, exist_ok=True)
    source_packet_path = workspace / "birth-loaded-source-packet.md"
    source_packet_path.write_text(source_packet_text, encoding="utf-8")
    prompt_stack = build_wiki_route_prompt_stack(system, row=row, source_packet_text=source_packet_text)
    prompt_stack = PromptStack(
        (
            *prompt_stack.parts,
            PromptPart(
                name="runtime_output_contract",
                text=render_runtime_output_contract(row=row, output_path=output_path, workspace=workspace),
            ),
        )
    )
    experience_packet = dict(row.get("source_packet") or {})
    experience_packet.setdefault("loaded_at_birth", True)
    experience_packet.setdefault("route_id", row.get("route_id", ""))
    experience_packet.setdefault("job_key", job_key)
    experience_packet.setdefault("path", str(source_packet_path))

    launch = HarnessLaunchSpec(
        harness=harness_id,
        isolation_mode=str(system.runtime_policy.get("default_harness_isolation", "account_clean")),
        argv=claude_account_clean_args(model=resolved_model, workspace=workspace, experience_path=workspace),
        cwd=workspace,
    )
    budget = row.get("budget") if isinstance(row.get("budget"), dict) else {}
    timeout_minutes = int(budget.get("timeout_minutes") or 0)
    return HiredAgentExecutionSpec(
        run_id=run_id,
        job_ids=[job_id],
        job_params={
            **route_job_params(row),
            "route_id": row.get("route_id", ""),
            "job_key": job_key,
        },
        experience_packet=experience_packet,
        prompt=prompt_stack.text,
        prompt_stack=prompt_stack.to_payload(),
        workspace=workspace,
        artifact=ArtifactSpec(kind=artifact_kind(row), path=output_path),
        harness_launch=launch,
        agent_id=agent_id,
        harness_id=harness_id,
        provider_id=provider_id,
        model=resolved_model,
        mode="new",
        run_harness=run_harness,
        completed_event="wiki.route_hire.execution_completed",
        validator=validate_wiki_route_output,
        harness_timeout_seconds=(timeout_minutes * 60) if timeout_minutes else None,
    )


def route_output_path(workspace: Path, row: dict[str, Any], *, run_harness: bool = False) -> Path:
    ownership = row.get("ownership") if isinstance(row.get("ownership"), dict) else {}
    for key in ("target_path", "path"):
        value = str(ownership.get(key) or "")
        if value and not value.endswith(".talk"):
            return Path(value) if run_harness else workspace / f"would-write-{Path(value).name}"
    return workspace / "output.md"


def artifact_kind(row: dict[str, Any]) -> str:
    expected = row.get("expected_outputs") if isinstance(row.get("expected_outputs"), list) else []
    if expected:
        return str(expected[0])
    return str(row.get("job") or "wiki_route_output")


def render_runtime_output_contract(*, row: dict[str, Any], output_path: Path, workspace: Path) -> str:
    ownership = row.get("ownership") if isinstance(row.get("ownership"), dict) else {}
    promotion_contract = ""
    if ownership_is_article_section(ownership):
        sections = ownership.get("sections")
        if not isinstance(sections, list):
            sections = [ownership.get("section")] if ownership.get("section") else []
        section_text = ", ".join(str(item) for item in sections if item) or "<target-section>"
        promotion_contract = (
            "\n\n## Source-Promotion Artifact Contract\n\n"
            "This route is source-promotable, but you must not edit the source article directly. "
            "Write a curator decision artifact that the state machine can apply in a sandbox and then promote.\n\n"
            "Use YAML frontmatter like this:\n\n"
            "```yaml\n"
            "---\n"
            "kind: decided\n"
            "author: claude-opus-curator\n"
            "ts: <UTC timestamp>\n"
            f"target-section: {section_text}\n"
            "outcome: applied\n"
            "---\n"
            "```\n\n"
            "Then include a `## Replacement` section with a fenced markdown block containing the final section body. "
            "Include the section marker and H2 heading inside the fence if available; the promotion step will preserve "
            "the source section marker and heading and replace only the body.\n\n"
            "```markdown\n"
            "<!-- section:\"<target-section>\" -->\n"
            "## <existing section heading>\n"
            "<final approved prose for this section>\n"
            "```\n\n"
            "If you defer, reject, skip, or need approval, set `outcome` accordingly and explain why instead of writing "
            "a replacement fence.\n"
        )
    return (
        "# Runtime Output Contract\n\n"
        "For this controlled route execution, write the completed route artifact to this exact path:\n\n"
        f"`{output_path}`\n\n"
        "Use the Write tool. Do not edit the original source wiki files in this run; the source packet is read-only "
        "context for producing this artifact. If the right outcome is skip, forget, defer, no_change, or "
        "needs_approval, still write a short markdown artifact at the path above that names the outcome and reason.\n\n"
        f"Runtime workspace: `{workspace}`\n\n"
        "The artifact must be markdown, non-empty, and should include YAML frontmatter with at least `kind`, "
        "`author`, and `ts` when the route contract calls for a talk-folder style entry.\n\n"
        "Route contract summary:\n\n"
        "```json\n"
        f"{route_contract_json(row)}\n"
        "```\n"
        f"{promotion_contract}"
    )


def route_contract_json(row: dict[str, Any]) -> str:
    import json

    payload = {
        "route_id": row.get("route_id", ""),
        "job_key": row.get("job_key", ""),
        "job": row.get("job", ""),
        "expected_outputs": row.get("expected_outputs", []),
        "validators": row.get("validators", []),
        "ownership": row.get("ownership", {}),
    }
    return json.dumps(payload, indent=2, sort_keys=True, default=str)


def planned_hire_rows(plan: WikiRoutePlan, *, limit: int = 0) -> tuple[dict[str, Any], ...]:
    rows_out: list[dict[str, Any]] = []
    for _group, rows in plan.route_plan.items():
        for row in rows:
            if str(row.get("outcome", "hire")) != "hire":
                continue
            rows_out.append(row)
            if limit and len(rows_out) >= limit:
                return tuple(rows_out)
    return tuple(rows_out)


def ownership_is_article_section(ownership: dict[str, Any]) -> bool:
    return str(ownership.get("kind") or "") in {"article_sections", "article_section"}


def route_promotion_item(
    row: dict[str, Any],
    *,
    status: str,
    apply_result: dict[str, Any] | None = None,
    promotion_result: dict[str, Any] | None = None,
    record: dict[str, Any] | None = None,
    promotion_record: dict[str, Any] | None = None,
    failures: tuple[str, ...] | list[str] = (),
) -> WikiRoutePromotionItem:
    return WikiRoutePromotionItem(
        route_id=str(row.get("route_id") or ""),
        job_key=str(row.get("job_key") or row.get("route_id") or row.get("job") or "wiki-route"),
        job=str(row.get("job") or ""),
        status=status,
        apply_result=apply_result or {},
        promotion_result=promotion_result or {},
        record=record or {},
        promotion_record=promotion_record or {},
        failures=tuple(str(item) for item in failures),
    )


def required_manifest(collection: dict[str, dict[str, Any]], manifest_id: str, kind: str) -> dict[str, Any]:
    manifest = collection.get(manifest_id)
    if not manifest:
        raise WikiRouteExecutorError(f"missing {kind} manifest {manifest_id!r}")
    return manifest


def safe_filename(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "-", value).strip("-")
    return cleaned[:120] or "route"
