#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/lib-gui-evidence.sh"

APP="${ONECONTEXT_INSTALLED_APP:-/Applications/1Context.app}"
CLI="${ONECONTEXT_INSTALLED_CLI:-$APP/Contents/MacOS/1context-cli}"
EXPECTED_VERSION="${ONECONTEXT_EXPECTED_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_SETUP_GUI_EVIDENCE_DIR:-$ROOT/dist/setup-gui-evidence/local-$EXPECTED_VERSION-setup-gui-$STAMP}"
READY_DIR="$EVIDENCE_DIR/granted-ready"
BLOCKED_DIR="$EVIDENCE_DIR/blocked-open-wiki"
BLOCKED_HARNESS_PID=""
RESTORE_INSTALLED_APP_ON_EXIT=0

cleanup() {
  if [[ -n "$BLOCKED_HARNESS_PID" ]]; then
    kill "$BLOCKED_HARNESS_PID" >/dev/null 2>&1 || true
    wait "$BLOCKED_HARNESS_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$RESTORE_INSTALLED_APP_ON_EXIT" == "1" && -d "$APP" ]]; then
    open "$APP" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

fail() {
  echo "macOS setup GUI harness failed: $*" >&2
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
    fail "$label contained unwanted text matching: $pattern"
  fi
}

capture_accessibility_for_pid() {
  local pid="$1"
  local output="$2"
  osascript - "$pid" >"$output" 2>&1 <<'APPLESCRIPT' || true
on run argv
  set targetPID to item 1 of argv as integer
  tell application "System Events"
    set reportLines to {}
    try
      set targetProc to first application process whose unix id is targetPID
    on error
      set end of reportLines to "missing process pid=" & (targetPID as text)
      set AppleScript's text item delimiters to linefeed
      return reportLines as text
    end try
    set end of reportLines to "process=" & (name of targetProc as text) & tab & "pid=" & (targetPID as text)
    repeat with win in windows of targetProc
      set end of reportLines to "window=" & (name of win as text) & tab & (description of win as text)
      repeat with textRef in static texts of win
        try
          set end of reportLines to "static-text" & tab & (name of textRef as text)
        end try
        try
          set end of reportLines to "static-text-value" & tab & (value of textRef as text)
        end try
      end repeat
      repeat with buttonRef in buttons of win
        try
          set end of reportLines to "button" & tab & (name of buttonRef as text)
        end try
      end repeat
      repeat with groupRef in groups of win
        repeat with textRef in static texts of groupRef
          try
            set end of reportLines to "group-static-text" & tab & (name of textRef as text)
          end try
          try
            set end of reportLines to "group-static-text-value" & tab & (value of textRef as text)
          end try
        end repeat
        repeat with buttonRef in buttons of groupRef
          try
            set end of reportLines to "group-button" & tab & (name of buttonRef as text)
          end try
        end repeat
      end repeat
      repeat with elementRef in entire contents of win
        set lineText to ""
        try
          set lineText to lineText & "role=" & (role of elementRef as text)
        end try
        try
          set lineText to lineText & tab & "description=" & (description of elementRef as text)
        end try
        try
          set lineText to lineText & tab & "name=" & (name of elementRef as text)
        end try
        try
          set lineText to lineText & tab & "value=" & (value of elementRef as text)
        end try
        if lineText is not "" then set end of reportLines to lineText
      end repeat
    end repeat
    set AppleScript's text item delimiters to linefeed
    return reportLines as text
  end tell
end run
APPLESCRIPT
}

capture_windows_for_pid() {
  local pid="$1"
  local output="$2"
  osascript - "$pid" >"$output" 2>&1 <<'APPLESCRIPT' || true
on run argv
  set targetPID to item 1 of argv as integer
  tell application "System Events"
    set reportLines to {}
    try
      set targetProc to first application process whose unix id is targetPID
    on error
      set end of reportLines to "missing process pid=" & (targetPID as text)
      set AppleScript's text item delimiters to linefeed
      return reportLines as text
    end try
    repeat with win in windows of targetProc
      set end of reportLines to (name of targetProc as text) & tab & (name of win as text) & tab & (description of win as text)
    end repeat
    set AppleScript's text item delimiters to linefeed
    return reportLines as text
  end tell
end run
APPLESCRIPT
}

click_menu_item_for_pid() {
  local pid="$1"
  local menu_item="$2"
  osascript - "$pid" "$menu_item" <<'APPLESCRIPT'
on run argv
  set targetPID to item 1 of argv as integer
  set targetItem to item 2 of argv as text
  tell application "System Events"
    set targetProc to first application process whose unix id is targetPID
    tell targetProc
      click menu bar item 1 of menu bar 1
      delay 0.3
      click menu item targetItem of menu 1 of menu bar item 1 of menu bar 1
    end tell
  end tell
end run
APPLESCRIPT
}

