#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="$(mktemp -d /tmp/1ctx-wiki-render-schema-runtime-XXXXXX)"
OUT_DIR="$(mktemp -d /tmp/1ctx-wiki-render-schema-output-XXXXXX)"
RESULT_JSON="$OUT_DIR/render-result.json"
FAILED_RESULT="$OUT_DIR/failed-result.json"

cleanup() {
  rm -rf "$RUNTIME_TEST"
  rm -rf "$OUT_DIR"
}
trap cleanup EXIT

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" >/tmp/1ctx-wiki-render-schema-init.out

SOURCE_ROOT="$RUNTIME_TEST/1Context/user-wiki/source"
SITE_OUT="$OUT_DIR/site"

node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$SOURCE_ROOT" \
  --output "$SITE_OUT" \
  --result-json "$RESULT_JSON" \
  >/tmp/1ctx-wiki-render-schema.out

if node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$OUT_DIR/missing-source" \
  --output "$OUT_DIR/missing-site" \
  --result-json "$FAILED_RESULT" \
  >/tmp/1ctx-wiki-render-schema-missing.out 2>/tmp/1ctx-wiki-render-schema-missing.err; then
  echo "render-site should fail for a missing source root" >&2
  exit 1
fi

ROOT="$ROOT" SITE_OUT="$SITE_OUT" RESULT_JSON="$RESULT_JSON" FAILED_RESULT="$FAILED_RESULT" uv run python - <<'PY'
from __future__ import annotations

import hashlib
import json
import os
import re
from pathlib import Path

root = Path(os.environ["ROOT"])
site = Path(os.environ["SITE_OUT"])
schemas = root / "wiki-engine" / "schemas"
result_path = Path(os.environ["RESULT_JSON"])
failed_result_path = Path(os.environ["FAILED_RESULT"])

schema_files = {
    "page-result.schema.json",
    "markdown-twin.schema.json",
    "route-manifest.schema.json",
    "content-index.schema.json",
    "render-result.schema.json",
    "render-success-result.schema.json",
    "render-error-result.schema.json",
}
for name in schema_files:
    doc = json.loads((schemas / name).read_text())
    assert doc["$schema"].startswith("https://json-schema.org/")
    assert doc["$id"].endswith(f"/{name}")

schema_text = "\n".join((schemas / name).read_text() for name in schema_files)
for ref in re.findall(r'"\\$ref"\\s*:\\s*"([^"]+)"', schema_text):
    if ref.startswith("#/"):
        continue
    assert (schemas / ref).exists(), f"schema ref is missing: {ref}"

result = json.loads(result_path.read_text())
assert result["schema_version"] == 1
assert result["status"] == "published"
assert result["route_manifest"] == ".1context/route-manifest.json"
assert result["content_index"] == ".1context/content-index.json"
assert result["route_count"] >= 8
assert result["markdown_twin_count"] >= 8

failed = json.loads(failed_result_path.read_text())
assert failed["schema_version"] == 1
assert failed["status"] == "failed"
assert "Source root does not exist" in failed["error"]
assert "route_manifest" not in failed
assert "content_index" not in failed

route_manifest = json.loads((site / result["route_manifest"]).read_text())
content_index = json.loads((site / result["content_index"]).read_text())

assert route_manifest["schema_version"] == "wiki.route-manifest.v1"
assert content_index["schema_version"] == "wiki.content-index.v1"
assert route_manifest["generated_at"] == result["rendered_at"]
assert content_index["generated_at"] == result["rendered_at"]
assert route_manifest["route_count"] == len(route_manifest["routes"]) == result["route_count"]
assert content_index["markdown_twin_count"] == len(content_index["markdown_twins"]) == result["markdown_twin_count"]
assert content_index["pages"] == route_manifest["routes"]
allowlist_text = "\n".join(content_index["export_allowlist"])
for forbidden in [
    "context-engine",
    "runtime-test",
    "source/families",
    "_curator",
    "_conventions",
    "prompts/",
    "observations/",
    "runs/",
    "artifacts/wiki/previews",
    "/Users/",
    "raw prompts",
]:
    assert forbidden not in allowlist_text, forbidden

routes = {entry["route"]: entry for entry in route_manifest["routes"]}
for expected in [
    "/for-you",
    "/for-you/talk",
    "/your-context",
    "/your-context/talk",
    "/projects",
    "/projects/talk",
    "/topics",
    "/topics/talk",
]:
    assert expected in routes, f"missing route: {expected}"

assert "/not-configured" not in routes

for route, entry in routes.items():
    assert route.startswith("/")
    assert entry["kind"] in {"page", "talk"}
    assert entry["access"] in {"private", "shared", "public"}
    assert not Path(entry["html_path"]).is_absolute()
    assert not Path(entry["markdown_path"]).is_absolute()
    assert ".." not in Path(entry["html_path"]).parts
    assert ".." not in Path(entry["markdown_path"]).parts
    html = site / entry["html_path"]
    md = site / entry["markdown_path"]
    assert html.exists(), f"missing html for {route}: {entry['html_path']}"
    assert md.exists(), f"missing markdown twin for {route}: {entry['markdown_path']}"
    html_text = html.read_text(errors="ignore")
    assert f'data-tier="{entry["access"]}"' in html_text
    if entry["route_index_path"]:
        assert (site / entry["route_index_path"]).exists(), f"missing route index for {route}"
    if entry["kind"] == "talk":
        assert route.endswith("/talk")
        assert entry["markdown_path"].endswith(".talk.md")

twins = {entry["path"]: entry for entry in content_index["markdown_twins"]}
for path, twin in twins.items():
    twin_path = site / path
    assert twin_path.exists(), path
    digest = hashlib.sha256(twin_path.read_bytes()).hexdigest()
    assert twin["sha256"] == digest
    assert twin["bytes"] == twin_path.stat().st_size
    assert twin["content_type"] == "text/markdown; charset=utf-8"
    assert twin["kind"] in {"page", "talk"}
    assert twin["access"] in {"private", "shared", "public"}
    if twin["html_path"]:
        assert (site / twin["html_path"]).exists()

for entry in route_manifest["routes"]:
    assert entry["markdown_path"] in twins
PY

echo "wiki renderer schema contract passed."
