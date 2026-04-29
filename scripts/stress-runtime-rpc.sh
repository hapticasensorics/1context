#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
STATE_DIR="$(mktemp -d /tmp/1ctx-rpc-stress-XXXXXX)"
COUNT="${ONECONTEXT_STRESS_COUNT:-500}"

swift build --package-path "$MACOS_DIR" >/dev/null
BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"

cleanup() {
  ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context" \
  ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context" \
  ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
  ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context" \
  ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context" \
  ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update" \
  ONECONTEXT_NO_UPDATE_CHECK=1 \
  "$BIN_DIR/1context" stop >/dev/null 2>&1 || true
  rm -rf "$STATE_DIR"
}
trap cleanup EXIT

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LAUNCH_AGENT_DISABLED=1
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
export ONECONTEXT_NO_UPDATE_CHECK=1

"$BIN_DIR/1context" start >/dev/null

for ((i = 1; i <= COUNT; i++)); do
  "$BIN_DIR/1context" status >/dev/null
done

"$BIN_DIR/1context" status --debug | grep -q "Socket: responding"
echo "1Context RPC stress passed ($COUNT status requests)."
