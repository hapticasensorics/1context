from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from onectx.state_machines import StateMachineError, load_state_machine_dir
from onectx.state_machines.mermaid import state_machine_to_mermaid
from onectx.state_machines.queue import (
    STATUS_DONE,
    STATUS_FAILED,
    STATUS_NEEDS_RETRY,
    STATUS_RUNNING,
    StateMachineQueueError,
    StateMachineWorkQueue,
)
from onectx.state_machines.runtime import (
    execute_transition_chain,
    load_scope_state,
    record_transition_execution,
    select_transition,
)
from onectx.state_machines.v0_1 import (
    Machine,
    emit,
    event,
    expect,
    parallel,
    sequence,
    spawn,
    step,
)


def sample_machine() -> dict:
    machine = Machine(
        "wiki_growth_fabric",
        version="0.1.0",
        title="Wiki Growth Fabric",
    )
    corpus = machine.scope(
        "corpus",
        key="wiki_workspace",
        states=["idle", "scanning", "routing", "running_agents", "review_ready"],
        initial="idle",
    )
    machine.artifact(
        "role_route_plan",
        kind="json",
        path="{runtime}/wiki/{wiki_id}/role-route-plan.json",
        schema="wiki_role_route_plan.v1",
        policies=["deterministic", "generated", "reviewable"],
    )
    machine.evidence(
        "role_route_plan.ready",
        artifact="role_route_plan",
        checks=["jobs have ownership", "expected outputs are declared"],
    )
    machine.evidence(
        "agent_layer.closed",
        checks=["agent jobs completed or failed with receipts"],
    )
    machine.from_(corpus, "scanning").on(event("wiki.inventory.ready")).to(
        corpus,
        "routing",
        do=sequence(
            step("derive_page_governance_map"),
            step("derive_role_route_plan"),
            expect("role_route_plan.ready"),
            emit("wiki.route_plan.ready"),
        ),
    )
    machine.from_(corpus, "routing").on(event("wiki.route_plan.ready")).to(
        corpus,
        "running_agents",
        do=sequence(
            parallel(
                spawn(
                    "memory.wiki.historian",
                    for_each="role_route_plan.historian_jobs",
                    key="job_key",
                    expects=["historian_entry.valid"],
                ),
                spawn(
                    "memory.wiki.librarian",
                    for_each="role_route_plan.librarian_jobs",
                    key="job_key",
                    expects=["concept_page.created_expanded_or_deferred"],
                ),
                max_concurrent="runtime_policy.max_concurrent_agents",
                fail="collect",
            ),
            expect("agent_layer.closed"),
            emit("wiki.agent_layer.closed"),
        ),
    )
    return machine.to_ir()


def test_versioned_authoring_api_compiles_private4_style_ir() -> None:
    machine = sample_machine()

    assert machine["id"] == "wiki_growth_fabric"
    assert machine["language"] == {
        "id": "ai_state_machine",
        "version": "0.1.0",
        "compatible_spec": ">=0.1.0,<0.2.0",
    }
    assert machine["scopes"][0]["initial"] == "idle"
    assert machine["artifacts"][0]["policies"] == ["deterministic", "generated", "reviewable"]
    assert machine["transitions"][0]["actions"][0]["kind"] == "sequence"


def test_loader_imports_python_dsl_files(tmp_path: Path) -> None:
    source = tmp_path / "wiki_growth_fabric.py"
    source.write_text(
        """
from onectx.state_machines.v0_1 import Machine, event, step


def build():
    machine = Machine("loaded_machine", version="0.1.0")
    scope = machine.scope("run", key="run_id", states=["idle", "done"], initial="idle")
    machine.from_(scope, "idle").on(event("tick")).to(scope, "done", do=step("finish"))
    return machine
""".lstrip(),
        encoding="utf-8",
    )

    machines = load_state_machine_dir(tmp_path)

    assert set(machines) == {"loaded_machine"}
    assert machines["loaded_machine"]["source_path"] == str(source)


def test_loader_rejects_incompatible_language_runtime(tmp_path: Path) -> None:
    source = tmp_path / "bad_machine.py"
    source.write_text(
        """
machine = {
    "id": "bad_machine",
    "version": "0.1.0",
    "language": {"id": "ai_state_machine", "version": "9.9.9"},
}
""".lstrip(),
        encoding="utf-8",
    )

    with pytest.raises(StateMachineError, match="was authored with ai_state_machine 9.9.9"):
        load_state_machine_dir(tmp_path)


