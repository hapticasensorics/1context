#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
STATE_DIR="$(mktemp -d /tmp/1ctx-test-XXXXXX)"

pick_free_port() {
  python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

HARNESS_WIKI_PORT="${ONECONTEXT_TEST_WIKI_PORT:-$(pick_free_port)}"
HARNESS_WIKI_API_PORT="${ONECONTEXT_TEST_WIKI_API_PORT:-$(pick_free_port)}"
if [[ "$HARNESS_WIKI_PORT" == "$HARNESS_WIKI_API_PORT" ]]; then
  HARNESS_WIKI_API_PORT="$(pick_free_port)"
fi

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
  ONECONTEXT_WIKI_URL_MODE="${ONECONTEXT_WIKI_URL_MODE:-high-port-http}" \
  ONECONTEXT_WIKI_PORT="${ONECONTEXT_WIKI_PORT:-$HARNESS_WIKI_PORT}" \
  ONECONTEXT_WIKI_API_PORT="${ONECONTEXT_WIKI_API_PORT:-$HARNESS_WIKI_API_PORT}" \
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

assert_url_contains() {
  local url="$1"
  local expected="$2"
  local output
  for _ in {1..40}; do
    if output="$(curl --fail --silent --max-time 3 "$url" 2>/dev/null)" && grep -q "$expected" <<<"$output"; then
      return 0
    fi
    sleep 0.25
  done

  echo "Expected URL to contain '$expected': $url" >&2
  curl --include --silent --show-error --max-time 5 "$url" >&2 || true
  return 1
}

assert_url_not_contains() {
  local url="$1"
  local unexpected="$2"
  local response status output
  if ! response="$(curl --silent --show-error --max-time 3 --write-out $'\n%{http_code}' "$url")"; then
    echo "Could not fetch URL while checking absence of '$unexpected': $url" >&2
    return 1
  fi
  status="${response##*$'\n'}"
  output="${response%$'\n'*}"
  if [[ "$status" == "404" ]]; then
    return 0
  fi
  if [[ ! "$status" =~ ^2[0-9][0-9]$ ]]; then
    echo "Expected 2xx or 404 while checking absence of '$unexpected', saw $status: $url" >&2
    return 1
  fi
  if grep -q "$unexpected" <<<"$output"; then
    echo "Expected URL not to contain '$unexpected': $url" >&2
    return 1
  fi
}

wait_for_runtime_running() {
  for _ in {1..60}; do
    if "$BIN_DIR/1context" status >/tmp/1ctx-test-status-running.$$ 2>&1 \
      && grep -q "Health: OK" /tmp/1ctx-test-status-running.$$ \
      && [ -f "$ONECONTEXT_APP_SUPPORT_DIR/run/1contextd.pid" ]; then
      rm -f /tmp/1ctx-test-status-running.$$
      return 0
    fi
    sleep 0.1
  done
  cat /tmp/1ctx-test-status-running.$$ >&2 || true
  rm -f /tmp/1ctx-test-status-running.$$
  echo "1Context did not report running in time" >&2
  return 1
}

wait_for_runtime_stopped() {
  for _ in {1..60}; do
    if ! "$BIN_DIR/1context" status >/tmp/1ctx-test-status-stopped.$$ 2>&1; then
      rm -f /tmp/1ctx-test-status-stopped.$$
      return 0
    fi
    sleep 0.1
  done
  cat /tmp/1ctx-test-status-stopped.$$ >&2 || true
  rm -f /tmp/1ctx-test-status-stopped.$$
  echo "1Context did not stop in time" >&2
  return 1
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
export ONECONTEXT_WIKI_URL_MODE=high-port-http
export ONECONTEXT_WIKI_PORT="$HARNESS_WIKI_PORT"
export ONECONTEXT_WIKI_API_PORT="$HARNESS_WIKI_API_PORT"
export ONECONTEXT_CADDY_PATH="${ONECONTEXT_CADDY_PATH:-$(command -v caddy 2>/dev/null || true)}"
if [[ -z "$ONECONTEXT_CADDY_PATH" || ! -x "$ONECONTEXT_CADDY_PATH" ]]; then
  echo "Smoke tests require Caddy. Install caddy or set ONECONTEXT_CADDY_PATH." >&2
  exit 1
fi
WIKI_TEST_URL="http://wiki.1context.localhost:$ONECONTEXT_WIKI_PORT"
WIKI_TEST_API_URL="http://127.0.0.1:$ONECONTEXT_WIKI_API_PORT"

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
"$BIN_DIR/1context" start | grep -q "1Context Remembering"
wait_for_runtime_running
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
"$BIN_DIR/1context" diagnose | grep -q "Health: OK"
"$BIN_DIR/1context" diagnose | grep -q "Local Web"
"$BIN_DIR/1context" diagnose | grep -q "URL: $WIKI_TEST_URL/your-context"
"$BIN_DIR/1context" diagnose | grep -q "API URL: $WIKI_TEST_API_URL/api/wiki/health"
"$BIN_DIR/1context" wiki local-url | grep -q "$WIKI_TEST_URL/your-context"
for _ in {1..40}; do
  if assert_url_contains "$WIKI_TEST_URL/your-context" "Your Context"; then
    break
  fi
  sleep 0.25
done
assert_url_contains "$WIKI_TEST_URL/your-context" "Your Context"
assert_url_contains "$WIKI_TEST_URL/for-you" "How This Page Works"
assert_url_not_contains "$WIKI_TEST_URL/goal" "Permission Doctrine"
assert_url_not_contains "$WIKI_TEST_URL/goal.talk" "Permission Doctrine"
if curl --fail --silent "$WIKI_TEST_URL/for-you" | grep -Eq "stub|empty: populated|<!-- empty|/Users/paulhan|/dev/1context"; then
  echo "published For You should not expose raw stubs" >&2
  exit 1
fi
assert_url_contains "$WIKI_TEST_API_URL/api/wiki/health" "1context-wiki-api"
state_response="$(curl --fail --silent --request POST \
  --header "Content-Type: application/json" \
  --data '{"settings":{"theme":"dark"},"bookmarks":[{"title":"For You","url":"/for-you"}]}' \
  "$WIKI_TEST_API_URL/api/wiki/state")"
grep -q "theme" <<<"$state_response"
assert_url_contains "$WIKI_TEST_API_URL/api/wiki/state" "bookmarks"
assert_url_contains "$WIKI_TEST_API_URL/api/wiki/search?q=for" "matches"
chat_status="$(curl --silent --output /dev/null --write-out "%{http_code}" "$WIKI_TEST_API_URL/api/wiki/chat/config")"
if [[ "$chat_status" != "404" ]]; then
  echo "unshipped chat API route returned $chat_status instead of 404" >&2
  exit 1
fi
"$BIN_DIR/1context" logs | grep -q "1Context Logs"
"$BIN_DIR/1context" restart | grep -q "1Context is running"
"$BIN_DIR/1context" stop | grep -q "1Context Stopped"
wait_for_runtime_stopped
test "$(tr -d '[:space:]' < "$ONECONTEXT_APP_SUPPORT_DIR/desired-state")" = "stopped"
if "$BIN_DIR/1context" status >"$STATE_DIR/status-down-again.out" 2>&1; then
  echo "status should fail after 1Context stops" >&2
  exit 1
fi
grep -q "1Context is not running" "$STATE_DIR/status-down-again.out"

PATH="$BIN_DIR:$PATH" 1context start | grep -q "1Context Remembering"
wait_for_runtime_running
PATH="$BIN_DIR:$PATH" 1context stop | grep -q "1Context Stopped"
wait_for_runtime_stopped
PATH="$BIN_DIR:$PATH" 1context start | grep -q "1Context Remembering"
wait_for_runtime_running
PATH="$BIN_DIR:$PATH" 1context quit | grep -q "1Context quit"

echo "1Context smoke tests passed."
