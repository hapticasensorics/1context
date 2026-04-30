from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.memory.replay import derive_replay_fires
from onectx.memory.replay import run_replay_dry_run
from onectx.storage.hour_events import HourEvent, parse_ts


def test_replay_fires_hour_day_and_week_boundaries() -> None:
    events = [
        HourEvent(
            event_id="event_1",
            hash="",
            session_id="s1",
            ts="2026-04-26T23:30:00Z",
            event="session.message",
            source="codex",
            kind="user",
            actor="user",
            cwd="",
            text="hello",
            payload={},
        )
    ]

    fires = derive_replay_fires(
        parse_ts("2026-04-26T23:00:00Z"),
        parse_ts("2026-04-27T00:00:00Z"),
        events,
    )

    assert [fire["boundary"] for fire in fires] == ["hour", "day", "day", "day", "week", "week", "week", "week", "week"]
    assert fires[0]["agent"] == "scribe"
    assert fires[0]["prior_window_event_count"] == 1
    assert {fire["agent"] for fire in fires if fire["boundary"] == "week"} == {
        "for-you-curator",
        "context-curator",
        "biographer",
        "librarian",
        "contradiction-flagger",
    }


def isolated_system(tmp_path: Path):
    system = load_system(Path.cwd())
    return replace(
        system,
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )


def test_replay_sandbox_does_not_mutate_source(tmp_path: Path) -> None:
    system = isolated_system(tmp_path)
    source = tmp_path / "source-wiki"
    source.mkdir()
    page = source / "page.md"
    page.write_text("# Page\n\nOriginal.\n", encoding="utf-8")

    result = run_replay_dry_run(
        system,
        start="2026-04-27T00:00:00Z",
        end="2026-04-27T01:00:00Z",
        sources=("codex",),
        replay_run_id="sandbox-no-mutation",
        sandbox=source,
    )

    assert page.read_text(encoding="utf-8") == "# Page\n\nOriginal.\n"
    assert result.sandbox["enabled"] is True
    assert result.sandbox["source_unchanged"] is True
    assert Path(result.sandbox["workspace"]).is_dir()
    assert json.loads((result.path / "snapshots" / "source-diff.json").read_text(encoding="utf-8")) == {
        "added": [],
        "changed": [],
        "removed": [],
    }


def test_replay_failure_injection_is_recorded_retryable(tmp_path: Path) -> None:
    system = isolated_system(tmp_path)
    source = tmp_path / "source-wiki"
    source.mkdir()
    (source / "page.md").write_text("# Page\n", encoding="utf-8")

    result = run_replay_dry_run(
        system,
        start="2026-04-27T00:00:00Z",
        end="2026-04-27T01:00:00Z",
        sources=("codex",),
        replay_run_id="sandbox-failure-injection",
        sandbox=source,
        failure_injections=("agent_timeout",),
    )

    assert len(result.injections) == 1
    assert result.injections[0]["kind"] == "failure"
    assert result.injections[0]["retryable"] is True
    assert result.injections[0]["expected_outcome"] == "needs_retry"
    summary = json.loads((result.path / "summary.json").read_text(encoding="utf-8"))
    assert summary["retryable_injection_count"] == 1


def test_replay_operator_edit_injection_produces_protected_outcome(tmp_path: Path) -> None:
    system = isolated_system(tmp_path)
    source = tmp_path / "source-wiki"
    source.mkdir()
    page = source / "talk" / "day.md"
    page.parent.mkdir()
    page.write_text("# Day\n\nOriginal.\n", encoding="utf-8")

    result = run_replay_dry_run(
        system,
        start="2026-04-27T00:00:00Z",
        end="2026-04-27T01:00:00Z",
        sources=("codex",),
        replay_run_id="sandbox-operator-edit",
        sandbox=source,
        operator_edit_injections=("talk/day.md",),
    )

    sandbox_page = Path(result.sandbox["workspace"]) / "talk" / "day.md"
    assert "operator-touched: replay fixture" in sandbox_page.read_text(encoding="utf-8")
    assert "operator-touched" not in page.read_text(encoding="utf-8")
    assert result.injections[0]["expected_outcome"] == "needs_approval"
    assert result.sandbox["source_unchanged"] is True
    assert result.sandbox["sandbox_diff"]["changed"] == ["talk/day.md"]
