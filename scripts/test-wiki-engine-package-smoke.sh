#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d /tmp/1ctx-wiki-engine-package-XXXXXX)"
PACKAGE="$WORK/wiki-engine"
FIXTURE_SOURCE="$WORK/fixture/1Context/user-wiki/source"
SITE_OUT="$WORK/site"
RESULT_JSON="$WORK/render-result.json"

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

mkdir -p "$PACKAGE"
(
  cd "$ROOT/wiki-engine"
  tar \
    --exclude './node_modules' \
    --exclude './.DS_Store' \
    -cf - .
) | (
  cd "$PACKAGE"
  tar -xf -
)

for forbidden in \
  "memory-core" \
  "1context-memory-core" \
  "wiki/menu" \
  "runtime-test" \
  "/Users/"; do
  if rg -n "$forbidden" "$PACKAGE" --glob '!package-lock.json' --glob '!node_modules/**'; then
    echo "wiki-engine package contains obsolete/runtime-coupled text: $forbidden" >&2
    exit 1
  fi
done

test ! -d "$PACKAGE/runtime"
test ! -d "$PACKAGE/runtime-test"
test ! -d "$PACKAGE/context-engine"

(
  cd "$PACKAGE"
  npm ci --ignore-scripts --offline
  npm test
)

mkdir -p "$FIXTURE_SOURCE/families/for-you/for-you/source"
mkdir -p "$FIXTURE_SOURCE/families/for-you/for-you/talk"
cp "$PACKAGE/tests/fixtures/for-you-2026-04-26.md" \
  "$FIXTURE_SOURCE/families/for-you/for-you/source/for-you-2026-04-26.md"
cp -R "$PACKAGE/tests/fixtures/basic.talk" \
  "$FIXTURE_SOURCE/families/for-you/for-you/talk/for-you-2026-04-26.talk"

(
  cd "$WORK"
  node "$PACKAGE/tools/render-site.mjs" \
    --source-root "$FIXTURE_SOURCE" \
    --output "$SITE_OUT" \
    --result-json "$RESULT_JSON" \
    >/tmp/1ctx-wiki-engine-package-render.out
)

test -f "$SITE_OUT/for-you-2026-04-26.html"
test -f "$SITE_OUT/for-you-2026-04-26/index.html"
test -f "$SITE_OUT/for-you-2026-04-26.md"
test -f "$SITE_OUT/for-you-2026-04-26.talk.html"
test -f "$SITE_OUT/for-you-2026-04-26/talk/index.html"
test -f "$SITE_OUT/for-you-2026-04-26.talk.md"
test -f "$SITE_OUT/.1context/route-manifest.json"
test -f "$SITE_OUT/.1context/content-index.json"

RESULT_JSON="$RESULT_JSON" SITE_OUT="$SITE_OUT" uv run python - <<'PY'
from __future__ import annotations

import json
import os
from pathlib import Path

result = json.loads(Path(os.environ["RESULT_JSON"]).read_text())
site = Path(os.environ["SITE_OUT"])
routes = json.loads((site / ".1context/route-manifest.json").read_text())["routes"]
route_set = {entry["route"] for entry in routes}

assert result["status"] == "published"
assert result["source_input_count"] == 1
assert result["talk_input_count"] == 1
assert "/for-you-2026-04-26" in route_set
assert "/for-you-2026-04-26/talk" in route_set
assert not any("runtime-test" in json.dumps(entry) for entry in routes)
PY

echo "wiki-engine package smoke passed."
