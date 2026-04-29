#!/usr/bin/env bash
set -euo pipefail

if [[ "${ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE:-0}" != "1" ]]; then
  echo "Refusing to run LaunchAgent smoke without ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1." >&2
  echo "This test uses the real 1Context user LaunchAgent labels." >&2
  exit 1
fi

if [[ "${CI:-}" != "true" && "${ONECONTEXT_ALLOW_INSTALLED_STATE_MUTATION:-0}" != "1" ]]; then
  if [[ -e "/Applications/1Context.app" ]] ||
    launchctl print "gui/$(id -u)/com.haptica.1context" >/dev/null 2>&1 ||
    launchctl print "gui/$(id -u)/com.haptica.1context.menu" >/dev/null 2>&1; then
    echo "Refusing to run LaunchAgent smoke against an installed local 1Context." >&2
    echo "This test mutates the real 1Context LaunchAgent labels." >&2
    echo "Set ONECONTEXT_ALLOW_INSTALLED_STATE_MUTATION=1 to run it anyway." >&2
    exit 1
  fi
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
ARCH="${ONECONTEXT_ARCH:-arm64}"
ARCHIVE="$ROOT/dist/1context-$VERSION-macos-$ARCH.tar.gz"
WORK_DIR="$(mktemp -d /tmp/1ctx-launch-agent-pkg-XXXXXX)"
STATE_DIR="$(mktemp -d /tmp/1ctx-launch-agent-state-XXXXXX)"
CANONICAL_APP_SUPPORT="$HOME/Library/Application Support/1Context"
CANONICAL_DESIRED_STATE="$CANONICAL_APP_SUPPORT/desired-state"
CANONICAL_DESIRED_STATE_BACKUP="$STATE_DIR/canonical-desired-state.backup"
CANONICAL_DESIRED_STATE_EXISTED=0

if [[ -f "$CANONICAL_DESIRED_STATE" ]]; then
  cp "$CANONICAL_DESIRED_STATE" "$CANONICAL_DESIRED_STATE_BACKUP"
  CANONICAL_DESIRED_STATE_EXISTED=1
fi

cleanup() {
  set +e
  export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
  export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
  export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
  export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
  export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
  export ONECONTEXT_NO_UPDATE_CHECK=1
  "$WORK_DIR"/1context-*/scripts/uninstall-macos-launch-agents.sh >/dev/null 2>&1
  for label in com.haptica.1context.menu com.haptica.1context; do
    launchctl bootout "gui/$(id -u)/$label" >/dev/null 2>&1
  done
  if [[ "$CANONICAL_DESIRED_STATE_EXISTED" == "1" ]]; then
    mkdir -p "$CANONICAL_APP_SUPPORT"
    cp "$CANONICAL_DESIRED_STATE_BACKUP" "$CANONICAL_DESIRED_STATE"
  else
    rm -f "$CANONICAL_DESIRED_STATE"
  fi
  rm -rf "$WORK_DIR" "$STATE_DIR"
}
trap cleanup EXIT

ALLOW_UNNOTARIZED=1 NOTARIZE=0 "$ROOT/scripts/package-macos-release.sh" >/dev/null
tar -C "$WORK_DIR" -xzf "$ARCHIVE"

PACKAGE_DIR="$WORK_DIR/1context-$VERSION-macos-$ARCH"
APP_PATH="$PACKAGE_DIR/1Context.app"
CLI_PATH="$PACKAGE_DIR/bin/1context"
RUNTIME_LABEL="com.haptica.1context"
MENU_LABEL="com.haptica.1context.menu"
RUNTIME_PLIST="$HOME/Library/LaunchAgents/$RUNTIME_LABEL.plist"
MENU_PLIST="$HOME/Library/LaunchAgents/$MENU_LABEL.plist"
STATUS_LOG="/tmp/1ctx-launch-agent-status.log"
STATUS_ERR="/tmp/1ctx-launch-agent-status.err"

