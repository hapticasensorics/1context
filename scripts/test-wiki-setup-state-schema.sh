#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_RUNTIME="$(mktemp -d /tmp/1ctx-wiki-setup-default-XXXXXX)"
IMPORT_RUNTIME="$(mktemp -d /tmp/1ctx-wiki-setup-import-XXXXXX)"
FIXTURE_HOME="$(mktemp -d /tmp/1ctx-wiki-setup-fixture-XXXXXX)"

cleanup() {
  rm -rf "$DEFAULT_RUNTIME"
  rm -rf "$IMPORT_RUNTIME"
  rm -rf "$FIXTURE_HOME"
}
trap cleanup EXIT

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$DEFAULT_RUNTIME" >/tmp/1ctx-wiki-setup-default.out

FIXTURE_PAGE="$FIXTURE_HOME/1Context/user-wiki/source/families/schema-fixture/schema-page"
mkdir -p "$FIXTURE_PAGE/source"
printf 'title = "Schema Fixture"\n' > "$FIXTURE_HOME/1Context/user-wiki/source/families/schema-fixture/group.toml"
printf 'title = "Schema Page"\n' > "$FIXTURE_PAGE/family.toml"
printf '# Schema Page\n\nFixture import for setup-state validation.\n' > "$FIXTURE_PAGE/source/schema-page.md"
"$ROOT/scripts/init-dev-wiki-runtime.sh" "$IMPORT_RUNTIME" "$FIXTURE_HOME" >/tmp/1ctx-wiki-setup-import.out

DEFAULT_RUNTIME="$DEFAULT_RUNTIME" IMPORT_RUNTIME="$IMPORT_RUNTIME" uv run python - <<'PY'
from __future__ import annotations

import datetime as dt
import os
import re
import tomllib
from pathlib import Path

HASH_RE = re.compile(r"^[0-9a-f]{64}$")
MATERIALIZE_PAGE_STATUSES = {"materialized", "disabled", "tombstoned"}
MATERIALIZE_FILE_STATUSES = {"installed", "unchanged", "skipped_existing", "tombstoned"}
IMPORT_FILE_STATUSES = {"installed", "unchanged", "skipped_modified"}


def parse_toml(path: Path) -> dict:
    if not path.exists():
        raise AssertionError(f"missing setup state: {path}")
    with path.open("rb") as handle:
        return tomllib.load(handle)


def assert_iso_utc(value: object, label: str) -> None:
    if not isinstance(value, str):
        raise AssertionError(f"{label} must be a string")
    try:
        dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise AssertionError(f"{label} must be ISO datetime: {value!r}") from exc


def assert_rel_runtime_path(value: object, label: str) -> None:
    if not isinstance(value, str) or not value:
        raise AssertionError(f"{label} must be a non-empty string")
    if value.startswith("/") or "\\" in value or "/../" in f"/{value}/" or value.startswith("../"):
        raise AssertionError(f"{label} must be a safe relative runtime path: {value!r}")
    if not (value.startswith("1Context/") or value.startswith("Library/")):
        raise AssertionError(f"{label} must live under 1Context/ or Library/: {value!r}")


def assert_hash(value: object, label: str) -> None:
    if not isinstance(value, str) or not HASH_RE.match(value):
        raise AssertionError(f"{label} must be a sha256 hex digest")


def validate_materialize(root: Path) -> None:
    path = root / "Library/Application Support/1Context/setup/wiki-page-materialize.toml"
    state = parse_toml(path)
    if state.get("schema_version") != 1:
        raise AssertionError("wiki-page-materialize schema_version must be 1")
    assert_iso_utc(state.get("materialized_at"), "materialized_at")
    assert_rel_runtime_path(state.get("wiki_config"), "wiki_config")
    assert_hash(state.get("wiki_config_hash"), "wiki_config_hash")

    pages = state.get("pages")
    if not isinstance(pages, list) or not pages:
        raise AssertionError("wiki-page-materialize pages must be a non-empty list")
    page_ids = set()
    for idx, page in enumerate(pages):
        if not isinstance(page, dict):
            raise AssertionError(f"pages[{idx}] must be a table")
        page_id = page.get("id")
        if not isinstance(page_id, str) or not page_id:
            raise AssertionError(f"pages[{idx}].id must be a non-empty string")
        page_ids.add(page_id)
        route = page.get("route")
        if not isinstance(route, str) or not route.startswith("/"):
            raise AssertionError(f"pages[{idx}].route must be an absolute route")
        if page.get("status") not in MATERIALIZE_PAGE_STATUSES:
            raise AssertionError(f"pages[{idx}].status has unexpected value: {page.get('status')!r}")
    for expected in {"for-you", "your-context", "projects", "topics"}:
        if expected not in page_ids:
            raise AssertionError(f"wiki-page-materialize missing configured page {expected}")

    files = state.get("files")
    if not isinstance(files, list) or not files:
        raise AssertionError("wiki-page-materialize files must be a non-empty list")
    for idx, file in enumerate(files):
        if not isinstance(file, dict):
            raise AssertionError(f"files[{idx}] must be a table")
        assert_rel_runtime_path(file.get("path"), f"files[{idx}].path")
        template = file.get("source_template")
        if not isinstance(template, str) or not template:
            raise AssertionError(f"files[{idx}].source_template must be non-empty")
        assert_hash(file.get("source_hash"), f"files[{idx}].source_hash")
        assert_hash(file.get("installed_hash"), f"files[{idx}].installed_hash")
        if file.get("status") not in MATERIALIZE_FILE_STATUSES:
            raise AssertionError(f"files[{idx}].status has unexpected value: {file.get('status')!r}")


def validate_import(root: Path) -> None:
    path = root / "Library/Application Support/1Context/setup/dev-user-data-import.toml"
    state = parse_toml(path)
    if state.get("schema_version") != 1:
        raise AssertionError("dev-user-data-import schema_version must be 1")
    assert_iso_utc(state.get("imported_at"), "imported_at")
    source_root = state.get("source_root")
    if not isinstance(source_root, str) or not source_root.startswith("/"):
        raise AssertionError("source_root must be an absolute local path in the dev import ledger")
    files = state.get("files")
    if not isinstance(files, list) or not files:
        raise AssertionError("dev-user-data-import files must be a non-empty list")
    for idx, file in enumerate(files):
        if not isinstance(file, dict):
            raise AssertionError(f"import files[{idx}] must be a table")
        assert_rel_runtime_path(file.get("path"), f"import files[{idx}].path")
        assert_hash(file.get("source_hash"), f"import files[{idx}].source_hash")
        assert_hash(file.get("installed_hash"), f"import files[{idx}].installed_hash")
        if file.get("status") not in IMPORT_FILE_STATUSES:
            raise AssertionError(f"import files[{idx}].status has unexpected value: {file.get('status')!r}")


default_root = Path(os.environ["DEFAULT_RUNTIME"])
import_root = Path(os.environ["IMPORT_RUNTIME"])
validate_materialize(default_root)
validate_materialize(import_root)
if (default_root / "Library/Application Support/1Context/setup/dev-user-data-import.toml").exists():
    raise AssertionError("default runtime should not write dev-user-data-import.toml without an import source")
validate_import(import_root)
PY

echo "wiki setup-state schema smoke passed."
