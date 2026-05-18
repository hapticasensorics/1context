#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_ROOT="$(mktemp -d /tmp/1ctx-wiki-materializer-validation-XXXXXX)"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

copy_runtime() {
  local dest="$1"
  mkdir -p "$dest"
  cp -R "$ROOT/runtime/." "$dest/"
}

expect_failure() {
  local name="$1"
  local expected="$2"
  local runtime_home="$TMP_ROOT/$name"
  copy_runtime "$runtime_home"

  CASE_NAME="$name" WIKI_TOML="$runtime_home/1Context/user-wiki/wiki.toml" uv run python - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["WIKI_TOML"])
text = path.read_text()
case = os.environ["CASE_NAME"]

if case == "duplicate-id":
    text += """

[[pages]]
id = "projects"
enabled = true
title = "Duplicate Projects"
slug = "duplicate-projects"
route = "/duplicate-projects"
family_group = "work"
family_id = "duplicate-projects"
type = "project-index"
template = "pages/e08/projects.md"
"""
elif case == "duplicate-route":
    text = text.replace('route = "/topics"', 'route = "/projects"', 1)
elif case == "invalid-kind":
    text = text.replace('kind = "generated"', 'kind = "mystery"', 1)
elif case == "template-escape":
    text = text.replace('template = "pages/e08/your-context.md"', 'template = "../bad.md"', 1)
elif case == "unsafe-family":
    text = text.replace('family_group = "context"', 'family_group = "../context"', 1)
else:
    raise SystemExit(f"unknown case: {case}")

path.write_text(text)
PY

  if uv run python "$ROOT/scripts/materialize-wiki-pages.py" "$runtime_home" >"$runtime_home/out.txt" 2>"$runtime_home/err.txt"; then
    echo "expected materializer validation failure for $name" >&2
    exit 1
  fi
  if ! grep -q "$expected" "$runtime_home/err.txt"; then
    echo "expected '$expected' in $name failure" >&2
    cat "$runtime_home/err.txt" >&2
    exit 1
  fi
}

expect_failure duplicate-id "Duplicate wiki page id"
expect_failure duplicate-route "Duplicate wiki route"
expect_failure invalid-kind "Invalid site page kind"
expect_failure template-escape "must stay inside templates"
expect_failure unsafe-family "Invalid pages.your-context.family_group"

echo "wiki materializer validation smoke passed."
