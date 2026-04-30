from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import lancedb
import pyarrow as pa


TABLE_ORDER = ("events", "sessions", "artifacts", "evidence", "documents")

TABLE_SCHEMAS: dict[str, pa.Schema] = {
    "events": pa.schema(
        [
            ("event_id", pa.string()),
            ("hash", pa.string()),
            ("session_id", pa.string()),
            ("ts", pa.string()),
            ("event", pa.string()),
            ("source", pa.string()),
            ("kind", pa.string()),
            ("actor", pa.string()),
            ("subject", pa.string()),
            ("cwd", pa.string()),
            ("char_count", pa.int64()),
            ("state_machine", pa.string()),
            ("scope", pa.string()),
            ("hired_agent_uuid", pa.string()),
            ("run_id", pa.string()),
            ("artifact_id", pa.string()),
            ("evidence_id", pa.string()),
            ("text", pa.string()),
            ("payload_json", pa.string()),
        ]
    ),
    "sessions": pa.schema(
        [
            ("session_id", pa.string()),
            ("source", pa.string()),
            ("cwd", pa.string()),
            ("first_ts", pa.string()),
            ("last_ts", pa.string()),
            ("event_count", pa.int64()),
            ("metadata_json", pa.string()),
        ]
    ),
    "artifacts": pa.schema(
        [
            ("artifact_id", pa.string()),
            ("ts", pa.string()),
            ("kind", pa.string()),
            ("uri", pa.string()),
            ("path", pa.string()),
            ("content_type", pa.string()),
            ("content_hash", pa.string()),
            ("bytes", pa.int64()),
            ("source", pa.string()),
            ("state", pa.string()),
            ("text", pa.string()),
            ("metadata_json", pa.string()),
        ]
    ),
    "evidence": pa.schema(
        [
            ("evidence_id", pa.string()),
            ("ts", pa.string()),
            ("artifact_id", pa.string()),
            ("check_id", pa.string()),
            ("status", pa.string()),
            ("checker", pa.string()),
            ("text", pa.string()),
            ("checks_json", pa.string()),
            ("payload_json", pa.string()),
        ]
    ),
    "documents": pa.schema(
        [
            ("document_id", pa.string()),
            ("ts", pa.string()),
            ("kind", pa.string()),
            ("uri", pa.string()),
            ("path", pa.string()),
            ("title", pa.string()),
            ("text", pa.string()),
            ("source", pa.string()),
            ("metadata_json", pa.string()),
        ]
    ),
}


class StorageError(RuntimeError):
    """Raised when the local lakestore cannot be opened or written."""


