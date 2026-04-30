from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
from pathlib import Path
from typing import Any
from urllib.parse import quote


DEFAULT_DB = Path("memory/runtime/raw-data/raw-data.sqlite")
ENV_DB = "ONECTX_RAW_DATA_DB"
MAX_LIMIT = 1000
READ_PREFIXES = ("select", "with", "pragma")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Read-only raw-data SQLite query tool")
    parser.add_argument("--db", help=f"SQLite database path; defaults to ${ENV_DB} or {DEFAULT_DB}")
    args = parser.parse_args(argv)

    try:
        payload = read_payload()
        db_path = resolve_db_path(args.db)
        result = run_query(db_path, payload)
    except ToolError as exc:
        print(json.dumps({"error": {"message": str(exc)}}), file=sys.stderr)
        return 1

    print(json.dumps(result, indent=2, sort_keys=True, default=str))
    return 0


def read_payload() -> dict[str, Any]:
    text = sys.stdin.read().strip()
    if not text:
        raise ToolError("expected JSON input on stdin")
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as exc:
        raise ToolError(f"invalid JSON input: {exc}") from exc
    if not isinstance(payload, dict):
        raise ToolError("input must be a JSON object")
    return payload


def resolve_db_path(value: str | None) -> Path:
    raw_path = value or os.environ.get(ENV_DB) or str(DEFAULT_DB)
    path = Path(raw_path).expanduser()
    if not path.is_absolute():
        path = Path.cwd() / path
    if not path.exists():
        raise ToolError(f"database does not exist: {path}")
    return path.resolve()


def run_query(db_path: Path, payload: dict[str, Any]) -> dict[str, Any]:
    sql = str(payload.get("sql", "")).strip()
    if not sql:
        raise ToolError("sql is required")
    if not sql.lower().startswith(READ_PREFIXES):
        raise ToolError("only SELECT, WITH, and PRAGMA statements are allowed")

    params = payload.get("params", [])
    if not isinstance(params, (list, dict)):
        raise ToolError("params must be an array or object")

    try:
        limit = int(payload.get("limit", 100))
    except (TypeError, ValueError) as exc:
        raise ToolError("limit must be an integer") from exc
    if limit < 1 or limit > MAX_LIMIT:
        raise ToolError(f"limit must be between 1 and {MAX_LIMIT}")

    uri = f"file:{quote(db_path.as_posix())}?mode=ro"
    try:
        with sqlite3.connect(uri, uri=True) as connection:
            connection.row_factory = sqlite3.Row
            cursor = connection.execute(sql, params)
            if cursor.description is None:
                raise ToolError("query did not return rows")
            columns = [str(item[0]) for item in cursor.description]
            fetched = cursor.fetchmany(limit + 1)
    except sqlite3.Error as exc:
        raise ToolError(f"sqlite error: {exc}") from exc

    truncated = len(fetched) > limit
    rows = [dict(row) for row in fetched[:limit]]
    return {
        "database": str(db_path),
        "columns": columns,
        "rows": rows,
        "row_count": len(rows),
        "truncated": truncated,
    }


class ToolError(RuntimeError):
    pass


if __name__ == "__main__":
    raise SystemExit(main())
