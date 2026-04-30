from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem, read_toml
from onectx.storage import LakeStore, stable_id, stable_json, utc_now


class MigrationError(RuntimeError):
    """Raised when memory contract migrations cannot be loaded or recorded."""


@dataclass(frozen=True)
class MigrationDefinition:
    migration_id: str
    title: str
    version: str
    kind: str
    description: str
    contract: str
    affects: tuple[str, ...]
    apply_mode: str
    verification: dict[str, Any]
    path: Path
    content_hash: str

    def to_payload(self) -> dict[str, Any]:
        return {
            "id": self.migration_id,
            "title": self.title,
            "version": self.version,
            "kind": self.kind,
            "description": self.description,
            "contract": self.contract,
            "affects": list(self.affects),
            "apply_mode": self.apply_mode,
            "verification": self.verification,
            "path": str(self.path),
            "content_hash": self.content_hash,
        }


@dataclass(frozen=True)
class MigrationReceipt:
    migration_id: str
    title: str
    status: str
    receipt_id: str
    run_id: str
    applied_at: str
    content_hash: str
    manifest_path: str
    durable_path: Path
    run_path: Path
    checks: tuple[dict[str, Any], ...]
    failures: tuple[str, ...]

    def to_payload(self) -> dict[str, Any]:
        return {
            "migration_id": self.migration_id,
            "title": self.title,
            "status": self.status,
            "receipt_id": self.receipt_id,
            "run_id": self.run_id,
            "applied_at": self.applied_at,
            "content_hash": self.content_hash,
            "manifest_path": self.manifest_path,
            "durable_path": str(self.durable_path),
            "run_path": str(self.run_path),
            "checks": list(self.checks),
            "failures": list(self.failures),
        }


@dataclass(frozen=True)
class MigrationRunResult:
    run_id: str
    status: str
    receipt_dir: Path
    receipts: tuple[MigrationReceipt, ...]
    artifact_id: str
    evidence_id: str
    event_id: str

    @property
    def applied_count(self) -> int:
        return sum(1 for receipt in self.receipts if receipt.status == "applied")

    @property
    def already_current_count(self) -> int:
        return sum(1 for receipt in self.receipts if receipt.status == "already_current")

    @property
    def failed_count(self) -> int:
        return sum(1 for receipt in self.receipts if receipt.status == "failed")

    def to_payload(self) -> dict[str, Any]:
        return {
            "run_id": self.run_id,
            "status": self.status,
            "receipt_dir": str(self.receipt_dir),
            "applied_count": self.applied_count,
            "already_current_count": self.already_current_count,
            "failed_count": self.failed_count,
            "artifact_id": self.artifact_id,
            "evidence_id": self.evidence_id,
            "event_id": self.event_id,
            "receipts": [receipt.to_payload() for receipt in self.receipts],
        }


def load_migration_definitions(system: MemorySystem) -> tuple[MigrationDefinition, ...]:
    migrations_dir = system.plugin_path / "migrations"
    if not migrations_dir.exists():
        return ()
    definitions: list[MigrationDefinition] = []
    seen: set[str] = set()
    for path in sorted(migrations_dir.glob("*.toml")):
        raw = read_toml(path)
        migration_id = required_str(raw, "id", path)
        if migration_id in seen:
            raise MigrationError(f"duplicate migration id {migration_id!r} in {path}")
        seen.add(migration_id)
        verification = raw.get("verification", {})
        if not isinstance(verification, dict):
            raise MigrationError(f"{path} [verification] must be a table")
        definitions.append(
            MigrationDefinition(
                migration_id=migration_id,
                title=required_str(raw, "title", path),
                version=str(raw.get("version", "0.1.0")),
                kind=str(raw.get("kind", "contract")),
                description=str(raw.get("description", "")),
                contract=str(raw.get("contract", "")),
                affects=tuple(str(item) for item in raw.get("affects", [])),
                apply_mode=str(raw.get("apply_mode", "receipt_only")),
                verification=verification,
                path=path,
                content_hash=sha256_text(path.read_text(encoding="utf-8")),
            )
        )
    return tuple(definitions)