click_settings_setup_for_pid() {
  local pid="$1"
  osascript - "$pid" <<'APPLESCRIPT'
on run argv
  set targetPID to item 1 of argv as integer
  tell application "System Events"
    set targetProc to first application process whose unix id is targetPID
    tell targetProc
      click menu bar item 1 of menu bar 1
      delay 0.3
      click menu item "Setup..." of menu 1 of menu item "Settings" of menu 1 of menu bar item 1 of menu bar 1
    end tell
  end tell
end run
APPLESCRIPT
}

wait_for_process_pid() {
  local executable_path="$1"
  local deadline=$(( "$(date +%s)" + 30 ))
  while true; do
    local pid
    pid="$(pgrep -f "$executable_path" | head -1 || true)"
    if [[ -n "$pid" ]]; then
      printf '%s\n' "$pid"
      return
    fi
    if (( "$(date +%s)" >= deadline )); then
      fail "Timed out waiting for process: $executable_path"
    fi
    sleep 1
  done
}

wait_for_pid_accessibility_text() {
  local pid="$1"
  local needle="$2"
  local output="$3"
  local deadline=$(( "$(date +%s)" + 30 ))
  local attempt=0
  while true; do
    attempt=$((attempt + 1))
    capture_accessibility_for_pid "$pid" "$output.$attempt"
    cp "$output.$attempt" "$output"
    if grep -Fq "$needle" "$output"; then
      return
    fi
    if (( "$(date +%s)" >= deadline )); then
      fail "Timed out waiting for pid $pid accessibility text: $needle"
    fi
    sleep 1
  done
}

close_setup_windows_for_pid() {
  local pid="$1"
  osascript - "$pid" >/dev/null 2>&1 <<'APPLESCRIPT' || true
on run argv
  set targetPID to item 1 of argv as integer
  tell application "System Events"
    try
      set targetProc to first application process whose unix id is targetPID
      repeat with win in windows of targetProc
        if (name of win as text) is "1Context Setup" then
          try
            click button 1 of win
          end try
        end if
      end repeat
    end try
  end tell
end run
APPLESCRIPT
}

stop_installed_menu_for_isolated_proof() {
  RESTORE_INSTALLED_APP_ON_EXIT=1
  launchctl bootout "gui/$(id -u)/com.haptica.1context.menu" >/dev/null 2>&1 || true
  while read -r installed_pid; do
    [[ -n "$installed_pid" ]] || continue
    kill "$installed_pid" >/dev/null 2>&1 || true
  done < <(pgrep -f "$APP/Contents/MacOS/1Context" || true)
  sleep 2
}

run_granted_ready_proof() {
  mkdir -p "$READY_DIR"
  [[ -d "$APP" ]] || fail "Installed app not found: $APP"
  [[ -x "$CLI" ]] || fail "Installed CLI not found or not executable: $CLI"

  local installed_version
  installed_version="$("$CLI" --version 2>/dev/null || true)"
  [[ "$installed_version" == "$EXPECTED_VERSION" ]] || fail "Installed CLI version is $installed_version, expected $EXPECTED_VERSION"

  open "$APP"
  local pid
  pid="$(wait_for_process_pid "$APP/Contents/MacOS/1Context")"
  printf '%s\n' "$pid" > "$READY_DIR/pid.txt"

  close_setup_windows_for_pid "$pid"
  click_settings_setup_for_pid "$pid" > "$READY_DIR/open-setup.applescript.txt" 2>&1
  wait_for_pid_accessibility_text "$pid" "Local Wiki Access is ready." "$READY_DIR/accessibility.txt"
  capture_windows_for_pid "$pid" "$READY_DIR/windows.txt"
  capture_screenshot "$READY_DIR/setup-window.png"

  require_text "Set Up 1Context" "$READY_DIR/accessibility.txt" "granted setup window"
  require_text "Local Wiki Access is ready." "$READY_DIR/accessibility.txt" "granted setup window"
  require_text "Local Wiki Access" "$READY_DIR/accessibility.txt" "granted setup window"
  require_text "Open Wiki" "$READY_DIR/accessibility.txt" "granted setup window"
  reject_text "Check Again|Granting Setup|Finish setup|needs setup|Update failed|release notes" "$READY_DIR/accessibility.txt" "granted setup window"

  {
    echo "result=passed"
    echo "mode=granted-ready"
    echo "version=$EXPECTED_VERSION"
    echo "pid=$pid"
  } > "$READY_DIR/result.txt"
}

