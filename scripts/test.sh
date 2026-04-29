#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
STATE_DIR="$(mktemp -d /tmp/1ctx-test-XXXXXX)"

cleanup() {
  ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context" \
  ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
  ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context" \
  "$BIN_DIR/1context" stop >/dev/null 2>&1 || true
  rm -rf "$STATE_DIR"
}

swift build --package-path "$MACOS_DIR"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"
trap cleanup EXIT

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_LAUNCH_AGENT_DISABLED=1
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_NO_UPDATE_CHECK=1

"$BIN_DIR/1context" | grep -q "1Context 0.1.4"
test "$("$BIN_DIR/1context" --version)" = "0.1.4"
"$BIN_DIR/1context" --help | grep -q "1context status"
"$BIN_DIR/1context" status | grep -q "1Context is not running"
"$BIN_DIR/1context" start | grep -q "1Context is running"
"$BIN_DIR/1context" status | grep -q "Health: OK"
"$BIN_DIR/1context" status --debug | grep -q "Socket: responding"
"$BIN_DIR/1context" restart | grep -q "1Context is running"
"$BIN_DIR/1context" stop | grep -q "1Context is stopped"
"$BIN_DIR/1context" status | grep -q "1Context is not running"

echo "1Context smoke tests passed."
