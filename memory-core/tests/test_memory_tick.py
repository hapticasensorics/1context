from __future__ import annotations

import json
from pathlib import Path

from onectx.config import load_system
from onectx.memory.tick import list_memory_cycles, load_memory_cycle, run_memory_tick, validate_memory_cycle


def test_wiki_only_tick_writes_durable_cycle_artifact() -> None:
    system = load_system(Path.cwd())

    result = run_memory_tick(
        system,
        wiki_only=True,
        execute_render=False,
        cycle_id="test-wiki-only-dry-run",
    )

    assert result.status == "completed"
    assert result.dry_run is True
    assert result.render_count == 0
    assert result.artifact_id
    payload = json.loads((result.path / "cycle.json").read_text(encoding="utf-8"))
    assert payload["state_machine"] == "memory_system_fabric"
    assert payload["scope"] == "cycle"
    assert payload["preflight"]["source_freshness"]["status"] == "skipped"
    assert payload["runtime_invariant_report"]["summary"]["passed"] is True
    assert payload["runtime_invariant_report"]["summary"]["silent_noops"] == 0
    assert (result.path / "runtime-invariants.json").is_file()
    assert payload["steps"][0]["id"] == "wiki_route_dry_run"
    assert payload["steps"][1]["id"] == "wiki_render"


def test_wiki_only_tick_can_execute_render_and_record_reader_gate() -> None:
    system = load_system(Path.cwd())

    result = run_memory_tick(
        system,
        wiki_only=True,
        execute_render=True,
        render_family_ids=("for-you",),
        cycle_id="test-wiki-only-render",
    )

    assert result.status == "completed"
    assert result.dry_run is False
    assert result.render_count == 1
    assert result.manifest_count >= 1
    assert result.route_count >= 1
    payload = json.loads((result.path / "cycle.json").read_text(encoding="utf-8"))
    assert payload["renders"][0]["family"]["id"] == "for-you"
    assert payload["ir_contract"]["machine"] == "memory_system_fabric"
    assert payload["ir_contract"]["event"] == "wiki.agent_layer.closed"
    assert payload["ir_contract"]["source"] == {"scope": "cycle", "state": "routing_wiki"}
    assert payload["ir_contract"]["target"] == {"scope": "cycle", "state": "building_reader_surface"}
    assert payload["ir_contract"]["steps"] == ["run_wiki_reader_loop", "render_wiki_engine_families"]
    assert payload["ir_contract"]["expects"] == ["reader_surface.ready"]
    execution = payload["state_machine_execution"]
    assert execution["terminal_state"] == "complete"
    assert [item["event"] for item in execution["transitions"]] == [
        "wiki.agent_layer.closed",
        "memory.reader_surface.ready",
    ]
    assert execution["transitions"][0]["produced_evidence"] == ["reader_surface.ready"]
    assert execution["transitions"][0]["missing_expected_evidence"] == []
    assert execution["scope_state"]["state"] == "complete"
    assert Path(execution["scope_state"]["path"]).is_file()
    assert "reader_surface.ready" in payload["dsl_contract"]["reader_surface_evidence"]


def test_memory_cycle_can_be_listed_loaded_and_validated() -> None:
    system = load_system(Path.cwd())
    cycle_id = "test-wiki-cycle-inspection"
    result = run_memory_tick(
        system,
        wiki_only=True,
        execute_render=True,
        render_family_ids=("for-you",),
        cycle_id=cycle_id,
    )

    cycles = list_memory_cycles(system, limit=50)
    assert any(cycle.cycle_id == cycle_id for cycle in cycles)

    payload = load_memory_cycle(system, cycle_id)
    assert payload["cycle_id"] == cycle_id
    assert payload["state_machine"] == "memory_system_fabric"

    validation = validate_memory_cycle(system, cycle_id)
    assert validation.passed is True
    assert validation.artifact_id == result.artifact_id
    assert {check["id"] for check in validation.checks} >= {
        "cycle_json.exists",
        "artifact.row_exists",
        "artifact.hash_matches_file",
        "evidence.memory_cycle_artifact_written",
        "runtime_invariant_report.exists",
        "evidence.runtime_invariants_passed",
        "preflight.source_freshness.present",
        "evidence.reader_surface_ready",
        "event.cycle_terminal",
        "dsl_contract.present",
        "ir_contract.present",
        "ir_contract.reader_surface_transition",
        "ir_contract.expected_evidence_satisfied",
        "state_machine_execution.present",
        "state_machine_scope_state.persisted",
        "state_machine_execution.reader_surface_transition",
        "state_machine_execution.terminal_complete",
    }


def test_memory_tick_freshness_preflight_can_block_when_required() -> None:
    system = load_system(Path.cwd())
    cycle_id = "test-wiki-freshness-blocked"

    result = run_memory_tick(
        system,
        wiki_only=True,
        execute_render=True,
        render_family_ids=("for-you",),
        require_fresh=True,
        freshness_check="always",
        sources=("definitely-missing-source",),
        cycle_id=cycle_id,
    )

    assert result.status == "blocked"
    assert result.dry_run is True
    assert result.render_count == 0
    payload = load_memory_cycle(system, cycle_id)
    assert payload["preflight"]["source_freshness"]["status"] == "failed"
    assert payload["steps"][1]["status"] == "blocked"
    validation = validate_memory_cycle(system, cycle_id)
    assert validation.passed is True
    assert {check["id"] for check in validation.checks} >= {
        "preflight.source_freshness.present",
        "evidence.source_import_fresh",
    }


def test_memory_tick_records_retryable_render_failure_when_budget_remains() -> None:
    system = load_system(Path.cwd())
    cycle_id = "test-wiki-render-retryable"

    result = run_memory_tick(
        system,
        wiki_only=True,
        execute_render=True,
        render_family_ids=("missing-family-for-retry",),
        retry_budget=1,
        cycle_id=cycle_id,
    )

    assert result.status == "retryable"
    assert result.dry_run is True
    payload = load_memory_cycle(system, cycle_id)
    assert payload["recovery"]["retryable"] is True
    assert payload["recovery"]["next_action"] == "retry_on_next_tick"
    assert payload["steps"][1]["status"] == "retryable"
    validation = validate_memory_cycle(system, cycle_id)
    assert validation.passed is True
    assert {check["id"] for check in validation.checks} >= {
        "recovery.recorded",
        "evidence.recovery_recorded",
    }


def test_memory_tick_records_failed_render_failure_without_retry_budget() -> None:
    system = load_system(Path.cwd())
    cycle_id = "test-wiki-render-failed"

    result = run_memory_tick(
        system,
        wiki_only=True,
        execute_render=True,
        render_family_ids=("missing-family-for-failure",),
        retry_budget=0,
        cycle_id=cycle_id,
    )

    assert result.status == "failed"
    payload = load_memory_cycle(system, cycle_id)
    assert payload["recovery"]["retryable"] is False
    assert payload["recovery"]["next_action"] == "operator_review"
    assert payload["steps"][1]["status"] == "failed"
    assert validate_memory_cycle(system, cycle_id).passed is True