def test_mermaid_renderer_preserves_transition_actions() -> None:
    source = state_machine_to_mermaid(sample_machine(), scope_name="corpus")

    assert "flowchart LR" in source
    assert "corpus_scanning" in source
    assert "wiki.inventory.ready" in source
    assert "step: derive_role_route_plan" in source
    assert "parallel: max runtime_policy.max_concurrent_agents" in source
    assert "spawn: memory.wiki.historian" in source


def test_runtime_selects_transitions_and_reports_missing_evidence() -> None:
    system = {"wiki_growth_fabric": sample_machine()}

    plan = select_transition(
        system,
        machine_id="wiki_growth_fabric",
        scope="corpus",
        source_state="scanning",
        event_name="wiki.inventory.ready",
        target_state="routing",
    )

    assert plan.transition_id == "wiki_growth_fabric.corpus.scanning--wiki.inventory.ready--routing"
    assert plan.summary["steps"] == ("derive_page_governance_map", "derive_role_route_plan")
    assert plan.summary["expects"] == ("role_route_plan.ready",)
    assert plan.summary["emits"] == ("wiki.route_plan.ready",)

    execution = record_transition_execution(
        system,
        machine_id="wiki_growth_fabric",
        scope="corpus",
        source_state="scanning",
        event_name="wiki.inventory.ready",
        target_state="routing",
        status="failed",
        completed_steps=("derive_page_governance_map", "derive_role_route_plan"),
    )

    assert execution.missing_expected_evidence == ("role_route_plan.ready",)
    assert execution.to_payload()["target_state"] == "routing"


def test_runtime_executes_and_persists_transition_chain(tmp_path: Path) -> None:
    context = SimpleNamespace(
        state_machines={"wiki_growth_fabric": sample_machine()},
        runtime_dir=tmp_path / "runtime",
    )

    result = execute_transition_chain(
        context,
        machine_id="wiki_growth_fabric",
        scope="corpus",
        key="wiki-refresh-001",
        initial_state="scanning",
        requests=[
            {
                "event": "wiki.inventory.ready",
                "target_state": "routing",
                "produced_evidence": ["role_route_plan.ready"],
                "completed_steps": ["derive_page_governance_map", "derive_role_route_plan"],
                "emitted_events": ["wiki.route_plan.ready"],
            },
            {
                "event": "wiki.route_plan.ready",
                "target_state": "running_agents",
                "produced_evidence": ["historian_entry.valid", "agent_layer.closed"],
                "emitted_events": ["wiki.agent_layer.closed"],
            },
        ],
    )

    assert result.status == "completed"
    assert result.terminal_state == "running_agents"
    assert result.missing_expected_evidence == ()

    state = load_scope_state(
        context,
        machine_id="wiki_growth_fabric",
        scope="corpus",
        key="wiki-refresh-001",
    )
    assert Path(state["path"]).is_file()
    assert state["state"] == "running_agents"
    assert state["history"][-1]["transitions"] == [
        "wiki_growth_fabric.corpus.scanning--wiki.inventory.ready--routing",
        "wiki_growth_fabric.corpus.routing--wiki.route_plan.ready--running_agents",
    ]


def test_queue_persists_retries_and_terminal_items(tmp_path: Path) -> None:
    queue = StateMachineWorkQueue.load(tmp_path / "queue")
    item = queue.enqueue(
        machine="wiki_growth_fabric",
        scope="corpus",
        key="wiki-refresh-001",
        event="wiki.inventory.ready",
        source_state="scanning",
        target_state="routing",
        payload={"reason": "manual-refresh"},
        max_attempts=2,
        now="2026-04-29T12:00:00Z",
    )

    running = queue.mark_running(item.queue_id, now="2026-04-29T12:01:00Z")
    assert running.status == STATUS_RUNNING
    assert running.attempts == 1

    retry = queue.mark_retryable(
        running.queue_id,
        scheduled_at="2026-04-29T12:05:00Z",
        payload={"error": "timeout"},
        now="2026-04-29T12:02:00Z",
    )
    assert retry.status == STATUS_NEEDS_RETRY
    assert retry.payload == {"reason": "manual-refresh", "error": "timeout"}

    second_attempt = queue.mark_running(retry.queue_id, now="2026-04-29T12:05:00Z")
    exhausted = queue.mark_retryable(
        second_attempt.queue_id,
        payload={"last_error": "still timed out"},
        now="2026-04-29T12:06:00Z",
    )

    assert exhausted.status == STATUS_FAILED
    assert exhausted.attempts_remaining == 0

    done = queue.enqueue(
        machine="wiki_growth_fabric",
        scope="corpus",
        key="wiki-refresh-002",
        event="wiki.route_plan.ready",
    )
    done = queue.mark_done(done.queue_id)
    assert queue.list(STATUS_DONE) == [done]

    with pytest.raises(StateMachineQueueError):
        queue.mark_running(done.queue_id)
