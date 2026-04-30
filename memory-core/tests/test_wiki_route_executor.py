from __future__ import annotations

import json
from pathlib import Path

from dataclasses import replace

from onectx.config import load_system
from onectx.memory.wiki_apply import PROMOTION_APPROVAL_TOKEN
from onectx.memory.runner import HiredAgentBatchResult, HiredAgentExecutionResult
from onectx.memory.tick import load_memory_cycle, run_memory_tick, validate_memory_cycle
from onectx.memory.wiki import WikiRoutePlan, plan_wiki_roles
from onectx.memory.wiki_executor import (
    WikiRouteHireExecution,
    execute_wiki_route_hires,
    promote_wiki_route_outputs,
    route_output_path,
)


def test_wiki_route_executor_births_dry_run_hired_agents(tmp_path: Path) -> None:
    system = load_system(Path.cwd())
    workspace, concepts = write_route_fixture(tmp_path)
    plan = plan_wiki_roles(workspace=workspace, concept_dir=concepts)

    execution = execute_wiki_route_hires(
        system,
        plan,
        run_id="test-wiki-route-executor",
        run_harness=False,
        limit=2,
    )

    assert execution.ok is True
    assert execution.spec_count == 2
    assert execution.completed_count == 2
    assert execution.error_count == 0
    payload = execution.to_payload()
    assert payload["batch"]["result_count"] == 2
    for result in payload["batch"]["results"]:
        assert result["dry_run"] is True
        assert result["hire"]["hired_agent_uuid"]
        assert Path(result["prompt_path"]).is_file()
        assert "Birth-Loaded Wiki Route Source Packet" in Path(result["prompt_path"]).read_text(encoding="utf-8")
        assert Path(result["experience_packet"]["path"]).is_file()


def test_route_output_path_uses_source_target_only_for_live_harness(tmp_path: Path) -> None:
    row = {"ownership": {"target_path": str(tmp_path / "source.md")}}

    assert route_output_path(tmp_path / "run", row, run_harness=False) == tmp_path / "run" / "would-write-source.md"
    assert route_output_path(tmp_path / "run", row, run_harness=True) == tmp_path / "source.md"


def test_memory_tick_executes_route_hire_dry_run_and_invariants_account_for_it(tmp_path: Path) -> None:
    system = load_system(Path.cwd())
    workspace, concepts = write_route_fixture(tmp_path)
    cycle_id = "test-wiki-route-hire-cycle"

    result = run_memory_tick(
        system,
        wiki_only=True,
        workspace=workspace,
        concept_dir=concepts,
        freshness_check="skip",
        execute_route_hires=True,
        route_hire_limit=1,
        cycle_id=cycle_id,
    )

    assert result.status == "completed"
    assert result.route_hire_count == 1
    assert result.route_hire_error_count == 0
    payload = load_memory_cycle(system, cycle_id)
    assert payload["route_hire_execution"]["spec_count"] == 1
    assert payload["route_hire_execution"]["completed_count"] == 1
    assert payload["state_machine_execution"]["terminal_state"] == "routing_wiki"
    assert [item["event"] for item in payload["state_machine_execution"]["transitions"]] == [
        "memory.agent_outputs.closed"
    ]
    assert payload["state_machine_execution"]["transitions"][0]["produced_evidence"] == ["wiki_route_plan.ready"]
    assert payload["runtime_invariant_report"]["summary"]["passed"] is True
    invariant_path = Path(payload["runtime_invariant_report"]["path"])
    invariant = json.loads(invariant_path.read_text(encoding="utf-8"))
    assert any(
        item["kind"] == "wiki_route_hired_agent_birth"
        for item in invariant["postflight_diff"]["produced"]
    )
    assert validate_memory_cycle(system, cycle_id).passed is True


