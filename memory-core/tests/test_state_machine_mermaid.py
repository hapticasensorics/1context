from __future__ import annotations

from pathlib import Path

from onectx.config import compile_system_map, load_system
from onectx.state_machines.mermaid import state_machine_to_mermaid


def test_wiki_growth_fabric_renders_mermaid_from_ir() -> None:
    system = load_system(Path.cwd())
    machine = compile_system_map(system)["state_machines"]["wiki_growth_fabric"]

    source = state_machine_to_mermaid(machine, scope_name="corpus")

    assert "flowchart LR" in source
    assert "corpus_idle" in source
    assert "corpus_scanning" in source
    assert "wiki.fabric.tick" in source
    assert "step: scan_wiki_inventory" in source
    assert "parallel: max runtime_policy.max_concurrent_agents" in source
    assert "spawn: memory.wiki.redactor" in source


def test_memory_system_fabric_renders_top_level_memory_loop() -> None:
    system = load_system(Path.cwd())
    machine = compile_system_map(system)["state_machines"]["memory_system_fabric"]

    source = state_machine_to_mermaid(machine, scope_name="cycle")

    assert "cycle_ingesting" in source
    assert "cycle_migrating_contracts" in source
    assert "cycle_rendering_experience" in source
    assert "cycle_birthing_agents" in source
    assert "step: run_contract_migrations_and_backfills" in source
    assert "step: render_braided_block_experience" in source
    assert "spawn: memory.hourly.block_scribe" in source
    assert "step: launch_claude_account_clean_harness" in source
    assert "step: run_runtime_invariant_preflight_postflight_diff" in source
    assert "step: run_wiki_growth_fabric" in source
    assert "step: run_wiki_reader_loop" in source
    assert "step: render_wiki_engine_families" in source


def test_wiki_reader_loop_renders_engine_stage_from_ir() -> None:
    system = load_system(Path.cwd())
    machine = compile_system_map(system)["state_machines"]["wiki_reader_loop"]

    source = state_machine_to_mermaid(machine, scope_name="wiki")

    assert "Wiki Reader Loop / wiki IR" in source
    assert "wiki_building_inputs" in source
    assert "wiki_rendering" in source
    assert "step: run_wiki_engine_render" in source
    assert "step: write_site_manifest_and_content_index" in source
    assert "expect: wiki_render.ready" in source


def test_memory_system_fabric_renders_replay_scope() -> None:
    system = load_system(Path.cwd())
    machine = compile_system_map(system)["state_machines"]["memory_system_fabric"]

    source = state_machine_to_mermaid(machine, scope_name="replay")

    assert "Memory System Fabric / replay IR" in source
    assert "replay_loading_events" in source
    assert "replay_snapshotting" in source
    assert "replay_injecting_failure" in source
    assert "step: derive_replay_fire_schedule" in source
    assert "step: execute_replay_fires" in source
    assert "step: capture_replay_wiki_snapshot" in source
    assert "step: apply_replay_failure_injection" in source
    assert "memory.real_time_policy_evidence.ready" in source


def test_wiki_growth_fabric_renders_page_governance_scope() -> None:
    system = load_system(Path.cwd())
    machine = compile_system_map(system)["state_machines"]["wiki_growth_fabric"]

    source = state_machine_to_mermaid(machine, scope_name="page")

    assert "Wiki Growth Fabric / page IR" in source
    assert "page_needs_curator" in source
    assert "page_curator_adjudicating" in source
    assert "page_needs_migration" in source
    assert "page_curator_adjudication_needed" in source
    assert "step: assign_page_curator_jurisdiction" in source
    assert "step: write_page_migration_or_backfill_receipt" in source
