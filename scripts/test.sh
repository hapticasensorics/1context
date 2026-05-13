#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
STATE_DIR="$(mktemp -d /tmp/1ctx-test-XXXXXX)"

cleanup() {
  rm -rf "$STATE_DIR"
}

swift build --package-path "$MACOS_DIR"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"
trap cleanup EXIT

"$ROOT/scripts/check-version-consistency.sh"
"$ROOT/scripts/test-release-train.sh"
"$BIN_DIR/1context" | grep -q "1Context $VERSION"
test "$("$BIN_DIR/1context" --version)" = "$VERSION"
"$BIN_DIR/1context" --help | grep -q "1context status"
"$BIN_DIR/1context" --help | grep -q "1context quit"
"$BIN_DIR/1context" --help | grep -q "1context logs"
"$BIN_DIR/1context" --help | grep -q "1context setup local-web"
"$BIN_DIR/1context" --help | grep -q "1context wiki <local-url|refresh>"
if "$BIN_DIR/1context" wiki status >"$STATE_DIR/wiki-old-status.out" 2>&1; then
  echo "old wiki status command should fail" >&2
  exit 1
fi
grep -q "Unknown wiki subcommand: status" "$STATE_DIR/wiki-old-status.out"
if "$BIN_DIR/1context" status --wat >"$STATE_DIR/unknown-arg.out" 2>&1; then
  echo "unknown arguments should fail" >&2
  exit 1
fi
grep -q "Unknown argument: --wat" "$STATE_DIR/unknown-arg.out"
"$BIN_DIR/1context" diagnose | grep -q "1Context Diagnose"
"$BIN_DIR/1context" diagnose | grep -q "~/"
"$BIN_DIR/1context" diagnose | grep -q "Local Web"
"$BIN_DIR/1context" diagnose | grep -q "Bundled Caddy Path"
"$BIN_DIR/1context" diagnose | grep -q "Current Has Health"
if "$BIN_DIR/1context" diagnose --no-redact >"$STATE_DIR/no-redact.out" 2>&1; then
  echo "diagnose --no-redact should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown argument: --no-redact" "$STATE_DIR/no-redact.out"
if "$BIN_DIR/1context" debug >"$STATE_DIR/debug.out" 2>&1; then
  echo "debug should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown command: debug" "$STATE_DIR/debug.out"
if "$BIN_DIR/1context" permissions >"$STATE_DIR/permissions.out" 2>&1; then
  echo "permissions should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown command: permissions" "$STATE_DIR/permissions.out"
if "$BIN_DIR/1context" status --debug >"$STATE_DIR/status-debug.out" 2>&1; then
  echo "status --debug should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown argument: --debug" "$STATE_DIR/status-debug.out"
"$BIN_DIR/1context" setup local-web status | grep -q "1Context Local HTTPS Setup"
"$BIN_DIR/1context" diagnose | grep -q "Local Web"
"$BIN_DIR/1context" diagnose | grep -q "URL Mode: local-https-portless"
"$BIN_DIR/1context" diagnose | grep -q "Privileged Bind Required: yes"
"$BIN_DIR/1context" logs | grep -q "1Context Logs"

echo "1Context smoke tests passed."
