#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="$(mktemp -d /tmp/1ctx-wiki-render-site-runtime-XXXXXX)"
OUT_DIR="$(mktemp -d /tmp/1ctx-wiki-render-site-output-XXXXXX)"
BEFORE_HASHES="$OUT_DIR/source-before.sha256"
AFTER_HASHES="$OUT_DIR/source-after.sha256"
RESULT_JSON="$OUT_DIR/render-result.json"

cleanup() {
  rm -rf "$RUNTIME_TEST"
  rm -rf "$OUT_DIR"
}
trap cleanup EXIT

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" >/tmp/1ctx-wiki-render-site-init.out

SOURCE_ROOT="$RUNTIME_TEST/1Context/user-wiki/source"
SITE_OUT="$OUT_DIR/site"

hash_source_tree() {
  (
    cd "$SOURCE_ROOT"
    find . -type f -print0 | sort -z | xargs -0 shasum -a 256
  )
}

hash_source_tree >"$BEFORE_HASHES"

node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$SOURCE_ROOT" \
  --output "$SITE_OUT" \
  --result-json "$RESULT_JSON" \
  >/tmp/1ctx-wiki-render-site.out

hash_source_tree >"$AFTER_HASHES"
if ! cmp -s "$BEFORE_HASHES" "$AFTER_HASHES"; then
  echo "render-site mutated wiki source" >&2
  diff -u "$BEFORE_HASHES" "$AFTER_HASHES" >&2 || true
  exit 1
fi

test -f "$RESULT_JSON"
test -f "$SITE_OUT/for-you.html"
test -f "$SITE_OUT/for-you/index.html"
test -f "$SITE_OUT/for-you.md"
test -f "$SITE_OUT/for-you.talk.html"
test -f "$SITE_OUT/for-you/talk/index.html"
test -f "$SITE_OUT/for-you.talk.md"
test -f "$SITE_OUT/assets/theme.css"
test -f "$SITE_OUT/assets/enhance.js"

RESULT_JSON="$RESULT_JSON" SITE_OUT="$SITE_OUT" uv run python - <<'PY'
from __future__ import annotations

import json
import os
from pathlib import Path

result = json.loads(Path(os.environ["RESULT_JSON"]).read_text())
site_out = Path(os.environ["SITE_OUT"])

assert result["schema_version"] == 1
assert result["status"] == "published"
assert result["output"] == str(site_out)
assert result["source_input_count"] == 4
assert result["talk_input_count"] == 4
assert len(result["source_inputs"]) == 4
assert len(result["talk_inputs"]) == 4
assert "assets/theme.css" in result["assets"]
assert "assets/enhance.js" in result["assets"]
assert all(Path(path).is_absolute() for path in result["source_inputs"])
assert all(Path(path).is_absolute() for path in result["talk_inputs"])
assert len(result["logs"]) == 8
PY

FAILED_RESULT="$OUT_DIR/failed-result.json"
if node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$OUT_DIR/missing-source" \
  --output "$OUT_DIR/missing-site" \
  --result-json "$FAILED_RESULT" \
  >/tmp/1ctx-wiki-render-site-missing.out 2>/tmp/1ctx-wiki-render-site-missing.err; then
  echo "render-site should fail for a missing source root" >&2
  exit 1
fi

FAILED_RESULT="$FAILED_RESULT" uv run python - <<'PY'
from __future__ import annotations

import json
import os
from pathlib import Path

result = json.loads(Path(os.environ["FAILED_RESULT"]).read_text())
assert result["schema_version"] == 1
assert result["status"] == "failed"
assert "Source root does not exist" in result["error"]
PY

echo "wiki render-site CLI smoke passed."