def run_contract_migrations(system: MemorySystem, *, run_id: str = "") -> MigrationRunResult:
    resolved_run_id = run_id or stable_id("migration_run", utc_now())
    receipt_dir = system.runtime_dir / "migrations" / resolved_run_id
    receipt_dir.mkdir(parents=True, exist_ok=True)
    definitions = load_migration_definitions(system)
    receipts = tuple(run_migration_definition(system, definition, resolved_run_id) for definition in definitions)
    status = "failed" if any(receipt.status == "failed" for receipt in receipts) else "passed"

    payload = {
        "kind": "memory_contract_migrations.v1",
        "run_id": resolved_run_id,
        "status": status,
        "created_at": utc_now(),
        "plugin": system.active_plugin,
        "receipt_dir": str(receipt_dir),
        "summary": {
            "total": len(receipts),
            "applied": sum(1 for receipt in receipts if receipt.status == "applied"),
            "already_current": sum(1 for receipt in receipts if receipt.status == "already_current"),
            "failed": sum(1 for receipt in receipts if receipt.status == "failed"),
        },
        "receipts": [receipt.to_payload() for receipt in receipts],
    }
    summary_path = receipt_dir / "summary.json"
    write_json(summary_path, payload)
    content = stable_json(payload)
    content_hash = sha256_text(content)

    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact = store.append_artifact(
        "memory_contract_migrations",
        uri=summary_path.as_uri(),
        path=str(summary_path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=summary_path.stat().st_size,
        source="memory.migrations",
        state=status,
        text=f"memory contract migrations {resolved_run_id}: {status}",
        metadata={"run_id": resolved_run_id, "receipt_count": len(receipts)},
    )
    evidence = store.append_evidence(
        "contract_migrations.closed",
        artifact_id=artifact["artifact_id"],
        status=status,
        checker="memory.migrations",
        text=f"contract migrations {status}",
        checks=[
            {
                "id": "migration_receipts.closed",
                "status": status,
                "receipt_count": len(receipts),
            }
        ],
        payload=payload,
    )
    event = store.append_event(
        "memory.contract_migrations.closed",
        source="memory.migrations",
        kind="state_machine_evidence",
        subject=resolved_run_id,
        run_id=resolved_run_id,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence["evidence_id"],
        payload={
            "status": status,
            "applied": payload["summary"]["applied"],
            "already_current": payload["summary"]["already_current"],
            "failed": payload["summary"]["failed"],
        },
    )

    return MigrationRunResult(
        run_id=resolved_run_id,
        status=status,
        receipt_dir=receipt_dir,
        receipts=receipts,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence["evidence_id"],
        event_id=event["event_id"],
    )


def run_migration_definition(system: MemorySystem, definition: MigrationDefinition, run_id: str) -> MigrationReceipt:
    if definition.apply_mode != "receipt_only":
        raise MigrationError(f"unsupported migration apply_mode {definition.apply_mode!r} for {definition.migration_id}")
    durable_path = applied_receipt_path(system, definition.migration_id)
    run_path = migration_receipt_dir(system, run_id) / f"{safe_filename(definition.migration_id)}.json"
    checks, failures = verify_migration(system, definition)
    status = "failed"
    if not failures:
        status = "already_current" if durable_path.exists() else "applied"
    receipt = build_receipt(
        definition,
        run_id=run_id,
        status=status,
        durable_path=durable_path,
        run_path=run_path,
        checks=tuple(checks),
        failures=tuple(failures),
    )
    if status != "failed":
        write_json(durable_path, receipt.to_payload())
    write_json(run_path, receipt.to_payload())
    return receipt


def verify_migration(system: MemorySystem, definition: MigrationDefinition) -> tuple[list[dict[str, Any]], list[str]]:
    verification = definition.verification
    kind = str(verification.get("kind", "")).strip()
    if not kind:
        return [], [f"{definition.migration_id}: missing verification.kind"]
    if kind != "plugin_path_exists":
        return [], [f"{definition.migration_id}: unsupported verification.kind {kind!r}"]

    relative_path = str(verification.get("path", "")).strip()
    if not relative_path:
        return [], [f"{definition.migration_id}: plugin_path_exists requires verification.path"]
    target = system.plugin_path / relative_path
    check = {
        "id": "verification.plugin_path_exists",
        "kind": kind,
        "path": str(target),
        "status": "passed" if target.exists() else "failed",
    }
    failures = [] if target.exists() else [f"{definition.migration_id}: missing required plugin path {relative_path}"]
    return [check], failures


def build_receipt(
    definition: MigrationDefinition,
    *,
    run_id: str,
    status: str,
    durable_path: Path,
    run_path: Path,
    checks: tuple[dict[str, Any], ...],
    failures: tuple[str, ...],
) -> MigrationReceipt:
    return MigrationReceipt(
        migration_id=definition.migration_id,
        title=definition.title,
        status=status,
        receipt_id=stable_id("migration_receipt", definition.migration_id, run_id, status, definition.content_hash),
        run_id=run_id,
        applied_at=utc_now(),
        content_hash=definition.content_hash,
        manifest_path=str(definition.path),
        durable_path=durable_path,
        run_path=run_path,
        checks=checks,
        failures=failures,
    )


def migration_receipt_dir(system: MemorySystem, run_id: str) -> Path:
    return system.runtime_dir / "migrations" / run_id


def applied_receipt_path(system: MemorySystem, migration_id: str) -> Path:
    return system.runtime_dir / "migrations" / ".applied" / f"{safe_filename(migration_id)}.json"


def required_str(raw: dict[str, Any], key: str, path: Path) -> str:
    value = str(raw.get(key, "")).strip()
    if not value:
        raise MigrationError(f"{path} must set {key}")
    return value


def safe_filename(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "-", value).strip("-") or "migration"


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
