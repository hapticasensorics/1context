from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.memory.migrations import load_migration_definitions, run_contract_migrations
from onectx.memory.tick import load_memory_cycle, run_memory_tick, validate_memory_cycle


def isolated_system(tmp_path: Path):
    system = load_system(Path.cwd())
    return replace(
        system,
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )


def test_plugin_migration_manifests_load() -> None:
    system = load_system(Path.cwd())

    definitions = load_migration_definitions(system)

    ids = {definition.migration_id for definition in definitions}
    assert "2026-04-29-concept-frontmatter-schema-v1" in ids
    assert "2026-04-29-hourly-block-role-backfill-v1" in ids
    assert all(definition.verification.get("kind") for definition in definitions)


def test_contract_migration_receipts_are_idempotent(tmp_path: Path) -> None:
    system = isolated_system(tmp_path)

    first = run_contract_migrations(system, run_id="migration-idempotent-first")
    second = run_contract_migrations(system, run_id="migration-idempotent-second")

    assert first.status == "passed"
    assert first.applied_count >= 1
    assert first.failed_count == 0
    assert second.status == "passed"
    assert second.already_current_count == len(second.receipts)
    assert second.failed_count == 0
    for receipt in second.receipts:
        assert receipt.durable_path.is_file()
        assert receipt.run_path.is_file()
        payload = json.loads(receipt.run_path.read_text(encoding="utf-8"))
        assert payload["status"] == "already_current"


def test_missing_migration_verification_fails_loud(tmp_path: Path) -> None:
    system = isolated_system(tmp_path)
    plugin_path = tmp_path / "plugin"
    migrations_dir = plugin_path / "migrations"
    migrations_dir.mkdir(parents=True)
    (migrations_dir / "bad.toml").write_text(
        """
id = "bad-migration"
title = "Bad migration"
version = "0.1.0"
kind = "schema"
description = "fixture"
contract = "fixture"
affects = ["fixture"]
apply_mode = "receipt_only"

[verification]
kind = "plugin_path_exists"
path = "missing-file.md"
""".strip()
        + "\n",
        encoding="utf-8",
    )
    system = replace(system, plugin_path=plugin_path)

    result = run_contract_migrations(system, run_id="migration-missing-verification")

    assert result.status == "failed"
    assert result.failed_count == 1
    assert result.receipts[0].status == "failed"
    assert "missing required plugin path" in result.receipts[0].failures[0]
    assert not result.receipts[0].durable_path.exists()
    assert result.receipts[0].run_path.is_file()


def test_memory_tick_can_run_contract_migrations(tmp_path: Path) -> None:
    system = isolated_system(tmp_path)

    result = run_memory_tick(
        system,
        wiki_only=True,
        execute_migrations=True,
        cycle_id="migration-tick",
    )

    assert result.status == "completed"
    payload = load_memory_cycle(system, "migration-tick")
    assert payload["contract_migrations"]["status"] == "passed"
    assert payload["contract_migrations"]["applied_count"] >= 1
    assert payload["steps"][0]["id"] == "contract_migrations"
    assert payload["steps"][0]["status"] == "passed"
    validation = validate_memory_cycle(system, "migration-tick")
    assert validation.passed is True
    assert "evidence.contract_migrations_closed" in {check["id"] for check in validation.checks}
