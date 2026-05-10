#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${ONECONTEXT_INSTALLED_APP:-/Applications/1Context.app}"
APPCAST_URL="${ONECONTEXT_REMOTE_APPCAST_URL:-https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml}"
EXPECTED_NEW_VERSION="${ONECONTEXT_EXPECTED_NEW_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
EXPECTED_OLD_VERSION="${ONECONTEXT_EXPECTED_OLD_VERSION:-}"
EXPECTED_UPDATE_CLASS="${ONECONTEXT_EXPECTED_UPDATE_CLASS:-mandatory}"
KICK_MODE="${ONECONTEXT_UPDATE_PROOF_KICK_MODE:-relaunch}"
TIMEOUT_SECONDS="${ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS:-360}"
POLL_SECONDS="${ONECONTEXT_UPDATE_PROOF_POLL_SECONDS:-5}"
OPTIONAL_DISCOVERY_TIMEOUT_SECONDS="${ONECONTEXT_OPTIONAL_DISCOVERY_TIMEOUT_SECONDS:-120}"
OPTIONAL_QUIET_SECONDS="${ONECONTEXT_OPTIONAL_QUIET_SECONDS:-20}"
OPTIONAL_PROMPT_TIMEOUT_SECONDS="${ONECONTEXT_OPTIONAL_PROMPT_TIMEOUT_SECONDS:-15}"

installed_plist_version() {
  plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist" 2>/dev/null || true
}

installed_cli_version() {
  if [[ -x "$APP/Contents/MacOS/1context-cli" ]]; then
    "$APP/Contents/MacOS/1context-cli" --version 2>/dev/null || true
  fi
}

if [[ ! -d "$APP" ]]; then
  echo "Installed app not found: $APP" >&2
  exit 1
fi
if [[ "$EXPECTED_UPDATE_CLASS" != "mandatory" && "$EXPECTED_UPDATE_CLASS" != "optional" ]]; then
  echo "ONECONTEXT_EXPECTED_UPDATE_CLASS must be mandatory or optional." >&2
  exit 1
fi

if [[ -z "$EXPECTED_OLD_VERSION" ]]; then
  EXPECTED_OLD_VERSION="$(installed_plist_version)"
fi
if [[ -z "$EXPECTED_OLD_VERSION" ]]; then
  echo "Could not determine installed old version from $APP." >&2
  exit 1
fi

EVIDENCE_DIR="${ONECONTEXT_REMOTE_UPDATE_EVIDENCE_DIR:-$ROOT/dist/remote-update-evidence/$EXPECTED_OLD_VERSION-to-$EXPECTED_NEW_VERSION-$EXPECTED_UPDATE_CLASS-remote}"
mkdir -p "$EVIDENCE_DIR"

log() {
  printf '[remote-update-proof] %s\n' "$*"
}

capture_windows() {
  local output="$1"
  osascript >"$output" 2>&1 <<'APPLESCRIPT' || true
tell application "System Events"
  set reportLines to {}
  repeat with proc in application processes
    set procName to name of proc
    if procName contains "1Context" or procName contains "Sparkle" or procName contains "Updater" then
      repeat with win in windows of proc
        set end of reportLines to procName & tab & (name of win) & tab & (description of win)
      end repeat
    end if
  end repeat
  set AppleScript's text item delimiters to linefeed
  return reportLines as text
end tell
APPLESCRIPT
}

capture_accessibility() {
  local output="$1"
  osascript >"$output" 2>&1 <<'APPLESCRIPT' || true
tell application "System Events"
  set reportLines to {}
  repeat with proc in application processes
    set procName to name of proc
    if procName contains "1Context" or procName contains "Sparkle" or procName contains "Updater" then
      set end of reportLines to "process=" & procName
      repeat with win in windows of proc
        set end of reportLines to "window=" & (name of win) & tab & (description of win)
        repeat with elementRef in UI elements of win
          set lineText to "ui"
          try
            set lineText to lineText & tab & "role=" & (role of elementRef as text)
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
          if lineText is not "ui" then set end of reportLines to lineText
          try
            repeat with childRef in UI elements of elementRef
              set childText to "ui-child"
              try
                set childText to childText & tab & "role=" & (role of childRef as text)
              end try
              try
                set childText to childText & tab & "description=" & (description of childRef as text)
              end try
              try
                set childText to childText & tab & "name=" & (name of childRef as text)
              end try
              try
                set childText to childText & tab & "value=" & (value of childRef as text)
              end try
              if childText is not "ui-child" then set end of reportLines to childText
            end repeat
          end try
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
    end if
  end repeat
  set AppleScript's text item delimiters to linefeed
  return reportLines as text
end tell
APPLESCRIPT
}

