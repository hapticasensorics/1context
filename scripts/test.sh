#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
STATE_DIR="$(mktemp -d /tmp/1ctx-test-XXXXXX)"

cleanup() {
  ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context" \
  ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context" \
  ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
  ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context" \
  ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context" \
  ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update" \
  "$BIN_DIR/1context" stop >/dev/null 2>&1 || true
  rm -rf "$STATE_DIR"
}

swift build --package-path "$MACOS_DIR"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"
trap cleanup EXIT

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LAUNCH_AGENT_DISABLED=1
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
export ONECONTEXT_NO_UPDATE_CHECK=1

"$BIN_DIR/1context" | grep -q "1Context 0.1.27"
test "$("$BIN_DIR/1context" --version)" = "0.1.27"
"$BIN_DIR/1context" --help | grep -q "1context status"
if "$BIN_DIR/1context" status --wat >"$STATE_DIR/unknown-arg.out" 2>&1; then
  echo "unknown arguments should fail" >&2
  exit 1
fi
grep -q "Unknown argument: --wat" "$STATE_DIR/unknown-arg.out"
"$BIN_DIR/1context" diagnose | grep -q "1Context Diagnose"
if "$BIN_DIR/1context" status >"$STATE_DIR/status-down.out" 2>&1; then
  echo "status should fail when 1Context is not running" >&2
  exit 1
fi
grep -q "1Context is not running" "$STATE_DIR/status-down.out"
"$BIN_DIR/1context" start | grep -q "1Context is running"
test -d "$ONECONTEXT_USER_CONTENT_DIR"
test -d "$ONECONTEXT_APP_SUPPORT_DIR/run"
test -f "$ONECONTEXT_APP_SUPPORT_DIR/run/1contextd.pid"
test -d "$ONECONTEXT_LOG_DIR"
test -d "$ONECONTEXT_CACHE_DIR"
test -d "$ONECONTEXT_CACHE_DIR/render-cache"
test -d "$ONECONTEXT_CACHE_DIR/download-cache"
test "$(stat -f "%Lp" "$ONECONTEXT_USER_CONTENT_DIR")" = "700"
test "$(stat -f "%Lp" "$ONECONTEXT_APP_SUPPORT_DIR")" = "700"
test "$(stat -f "%Lp" "$ONECONTEXT_APP_SUPPORT_DIR/run")" = "700"
test "$(stat -f "%Lp" "$ONECONTEXT_LOG_DIR")" = "700"
test "$(stat -f "%Lp" "$ONECONTEXT_CACHE_DIR")" = "700"
test "$(stat -f "%Lp" "$ONECONTEXT_APP_SUPPORT_DIR/desired-state")" = "600"
test "$(stat -f "%Lp" "$ONECONTEXT_APP_SUPPORT_DIR/run/1contextd.pid")" = "600"
test "$(stat -f "%Lp" "$ONECONTEXT_LOG_DIR/1contextd.log")" = "600"
"$BIN_DIR/1context" status | grep -q "Health: OK"
"$BIN_DIR/1context" status --debug | grep -q "Socket: responding"
"$BIN_DIR/1context" restart --debug | grep -q "Completed in"
"$BIN_DIR/1context" stop | grep -q "1Context is stopped"
if "$BIN_DIR/1context" status >"$STATE_DIR/status-down-again.out" 2>&1; then
  echo "status should fail after 1Context stops" >&2
  exit 1
fi
grep -q "1Context is not running" "$STATE_DIR/status-down-again.out"

PATH="$BIN_DIR:$PATH" 1context start | grep -q "1Context is running"
PATH="$BIN_DIR:$PATH" 1context stop | grep -q "1Context is stopped"

echo "1Context smoke tests passed."
