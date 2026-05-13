#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  echo "Usage: scripts/redact-evidence.sh <evidence-dir>" >&2
  exit 2
fi
if [[ ! -d "$TARGET" ]]; then
  echo "Evidence directory not found: $TARGET" >&2
  exit 1
fi

PATTERNS=()
while IFS= read -r pattern; do
  PATTERNS+=("$pattern")
done < <("$ROOT/scripts/release-manifest.py" forbidden-patterns)
python3 - "$TARGET" "${PATTERNS[@]}" <<'PY'
from __future__ import annotations

import datetime as dt
import json
import sys
from pathlib import Path

target = Path(sys.argv[1])
patterns = [pattern for pattern in sys.argv[2:] if pattern]
report_path = target / "redaction-report.json"
text_suffixes = {
  ".txt", ".log", ".json", ".xml", ".md", ".toml", ".yml", ".yaml",
  ".html", ".css", ".js", ".swift", ".sh", ".out", ".err"
}


def is_text_file(path: Path) -> bool:
  if path.suffix.lower() in text_suffixes:
    return True
  try:
    chunk = path.read_bytes()[:4096]
  except OSError:
    return False
  return b"\0" not in chunk


def replacement(pattern: str) -> str:
  return "[REDACTED]"


files_scanned = 0
files_changed = 0
replacements = 0
changed_paths: list[str] = []
for path in sorted(target.rglob("*")):
  if not path.is_file() or path == report_path or not is_text_file(path):
    continue
  files_scanned += 1
  try:
    text = path.read_text(encoding="utf-8")
  except UnicodeDecodeError:
    continue
  original = text
  for pattern in patterns:
    count = text.count(pattern)
    if count:
      text = text.replace(pattern, replacement(pattern))
      replacements += count
  if text != original:
    path.write_text(text, encoding="utf-8")
    files_changed += 1
    changed_paths.append(path.relative_to(target).as_posix())

report_path.write_text(json.dumps({
  "schema_version": "1context.redaction-report.v1",
  "status": "passed",
  "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
  "files_scanned": files_scanned,
  "files_changed": files_changed,
  "replacement_count": replacements,
  "changed_paths": changed_paths,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
