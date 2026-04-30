from __future__ import annotations

import sys
from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.memory.runner import (
    ArtifactSpec,
    HarnessLaunchSpec,
    HiredAgentExecutionSpec,
    execute_hired_agents,
    parse_duration_seconds,
)


def test_duration_parser_accepts_runtime_policy_units() -> None:
    assert parse_duration_seconds("45s") == 45
    assert parse_duration_seconds("30m") == 1800
    assert parse_duration_seconds("1h") == 3600
    assert parse_duration_seconds(2) == 2


def test_hired_agent_runner_times_out_stuck_harness(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    workspace = tmp_path / "workspace"
    spec = HiredAgentExecutionSpec(
        run_id="timeout-proof",
        job_ids=("memory.hourly.scribe",),
        job_params={},
        experience_packet={"loaded_at_birth": True},
        prompt="This harness intentionally sleeps longer than the timeout.",
        workspace=workspace,
        artifact=ArtifactSpec(kind="timeout-proof", path=workspace / "output.md"),
        harness_launch=HarnessLaunchSpec(
            harness="test-harness",
            isolation_mode="test",
            argv=[sys.executable, "-c", "import time; time.sleep(5)"],
            cwd=tmp_path,
        ),
        run_harness=True,
        harness_timeout_seconds=1,
    )

    batch = execute_hired_agents(system, (spec,), max_concurrent=1, run_id="timeout-proof")

    assert batch.ok is False
    assert batch.errors[0]["error_type"] == "HiredAgentRunnerError"
    assert "timed out after 1 seconds" in batch.errors[0]["message"]
    stderr_files = list((system.runtime_dir / "experiences").glob("*/harness.stderr.log"))
    assert stderr_files
    assert "timed out after 1 seconds" in stderr_files[0].read_text(encoding="utf-8")
