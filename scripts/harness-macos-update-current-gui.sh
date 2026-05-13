#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/lib-gui-evidence.sh"

APP="${ONECONTEXT_INSTALLED_APP:-/Applications/1Context.app}"
CLI="${ONECONTEXT_INSTALLED_CLI:-$APP/Contents/MacOS/1context-cli}"
EXPECTED_VERSION="${ONECONTEXT_EXPECTED_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_GUI_POLICY_EVIDENCE_DIR:-$ROOT/dist/gui-policy-evidence/local-$EXPECTED_VERSION-current-$STAMP}"

fail() {
  echo "macOS update-current GUI harness failed: $*" >&2
  mkdir -p "$EVIDENCE_DIR"
  printf '%s\n' "$*" > "$EVIDENCE_DIR/failure.txt"
  exit 1
}

require_text() {
  local needle="$1"
  local file="$2"
  local label="$3"
  if ! grep -Fq "$needle" "$file"; then
    fail "$label did not contain expected text: $needle"
  fi
}

reject_text() {
  local pattern="$1"
  local file="$2"
  local label="$3"
  if grep -Eiq "$pattern" "$file"; then
    fail "$label contained unwanted update text matching: $pattern"
  fi
}

[[ -d "$APP" ]] || fail "Installed app not found: $APP"
[[ -x "$CLI" ]] || fail "Installed CLI not found or not executable: $CLI"

mkdir -p "$EVIDENCE_DIR"

installed_version="$("$CLI" --version 2>/dev/null || true)"
if [[ "$installed_version" != "$EXPECTED_VERSION" ]]; then
  fail "Installed CLI version is $installed_version, expected $EXPECTED_VERSION"
fi

{
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "app=$APP"
  echo "cli=$CLI"
  echo "expected_version=$EXPECTED_VERSION"
  echo "installed_version=$installed_version"
  echo "evidence_dir=$EVIDENCE_DIR"
} > "$EVIDENCE_DIR/environment.txt"

printf '%s\n' "$installed_version" > "$EVIDENCE_DIR/version.txt"
"$CLI" status --debug > "$EVIDENCE_DIR/status-debug.txt" 2>&1 || true
capture_menu "$EVIDENCE_DIR/menu.txt"

osascript > "$EVIDENCE_DIR/open-menu.applescript.txt" 2>&1 <<'APPLESCRIPT' || true
tell application "System Events"
  tell process "1Context"
    click menu bar item 1 of menu bar 1
  end tell
end tell
APPLESCRIPT
sleep 1
capture_screenshot "$EVIDENCE_DIR/menu-open.png"
osascript -e 'tell application "System Events" to key code 53' >/dev/null 2>&1 || true

require_text "Version $EXPECTED_VERSION" "$EVIDENCE_DIR/menu.txt" "menu capture"
require_text "Check for Updates" "$EVIDENCE_DIR/menu.txt" "menu capture"
reject_text "Please Update|release notes|Install and Relaunch|installer|relaunch the app|Update failed|Please contact support" "$EVIDENCE_DIR/menu.txt" "menu capture"

click_menu_item "Check for Updates" > "$EVIDENCE_DIR/click-check-for-updates.txt" 2>&1

deadline=$(( "$(date +%s)" + 30 ))
attempt=0
while true; do
  attempt=$((attempt + 1))
  accessibility="$EVIDENCE_DIR/manual-up-to-date-accessibility-$attempt.txt"
  screenshot="$EVIDENCE_DIR/manual-up-to-date-desktop-$attempt.png"
  capture_accessibility "$accessibility"
  capture_screenshot "$screenshot"
  cp "$accessibility" "$EVIDENCE_DIR/manual-up-to-date-accessibility.txt"
  cp "$screenshot" "$EVIDENCE_DIR/manual-up-to-date-desktop.png"

  if grep -Fq "1Context is up to date." "$EVIDENCE_DIR/manual-up-to-date-accessibility.txt"; then
    printf '%s\n' "title=1Context is up to date." > "$EVIDENCE_DIR/manual-up-to-date-message.txt"
    require_text "button"$'\t'"OK" "$EVIDENCE_DIR/manual-up-to-date-accessibility.txt" "manual update alert"
    reject_text "release notes|verify the signed release|installer|relaunch the app|Update failed|Please contact support" "$EVIDENCE_DIR/manual-up-to-date-accessibility.txt" "manual update alert"
    click_window_button "OK" > "$EVIDENCE_DIR/click-ok.txt" 2>&1 || true
    break
  fi

  reject_text "release notes|verify the signed release|installer|relaunch the app|Update failed|Please contact support" "$EVIDENCE_DIR/manual-up-to-date-accessibility.txt" "manual update alert"
  if (( "$(date +%s)" >= deadline )); then
    fail "Timed out waiting for manual up-to-date alert."
  fi
  sleep 1
done

require_text "Version: $EXPECTED_VERSION" "$EVIDENCE_DIR/status-debug.txt" "status debug"
require_text "Health: OK" "$EVIDENCE_DIR/status-debug.txt" "status debug"
require_text "Menu Bar: running" "$EVIDENCE_DIR/status-debug.txt" "status debug"

{
  echo "result=passed"
  echo "version=$EXPECTED_VERSION"
  echo "evidence_dir=$EVIDENCE_DIR"
} > "$EVIDENCE_DIR/result.txt"

printf '%s\n' "$EVIDENCE_DIR"
