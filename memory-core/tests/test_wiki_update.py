from __future__ import annotations

import json
import os
from dataclasses import replace
from datetime import datetime, timezone
from pathlib import Path
import stat

from onectx.config import load_system
from onectx.memory.wiki_memory_plan import build_wiki_memory_plan
from onectx.memory.wiki_update import DEFAULT_WIKI_UPDATE_PHASES, WikiUpdateJobResult, default_wiki_update_jobs, run_wiki_update
from onectx.memory_core_cli import is_memory_update_wiki_shape, parse_memory_update_wiki_args


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def fake_memory_event(ts: str, text: str, *, session: str = "session") -> dict[str, str]:
    return {
        "ts": ts,
        "kind": "user",
        "source": "codex",
        "session_id": session,
        "cwd": "/Users/paulhan/dev/1context-public-launch",
        "project_key": "1context-public-launch",
        "text": text,
    }


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
    assert "memory.wiki.redactor" not in job_ids
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
    assert payload["jobs"][0]["plan"]["model_policy"]["reasoning_effort"] == "high"
    assert payload["jobs"][0]["plan"]["agent_harness_call"]["request"]["metadata"]["model_policy"]["reasoning_effort"] == "high"
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
    assert receipt["jobs"][0]["plan"]["model_policy"]["reasoning_effort"] == "high"
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


