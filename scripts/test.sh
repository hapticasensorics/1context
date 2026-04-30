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
  ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update" \
  ONECONTEXT_WIKI_PORT="${ONECONTEXT_WIKI_PORT:-17419}" \
  ONECONTEXT_WIKI_API_PORT="${ONECONTEXT_WIKI_API_PORT:-17420}" \
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

swift build --package-path "$MACOS_DIR"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"
trap cleanup EXIT
cleanup_stale_test_caddy

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LAUNCH_AGENT_DISABLED=1
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
export ONECONTEXT_NO_UPDATE_CHECK=1
export ONECONTEXT_CLAUDE_SETTINGS_PATH="$STATE_DIR/.claude/settings.json"
export ONECONTEXT_CODEX_CONFIG_PATH="$STATE_DIR/.codex/config.toml"
export ONECONTEXT_AGENT_ALLOW_ENV_OVERRIDES=1
export ONECONTEXT_WIKI_PORT=17419
export ONECONTEXT_WIKI_API_PORT=17420

"$ROOT/scripts/check-version-consistency.sh"
"$ROOT/scripts/test-menu-lifecycle-deterministic.sh"
"$BIN_DIR/1context" | grep -q "1Context $VERSION"
test "$("$BIN_DIR/1context" --version)" = "$VERSION"
"$BIN_DIR/1context" --help | grep -q "1context status"
"$BIN_DIR/1context" --help | grep -q "1context quit"
"$BIN_DIR/1context" --help | grep -q "1context logs"
"$BIN_DIR/1context" --help | grep -q "1context debug"
"$BIN_DIR/1context" --help | grep -q "1context agent integrations"
"$BIN_DIR/1context" --help | grep -q "1context agent statusline --provider <claude|codex>"
"$BIN_DIR/1context" --help | grep -q "1context memory-core"
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
"$BIN_DIR/1context" diagnose | grep -q "Memory Core"
"$BIN_DIR/1context" diagnose | grep -q "Local Web"
"$BIN_DIR/1context" diagnose | grep -q "Bundled Caddy Path"
"$BIN_DIR/1context" diagnose | grep -q "Current Has Theme"
"$BIN_DIR/1context" debug | grep -q "1Context Diagnose"
"$BIN_DIR/1context" debug --no-redact | grep -q "$STATE_DIR"
BIN_DIR="$BIN_DIR" "$ROOT/scripts/test-memory-core-contract.sh"
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
"$BIN_DIR/1context" status --debug | grep -q "Local Web"
"$BIN_DIR/1context" status --debug | grep -q "URL: http://wiki.1context.localhost:17419/your-context"
"$BIN_DIR/1context" status --debug | grep -q "API URL: http://127.0.0.1:17420/api/wiki/health"
"$BIN_DIR/1context" wiki local-url | grep -q "http://wiki.1context.localhost:17419/your-context"
python3 - "$ONECONTEXT_APP_SUPPORT_DIR/agent/config.json" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text(encoding="utf-8"))
assert payload["wiki_url"] == "http://wiki.1context.localhost:17419/your-context"
PY
for _ in {1..40}; do
  if assert_url_contains "http://wiki.1context.localhost:17419/your-context" "Your Context"; then
    break
  fi
  sleep 0.25
done
assert_url_contains "http://wiki.1context.localhost:17419/your-context" "Your Context"
assert_url_contains "http://wiki.1context.localhost:17419/for-you" "For You"
assert_url_contains "http://wiki.1context.localhost:17419/for-you.talk" "Talk Conventions"
assert_url_contains "http://wiki.1context.localhost:17419/for-you.talk" "How to use this talk page"
assert_url_contains "http://wiki.1context.localhost:17419/projects" "Projects"
assert_url_contains "http://wiki.1context.localhost:17419/topics" "Topics"
assert_url_contains "http://wiki.1context.localhost:17419/your-context.talk" "Talk"
assert_url_contains "http://wiki.1context.localhost:17419/projects.talk" "Talk"
assert_url_contains "http://wiki.1context.localhost:17419/topics.talk" "Talk"
if curl --fail --silent "http://wiki.1context.localhost:17419/for-you" | grep -Eq "stub|empty: populated|<!-- empty"; then
  echo "published For You should not expose raw stubs" >&2
  exit 1
fi
assert_url_contains "http://127.0.0.1:17420/api/wiki/health" "1context-wiki-api"
state_response="$(curl --fail --silent --request POST \
  --header "Content-Type: application/json" \
  --data '{"settings":{"theme":"dark"},"bookmarks":[{"title":"For You","url":"/for-you"}]}' \
  "http://127.0.0.1:17420/api/wiki/state")"
