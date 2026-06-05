from __future__ import annotations

import json
from dataclasses import replace
from datetime import datetime, timezone
from pathlib import Path
import stat

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
    assert payload["source_packet_path"].endswith("perception-source-packet.md")
    assert payload["perception_snapshot"] is None
    assert payload["wiki_synthesis"]["status"] == "drafted"
    assert result.path.is_file()
    assert result.wiki_tick.render_count == 1
    prompt = Path(payload["jobs"][0]["plan"]["paths"]["prompt"]).read_text(encoding="utf-8")
    assert "## Injected Session History" in prompt
    assert "Source import was not requested" in prompt

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
        "source_window_days": 30,
        "source_max_events": 5000,
        "source_max_lines": 250000,
        "source_query_limit": 2400,
        "source_cursor_name": "",
        "memoryd_bin": None,
        "runtime_root": None,
        "wiki_core_bin": None,
    }


def test_wiki_update_uses_perception_db_source_packet(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime", storage_dir=tmp_path / "storage")
    fake_memoryd = tmp_path / "bin" / "onecontext-memoryd"
    fake_memoryd.parent.mkdir(parents=True)
    fake_memoryd.write_text(
        """#!/usr/bin/env python3
import json
import sys

method = sys.argv[2]
_request = json.load(sys.stdin)
if method == "memory.ingestSources":
    result = {
        "ok": True,
        "source_results": [
            {
                "source_type": "codex",
                "read_count": 1,
                "written_count": 1,
                "cursor": {"position": "codex-1"},
            },
            {
                "source_type": "claude",
                "read_count": 1,
                "written_count": 1,
                "cursor": {"position": "claude-1"},
            },
        ],
    }
elif method == "memory.queryViewport":
    result = {
        "ok": True,
        "objects": [
            {
                "object_id": "20000000-0000-0000-0000-000000000001",
                "source_id": "10000000-0000-0000-0000-000000000001",
                "object_kind": "agent_message",
                "body_type": "text",
                "modality": "text",
                "captured_at": "2026-06-05T10:20:00Z",
                "event_time": "2026-06-05T10:20:00Z",
                "summary": "Codex session: wire Perception DB into the wiki update backfill path.",
                "payload": {
                    "role": "assistant",
                    "text": "We wired the Perception DB session history into the wiki update path.",
                    "session_id": "codex-session",
                    "cwd": "/Users/paulhan/dev/1context-public-launch",
                    "project_key": "1context-public-launch",
                    "source_uri": "codex://session/codex-session",
                },
            },
            {
                "object_id": "20000000-0000-0000-0000-000000000002",
                "source_id": "10000000-0000-0000-0000-000000000002",
                "object_kind": "agent_message",
                "body_type": "text",
                "modality": "text",
                "captured_at": "2026-06-05T10:25:00Z",
                "event_time": "2026-06-05T10:25:00Z",
                "summary": "Claude session: scribe prompts need source packets.",
                "payload": {
                    "role": "user",
                    "text": "Backfill Claude and Codex sessions for the wiki scribes.",
                    "session_id": "claude-session",
                    "cwd": "/Users/paulhan/dev/1context-public-launch",
                    "project_key": "1context-public-launch",
                    "source_uri": "claude://session/claude-session",
                },
            },
        ],
    }
else:
    result = {"ok": False, "error": {"message": "unknown method"}}
print(json.dumps({
    "schema_version": 1,
    "protocol": "memory.query.v1",
    "surface": "perception_db",
    "status": "ok",
    "result": result,
    "stats": {"elapsed_ms": 1},
}))
""",
        encoding="utf-8",
    )
    fake_memoryd.chmod(fake_memoryd.stat().st_mode | stat.S_IXUSR)

    result = run_wiki_update(
        system,
        provider="codex",
        run_id="perception-update",
        now=datetime(2026, 6, 5, 10, 30, tzinfo=timezone.utc),
        import_sources=True,
        import_ticks=1,
        source_window_days=30,
        memoryd_bin=fake_memoryd,
    )
    payload = result.to_payload(root=system.root)

    assert payload["status"] == "completed"
    assert payload["perception_snapshot"]["status"] == "ok"
    assert payload["perception_snapshot"]["event_count"] == 2
    assert payload["wiki_synthesis"]["source_store"] == "perception_db"
    assert payload["wiki_synthesis"]["source_event_count"] == 2
    packet = Path(payload["source_packet_path"]).read_text(encoding="utf-8")
    assert "Perception DB session history" in packet
    assert "Backfill Claude and Codex sessions" in packet
    prompt = Path(payload["jobs"][0]["plan"]["paths"]["prompt"]).read_text(encoding="utf-8")
    assert "## Injected Session History" in prompt
    assert "Backfill Claude and Codex sessions" in prompt


def test_memory_update_wiki_cli_shape_rejects_claude_provider() -> None:
    assert not is_memory_update_wiki_shape(["memory", "update-wiki", "--provider", "claude", "--json"])
