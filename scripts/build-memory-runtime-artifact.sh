#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE_DIR="${ONECONTEXT_MEMORY_RUNTIME_SOURCE_DIR:-$ROOT/release/memory-runtime/source}"
OUTPUT_DIR="${ONECONTEXT_MEMORY_RUNTIME_OUTPUT_DIR:-$ROOT/dist/release-tools/memory-runtime}"
SITE_SOURCE="$SOURCE_DIR/wiki-site"
SITE_OUTPUT="$OUTPUT_DIR/wiki-site"
MANIFEST="$OUTPUT_DIR/manifest.json"

fail() {
  echo "memory runtime artifact failed: $*" >&2
  exit 1
}

[[ -d "$SITE_SOURCE" ]] || fail "missing wiki-site source: $SITE_SOURCE"

rm -rf "$OUTPUT_DIR"
mkdir -p "$SITE_OUTPUT"
ditto "$SITE_SOURCE" "$SITE_OUTPUT"

if find "$OUTPUT_DIR" -type f \( \
  -name '*.py' -o -name '*.pyc' -o -name '*.sh' -o -name '*.swift' -o \
  -name '*.ts' -o -name '*.tsx' -o -name '*.js' -o -name '*.mjs' -o \
  -name '*.md' -o -name 'package.json' -o -name 'pyproject.toml' \) \
  -print -quit | grep -q .; then
  fail "artifact contains source, script, or package-manager files"
fi

if find "$OUTPUT_DIR" -type d \( \
  -name '.git' -o -name '.venv' -o -name 'node_modules' -o -name '__pycache__' -o \
  -name 'memory-core' -o -name 'generated' \) \
  -print -quit | grep -q .; then
  fail "artifact contains forbidden source-checkout or generated directories"
fi

if grep -R -I -n -E '/Users/|/opt/homebrew|/usr/local|/dev/1context|(^|[^[:alnum:]_])/goal([^[:alnum:]_]|$)|goal\\.html|goal\\.md|memory-core|uv run|npm ci|node_modules' \
  "$OUTPUT_DIR" >/tmp/1context-memory-runtime-forbidden.txt; then
  cat /tmp/1context-memory-runtime-forbidden.txt >&2
  fail "artifact contains local paths, dev goal routes, or source-checkout references"
fi

python3 - "$OUTPUT_DIR" "$MANIFEST" <<'PY'
import datetime as dt
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
manifest = Path(sys.argv[2])
files = []
total_bytes = 0
for path in sorted(root.rglob("*")):
    if path == manifest or not path.is_file():
        continue
    relative = path.relative_to(root).as_posix()
    data = path.read_bytes()
    total_bytes += len(data)
    files.append({
        "path": relative,
        "size": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
    })

required = {
    "wiki-site/index.html",
    "wiki-site/your-context/index.html",
    "wiki-site/for-you/index.html",
    "wiki-site/__1context/health",
    "wiki-site/api/wiki/search.json",
    "wiki-site/api/wiki/bookmarks.json",
    "wiki-site/api/wiki/state.json",
}
present = {entry["path"] for entry in files}
missing = sorted(required - present)
if missing:
    raise SystemExit("missing required runtime files: " + ", ".join(missing))
if total_bytes > 262_144:
    raise SystemExit(f"memory runtime artifact too large: {total_bytes} bytes")

manifest.write_text(json.dumps({
    "schema_version": "1context.memory-runtime.v1",
    "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
    "contract": "release/memory-runtime/CONTRACT.md",
    "total_bytes": total_bytes,
    "files": files,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "memory runtime artifact: $OUTPUT_DIR"
