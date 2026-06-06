from __future__ import annotations

import json
import os
from dataclasses import replace
from pathlib import Path

from onectx.agent.launch_plan import build_agent_launch_plan
from onectx.config import load_system
from onectx.memory_core_cli import is_agent_launch_plan_shape, parse_agent_launch_plan_args


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def test_codex_launch_plan_materializes_prompt_and_run_script(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime")

    plan = build_agent_launch_plan(
        system,
        job_id="memory.hourly.scribe",
        provider="codex",
        run_id="test-codex-run",
        params={"date": "2026-06-05", "hour": "10"},
    )
    payload = plan.to_payload(root=system.root)

    assert payload["provider"] == "codex"
    assert payload["harness"]["id"] == "codex-harness"
    assert payload["model"] == "gpt-5.5"
    assert payload["model_policy"]["reasoning_effort"] == "high"
    assert payload["model_policy"]["usable_context_tokens"] == 258400
    assert payload["model_policy"]["context_fraction"] == 0.62
    assert payload["command"]["kind"] == "codex.exec"
    assert 'model_reasoning_effort="high"' in payload["command"]["argv"]
    assert payload["command"]["model_policy"]["reasoning_effort"] == "high"
    assert payload["agent_harness_call"]["operation"] == "agent.harness.call"
    assert payload["agent_harness_call"]["request"]["unit_id"] == "test-codex-run"
    assert payload["agent_harness_call"]["request"]["role"] == "hourly-scribe"
    assert payload["agent_harness_call"]["request"]["model_policy"]["reasoning_effort"] == "high"
    assert payload["agent_harness_call"]["request"]["metadata"]["model_policy"]["reasoning_effort"] == "high"
    capabilities = {item["id"]: item for item in payload["agent_harness_call"]["request"]["capabilities"]}
    assert capabilities["context_injection"]["proof_required"] == ["context_injection"]
    assert capabilities["prompt_packet"]["transport"] == "codex_skill"
    assert payload["command"]["env"]["CODEX_HOME"].endswith("test-codex-run/CODEX_HOME")
    assert payload["paths"]["run_script"].endswith("test-codex-run/run.sh")
    assert plan.prompt_path.is_file()
    assert plan.launch_path.is_file()
    assert os.access(plan.script_path, os.X_OK)

    prompt = plan.prompt_path.read_text(encoding="utf-8")
    assert "Job: `memory.hourly.scribe`" in prompt
    assert "Agent: `hourly-scribe`" in prompt
    assert "Hourly Scribe" in prompt
    assert '"date": "2026-06-05"' in prompt
    assert "## Agent Harness Birth Request" in prompt
    assert '"operation": "agent.harness.call"' in prompt

    launch = json.loads(plan.launch_path.read_text(encoding="utf-8"))
    assert launch["operation"] == "agent.launch.plan"
    assert launch["model_policy"]["reasoning_effort"] == "high"
    assert launch["paths"]["prompt"].endswith("test-codex-run/prompt.md")


def test_biographer_codex_launch_plan_uses_xhigh_model_policy(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime")

    plan = build_agent_launch_plan(
        system,
        job_id="memory.wiki.biographer",
        provider="codex",
        run_id="test-biographer-run",
    )
    payload = plan.to_payload(root=system.root)

    assert payload["agent"]["id"] == "biographer"
    assert payload["model"] == "gpt-5.5"
    assert payload["model_policy"]["reasoning_effort"] == "xhigh"
    assert 'model_reasoning_effort="xhigh"' in payload["command"]["argv"]
    assert payload["agent_harness_call"]["request"]["model_policy"]["reasoning_effort"] == "xhigh"
    capabilities = {item["id"]: item for item in payload["agent_harness_call"]["request"]["capabilities"]}
    assert capabilities["wiki_core"]["tool_names"] == ["wiki.talk.append"]


def test_curator_codex_launch_plan_gets_page_write_tools(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime")

    plan = build_agent_launch_plan(
        system,
        job_id="memory.wiki.for_you_curator",
        provider="codex",
        run_id="test-curator-run",
    )
    payload = plan.to_payload(root=system.root)

    capabilities = {item["id"]: item for item in payload["agent_harness_call"]["request"]["capabilities"]}
    assert capabilities["wiki_core"]["tool_names"] == [
        "wiki.page.write_body",
        "wiki.page.patch_body",
        "wiki.publish",
        "wiki.talk.append",
    ]


def test_librarian_sweep_is_cleanup_only_and_reports_removals(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime")

    plan = build_agent_launch_plan(
        system,
        job_id="memory.wiki.librarian_sweep",
        provider="codex",
        run_id="test-librarian-sweep-run",
    )
    payload = plan.to_payload(root=system.root)

    capabilities = {item["id"]: item for item in payload["agent_harness_call"]["request"]["capabilities"]}
    assert capabilities["wiki_core"]["tool_names"] == ["wiki.talk.append"]
    prompt = plan.prompt_path.read_text(encoding="utf-8")
    assert "remove / merge / archive / keep" in prompt
    assert "Removed-proposed" in prompt
    assert "page_change_summary (added, updated, removed, merged, left_unchanged)" in prompt


def test_claude_launch_plan_uses_declared_agent_model_and_harness(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))
    system = replace(load_system(PROJECT_ROOT), runtime_dir=tmp_path / "runtime")

    plan = build_agent_launch_plan(
        system,
        job_id="memory.hourly.scribe",
        provider="claude",
        run_id="test-claude-run",
    )
    payload = plan.to_payload(root=system.root)

    assert payload["provider"] == "claude"
    assert payload["harness"]["id"] == "claude-code"
    assert payload["model"] == "opus"
    assert payload["command"]["kind"] == "claude.print"
    assert "--output-format" in payload["command"]["argv"]
    assert "$(cat " in payload["command"]["shell"]
    assert plan.prompt_path.is_file()
    assert plan.script_path.read_text(encoding="utf-8").startswith("#!/usr/bin/env bash")


def test_agent_launch_plan_cli_shape_accepts_provider_model_and_params() -> None:
    args = [
        "agent",
        "launch-plan",
        "memory.hourly.scribe",
        "--provider",
        "codex",
        "--model",
        "gpt-5.5",
        "--run-id",
        "demo-run",
        "--param",
        "hour=10",
        "--json",
    ]

    assert is_agent_launch_plan_shape(args)
    parsed = parse_agent_launch_plan_args(args[2:-1])
    assert parsed == {
        "job_id": "memory.hourly.scribe",
        "provider": "codex",
        "model": "gpt-5.5",
        "run_id": "demo-run",
        "params": {"hour": "10"},
    }


def test_agent_launch_plan_cli_shape_accepts_declared_provider_default() -> None:
    args = [
        "agent",
        "launch-plan",
        "memory.hourly.scribe",
        "--json",
    ]

    assert is_agent_launch_plan_shape(args)
    parsed = parse_agent_launch_plan_args(args[2:-1])
    assert parsed == {
        "job_id": "memory.hourly.scribe",
        "provider": "declared",
        "model": "",
        "run_id": "",
        "params": {},
    }
