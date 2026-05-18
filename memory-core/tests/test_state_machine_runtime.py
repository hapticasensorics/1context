from __future__ import annotations

from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.state_machines.runtime import (
    load_scope_state,
    persist_scope_state,
    record_transition_execution,
    select_transition,
)


def test_runtime_selects_reader_surface_transition_from_compiled_ir() -> None:
    system = load_system(Path.cwd())

    plan = select_transition(
        system,
        machine_id="memory_system_fabric",
        scope="cycle",
        source_state="routing_wiki",
        event_name="wiki.agent_layer.closed",
        target_state="building_reader_surface",
    )

    assert plan.transition_id == "memory_system_fabric.cycle.routing_wiki--wiki.agent_layer.closed--building_reader_surface"
    assert plan.source == {"scope": "cycle", "state": "routing_wiki"}
    assert plan.target == {"scope": "cycle", "state": "building_reader_surface"}
    assert plan.summary["steps"] == ("write_wiki_refresh_request", "notify_wiki_render_queue")
    assert plan.summary["expects"] == ("wiki.refresh.requested",)
    assert plan.summary["emits"] == ("wiki.refresh.requested",)


def test_runtime_execution_reports_missing_expected_evidence() -> None:
    system = load_system(Path.cwd())

    execution = record_transition_execution(
        system,
        machine_id="memory_system_fabric",
        scope="cycle",
        source_state="routing_wiki",
        event_name="wiki.agent_layer.closed",
        target_state="building_reader_surface",
        status="failed",
        completed_steps=("write_wiki_refresh_request", "notify_wiki_render_queue"),
    )

    assert execution.missing_expected_evidence == ("wiki.refresh.requested",)
    payload = execution.to_payload()
    assert payload["status"] == "failed"
    assert payload["target_state"] == "building_reader_surface"
    assert payload["missing_expected_evidence"] == ["wiki.refresh.requested"]


def test_runtime_persists_scope_state_for_restartable_cycles(tmp_path: Path) -> None:
    system = replace(load_system(Path.cwd()), runtime_dir=tmp_path / "runtime")
    execution = record_transition_execution(
        system,
        machine_id="memory_system_fabric",
        scope="cycle",
        source_state="building_reader_surface",
        event_name="wiki.refresh.requested",
        target_state="complete",
        status="passed",
        completed_steps=("append_cycle_summary_event",),
        emitted_events=("memory.cycle.complete",),
    )

    state = persist_scope_state(
        system,
        machine_id="memory_system_fabric",
        scope="cycle",
        key="cycle-001",
        initial_state="building_reader_surface",
        terminal_state="complete",
        transitions=(execution,),
        status="completed",
    )

    assert Path(state["path"]).is_file()
    loaded = load_scope_state(
        system,
        machine_id="memory_system_fabric",
        scope="cycle",
        key="cycle-001",
    )
    assert loaded["state"] == "complete"
    assert loaded["transitions"][0]["event"] == "wiki.refresh.requested"
    assert loaded["history"][-1]["transitions"] == [
        "memory_system_fabric.cycle.building_reader_surface--wiki.refresh.requested--complete"
    ]
