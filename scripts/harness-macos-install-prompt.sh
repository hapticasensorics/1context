#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/dist/1Context.app}"
OUT="${2:-/tmp/1context-install-prompt.png}"
MENU="$APP/Contents/MacOS/1Context"

if [[ ! -d "$APP" || ! -x "$MENU" ]]; then
  echo "Built app not found: $APP" >&2
  echo "Build one first with: ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh" >&2
  exit 1
fi

DESTINATION="$(mktemp -d /tmp/1ctx-install-destination-XXXXXX)/Applications/1Context.app"
LOG="/tmp/1context-install-prompt.log"

pkill -x 1Context >/dev/null 2>&1 || true
rm -f "$OUT" "$LOG"

echo "Launching install prompt with temporary destination:"
echo "  $DESTINATION"

ONECONTEXT_APP_INSTALL_DESTINATION="$DESTINATION" \
ONECONTEXT_NO_UPDATE_CHECK=1 \
"$MENU" >"$LOG" 2>&1 &
MENU_PID=$!

cleanup() {
  kill "$MENU_PID" >/dev/null 2>&1 || true
  wait "$MENU_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

sleep 1.5
osascript -e "tell application \"System Events\" to set frontmost of first process whose unix id is $MENU_PID to true" >/dev/null 2>&1 || true
sleep 0.5
screencapture -x "$OUT"

if [[ ! -s "$OUT" ]]; then
  echo "Install prompt screenshot was not created." >&2
  exit 1
fi

echo "$OUT"
