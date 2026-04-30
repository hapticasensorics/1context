#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
ARCHIVE="${1:-$ROOT/dist/1context-$VERSION-macos-$ARCH.tar.gz}"
PORT="${ONECONTEXT_RELEASE_TEST_WIKI_PORT:-17519}"
API_PORT="${ONECONTEXT_RELEASE_TEST_WIKI_API_PORT:-17520}"

if [[ ! -f "$ARCHIVE" ]]; then
  echo "Release archive not found: $ARCHIVE" >&2
  echo "Build one first with: ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh" >&2
  exit 1
fi

TMPDIR="$(mktemp -d /tmp/1ctx-release-app-XXXXXX)"
STATE_DIR="$(mktemp -d /tmp/1ctx-release-state-XXXXXX)"
cleanup() {
  if [[ -n "${MENU_PID:-}" ]]; then
    kill "$MENU_PID" >/dev/null 2>&1 || true
    wait "$MENU_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${CLI:-}" && -x "${CLI:-}" ]]; then
    ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context" \
    ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context" \
    ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
    ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context" \
    ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context" \
    ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update" \
    ONECONTEXT_WIKI_PORT="$PORT" \
    ONECONTEXT_WIKI_API_PORT="$API_PORT" \
    "$CLI" quit >/dev/null 2>&1 || true
  fi
  pkill -f "caddy run --config $STATE_DIR/Application Support/1Context/local-web/caddy/Caddyfile" >/dev/null 2>&1 || true
  rm -rf "$TMPDIR" "$STATE_DIR" >/dev/null 2>&1 || true
}
trap cleanup EXIT

assert_url_contains() {
  local url="$1"
  local expected="$2"
  local output
  output="$(curl --fail --silent "$url")"
  grep -q "$expected" <<<"$output"
}

tar -C "$TMPDIR" -xzf "$ARCHIVE"
APP="$(find "$TMPDIR" -maxdepth 2 -type d -name '1Context.app' -print -quit)"
if [[ -z "$APP" || ! -d "$APP" ]]; then
  echo "Extracted archive does not contain 1Context.app." >&2
  exit 1
fi

CLI="$APP/Contents/MacOS/1context-cli"
if [[ ! -x "$CLI" ]]; then
  echo "Packaged CLI is missing or not executable: $CLI" >&2
  exit 1
fi
MENU_APP="$APP/Contents/MacOS/1Context"
if [[ ! -x "$MENU_APP" ]]; then
  echo "Packaged menu app is missing or not executable: $MENU_APP" >&2
  exit 1
fi

if [[ "$("$CLI" --version)" != "$VERSION" ]]; then
  echo "Packaged CLI version does not match VERSION." >&2
  exit 1
fi

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LAUNCH_AGENT_DISABLED=1
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
export ONECONTEXT_NO_UPDATE_CHECK=1
export ONECONTEXT_AGENT_ALLOW_ENV_OVERRIDES=1
export ONECONTEXT_WIKI_PORT="$PORT"
export ONECONTEXT_WIKI_API_PORT="$API_PORT"

MENU_LOG="$STATE_DIR/menu-app-smoke.log"
"$MENU_APP" >"$MENU_LOG" 2>&1 &
MENU_PID=$!

"$CLI" start | grep -q "1Context is running"

for _ in {1..60}; do
  if assert_url_contains "http://wiki.1context.localhost:$PORT/for-you" "How This Page Works"; then
    break
  fi
  sleep 0.25
done

assert_url_contains "http://wiki.1context.localhost:$PORT/your-context" "Your Context"
assert_url_contains "http://wiki.1context.localhost:$PORT/for-you" "How This Page Works"
assert_url_contains "http://wiki.1context.localhost:$PORT/for-you" "No observations have been promoted yet"
assert_url_contains "http://wiki.1context.localhost:$PORT/for-you.talk" "Talk-page conventions"
assert_url_contains "http://wiki.1context.localhost:$PORT/for-you.talk" "How to use this talk page"
assert_url_contains "http://wiki.1context.localhost:$PORT/projects" "No active projects have been promoted yet"
assert_url_contains "http://wiki.1context.localhost:$PORT/projects.talk" "Talk-page conventions"
assert_url_contains "http://wiki.1context.localhost:$PORT/topics" "No engineering topics have been promoted yet"
assert_url_contains "http://wiki.1context.localhost:$PORT/topics.talk" "Talk-page conventions"
assert_url_contains "http://127.0.0.1:$API_PORT/api/wiki/health" "1context-wiki-api"
assert_url_contains "http://wiki.1context.localhost:$PORT/api/wiki/health" "1context-wiki-api"
assert_url_contains "http://127.0.0.1:$API_PORT/api/wiki/chat/config" "chat_available"

if curl --fail --silent "http://wiki.1context.localhost:$PORT/for-you" | grep -Eq "stub|empty: populated|<!-- empty"; then
  echo "Packaged For You page exposes raw stubs." >&2
  exit 1
fi

if ! curl --fail --silent "http://wiki.1context.localhost:$PORT/for-you.talk" \
  | grep -q 'class="opctx-tier-badge" data-tier="private" title="Only you">Private</span>'; then
  echo "Packaged For You talk page is not private." >&2
  exit 1
fi

state_response="$(curl --fail --silent --request PATCH \
  --header "Content-Type: application/json" \
  --data '{"settings":{"theme":"dark"},"bookmarks":[{"title":"For You","url":"/for-you"}]}' \
  "http://127.0.0.1:$API_PORT/api/wiki/state")"
grep -q "theme" <<<"$state_response"
assert_url_contains "http://127.0.0.1:$API_PORT/api/wiki/search?q=context" "matches"
assert_url_contains "http://wiki.1context.localhost:$PORT/api/wiki/search?q=context" "matches"

"$CLI" wiki refresh | grep -q "Refreshed 1Context wiki."
assert_url_contains "http://wiki.1context.localhost:$PORT/for-you" "How This Page Works"

"$CLI" status --debug | grep -q "Bundled Caddy: yes"
"$CLI" status --debug | grep -q "URL: http://wiki.1context.localhost:$PORT/your-context"
printf '{"cwd":"%s"}\n' "$ROOT" \
  | "$CLI" agent hook --provider codex --event SessionStart \
  | grep -q "wiki.1context.localhost:$PORT"

"$CLI" quit | grep -q "1Context quit"
echo "Release app local smoke passed."