capture_menu() {
  local output="$1"
  osascript >"$output" 2>&1 <<'APPLESCRIPT' || true
tell application "System Events"
  tell process "1Context"
    click menu bar item 1 of menu bar 1
    delay 0.5
    set reportLines to {}
    repeat with menuItemRef in menu items of menu 1 of menu bar item 1 of menu bar 1
      set itemName to name of menuItemRef as text
      set end of reportLines to itemName
      if itemName is "Settings" then
        try
          repeat with settingsItemRef in menu items of menu 1 of menuItemRef
            set end of reportLines to "  " & (name of settingsItemRef as text)
          end repeat
        end try
      end if
    end repeat
    key code 53
    set AppleScript's text item delimiters to linefeed
    return reportLines as text
  end tell
end tell
APPLESCRIPT
}

capture_hammerspoon_screenshot() {
  local output="$1"
  local endpoint="${ONECONTEXT_HAMMERSPOON_GUI_CAPTURE_URL:-http://127.0.0.1:8742/capture}"
  python3 - "$endpoint" "$output" > "$output.hammerspoon.json" 2> "$output.hammerspoon.err" <<'PY'
import json
import sys
import urllib.request
from pathlib import Path

endpoint = sys.argv[1]
path = sys.argv[2]
body = json.dumps({"target": "desktop", "path": path}).encode("utf-8")
request = urllib.request.Request(
    endpoint,
    data=body,
    headers={"Content-Type": "application/json"},
    method="POST",
)
with urllib.request.urlopen(request, timeout=3) as response:
    payload = json.loads(response.read().decode("utf-8"))
    print(json.dumps(payload, sort_keys=True))
    if response.status >= 400 or not payload.get("ok"):
        raise SystemExit(1)

output = Path(path)
if not output.exists() or output.stat().st_size == 0:
    raise SystemExit(1)
PY
}

capture_screenshot() {
  local output="$1"
  local mode="${ONECONTEXT_SCREENSHOT_MODE:-auto}"
  if [[ "$mode" != "screencapture" ]] && capture_hammerspoon_screenshot "$output"; then
    return
  fi
  if [[ "$mode" != "hammerspoon" ]]; then
    screencapture -x "$output" >/dev/null 2>&1 || true
  fi
}

write_versions() {
  local output="$1"
  {
    echo "plist=$(installed_plist_version)"
    echo "cli=$(installed_cli_version)"
  } > "$output"
}

validate_appcast() {
  local appcast="$1"
  python3 - "$appcast" "$EXPECTED_NEW_VERSION" "$EXPECTED_OLD_VERSION" "$EXPECTED_UPDATE_CLASS" <<'PY'
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

appcast = Path(sys.argv[1])
expected_new = sys.argv[2]
expected_old = sys.argv[3]
expected_class = sys.argv[4]
ns = {"sparkle": "http://www.andymatuschak.org/xml-namespaces/sparkle"}
root = ET.parse(appcast).getroot()
item = root.find("./channel/item")
if item is None:
    raise SystemExit("appcast missing channel/item")
version = item.findtext("sparkle:version", namespaces=ns)
if version != expected_new:
    raise SystemExit(f"appcast version {version!r} != expected {expected_new!r}")
critical = item.find("sparkle:criticalUpdate", namespaces=ns)
if expected_class == "mandatory" and critical is None:
    raise SystemExit("appcast missing sparkle:criticalUpdate")
if expected_class == "optional" and critical is not None:
    raise SystemExit("optional appcast unexpectedly contains sparkle:criticalUpdate")
minimum_auto = item.findtext("sparkle:minimumAutoupdateVersion", namespaces=ns)
if expected_class == "mandatory" and minimum_auto != expected_old:
    raise SystemExit(f"minimumAutoupdateVersion {minimum_auto!r} != expected {expected_old!r}")
if expected_class == "optional" and minimum_auto:
    raise SystemExit(f"optional appcast unexpectedly contains minimumAutoupdateVersion {minimum_auto!r}")
description = item.find("description")
if description is not None and (description.text or "").strip():
    raise SystemExit("appcast unexpectedly contains release notes description")
enclosure = item.find("enclosure")
if enclosure is None or not enclosure.attrib.get("{http://www.andymatuschak.org/xml-namespaces/sparkle}edSignature"):
    raise SystemExit("appcast enclosure missing sparkle:edSignature")
PY
}

click_menu_item() {
  local menu_item="$1"
  osascript <<APPLESCRIPT
tell application "System Events"
  tell process "1Context"
    click menu bar item 1 of menu bar 1
    delay 0.5
    click menu item "$menu_item" of menu 1 of menu bar item 1 of menu bar 1
  end tell
end tell
APPLESCRIPT
}

