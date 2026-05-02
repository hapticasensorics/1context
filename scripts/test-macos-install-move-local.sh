#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/dist/1Context.app}"
MENU="$APP/Contents/MacOS/1Context"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"

if [[ ! -d "$APP" || ! -x "$MENU" ]]; then
  echo "Built app not found: $APP" >&2
  echo "Build one first with: ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d /tmp/1ctx-install-move-XXXXXX)"
DESTINATION="$WORK_DIR/Applications/1Context.app"
LOG="$WORK_DIR/install-move.log"

cleanup() {
  pkill -f "$DESTINATION/Contents/MacOS/1Context" >/dev/null 2>&1 || true
  pkill -f "$APP/Contents/MacOS/1Context" >/dev/null 2>&1 || true
  rm -rf "$WORK_DIR" >/dev/null 2>&1 || true
}
trap cleanup EXIT

pkill -x 1Context >/dev/null 2>&1 || true

ONECONTEXT_APP_INSTALL_DESTINATION="$DESTINATION" \
ONECONTEXT_NO_UPDATE_CHECK=1 \
"$MENU" >"$LOG" 2>&1 &
MENU_PID=$!

for _ in {1..50}; do
  if osascript <<OSA >/dev/null 2>&1
tell application "System Events"
  tell process "1Context"
    if exists button "Move" of window 1 then
      click button "Move" of window 1
      return
    end if
  end tell
end tell
error "Move button not ready"
OSA
  then
    break
  fi
  sleep 0.1
done

for _ in {1..100}; do
  moved_version="$(plutil -extract CFBundleShortVersionString raw "$DESTINATION/Contents/Info.plist" 2>/dev/null || true)"
  if [[ "$moved_version" == "$VERSION" ]]; then
    break
  fi
  sleep 0.1
done

if [[ ! -d "$DESTINATION" ]]; then
  echo "App was not moved to temporary Applications destination." >&2
  echo "Log: $LOG" >&2
  exit 1
fi

if [[ "$(plutil -extract CFBundleShortVersionString raw "$DESTINATION/Contents/Info.plist" 2>/dev/null || true)" != "$VERSION" ]]; then
  echo "Moved app version does not match VERSION." >&2
  exit 1
fi

if [[ ! -x "$DESTINATION/Contents/MacOS/1Context" || ! -x "$DESTINATION/Contents/MacOS/1context-cli" ]]; then
  echo "Moved app is missing expected executables." >&2
  exit 1
fi

codesign --verify --deep --strict "$DESTINATION" >/dev/null

for _ in {1..100}; do
  if ps -axo args= | grep -F "$DESTINATION/Contents/MacOS/1Context" | grep -v grep >/dev/null; then
    echo "Install move smoke passed."
    exit 0
  fi
  sleep 0.1
done

echo "Moved app did not relaunch from temporary Applications destination." >&2
echo "Log: $LOG" >&2
exit 1