dump_diagnostics() {
  {
    echo "=== 1Context LaunchAgent smoke diagnostics ==="
    echo "Version: $VERSION"
    echo "Package: $PACKAGE_DIR"
    echo "App: $APP_PATH"
    echo "CLI: $CLI_PATH"
    echo
    echo "=== status stdout ==="
    cat "$STATUS_LOG" 2>/dev/null || true
    echo
    echo "=== status stderr ==="
    cat "$STATUS_ERR" 2>/dev/null || true
    echo
    echo "=== runtime launchctl ==="
    launchctl print "gui/$(id -u)/$RUNTIME_LABEL" 2>&1 || true
    echo
    echo "=== menu launchctl ==="
    launchctl print "gui/$(id -u)/$MENU_LABEL" 2>&1 || true
    echo
    echo "=== runtime plist ==="
    cat "$RUNTIME_PLIST" 2>/dev/null || true
    echo
    echo "=== menu plist ==="
    cat "$MENU_PLIST" 2>/dev/null || true
    echo
    echo "=== runtime log ==="
    tail -n 120 "$ONECONTEXT_LOG_DIR/1contextd.log" 2>/dev/null || true
    echo
    echo "=== menu log ==="
    tail -n 120 "$ONECONTEXT_LOG_DIR/menu.log" 2>/dev/null || true
  } >&2
}

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
export ONECONTEXT_NO_UPDATE_CHECK=1
export ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES=1
export ONECONTEXT_AGENT_ALLOW_ENV_OVERRIDES=1
export ONECONTEXT_CLAUDE_SETTINGS_PATH="$STATE_DIR/.claude/settings.json"

mkdir -p "$CANONICAL_APP_SUPPORT"
printf 'running\n' > "$CANONICAL_DESIRED_STATE"
"$PACKAGE_DIR/scripts/install-macos-launch-agents.sh" "$APP_PATH" "$CLI_PATH"

grep -q "agent hook --provider claude --event SessionStart" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"
grep -q "/opt/homebrew/bin/1context\\|/usr/local/bin/1context\\|1context-cli" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"

launchctl print "gui/$(id -u)/$RUNTIME_LABEL" >/dev/null
launchctl print "gui/$(id -u)/$MENU_LABEL" >/dev/null

for attempt in {1..60}; do
  if "$CLI_PATH" status --debug >"$STATUS_LOG" 2>"$STATUS_ERR" &&
    grep -q "Socket: responding" "$STATUS_LOG"; then
    break
  fi
  if [[ "$attempt" == "60" ]]; then
    dump_diagnostics
    exit 1
  fi
  sleep 0.25
done

test "$(plutil -extract ProgramArguments.0 raw "$RUNTIME_PLIST")" = "$APP_PATH/Contents/MacOS/1contextd"
test "$(plutil -extract ProgramArguments.0 raw "$MENU_PLIST")" = "$APP_PATH/Contents/MacOS/1Context"

"$PACKAGE_DIR/scripts/uninstall-macos-launch-agents.sh"

if grep -q "agent hook --provider claude" "$ONECONTEXT_CLAUDE_SETTINGS_PATH" 2>/dev/null; then
  echo "Claude hook remained after package uninstall." >&2
  exit 1
fi

if launchctl print "gui/$(id -u)/$RUNTIME_LABEL" >/dev/null 2>&1; then
  echo "Runtime LaunchAgent still loaded after uninstall." >&2
  exit 1
fi

if launchctl print "gui/$(id -u)/$MENU_LABEL" >/dev/null 2>&1; then
  echo "Menu LaunchAgent still loaded after uninstall." >&2
  exit 1
fi

printf 'stopped\n' > "$CANONICAL_DESIRED_STATE"
"$PACKAGE_DIR/scripts/install-macos-launch-agents.sh" "$APP_PATH" "$CLI_PATH"

grep -q "agent hook --provider claude --event SessionStart" "$ONECONTEXT_CLAUDE_SETTINGS_PATH"

launchctl print "gui/$(id -u)/$MENU_LABEL" >/dev/null

if launchctl print "gui/$(id -u)/$RUNTIME_LABEL" >/dev/null 2>&1; then
  echo "Runtime LaunchAgent should not start when desired-state is stopped." >&2
  exit 1
fi

"$PACKAGE_DIR/scripts/uninstall-macos-launch-agents.sh"

if grep -q "agent hook --provider claude" "$ONECONTEXT_CLAUDE_SETTINGS_PATH" 2>/dev/null; then
  echo "Claude hook remained after stopped-state package uninstall." >&2
  exit 1
fi

if launchctl print "gui/$(id -u)/$MENU_LABEL" >/dev/null 2>&1; then
  echo "Menu LaunchAgent still loaded after stopped-state uninstall." >&2
  exit 1
fi

echo "1Context packaged LaunchAgent smoke passed."
