from __future__ import annotations

import json
from dataclasses import replace
from datetime import datetime, timezone
from pathlib import Path

from onectx.config import load_system
from onectx.memory.wiki_update import DEFAULT_WIKI_UPDATE_PHASES, default_wiki_update_jobs, run_wiki_update
from onectx.memory_core_cli import is_memory_update_wiki_shape, parse_memory_update_wiki_args


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def test_default_wiki_update_jobs_include_scribes_and_wiki_roles() -> None:
    jobs = default_wiki_update_jobs(now=datetime(2026, 6, 5, 10, 30, tzinfo=timezone.utc))
    job_ids = [job.job_id for job in jobs]

    assert len(jobs) == sum(len(job_ids) for _, job_ids in DEFAULT_WIKI_UPDATE_PHASES)
    assert "memory.hourly.scribe" in job_ids
    assert "memory.hourly.block_scribe" in job_ids
    assert "memory.hourly.shard_scribe" in job_ids
    assert "memory.hourly.aggregate_scribe" in job_ids
    assert "memory.wiki.biographer" in job_ids
    assert "memory.wiki.librarian" in job_ids
    assert "memory.wiki.redactor" in job_ids
    assert jobs[0].params["date"] == "2026-06-05"
    assert jobs[0].params["hour"] == "10"


def test_wiki_update_materializes_agent_plans_and_refresh_request(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime", storage_dir=tmp_path / "storage")

    result = run_wiki_update(
        system,
        provider="codex",
        run_id="test-update",
        now=datetime(2026, 6, 5, 10, 30, tzinfo=timezone.utc),
    )
    payload = result.to_payload(root=system.root)

    assert payload["operation"] == "memory.update_wiki"
    assert payload["status"] == "completed"
    assert payload["execute_agents"] is False
    assert payload["planned_count"] == sum(len(job_ids) for _, job_ids in DEFAULT_WIKI_UPDATE_PHASES)
    assert payload["jobs"][0]["status"] == "planned"
    assert payload["jobs"][0]["plan"]["provider"] == "codex"
    assert payload["jobs"][0]["plan"]["harness"]["id"] == "codex-harness"
    assert payload["jobs"][0]["plan"]["paths"]["prompt"].endswith("prompt.md")
    assert payload["wiki_synthesis"]["status"] == "drafted"
    assert result.path.is_file()
    assert result.wiki_tick.render_count == 1

    receipt = json.loads(result.path.read_text(encoding="utf-8"))
    assert receipt["run_id"] == "test-update"
    assert receipt["wiki_refresh"]["render_count"] == 1


def test_wiki_update_writes_page_drafts_through_wiki_core(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime", storage_dir=tmp_path / "storage")
    fake_wiki_core = tmp_path / "bin" / "onecontext-wiki"
    calls = tmp_path / "wiki-core-calls.txt"
    fake_wiki_core.parent.mkdir(parents=True)
    fake_wiki_core.write_text(
        "#!/bin/sh\n"
        f"printf '%s\\n' \"$*\" >> {json.dumps(str(calls))}\n"
        "printf '{\"status\":\"ok\"}\\n'\n",
        encoding="utf-8",
    )
    fake_wiki_core.chmod(0o755)

    result = run_wiki_update(
        system,
        provider="codex",
        run_id="write-update",
        now=datetime(2026, 6, 5, 10, 30, tzinfo=timezone.utc),
        runtime_root=tmp_path / "1Context Dev",
        wiki_core_bin=fake_wiki_core,
    )
    payload = result.to_payload(root=system.root)

    assert payload["status"] == "completed"
    assert payload["wiki_synthesis"]["status"] == "written"
    assert {write["page_id"]: write["status"] for write in payload["wiki_synthesis"]["writes"]} == {
        "for-you": "written",
        "your-context": "written",
        "projects": "written",
        "topics": "written",
    }
    call_lines = calls.read_text(encoding="utf-8").splitlines()
    assert any("page-create-all" in line for line in call_lines)
    assert sum("page-write-body" in line for line in call_lines) == 4


def test_memory_update_wiki_cli_shape_accepts_execute_options() -> None:
    args = [
        "memory",
        "update-wiki",
        "--provider",
        "codex",
        "--run-id",
        "manual-run",
        "--execute-agents",
        "--max-concurrent",
        "2",
        "--timeout-seconds",
        "30",
        "--json",
    ]

    assert is_memory_update_wiki_shape(args)
    parsed = parse_memory_update_wiki_args(args[2:-1])
    assert parsed == {
        "provider": "codex",
        "run_id": "manual-run",
        "execute_agents": True,
        "max_concurrent": 2,
        "timeout_seconds": 30,
        "import_sources": False,
        "import_ticks": 1,
        "runtime_root": None,
        "wiki_core_bin": None,
    }


def test_memory_update_wiki_cli_shape_rejects_claude_provider() -> None:
    assert not is_memory_update_wiki_shape(["memory", "update-wiki", "--provider", "claude", "--json"])
