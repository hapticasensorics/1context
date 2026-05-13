#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
STATE_DIR="$(mktemp -d /tmp/1ctx-test-XXXXXX)"

kill_processes_matching() {
  local pattern="$1"
  local pids
  pids="$(ps -axo pid=,command= | awk -v pattern="$pattern" '$0 ~ pattern { print $1 }')"
  if [ -n "$pids" ]; then
    kill $pids >/dev/null 2>&1 || true
    sleep 0.2
    kill -KILL $pids >/dev/null 2>&1 || true
  fi
}

cleanup_stale_test_caddy() {
  kill_processes_matching 'caddy run --config /tmp/1ctx-test-[^ ]+/Application Support/1Context/local-web/caddy/Caddyfile'
}

cleanup() {
  ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context" \
  ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context" \
  ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
  ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context" \
  ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context" \
  "$BIN_DIR/1context" quit >/dev/null 2>&1 || true
  if [ -f "$STATE_DIR/Application Support/1Context/run/local-web-caddy.pid" ]; then
    local caddy_pid
    caddy_pid="$(tr -d '[:space:]' < "$STATE_DIR/Application Support/1Context/run/local-web-caddy.pid" 2>/dev/null || true)"
    if [[ "$caddy_pid" =~ ^[0-9]+$ ]]; then
      kill "$caddy_pid" >/dev/null 2>&1 || true
      sleep 0.2
      kill -KILL "$caddy_pid" >/dev/null 2>&1 || true
    fi
  fi
  kill_processes_matching "caddy run --config $STATE_DIR/Application Support/1Context/local-web/caddy/Caddyfile"
  rm -rf "$STATE_DIR"
}

swift build --package-path "$MACOS_DIR"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"
trap cleanup EXIT
cleanup_stale_test_caddy

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LAUNCH_AGENT_DISABLED=1
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_CADDY_PATH="${ONECONTEXT_CADDY_PATH:-$(command -v caddy 2>/dev/null || true)}"
if [[ -z "$ONECONTEXT_CADDY_PATH" || ! -x "$ONECONTEXT_CADDY_PATH" ]]; then
  echo "Smoke tests require Caddy. Install caddy or set ONECONTEXT_CADDY_PATH." >&2
  exit 1
fi
WIKI_TEST_URL="https://wiki.1context.localhost"

"$ROOT/scripts/check-version-consistency.sh"
"$ROOT/scripts/test-release-train.sh"
"$BIN_DIR/1context" | grep -q "1Context $VERSION"
test "$("$BIN_DIR/1context" --version)" = "$VERSION"
"$BIN_DIR/1context" --help | grep -q "1context status"
"$BIN_DIR/1context" --help | grep -q "1context quit"
"$BIN_DIR/1context" --help | grep -q "1context logs"
"$BIN_DIR/1context" --help | grep -q "1context setup local-web"
"$BIN_DIR/1context" --help | grep -q "1context wiki <local-url|refresh>"
BIN_DIR="$BIN_DIR" "$ROOT/scripts/test-macos-uninstall-command.sh"
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
if "$BIN_DIR/1context" status >"$STATE_DIR/status-down.out" 2>&1; then
  echo "status should fail when 1Context is not running" >&2
  exit 1
fi
grep -q "1Context is not running" "$STATE_DIR/status-down.out"
if "$BIN_DIR/1context" start >"$STATE_DIR/start-needs-setup.out" 2>&1; then
  echo "start should require Local Wiki Access setup before launching the runtime" >&2
  exit 1
fi
grep -q "Local wiki access is not set up" "$STATE_DIR/start-needs-setup.out"
"$BIN_DIR/1context" setup local-web status | grep -q "Setup Ready: no"
"$BIN_DIR/1context" diagnose | grep -q "Local Web"
"$BIN_DIR/1context" diagnose | grep -q "Health: setup required"
"$BIN_DIR/1context" diagnose | grep -q "URL: $WIKI_TEST_URL/your-context"
"$BIN_DIR/1context" diagnose | grep -q "URL Mode: local-https-portless"
"$BIN_DIR/1context" diagnose | grep -q "Privileged Bind Required: yes"
if "$BIN_DIR/1context" wiki local-url >"$STATE_DIR/wiki-local-url-needs-setup.out" 2>&1; then
  echo "wiki local-url should require Local Wiki Access setup" >&2
  exit 1
fi
grep -q "Local wiki access is not set up" "$STATE_DIR/wiki-local-url-needs-setup.out"
"$BIN_DIR/1context" logs | grep -q "1Context Logs"

echo "1Context smoke tests passed."
