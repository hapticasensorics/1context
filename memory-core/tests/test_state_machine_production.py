from __future__ import annotations

import json
from pathlib import Path

from onectx.config import load_system
from onectx.state_machines.production import (
    compile_state_machine_artifacts,
    verify_state_machine_artifacts,
)


def test_state_machine_production_writes_ir_and_diagrams(tmp_path) -> None:
    system = load_system(Path.cwd())

    result = compile_state_machine_artifacts(system, output_dir=tmp_path, run_id="test-production")

    assert result.run_id == "test-production"
    manifest = json.loads((tmp_path / "manifest.json").read_text(encoding="utf-8"))
    assert "memory_system_fabric" in manifest["machines"]
    assert (tmp_path / "memory_system_fabric" / "memory_system_fabric.ir.json").is_file()
    assert (tmp_path / "memory_system_fabric" / "memory_system_fabric.cycle.mmd").is_file()
    assert (tmp_path / "wiki_reader_loop" / "wiki_reader_loop.wiki.mmd").is_file()


def test_state_machine_verification_passes_current_plugin(tmp_path) -> None:
    system = load_system(Path.cwd())

    result = verify_state_machine_artifacts(system, output_dir=tmp_path, run_id="test-verification")

    assert result.passed is True
    check_ids = {check["id"] for check in result.checks}
    assert "memory_system_fabric.runner_evidence_declared" in check_ids
    assert "memory_system_fabric.cycle_terminal_states" in check_ids
    assert (tmp_path / "checks.json").is_file()
    assert (tmp_path / "summary.md").is_file()
