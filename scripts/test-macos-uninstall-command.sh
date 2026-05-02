#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${TMPDIR:-/tmp}/1context-uninstall-command-test-$$"

cleanup() {
  rm -rf "$OUT_DIR"
}
trap cleanup EXIT

mkdir -p "$OUT_DIR"

swift build --package-path "$ROOT/macos" >/dev/null
BIN_DIR="$(swift build --package-path "$ROOT/macos" --show-bin-path)"
CLI="$BIN_DIR/1context"

"$CLI" --help | grep -q "1context uninstall \\[--delete-data\\] \\[--keep-app\\]"
"$CLI" --help | grep -q "1context setup local-web <status|install|repair|uninstall>"
grep -q "Uninstall 1Context..." "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q "runBundledCLI(arguments: arguments)" "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q '"uninstall", "--menu-process"' "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q "1Context was moved to Trash." "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q "AppBundleTrasher" "$ROOT/macos/Sources/OneContextCLI/main.swift"
grep -q "Application bundle" "$ROOT/macos/Sources/OneContextCLI/main.swift"

if "$CLI" uninstall --definitely-not-real >"$OUT_DIR/uninstall-unknown.out" 2>&1; then
  echo "Expected uninstall with an unknown option to fail before cleanup." >&2
  exit 1
fi
grep -q "Unknown argument: --definitely-not-real" "$OUT_DIR/uninstall-unknown.out"

echo "macOS uninstall command smoke passed."