def test_execute_agents_uses_harness_units_and_posts_talk_receipts(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime", storage_dir=tmp_path / "storage")

    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    harness_calls = tmp_path / "harness-calls.jsonl"
    codex_calls = tmp_path / "codex-calls.jsonl"
    wiki_calls = tmp_path / "wiki-calls.jsonl"

    fake_harness = bin_dir / "onecontext-agent-harness"
    fake_harness.write_text(
        f"""#!/usr/bin/env python3
import json
import sys
from pathlib import Path

args = sys.argv[1:]
root = args[args.index("--root") + 1] if "--root" in args else ""
command = next(arg for arg in args if not arg.startswith("--") and arg != root)
request = {{}}
if "--request-json" in args:
    request = json.loads(args[args.index("--request-json") + 1])
Path({json.dumps(str(harness_calls))}).parent.mkdir(parents=True, exist_ok=True)
with Path({json.dumps(str(harness_calls))}).open("a", encoding="utf-8") as handle:
    handle.write(json.dumps({{"command": command, "request": request}}, sort_keys=True) + "\\n")
unit_id = request.get("unit_id") or request.get("request", {{}}).get("unit_id") or "unit"
print(json.dumps({{"status": "ok", "operation": "agent.harness." + command, "unit_id": unit_id, "receipt": {{"kind": command.replace("-", "_")}}}}))
""",
        encoding="utf-8",
    )
    fake_harness.chmod(fake_harness.stat().st_mode | stat.S_IXUSR)

    fake_codex = bin_dir / "codex"
    fake_codex.write_text(
        f"""#!/usr/bin/env python3
import json
import sys
from pathlib import Path

args = sys.argv[1:]
message_path = Path(args[args.index("--output-last-message") + 1])
message_path.parent.mkdir(parents=True, exist_ok=True)
message_path.write_text("status: completed\\nevidence: fake codex adapter report and [private evidence](/Users/paulhan/dev/1Context-private-2)\\nproposed_wiki_talk: posted\\npage_change_summary:\\n  added: refreshed orientation\\n  updated: agent company report\\n  removed: stale private path link\\n  merged: duplicate branch notes\\n  left_unchanged: source evidence\\nnext_agent_requests: none\\nnext_state_machine_event: wiki.agent.reported\\n", encoding="utf-8")
with Path({json.dumps(str(codex_calls))}).open("a", encoding="utf-8") as handle:
    handle.write(json.dumps({{"argv": args, "stdin": sys.stdin.read()}}, sort_keys=True) + "\\n")
print(json.dumps({{"type": "message", "status": "completed"}}))
""",
        encoding="utf-8",
    )
    fake_codex.chmod(fake_codex.stat().st_mode | stat.S_IXUSR)

    fake_wiki_core = bin_dir / "onecontext-wiki"
    fake_wiki_core.write_text(
        f"""#!/usr/bin/env python3
import json
import sys
from pathlib import Path

args = sys.argv[1:]
command = args[args.index("--root") + 2]
with Path({json.dumps(str(wiki_calls))}).open("a", encoding="utf-8") as handle:
    handle.write(json.dumps({{"args": args}}, sort_keys=True) + "\\n")
if command == "agent-identify":
    thread_id = args[args.index("--thread-id") + 1]
    roles = [args[index + 1] for index, value in enumerate(args) if value == "--role"]
    print(json.dumps({{"status": "ok", "operation": "wiki.agent.identify", "agent": {{"agent_id": "agent-" + thread_id, "primary_address": "agent://codex/agent-" + thread_id, "granted_roles": roles}}}}))
elif command == "agent-inbox":
    agent_id = args[args.index("agent-inbox") + 1]
    print(json.dumps({{"status": "ok", "operation": "wiki.agent.inbox", "agent_id": agent_id, "message_count": 1, "deliveries": [{{"delivery_id": "delivery-1", "state": "open", "subject": "Prior curator note"}}]}}))
elif command == "talk-append":
    page = args[args.index("--page") + 1]
    print(json.dumps({{"status": "appended", "operation": "wiki.talk.append", "page_id": page, "delivery_mode": "mail"}}))
else:
    print(json.dumps({{"status": "ok", "command": command}}))
""",
        encoding="utf-8",
    )
    fake_wiki_core.chmod(fake_wiki_core.stat().st_mode | stat.S_IXUSR)

    monkeypatch.setenv("ONECONTEXT_AGENT_HARNESS_BIN", str(fake_harness))
    monkeypatch.setenv("PATH", f"{bin_dir}:{os.environ.get('PATH', '')}")

    result = run_wiki_update(
        system,
        provider="codex",
        run_id="agent-company-update",
        now=datetime(2026, 6, 5, 10, 30, tzinfo=timezone.utc),
        execute_agents=True,
        max_concurrent=4,
        timeout_seconds=30,
        runtime_root=tmp_path / "1Context Dev",
        wiki_core_bin=fake_wiki_core,
    )
    payload = result.to_payload(root=system.root)

    assert payload["status"] == "completed"
    assert payload["execute_agents"] is True
    assert payload["completed_count"] == payload["planned_count"]
    assert payload["phases"][2]["id"] == "birth_agent_units"
    assert payload["phases"][2]["born_count"] == payload["planned_count"]
    assert payload["phases"][3]["talk_receipt_count"] == payload["planned_count"]
    assert payload["wiki_synthesis"]["agent_report_count"] == payload["planned_count"]
    assert all(job["harness_call"] for job in payload["jobs"])
    assert all(job["agent_report_path"] for job in payload["jobs"])
    assert all(job["mail_context_path"] for job in payload["jobs"])
    assert all(job["harness_turn_complete"] for job in payload["jobs"])
    assert all(job["talk_receipt"]["operation"] == "wiki.talk.append" for job in payload["jobs"])
    assert sum(1 for line in harness_calls.read_text(encoding="utf-8").splitlines() if '"command": "call"' in line) == payload["planned_count"]
    codex_rows = [json.loads(line) for line in codex_calls.read_text(encoding="utf-8").splitlines()]
    assert len(codex_rows) == payload["planned_count"]
    assert all("## Wiki Mail Context" in row["stdin"] for row in codex_rows)
    assert all("Prior curator note" in row["stdin"] for row in codex_rows)
    wiki_rows = [json.loads(line) for line in wiki_calls.read_text(encoding="utf-8").splitlines()]
    assert sum('"agent-identify"' in json.dumps(row) for row in wiki_rows) == payload["planned_count"]
    assert sum('"agent-inbox"' in json.dumps(row) for row in wiki_rows) == payload["planned_count"]
    assert sum('"talk-append"' in json.dumps(row) for row in wiki_rows) == payload["planned_count"]
    talk_rows = [row for row in wiki_rows if "talk-append" in row["args"]]
    assert all("--thread-id" in row["args"] for row in talk_rows)
    assert all("--cc" in row["args"] and "mailbox://page/" in row["args"][row["args"].index("--cc") + 1] for row in talk_rows)
    assert all(row["args"][row["args"].index("--from") + 1].startswith("agent://codex/") for row in talk_rows)
    for_you_draft = next(draft for draft in payload["wiki_synthesis"]["drafts"] if draft["page_id"] == "for-you")
    for_you_body = Path(for_you_draft["draft_path"]).read_text(encoding="utf-8")
    assert "## Agent Company Reports" in for_you_body
    assert "## Agent Change Ledger" in for_you_body
    assert "fake codex adapter report" in for_you_body
    assert "proposed removing (removed): stale private path link" in for_you_body
    assert "proposed merging (merged): duplicate branch notes" in for_you_body
    assert "](/Users/" not in for_you_body
    assert "`/Users/paulhan/dev/1Context-private-2`" in for_you_body


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
    monkeypatch.delenv("ONECONTEXT_WIKI_SCRIBE_USABLE_CONTEXT_TOKENS", raising=False)
    monkeypatch.delenv("ONECONTEXT_WIKI_SCRIBE_CONTEXT_FRACTION", raising=False)
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
                    "text": "We wired the Perception DB session history into the wiki update path from [private evidence](/Users/paulhan/dev/1Context-private-2).",
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
    assert payload["wiki_memory_plan"]["mode"] == "catch_up_backfill"
    assert payload["wiki_memory_plan"]["usable_context_tokens"] == 258400
    assert payload["wiki_memory_plan"]["context_fraction"] == 0.62
    assert 159000 <= payload["wiki_memory_plan"]["target_packet_tokens"] <= 161000
    assert payload["wiki_memory_plan"]["selected_packet_count"] == 1
    assert payload["wiki_synthesis"]["source_store"] == "perception_db"
    assert payload["wiki_synthesis"]["source_event_count"] == 2
    packet = Path(payload["source_packet_path"]).read_text(encoding="utf-8")
    assert "Perception DB session history" in packet
    assert "Backfill Claude and Codex sessions" in packet
    assert payload["jobs"][0]["phase"] == "scribe_wave"
    assert payload["jobs"][0]["job_id"] == "memory.hourly.scribe"
    scribe_packet_path = Path(payload["jobs"][0]["plan"]["params"]["source_packet_path"])
    assert scribe_packet_path != Path(payload["source_packet_path"])
    scribe_packet = scribe_packet_path.read_text(encoding="utf-8")
    assert "Bounded Scribe Source Packet" in scribe_packet
    assert "Downstream editors, curators, biographers, librarians, and redactors should read scribe artifacts" in scribe_packet
    prompt = Path(payload["jobs"][0]["plan"]["paths"]["prompt"]).read_text(encoding="utf-8")
    assert "## Injected Session History" in prompt
    assert "Backfill Claude and Codex sessions" in prompt
    for_you_draft = next(draft for draft in payload["wiki_synthesis"]["drafts"] if draft["page_id"] == "for-you")
    for_you_body = Path(for_you_draft["draft_path"]).read_text(encoding="utf-8")
    assert "](/Users/" not in for_you_body
    assert "`/Users/paulhan/dev/1Context-private-2`" in for_you_body


def test_wiki_memory_plan_prioritizes_recent_three_days_then_backlog(tmp_path: Path) -> None:
    events = [
        fake_memory_event("2026-05-10T09:00:00Z", "oldest backlog", session="oldest"),
        fake_memory_event("2026-06-02T09:00:00Z", "near backlog", session="near"),
        fake_memory_event("2026-06-03T09:00:00Z", "recent day one", session="recent-1"),
        fake_memory_event("2026-06-05T09:00:00Z", "recent day three", session="recent-3"),
    ]

    plan = build_wiki_memory_plan(
        run_id="recent-first",
        update_dir=tmp_path / "update",
        events=events,
        cursor_name="wiki_backfill_30d_v1",
        window_days=30,
        cache_root=tmp_path / "cache",
        max_packets_per_run=4,
    )

    assert plan.selection_strategy == "recent_three_days_first_then_oldest_to_newest"
    assert plan.recent_priority_day_count == 3
    assert [packet.date for packet in plan.selected_packets] == [
        "2026-06-03",
        "2026-06-05",
        "2026-05-10",
        "2026-06-02",
    ]
    assert plan.to_payload()["selected_packets"][0]["date"] == "2026-06-03"


def test_execute_agents_runs_scribes_before_compact_downstream_packets(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    monkeypatch.setenv("ONECONTEXT_WIKI_MAX_SCRIBE_PACKETS_PER_RUN", "2")
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
    result = {"ok": True, "source_results": [{"source_type": "codex", "read_count": 2, "written_count": 2}]}
elif method == "memory.queryViewport":
    objects = []
    for idx, (event_time, session, role, text) in enumerate([
        ("2026-06-05T05:05:00Z", "codex-a", "user", "Please update the wiki from the new branch work."),
        ("2026-06-05T05:10:00Z", "codex-a", "assistant", "Updated branch notes and ran tests."),
        ("2026-06-05T06:00:00Z", "codex-b", "user", "This is a personal context preference for agents."),
    ], start=1):
        objects.append({
            "object_id": f"20000000-0000-0000-0000-{idx:012d}",
            "source_id": "10000000-0000-0000-0000-000000000001",
            "object_kind": "agent_message",
            "captured_at": event_time,
            "event_time": event_time,
            "payload": {
                "role": role,
                "text": text,
                "session_id": session,
                "cwd": "/Users/paulhan/dev/1context-public-launch",
                "project_key": "1context-public-launch",
                "source_uri": f"codex://session/{session}",
            },
        })
    result = {"ok": True, "objects": objects}
else:
    result = {"ok": False, "error": {"message": "unknown method"}}
print(json.dumps({"schema_version": 1, "protocol": "memory.query.v1", "surface": "perception_db", "status": "ok", "result": result}))
""",
        encoding="utf-8",
    )
    fake_memoryd.chmod(fake_memoryd.stat().st_mode | stat.S_IXUSR)

    waves: list[list[tuple[str, str, str]]] = []

    def fake_execute_launch_plans(_system, jobs, **_kwargs):
        waves.append(
            [
                (
                    job.phase,
                    job.job_id,
                    str(job.plan.params.get("source_packet_kind") or ""),
                )
                for job in jobs
            ]
        )
        completed = []
        for job in jobs:
            report_path = job.plan.run_dir / "final-message.md"
            report_path.parent.mkdir(parents=True, exist_ok=True)
            if job.job_id == "memory.daily.editor":
                output_path = Path(str(job.plan.params.get("output_path") or ""))
                output_path.parent.mkdir(parents=True, exist_ok=True)
                date = str(job.plan.params.get("date") or "2026-06-05")
                output_path.write_text(
                    f"---\n"
                    f"kind: proposal\n"
                    f"author: fake-daily-editor\n"
                    f"ts: {date}T23:59:00Z\n"
                    f"target-article: for-you.private.md\n"
                    f"target-section: {date}\n"
                    f"---\n\n"
                    f"You turned the {date} work into a real agent-authored daily memory section.\n",
                    encoding="utf-8",
                )
            report_path.write_text(
                f"status: completed\nevidence: fake report for {job.job_id}\nproposed_wiki_talk: posted\nnext_agent_requests: none\nnext_state_machine_event: {job.phase}.completed\n",
                encoding="utf-8",
            )
            completed.append(
                WikiUpdateJobResult(
                    phase=job.phase,
                    job_id=job.job_id,
                    run_id=job.run_id,
                    status="completed",
                    plan=job.plan,
                    returncode=0,
                    agent_report_path=report_path,
                    talk_receipt={"operation": "wiki.talk.append", "status": "appended", "delivery_mode": "mail"},
                )
            )
        return completed

    monkeypatch.setattr("onectx.memory.wiki_update.execute_launch_plans", fake_execute_launch_plans)

    result = run_wiki_update(
        system,
        provider="codex",
        run_id="wave-routing",
        now=datetime(2026, 6, 5, 10, 30, tzinfo=timezone.utc),
        execute_agents=True,
        max_concurrent=10,
        import_sources=True,
        import_ticks=1,
        source_window_days=30,
        memoryd_bin=fake_memoryd,
    )
    payload = result.to_payload(root=system.root)

    assert payload["status"] == "completed"
    assert payload["wiki_memory_plan"]["mode"] == "catch_up_backfill"
    assert payload["wiki_memory_plan"]["selected_packet_count"] == 2
    assert waves[0] == [
        ("scribe_wave", "memory.hourly.scribe", "perception_db_hour"),
        ("scribe_wave", "memory.hourly.scribe", "perception_db_hour"),
    ]
    assert all(kind != "perception_db_hour" for wave in waves[1:] for _, _, kind in wave)
    assert any(kind == "compact_scribe_artifacts" for wave in waves[1:] for _, _, kind in wave)
    assert any(kind == "compact_daily_and_scribe_artifacts" for wave in waves[1:] for _, _, kind in wave)
    assert any(job["phase"] == "daily_editor_wave" for job in payload["jobs"])
    assert any(job["phase"] in {"specialist_wave", "promotion_wave"} for job in payload["jobs"])
    for_you_draft = next(draft for draft in payload["wiki_synthesis"]["drafts"] if draft["page_id"] == "for-you")
    for_you_body = Path(for_you_draft["draft_path"]).read_text(encoding="utf-8")
    assert "## Daily Memory" in for_you_body
    assert "### 2026-06-05" in for_you_body
    assert "real agent-authored daily memory section" in for_you_body
    librarian_sweep_jobs = [job for job in payload["jobs"] if job["job_id"] == "memory.wiki.librarian_sweep"]
    assert librarian_sweep_jobs
    assert librarian_sweep_jobs[0]["plan"]["params"]["librarian_mode"] == "sweep"
    assert librarian_sweep_jobs[0]["plan"]["params"]["cleanup_priority"] == "aggressive_generated_junk_removal"
    assert "remove, merge, archive, or keep" in librarian_sweep_jobs[0]["plan"]["params"]["cleanup_contract"]


def test_memory_update_wiki_cli_shape_rejects_claude_provider() -> None:
    assert not is_memory_update_wiki_shape(["memory", "update-wiki", "--provider", "claude", "--json"])