@dataclass(frozen=True)
class LakeStore:
    """Tiny LanceDB-backed lake store for runtime records.

    The store deliberately starts as a few append-friendly tables. Exact table
    schemas live here so state-machine experiments have one stable place to
    write events, artifacts, and evidence.
    """

    path: Path

    def connect(self):
        self.path.mkdir(parents=True, exist_ok=True)
        return lancedb.connect(self.path)

    def ensure(self) -> dict[str, int | None]:
        db = self.connect()
        for table_name in TABLE_ORDER:
            ensure_table(db, table_name)
        return self.counts()

    def counts(self) -> dict[str, int | None]:
        db = self.connect()
        existing = set(list_tables(db))
        counts: dict[str, int | None] = {}
        for table_name in TABLE_ORDER:
            if table_name not in existing:
                counts[table_name] = None
                continue
            counts[table_name] = db.open_table(table_name).count_rows()
        return counts

    def append_event(self, event: str, **values: Any) -> dict[str, Any]:
        row = self.event_row(event, **values)
        self.append_rows("events", [row])
        return row

    def event_row(self, event: str, **values: Any) -> dict[str, Any]:
        payload = values.pop("payload", None)
        row = normalize_row(
            "events",
            {
                "ts": values.pop("ts", None) or utc_now(),
                "event": event,
                "payload_json": stable_json(payload if payload is not None else values.pop("payload_json", {})),
                **values,
            },
        )
        if not row["event_id"]:
            row["event_id"] = stable_id("event", row["ts"], row["event"], row["source"], row["payload_json"])
        return row

    def append_artifact(self, kind: str, **values: Any) -> dict[str, Any]:
        row = self.artifact_row(kind, **values)
        self.append_rows("artifacts", [row])
        return row

    def artifact_row(self, kind: str, **values: Any) -> dict[str, Any]:
        metadata = values.pop("metadata", None)
        row = normalize_row(
            "artifacts",
            {
                "ts": values.pop("ts", None) or utc_now(),
                "kind": kind,
                "metadata_json": stable_json(metadata if metadata is not None else values.pop("metadata_json", {})),
                **values,
            },
        )
        if not row["artifact_id"]:
            row["artifact_id"] = stable_id(
                "artifact",
                row["ts"],
                row["kind"],
                row["uri"],
                row["path"],
                row["content_hash"],
            )
        return row

    def append_session(self, session_id: str, **values: Any) -> dict[str, Any]:
        row = self.session_row(session_id, **values)
        self.append_rows("sessions", [row])
        return row

    def session_row(self, session_id: str, **values: Any) -> dict[str, Any]:
        metadata = values.pop("metadata", None)
        return normalize_row(
            "sessions",
            {
                "session_id": session_id,
                "metadata_json": stable_json(metadata if metadata is not None else values.pop("metadata_json", {})),
                **values,
            },
        )

    def replace_session(self, session_id: str, **values: Any) -> dict[str, Any]:
        metadata = values.pop("metadata", None)
        row = normalize_row(
            "sessions",
            {
                "session_id": session_id,
                "metadata_json": stable_json(metadata if metadata is not None else values.pop("metadata_json", {})),
                **values,
            },
        )
        db = self.connect()
        table = ensure_table(db, "sessions")
        table.delete(f"session_id = {sql_literal(row['session_id'])}")
        table.add([row])
        return row

    def append_evidence(self, check_id: str, *, artifact_id: str = "", status: str = "passed", **values: Any) -> dict[str, Any]:
        checks = values.pop("checks", None)
        payload = values.pop("payload", None)
        row = normalize_row(
            "evidence",
            {
                "ts": values.pop("ts", None) or utc_now(),
                "artifact_id": artifact_id,
                "check_id": check_id,
                "status": status,
                "checks_json": stable_json(checks if checks is not None else values.pop("checks_json", [])),
                "payload_json": stable_json(payload if payload is not None else values.pop("payload_json", {})),
                **values,
            },
        )
        if not row["evidence_id"]:
            row["evidence_id"] = stable_id("evidence", row["ts"], row["artifact_id"], row["check_id"], row["status"])
        self.append_rows("evidence", [row])
        return row

    def append_document(self, kind: str, **values: Any) -> dict[str, Any]:
        metadata = values.pop("metadata", None)
        row = normalize_row(
            "documents",
            {
                "ts": values.pop("ts", None) or utc_now(),
                "kind": kind,
                "metadata_json": stable_json(metadata if metadata is not None else values.pop("metadata_json", {})),
                **values,
            },
        )
        if not row["document_id"]:
            row["document_id"] = stable_id("document", row["ts"], row["kind"], row["uri"], row["path"], row["title"])
        self.append_rows("documents", [row])
        return row

    def append_rows(self, table_name: str, rows: list[dict[str, Any]]) -> None:
        if not rows:
            return
        db = self.connect()
        table = ensure_table(db, table_name)
        table.add([normalize_row(table_name, row) for row in rows])

    def replace_rows(self, table_name: str, key_field: str, rows: list[dict[str, Any]]) -> None:
        if not rows:
            return
        db = self.connect()
        table = ensure_table(db, table_name)
        normalized = [normalize_row(table_name, row) for row in rows]
        if hasattr(table, "merge_insert"):
            table.merge_insert(key_field).when_matched_update_all().when_not_matched_insert_all().execute(normalized)
            return
        for row in rows:
            table.delete(f"{key_field} = {sql_literal(row[key_field])}")
        table.add(normalized)

    def rows(self, table_name: str, *, limit: int = 20) -> list[dict[str, Any]]:
        if table_name not in TABLE_SCHEMAS:
            raise StorageError(f"unknown lakestore table {table_name!r}")
        db = self.connect()
        if table_name not in set(list_tables(db)):
            return []
        rows = db.open_table(table_name).to_arrow().to_pylist()
        return rows[-limit:] if limit else rows

    def row_by_value(self, table_name: str, field_name: str, value: Any) -> dict[str, Any] | None:
        for row in self.rows(table_name, limit=0):
            if str(row.get(field_name) or "") == str(value):
                return row
        return None

    def column_values(self, table_name: str, field_name: str) -> set[str]:
        if table_name not in TABLE_SCHEMAS:
            raise StorageError(f"unknown lakestore table {table_name!r}")
        db = self.connect()
        if table_name not in set(list_tables(db)):
            return set()
        table = db.open_table(table_name)
        try:
            column = table.to_arrow(columns=[field_name]).column(field_name)
        except TypeError:
            column = table.to_arrow().select([field_name]).column(field_name)
        return {str(value) for value in column.to_pylist() if value}

    def search(self, table_name: str, query: str, *, limit: int = 20) -> list[dict[str, Any]]:
        needle = query.casefold()
        matches: list[dict[str, Any]] = []
        for row in self.rows(table_name, limit=0):
            haystack = stable_json(row).casefold()
            if needle in haystack:
                matches.append(row)
        return matches[-limit:] if limit else matches

    def snapshot(self, *, limit: int = 250) -> dict[str, Any]:
        counts = self.counts()
        return {
            "generated_at": utc_now(),
            "storage_dir": str(self.path),
            "tables": {
                table_name: {
                    "count": counts.get(table_name),
                    "rows": self.rows(table_name, limit=limit),
                }
                for table_name in TABLE_ORDER
            },
        }


