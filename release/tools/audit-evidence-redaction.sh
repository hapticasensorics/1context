#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  echo "Usage: release/tools/audit-evidence-redaction.sh <evidence-dir>" >&2
  exit 2
fi
if [[ ! -d "$TARGET" ]]; then
  echo "Evidence directory not found: $TARGET" >&2
  exit 1
fi

PATTERNS=()
while IFS= read -r pattern; do
  PATTERNS+=("$pattern")
done < <("$ROOT/scripts/release-train.sh" manifest forbidden-patterns)
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


files_scanned = 0
findings: list[dict[str, object]] = []
for path in sorted(target.rglob("*")):
  if not path.is_file() or path == report_path or not is_text_file(path):
    continue
  files_scanned += 1
  try:
    text = path.read_text(encoding="utf-8")
  except UnicodeDecodeError:
    continue
  hits = sum(1 for pattern in patterns if pattern in text)
  if hits:
    findings.append({
      "path": path.relative_to(target).as_posix(),
      "forbidden_pattern_count": hits,
    })

status = "passed" if not findings else "failed"
report_path.write_text(json.dumps({
  "schema_version": "1context.redaction-report.v1",
  "status": status,
  "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
  "files_scanned": files_scanned,
  "findings": findings,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
if findings:
  raise SystemExit(f"redaction audit failed with {len(findings)} file(s) containing forbidden evidence")
print(f"redaction audit passed: {target}")
PY