click_update_button() {
  osascript <<'APPLESCRIPT'
tell application "System Events"
  tell process "1Context"
    repeat with win in windows
      try
        click button "Update" of win
        return
      end try
    end repeat
    error "Update button not found"
  end tell
end tell
APPLESCRIPT
}

wait_for_optional_prompt() {
  local deadline=$(( "$(date +%s)" + OPTIONAL_PROMPT_TIMEOUT_SECONDS ))
  local attempt=0
  while true; do
    attempt=$((attempt + 1))
    capture_windows "$EVIDENCE_DIR/windows-optional-prompt-$attempt.txt"
    capture_accessibility "$EVIDENCE_DIR/accessibility-optional-prompt-$attempt.txt"
    capture_screenshot "$EVIDENCE_DIR/desktop-optional-prompt-$attempt.png"
    cp "$EVIDENCE_DIR/windows-optional-prompt-$attempt.txt" "$EVIDENCE_DIR/windows-optional-prompt.txt"
    cp "$EVIDENCE_DIR/accessibility-optional-prompt-$attempt.txt" "$EVIDENCE_DIR/accessibility-optional-prompt.txt"
    cp "$EVIDENCE_DIR/desktop-optional-prompt-$attempt.png" "$EVIDENCE_DIR/desktop-optional-prompt.png"
    if grep -Fq "Update 1Context?" "$EVIDENCE_DIR/accessibility-optional-prompt.txt" &&
      grep -Fq "A 1Context update is ready." "$EVIDENCE_DIR/accessibility-optional-prompt.txt"; then
      return 0
    fi
    if (( "$(date +%s)" >= deadline )); then
      return 1
    fi
    sleep 1
  done
}

kick_update_check() {
  case "$KICK_MODE" in
    none)
      return 0
      ;;
    relaunch)
      log "relaunching installed app to trigger launch update check"
      pkill -f "$APP/Contents/MacOS/1Context" >/dev/null 2>&1 || true
      sleep 2
      open "$APP"
      ;;
    open)
      log "opening installed app"
      open "$APP"
      ;;
    *)
      echo "Unknown ONECONTEXT_UPDATE_PROOF_KICK_MODE: $KICK_MODE" >&2
      exit 1
      ;;
  esac
}

{
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "app=$APP"
  echo "appcast_url=$APPCAST_URL"
  echo "expected_old_version=$EXPECTED_OLD_VERSION"
  echo "expected_new_version=$EXPECTED_NEW_VERSION"
  echo "expected_update_class=$EXPECTED_UPDATE_CLASS"
  echo "kick_mode=$KICK_MODE"
  echo "timeout_seconds=$TIMEOUT_SECONDS"
  echo "poll_seconds=$POLL_SECONDS"
  echo "optional_prompt_timeout_seconds=$OPTIONAL_PROMPT_TIMEOUT_SECONDS"
  echo "evidence_dir=$EVIDENCE_DIR"
} > "$EVIDENCE_DIR/environment.txt"

log "fetching live appcast"
curl --fail --location --silent "$APPCAST_URL" > "$EVIDENCE_DIR/live-appcast.xml"
validate_appcast "$EVIDENCE_DIR/live-appcast.xml"
if [[ "${ONECONTEXT_REMOTE_UPDATE_VALIDATE_REPO_POLICY:-1}" == "1" ]]; then
  "$ROOT/scripts/check-update-policy.sh" --appcast "$EVIDENCE_DIR/live-appcast.xml"
else
  log "skipping repo update-policy validation for staged appcast"
fi

write_versions "$EVIDENCE_DIR/version-before.txt"
capture_windows "$EVIDENCE_DIR/windows-before.txt"
capture_accessibility "$EVIDENCE_DIR/accessibility-before.txt"
capture_menu "$EVIDENCE_DIR/menu-before.txt"
capture_screenshot "$EVIDENCE_DIR/desktop-before.png"
if [[ -x "$APP/Contents/MacOS/1context-cli" ]]; then
  "$APP/Contents/MacOS/1context-cli" status --debug > "$EVIDENCE_DIR/status-before.txt" 2>&1 || true
fi

if [[ "$(installed_plist_version)" != "$EXPECTED_OLD_VERSION" ]]; then
  echo "Installed plist version is not expected old version. See $EVIDENCE_DIR/version-before.txt" >&2
  exit 1
fi
if [[ "$(installed_cli_version)" != "$EXPECTED_OLD_VERSION" ]]; then
  echo "Installed CLI version is not expected old version. See $EVIDENCE_DIR/version-before.txt" >&2
  exit 1
fi

kick_update_check