grep -q "theme" <<<"$state_response"
assert_url_contains "http://127.0.0.1:17420/api/wiki/state" "bookmarks"
assert_url_contains "http://127.0.0.1:17420/api/wiki/search?q=for" "matches"
assert_url_contains "http://127.0.0.1:17420/api/wiki/chat/config" "chat_available"
"$BIN_DIR/1context" logs | grep -q "1Context Logs"
"$BIN_DIR/1context" restart --debug | grep -q "Completed in"
"$BIN_DIR/1context" stop | grep -q "1Context is stopped"
test "$(tr -d '[:space:]' < "$ONECONTEXT_APP_SUPPORT_DIR/desired-state")" = "stopped"
if "$BIN_DIR/1context" status >"$STATE_DIR/status-down-again.out" 2>&1; then
  echo "status should fail after 1Context stops" >&2
  exit 1
fi
grep -q "1Context is not running" "$STATE_DIR/status-down-again.out"

PATH="$BIN_DIR:$PATH" 1context start | grep -q "1Context is running"
PATH="$BIN_DIR:$PATH" 1context stop | grep -q "1Context is stopped"
PATH="$BIN_DIR:$PATH" 1context start | grep -q "1Context is running"
PATH="$BIN_DIR:$PATH" 1context quit | grep -q "1Context quit"

"$BIN_DIR/1context" agent integrations uninstall | grep -q "Claude: not installed"
"$BIN_DIR/1context" agent integrations status | grep -q "Codex: not installed"
"$BIN_DIR/1context" agent integrations install | grep -q "Claude: installed"
"$BIN_DIR/1context" agent integrations status | grep -q "Codex: installed"
test -f "$ONECONTEXT_APP_SUPPORT_DIR/agent/config.json"
test -f "$ONECONTEXT_APP_SUPPORT_DIR/agent/integrations.json"
grep -q "agent hook --provider claude --event SessionStart" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"
grep -q "agent statusline --provider claude" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"
grep -q "agent hook --provider codex --event SessionStart" "$ONECONTEXT_CODEX_CONFIG_PATH"
grep -q 'matcher = "startup"' "$ONECONTEXT_CODEX_CONFIG_PATH"
grep -q 'matcher = "resume"' "$ONECONTEXT_CODEX_CONFIG_PATH"
grep -q 'matcher = "clear"' "$ONECONTEXT_CODEX_CONFIG_PATH"
grep -q 'matcher = "compact"' "$ONECONTEXT_CODEX_CONFIG_PATH"
if grep -q "agent hook --provider claude --event UserPromptSubmit" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"; then
  echo "public preview should not install prompt-submit hooks by default" >&2
  exit 1
fi
if grep -q "1Context-private-4" "$ONECONTEXT_CODEX_CONFIG_PATH"; then
  echo "agent install should remove legacy private Codex hooks" >&2
  exit 1
fi
printf '{"cwd":"%s"}\n' "$ROOT" \
  | "$BIN_DIR/1context" agent hook --provider claude --event SessionStart \
  | grep -q '"systemMessage"'
printf '{"cwd":"%s"}\n' "$ROOT" \
  | "$BIN_DIR/1context" agent hook --provider claude --event SessionStart \
  | grep -q "wiki.1context.localhost:17419"
printf '{"cwd":"%s"}\n' "$ROOT" \
  | "$BIN_DIR/1context" agent hook --provider codex --event SessionStart \
  | grep -q "wiki.1context.localhost:17419"
printf '{}\n' \
  | "$BIN_DIR/1context" agent hook --provider claude --event PostToolUse \
  | grep -q '"hookEventName":"PostToolUse"'
printf '{}\n' \
  | "$BIN_DIR/1context" agent statusline --provider claude \
  | grep -q "View 1Context wiki: http://wiki.1context.localhost:17419/your-context"
printf '{}\n' \
  | "$BIN_DIR/1context" agent statusline --provider codex \
  | grep -q "View 1Context wiki: http://wiki.1context.localhost:17419/your-context"
python3 - "$ONECONTEXT_APP_SUPPORT_DIR/agent/config.json" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text(encoding="utf-8"))
payload["wiki_url"] = "http://localhost:4101"
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
printf '{}\n' \
  | "$BIN_DIR/1context" agent hook --provider claude --event SessionStart \
  | grep -q "localhost:4101"
printf '{}\n' \
  | "$BIN_DIR/1context" agent statusline --provider claude \
  | grep -q "View 1Context wiki: http://localhost:4101"
"$BIN_DIR/1context" agent integrations repair | grep -q "Claude: installed"
"$BIN_DIR/1context" agent integrations uninstall | grep -q "Claude: not installed"
if grep -q "agent hook --provider claude" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"; then
  echo "agent uninstall should remove managed Claude hooks" >&2
  exit 1
fi
if grep -q "agent hook --provider codex" "$ONECONTEXT_CODEX_CONFIG_PATH"; then
  echo "agent uninstall should remove managed Codex hooks" >&2
  exit 1
fi

echo "1Context smoke tests passed."
