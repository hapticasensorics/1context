#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
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
export ONECONTEXT_CLAUDE_SETTINGS_PATH="$STATE_DIR/.claude/settings.json"
export ONECONTEXT_AGENT_ALLOW_ENV_OVERRIDES=1

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
"$BIN_DIR/1context" --help | grep -q "1context wiki <open|local-url|start|status|stop>"
if "$BIN_DIR/1context" status --wat >"$STATE_DIR/unknown-arg.out" 2>&1; then
  echo "unknown arguments should fail" >&2
  exit 1
fi
grep -q "Unknown argument: --wat" "$STATE_DIR/unknown-arg.out"
"$BIN_DIR/1context" diagnose | grep -q "1Context Diagnose"
"$BIN_DIR/1context" diagnose | grep -q "~/"
"$BIN_DIR/1context" diagnose | grep -q "Memory Core"
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
"$BIN_DIR/1context" agent integrations status | grep -q "Codex: plan only"
"$BIN_DIR/1context" agent integrations install | grep -q "Claude: installed"
test -f "$ONECONTEXT_APP_SUPPORT_DIR/agent/config.json"
test -f "$ONECONTEXT_APP_SUPPORT_DIR/agent/integrations.json"
grep -q "agent hook --provider claude --event SessionStart" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"
grep -q "agent statusline --provider claude" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"
if grep -q "agent hook --provider claude --event UserPromptSubmit" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"; then
  echo "public preview should not install prompt-submit hooks by default" >&2
  exit 1
fi
printf '{"cwd":"%s"}\n' "$ROOT" \
  | "$BIN_DIR/1context" agent hook --provider claude --event SessionStart \
  | grep -q '"systemMessage"'
printf '{"cwd":"%s"}\n' "$ROOT" \
  | "$BIN_DIR/1context" agent hook --provider claude --event SessionStart \
  | grep -q "127.0.0.1:17319"
printf '{"cwd":"%s"}\n' "$ROOT" \
  | "$BIN_DIR/1context" agent hook --provider codex --event SessionStart \
  | grep -q "127.0.0.1:17319"
printf '{}\n' \
  | "$BIN_DIR/1context" agent hook --provider claude --event PostToolUse \
  | grep -q '"hookEventName":"PostToolUse"'
printf '{}\n' \
  | "$BIN_DIR/1context" agent statusline --provider claude \
  | grep -q "View 1Context wiki: http://127.0.0.1:17319/for-you"
printf '{}\n' \
  | "$BIN_DIR/1context" agent statusline --provider codex \
  | grep -q "View 1Context wiki: http://127.0.0.1:17319/for-you"
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

echo "1Context smoke tests passed."
