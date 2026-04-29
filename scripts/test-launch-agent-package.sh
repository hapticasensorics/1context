#!/usr/bin/env bash
set -euo pipefail

if [[ "${ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE:-0}" != "1" ]]; then
  echo "Refusing to run LaunchAgent smoke without ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1." >&2
  echo "This test uses the real 1Context user LaunchAgent labels." >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
ARCH="${ONECONTEXT_ARCH:-arm64}"
ARCHIVE="$ROOT/dist/1context-$VERSION-macos-$ARCH.tar.gz"
WORK_DIR="$(mktemp -d /tmp/1ctx-launch-agent-pkg-XXXXXX)"
STATE_DIR="$(mktemp -d /tmp/1ctx-launch-agent-state-XXXXXX)"

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

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
export ONECONTEXT_NO_UPDATE_CHECK=1
export ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES=1

"$PACKAGE_DIR/scripts/install-macos-launch-agents.sh" "$APP_PATH" "$CLI_PATH"

launchctl print "gui/$(id -u)/$RUNTIME_LABEL" >/dev/null
launchctl print "gui/$(id -u)/$MENU_LABEL" >/dev/null
"$CLI_PATH" status --debug | grep -q "Socket: responding"

test "$(plutil -extract ProgramArguments.0 raw "$RUNTIME_PLIST")" = "$APP_PATH/Contents/MacOS/1contextd"
test "$(plutil -extract ProgramArguments.0 raw "$MENU_PLIST")" = "$APP_PATH/Contents/MacOS/1Context"

"$PACKAGE_DIR/scripts/uninstall-macos-launch-agents.sh"

if launchctl print "gui/$(id -u)/$RUNTIME_LABEL" >/dev/null 2>&1; then
  echo "Runtime LaunchAgent still loaded after uninstall." >&2
  exit 1
fi

if launchctl print "gui/$(id -u)/$MENU_LABEL" >/dev/null 2>&1; then
  echo "Menu LaunchAgent still loaded after uninstall." >&2
  exit 1
fi

echo "1Context packaged LaunchAgent smoke passed."