def test_wiki_route_promotion_applies_valid_curator_artifact_to_source(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    workspace, concepts = write_route_fixture(tmp_path)
    plan = plan_with_first_for_you_curator(workspace, concepts)
    row = plan.route_plan["for_you_curator_jobs"][0]
    decision = tmp_path / "decision.md"
    write_route_decision(decision)
    article = workspace / "2026-04-20.md"
    original = article.read_text(encoding="utf-8")
    route_result = fake_route_result(decision, dry_run=False, validation_ok=True)
    execution = WikiRouteHireExecution(
        run_id="route-promote",
        dry_run=False,
        spec_count=1,
        max_concurrent=1,
        batch=HiredAgentBatchResult(max_concurrent=1, results=(route_result,), errors=()),
    )

    promotion = promote_wiki_route_outputs(
        system,
        plan,
        execution,
        run_id="route-promote",
        operator_approval=PROMOTION_APPROVAL_TOKEN,
    )

    assert promotion.ok is True
    assert promotion.promoted_count == 1
    promoted_text = article.read_text(encoding="utf-8")
    assert "New route-promoted Monday prose." in promoted_text
    assert "## Monday\nFilled." not in promoted_text
    assert "## Tuesday\nFilled." in promoted_text
    item = promotion.items[0]
    assert item.status == "promoted"
    assert item.record["status"] == "applied"
    assert item.promotion_record["status"] == "promoted"
    backup = Path(item.promotion_result["backup_path"])
    assert backup.read_text(encoding="utf-8") == original


def test_wiki_route_promotion_blocks_dry_run_outputs(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    workspace, concepts = write_route_fixture(tmp_path)
    plan = plan_with_first_for_you_curator(workspace, concepts)
    decision = tmp_path / "decision.md"
    write_route_decision(decision)
    route_result = fake_route_result(decision, dry_run=True, validation_ok=True)
    execution = WikiRouteHireExecution(
        run_id="route-promote-dry",
        dry_run=True,
        spec_count=1,
        max_concurrent=1,
        batch=HiredAgentBatchResult(max_concurrent=1, results=(route_result,), errors=()),
    )

    promotion = promote_wiki_route_outputs(
        system,
        plan,
        execution,
        run_id="route-promote-dry",
        operator_approval=PROMOTION_APPROVAL_TOKEN,
    )

    assert promotion.ok is False
    assert promotion.blocked_count == 1
    assert "route hire was dry-run" in promotion.items[0].failures[0]


def write_route_fixture(tmp_path: Path) -> tuple[Path, Path]:
    workspace = tmp_path / "wiki"
    concepts = tmp_path / "concepts"
    workspace.mkdir()
    concepts.mkdir()
    (concepts / "1context.md").write_text("## 1Context\n\nMemory layer.\n", encoding="utf-8")
    (workspace / "your-context.md").write_text("# Your Context\n", encoding="utf-8")
    (workspace / "2026-04-20.md").write_text(
        """# For You

<!-- section:"2026-04-20" -->
## Monday
Filled.

<!-- section:"2026-04-21" -->
## Tuesday
Filled.
""",
        encoding="utf-8",
    )
    talk = workspace / "2026-04-20.private.talk"
    talk.mkdir()
    (talk / "2026-04-20T09-00Z.conversation.md").write_text(
        "---\nkind: conversation\nts: 2026-04-20T09:00:00Z\n---\n\nHour.\n",
        encoding="utf-8",
    )
    (talk / "2026-04-20T23-59Z.proposal.concept-1context.md").write_text(
        "---\nkind: proposal\nts: 2026-04-20T23:59:00Z\n---\n\nProposal.\n",
        encoding="utf-8",
    )
    (talk / "2026-04-20T23-59Z.proposal.editor-day-2026-04-20.md").write_text(
        "---\nkind: proposal\nts: 2026-04-20T23:59:00Z\n---\n\nProposal.\n",
        encoding="utf-8",
    )
    ycx_talk = workspace / "your-context.talk"
    ycx_talk.mkdir()
    (ycx_talk / "2026-04-20T23-59Z.proposal.ycx-working-style.md").write_text(
        "---\nkind: proposal\n---\n\nProposal.\n",
        encoding="utf-8",
    )
    return workspace, concepts


def plan_with_first_for_you_curator(workspace: Path, concepts: Path) -> WikiRoutePlan:
    full_plan = plan_wiki_roles(workspace=workspace, concept_dir=concepts)
    row = full_plan.route_plan["for_you_curator_jobs"][0]
    return WikiRoutePlan(
        workspace=full_plan.workspace,
        concept_dir=full_plan.concept_dir,
        audience=full_plan.audience,
        inventory=full_plan.inventory,
        route_plan={"for_you_curator_jobs": [row]},
    )


def write_route_decision(path: Path) -> None:
    path.write_text(
        """---
kind: decided
author: claude-opus-curator
ts: 2026-04-21T00:00:00Z
target-section: 2026-04-20
outcome: applied
---

## Replacement

```markdown
<!-- section:"2026-04-20" -->
## Monday
New route-promoted Monday prose.
```
""",
        encoding="utf-8",
    )


def fake_route_result(path: Path, *, dry_run: bool, validation_ok: bool) -> HiredAgentExecutionResult:
    return HiredAgentExecutionResult(
        dry_run=dry_run,
        workspace=path.parent,
        output_path=path,
        prompt_path=path.parent / "prompt.md",
        experience_packet={},
        hire={"hired_agent_uuid": "hire-test"},
        validation={"ok": validation_ok, "checks": [], "failures": []},
        harness_launch={},
    )
