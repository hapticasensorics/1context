from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.memory.health import build_memory_health_payload, write_memory_health_artifact
from onectx.memory.scheduler import plan_cadence_fires, plan_scheduler_tick, write_scheduler_plan
from onectx.storage import LakeStore


def isolated_system(tmp_path: Path):
    system = load_system(Path.cwd())
    return replace(
        system,
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )


def test_scheduler_cadence_plans_hour_day_and_week_fires() -> None:
    fires = plan_cadence_fires("2026-04-26T23:00:00Z", "2026-04-27T00:00:00Z")

    assert [fire.phase for fire in fires] == ["hour", "day", "week", "week", "week", "week", "week"]
    assert fires[0].job == "memory.hourly.block_scribe"
    assert fires[1].job == "memory.daily.editor"
    assert {fire.job for fire in fires if fire.phase == "week"} == {
        "memory.wiki.for_you_curator",
        "memory.wiki.context_curator",
        "memory.wiki.biographer",
        "memory.wiki.librarian_sweep",
        "memory.wiki.contradiction_flagger",
    }


def test_stale_importer_blocks_source_derived_fires(tmp_path: Path) -> None:
    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    store.append_event(
        "session.message",
        ts="2026-04-25T00:00:00Z",
        source="codex",
        kind="user",
        actor="user",
        text="old event",
    )

    plan = plan_scheduler_tick(
        store,
        start="2026-04-27T00:00:00Z",
        end="2026-04-27T01:00:00Z",
        sources=("codex",),
        max_source_age_hours=24,
        require_fresh=True,
        now="2026-04-27T01:00:00Z",
    )

    assert plan.freshness["passed"] is False
    assert plan.freshness["sources"]["codex"]["status"] == "stale"
    assert plan.blocked_count == plan.fire_count
    assert all(fire.status == "blocked" for fire in plan.fires)
    assert all(fire.reason == "source import freshness failed" for fire in plan.fires)


def test_health_payload_names_last_successful_fire_per_phase(tmp_path: Path) -> None:
    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    first = store.append_event(
        "memory.scheduler.fire_completed",
        ts="2026-04-27T00:00:00Z",
        source="memory.scheduler",
        subject="hour-old",
        payload={"phase": "hour", "status": "completed"},
    )
    latest = store.append_event(
        "memory.scheduler.fire_completed",
        ts="2026-04-27T01:00:00Z",
        source="memory.scheduler",
        subject="hour-new",
        payload={"phase": "hour", "status": "completed"},
    )
    store.append_event(
        "memory.scheduler.fire_failed",
        ts="2026-04-27T01:05:00Z",
        source="memory.scheduler",
        subject="day-failed",
        payload={"phase": "day", "status": "failed"},
    )

    payload = build_memory_health_payload(store)

    assert payload["last_success_by_phase"]["hour"]["event_id"] == latest["event_id"]
    assert payload["last_success_by_phase"]["hour"]["event_id"] != first["event_id"]
    assert payload["last_failure_by_phase"]["day"]["status"] == "failed"


def test_scheduler_and_health_artifacts_can_be_recorded(tmp_path: Path) -> None:
    system = isolated_system(tmp_path)
    store = LakeStore(system.storage_dir)
    store.ensure()
    store.append_event(
        "session.message",
        ts="2026-04-27T00:30:00Z",
        source="codex",
        kind="user",
        actor="user",
        text="fresh event",
    )

    plan = plan_scheduler_tick(
        store,
        start="2026-04-27T00:00:00Z",
        end="2026-04-27T01:00:00Z",
        sources=("codex",),
        now="2026-04-27T01:00:00Z",
    )
    scheduler_record = write_scheduler_plan(system, plan, run_id="scheduler-record")
    health_payload = build_memory_health_payload(store, scheduler_plan=plan)
    health_record = write_memory_health_artifact(system, health_payload, run_id="health-record")

    scheduler_path = Path(scheduler_record["path"])
    health_path = Path(health_record["path"])
    assert scheduler_path.is_file()
    assert health_path.is_file()
    assert json.loads(scheduler_path.read_text(encoding="utf-8"))["run_id"] == "scheduler-record"
    assert json.loads(health_path.read_text(encoding="utf-8"))["run_id"] == "health-record"
    assert scheduler_record["artifact_id"]
    assert health_record["artifact_id"]