if [[ "$EXPECTED_UPDATE_CLASS" == "optional" ]]; then
  log "waiting for quiet optional update discovery"
  discovery_start_epoch="$(date +%s)"
  discovery_iteration=0
  while true; do
    discovery_iteration=$((discovery_iteration + 1))
    now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    plist_version="$(installed_plist_version)"
    cli_version="$(installed_cli_version)"
    capture_menu "$EVIDENCE_DIR/menu-discovery-$discovery_iteration.txt"
    capture_windows "$EVIDENCE_DIR/windows-discovery-$discovery_iteration.txt"
    echo "$now plist=$plist_version cli=$cli_version" | tee -a "$EVIDENCE_DIR/discovery-watch.log"
    if [[ "$plist_version" != "$EXPECTED_OLD_VERSION" || "$cli_version" != "$EXPECTED_OLD_VERSION" ]]; then
      echo "Optional update installed before user confirmation. Evidence: $EVIDENCE_DIR" >&2
      exit 1
    fi
    if grep -Fxq "Please Update" "$EVIDENCE_DIR/menu-discovery-$discovery_iteration.txt"; then
      cp "$EVIDENCE_DIR/menu-discovery-$discovery_iteration.txt" "$EVIDENCE_DIR/menu-after-background-discovery.txt"
      break
    fi
    if (( "$(date +%s)" - discovery_start_epoch >= OPTIONAL_DISCOVERY_TIMEOUT_SECONDS )); then
      capture_screenshot "$EVIDENCE_DIR/desktop-discovery-timeout.png"
      echo "Timed out waiting for optional update menu discovery. Evidence: $EVIDENCE_DIR" >&2
      exit 1
    fi
    sleep "$POLL_SECONDS"
  done

  log "proving optional update remains quiet"
  sleep "$OPTIONAL_QUIET_SECONDS"
  write_versions "$EVIDENCE_DIR/version-after-quiet-background.txt"
  capture_accessibility "$EVIDENCE_DIR/accessibility-after-quiet-background.txt"
  capture_screenshot "$EVIDENCE_DIR/desktop-after-quiet-background.png"
  if [[ "$(installed_plist_version)" != "$EXPECTED_OLD_VERSION" || "$(installed_cli_version)" != "$EXPECTED_OLD_VERSION" ]]; then
    echo "Optional update installed during quiet window. Evidence: $EVIDENCE_DIR" >&2
    exit 1
  fi

  log "clicking menu pending update action"
  click_menu_item "Please Update"
  wait_for_optional_prompt || {
    echo "Optional prompt title was not visible. Evidence: $EVIDENCE_DIR" >&2
    exit 1
  }
  if grep -Eiq "release notes|verify the signed release|installer|relaunch the app" "$EVIDENCE_DIR/accessibility-optional-prompt.txt"; then
    echo "Optional prompt exposed extra updater explanation. Evidence: $EVIDENCE_DIR" >&2
    exit 1
  fi

  log "confirming optional update"
  click_update_button
fi

log "watching installed app for update"
start_epoch="$(date +%s)"
iteration=0
while true; do
  iteration=$((iteration + 1))
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  plist_version="$(installed_plist_version)"
  cli_version="$(installed_cli_version)"
  echo "$now plist=$plist_version cli=$cli_version" | tee -a "$EVIDENCE_DIR/watch.log"
  capture_windows "$EVIDENCE_DIR/windows-$iteration.txt"

  if [[ "$plist_version" == "$EXPECTED_NEW_VERSION" && "$cli_version" == "$EXPECTED_NEW_VERSION" ]]; then
    echo "updated at iteration $iteration" >> "$EVIDENCE_DIR/watch.log"
    break
  fi

  if (( "$(date +%s)" - start_epoch >= TIMEOUT_SECONDS )); then
    capture_screenshot "$EVIDENCE_DIR/desktop-timeout.png"
    write_versions "$EVIDENCE_DIR/version-timeout.txt"
    echo "Timed out waiting for $EXPECTED_NEW_VERSION. Evidence: $EVIDENCE_DIR" >&2
    exit 1
  fi

  sleep "$POLL_SECONDS"
done

sleep 3
write_versions "$EVIDENCE_DIR/version-after-watch.txt"
capture_windows "$EVIDENCE_DIR/windows-after.txt"
capture_screenshot "$EVIDENCE_DIR/desktop-after-watch.png"
if [[ -x "$APP/Contents/MacOS/1context-cli" ]]; then
  "$APP/Contents/MacOS/1context-cli" status --debug > "$EVIDENCE_DIR/status-after-watch.txt" 2>&1 || true
fi

{
  echo "result=passed"
  echo "old_version=$EXPECTED_OLD_VERSION"
  echo "new_version=$EXPECTED_NEW_VERSION"
  echo "evidence_dir=$EVIDENCE_DIR"
} > "$EVIDENCE_DIR/result.txt"

log "passed; evidence at $EVIDENCE_DIR"