run_blocked_open_wiki_proof() {
  mkdir -p "$BLOCKED_DIR"
  local state_dir="$BLOCKED_DIR/state"
  local harness_app="$BLOCKED_DIR/1Context-SetupHarness.app"
  local previous_dist_app="$BLOCKED_DIR/previous-dist-1Context.app"
  local menu_log="$BLOCKED_DIR/menu.log"
  rm -rf "$state_dir" "$harness_app" "$previous_dist_app"
  mkdir -p "$state_dir"
  if [[ -d "$ROOT/dist/1Context.app" ]]; then
    ditto "$ROOT/dist/1Context.app" "$previous_dist_app"
  fi

  (
    cd "$ROOT"
    env \
      ONECONTEXT_BUNDLE_IDENTIFIER="com.haptica.1context.setup-harness" \
      ONECONTEXT_SIGNING_MODE=adhoc \
      ./scripts/build-macos-app.sh
  ) > "$BLOCKED_DIR/build.log" 2>&1
  ditto "$ROOT/dist/1Context.app" "$harness_app"
  if [[ -d "$previous_dist_app" ]]; then
    rm -rf "$ROOT/dist/1Context.app"
    ditto "$previous_dist_app" "$ROOT/dist/1Context.app"
  else
    rm -rf "$ROOT/dist/1Context.app"
  fi

  env \
    ONECONTEXT_SKIP_APP_INSTALL_PROMPT=1 \
    ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
    ONECONTEXT_APP_SUPPORT_DIR="$state_dir/Application Support/1Context" \
    ONECONTEXT_USER_CONTENT_DIR="$state_dir/1Context" \
    ONECONTEXT_LOG_DIR="$state_dir/Logs/1Context" \
    ONECONTEXT_CACHE_DIR="$state_dir/Caches/1Context" \
    ONECONTEXT_WIKI_URL_MODE=local-https-portless \
    ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR="$state_dir/local-web-system" \
    ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR="$state_dir/Logs/1Context" \
    ONECONTEXT_LOCAL_WEB_SERVICE_STATUS=notFound \
    ONECONTEXT_LOCAL_WEB_LAUNCH_DAEMON_PATH="$state_dir/local-web-system/missing.plist" \
    ONECONTEXT_LOCAL_WEB_PROXY_EXECUTABLE_PATH="$state_dir/local-web-system/bin/1context-local-web-proxy" \
    "$harness_app/Contents/MacOS/1Context" > "$menu_log" 2>&1 &
  local pid=$!
  BLOCKED_HARNESS_PID="$pid"
  printf '%s\n' "$pid" > "$BLOCKED_DIR/pid.txt"

  local deadline=$(( "$(date +%s)" + 30 ))
  while true; do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      fail "Setup harness app exited early. See $menu_log"
    fi
    capture_accessibility_for_pid "$pid" "$BLOCKED_DIR/preclick-accessibility.txt"
    if grep -Fq "process=1Context" "$BLOCKED_DIR/preclick-accessibility.txt"; then
      break
    fi
    if (( "$(date +%s)" >= deadline )); then
      fail "Timed out waiting for setup harness process accessibility."
    fi
    sleep 1
  done

  click_menu_item_for_pid "$pid" "Open Wiki" > "$BLOCKED_DIR/click-open-wiki.applescript.txt" 2>&1
  wait_for_pid_accessibility_text "$pid" "Finish setup to open your wiki." "$BLOCKED_DIR/accessibility.txt"
  capture_windows_for_pid "$pid" "$BLOCKED_DIR/windows.txt"
  capture_screenshot "$BLOCKED_DIR/setup-window.png"

  require_text "Set Up 1Context" "$BLOCKED_DIR/accessibility.txt" "blocked setup window"
  require_text "Finish setup to open your wiki." "$BLOCKED_DIR/accessibility.txt" "blocked setup window"
  require_text "Local Wiki Access" "$BLOCKED_DIR/accessibility.txt" "blocked setup window"
  reject_text "Local Wiki Access is ready.|Update failed|release notes|Install and Relaunch" "$BLOCKED_DIR/accessibility.txt" "blocked setup window"

  {
    echo "result=passed"
    echo "mode=blocked-open-wiki"
    echo "version=$EXPECTED_VERSION"
    echo "pid=$pid"
  } > "$BLOCKED_DIR/result.txt"

  kill "$pid" >/dev/null 2>&1 || true
  wait "$pid" >/dev/null 2>&1 || true
  BLOCKED_HARNESS_PID=""
  open "$APP" >/dev/null 2>&1 || true
  RESTORE_INSTALLED_APP_ON_EXIT=0
}

mkdir -p "$EVIDENCE_DIR"
{
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "expected_version=$EXPECTED_VERSION"
  echo "installed_app=$APP"
  echo "evidence_dir=$EVIDENCE_DIR"
} > "$EVIDENCE_DIR/environment.txt"

run_granted_ready_proof
stop_installed_menu_for_isolated_proof
run_blocked_open_wiki_proof

{
  echo "result=passed"
  echo "version=$EXPECTED_VERSION"
  echo "granted_ready=$READY_DIR"
  echo "blocked_open_wiki=$BLOCKED_DIR"
} > "$EVIDENCE_DIR/result.txt"

printf '%s\n' "$EVIDENCE_DIR"
