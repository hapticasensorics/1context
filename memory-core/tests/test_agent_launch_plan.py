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
    assert payload["command"]["kind"] == "codex.exec"
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

    launch = json.loads(plan.launch_path.read_text(encoding="utf-8"))
    assert launch["operation"] == "agent.launch.plan"
    assert launch["paths"]["prompt"].endswith("test-codex-run/prompt.md")


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
