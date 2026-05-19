#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP="${1:-$ROOT/dist/1Context.app}"

if [[ ! -d "$APP/Contents" ]]; then
  echo "App bundle not found: $APP" >&2
  exit 1
fi

DEPENDENCY_REPORT="$(mktemp /tmp/onecontext-app-dependencies.XXXXXX)"
SCRIPT_REPORT="$(mktemp /tmp/onecontext-app-scripts.XXXXXX)"
trap 'rm -f "$DEPENDENCY_REPORT" "$SCRIPT_REPORT"' EXIT

is_allowed_dylib_reference() {
  local reference="$1"
  case "$reference" in
    @rpath/*|@executable_path/*|@loader_path/*|/usr/lib/*|/System/Library/*)
      return 0
      ;;
  esac
  return 1
}

while IFS= read -r -d '' file_path; do
  file_type="$(file -b "$file_path" 2>/dev/null || true)"
  case "$file_type" in
    *Mach-O*)
      while IFS= read -r dependency_line; do
        [[ "$dependency_line" == $'\t'* ]] || continue
        dependency="$(printf '%s\n' "$dependency_line" | awk '{print $1}')"
        [[ -z "$dependency" ]] && continue
        if ! is_allowed_dylib_reference "$dependency"; then
          printf '%s -> %s\n' "$file_path" "$dependency" >> "$DEPENDENCY_REPORT"
        fi
      done < <(otool -L "$file_path" 2>/dev/null | tail -n +2)
      ;;
    *text*|*script*)
      first_line="$(LC_ALL=C head -n 1 "$file_path" 2>/dev/null || true)"
      if [[ "$first_line" == '#!'* ]]; then
        if printf '%s\n' "$first_line" | grep -E '(^#!.*(brew|python|node|npm|uv))|/opt/homebrew|/usr/local|/opt/local' >/dev/null; then
          printf '%s -> %s\n' "$file_path" "$first_line" >> "$SCRIPT_REPORT"
        fi
      fi
      ;;
  esac
done < <(find "$APP/Contents" -type f -print0)

if [[ -s "$DEPENDENCY_REPORT" ]]; then
  echo "Packaged app contains non-system, non-bundled dynamic library references:" >&2
  cat "$DEPENDENCY_REPORT" >&2
  exit 1
fi

if [[ -s "$SCRIPT_REPORT" ]]; then
  echo "Packaged app contains executable scripts that rely on host package managers or language runtimes:" >&2
  cat "$SCRIPT_REPORT" >&2
  exit 1
fi

echo "Packaged app dependency audit passed."