def ensure_table(db: Any, table_name: str):
    if table_name not in TABLE_SCHEMAS:
        raise StorageError(f"unknown lakestore table {table_name!r}")
    if table_name in set(list_tables(db)):
        return db.open_table(table_name)
    return db.create_table(table_name, schema=TABLE_SCHEMAS[table_name])


def list_tables(db: Any) -> list[str]:
    if hasattr(db, "list_tables"):
        response = db.list_tables()
        if hasattr(response, "tables"):
            return [str(name) for name in response.tables]
        return [str(name) for name in response]
    return list(db.table_names())


def normalize_row(table_name: str, values: dict[str, Any]) -> dict[str, Any]:
    schema = TABLE_SCHEMAS[table_name]
    row: dict[str, Any] = {}
    for field in schema:
        value = values.get(field.name)
        if pa.types.is_integer(field.type):
            row[field.name] = int(value or 0)
        elif field.name.endswith("_json"):
            row[field.name] = stable_json(value)
        else:
            row[field.name] = "" if value is None else str(value)
    return row


def stable_json(value: Any) -> str:
    if value is None:
        value = {}
    if isinstance(value, str):
        return value
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def stable_id(prefix: str, *parts: Any) -> str:
    digest = hashlib.sha256("\x1f".join(str(part) for part in parts).encode("utf-8")).hexdigest()[:24]
    return f"{prefix}_{digest}"


def sql_literal(value: Any) -> str:
    return "'" + str(value).replace("'", "''") + "'"


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def storage_dir_path(runtime_dir: Path) -> Path:
    if runtime_dir.name == "runtime" and runtime_dir.parent.name == "memory":
        return runtime_dir.parent.parent / "storage" / "lakestore"
    return runtime_dir / "lakestore"
