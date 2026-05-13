#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 - "$ROOT" <<'PY'
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
baseline = int(os.environ.get("ONECONTEXT_SHIPPED_APP_BASELINE_LINES", "61309"))

# Source-equivalent footprint for code/config that the current shipped macOS app
# owns directly. The baseline was captured when the package still copied the
# memory-core source checkout into the app bundle; after that copy path is
# deleted, memory-core remains in the repo but leaves this shipped-app boundary.
scope_roots = [
  root / "macos" / "Sources",
]

code_suffixes = {
  ".css",
  ".html",
  ".js",
  ".json",
  ".mjs",
  ".plist",
  ".py",
  ".sh",
  ".swift",
  ".toml",
  ".ts",
  ".tsx",
  ".xml",
  ".yaml",
  ".yml",
}

excluded_parts = {
  ".pytest_cache",
  ".venv",
  "__pycache__",
  "node_modules",
}


def is_counted(path: Path) -> bool:
  if path.suffix not in code_suffixes:
    return False
  relative = path.relative_to(root).as_posix()
  if any(part in excluded_parts for part in path.parts):
    return False
  return True


files: list[dict[str, object]] = []
for scope_root in scope_roots:
  if not scope_root.exists():
    continue
  for path in sorted(scope_root.rglob("*")):
    if not path.is_file() or not is_counted(path):
      continue
    try:
      text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
      continue
    nonblank_lines = sum(1 for line in text.splitlines() if line.strip())
    files.append({
      "path": path.relative_to(root).as_posix(),
      "nonblank_lines": nonblank_lines,
    })

total = sum(int(item["nonblank_lines"]) for item in files)
target = baseline * 40 // 100
removed = baseline - total
result = {
  "schema_version": "1context.shipped-app-lines.v1",
  "scope": "macos/Sources for the shipped app boundary; baseline included the former bundled memory-core checkout",
  "baseline_nonblank_lines": baseline,
  "file_count": len(files),
  "nonblank_lines": total,
  "lines_removed_from_baseline": removed,
  "reduction_percent_from_baseline": round((removed / baseline) * 100, 2) if baseline else 0,
  "sixty_percent_reduction_target_lines": target,
  "largest_files": sorted(files, key=lambda item: int(item["nonblank_lines"]), reverse=True)[:20],
}
print(json.dumps(result, indent=2, sort_keys=True))
PY
