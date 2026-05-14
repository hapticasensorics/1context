#!/usr/bin/env bash

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

click_settings_menu_item() {
  local settings_item="$1"
  osascript <<APPLESCRIPT
tell application "System Events"
  tell process "1Context"
    click menu bar item 1 of menu bar 1
    delay 0.5
    click menu item "$settings_item" of menu 1 of menu item "Settings" of menu 1 of menu bar item 1 of menu bar 1
  end tell
end tell
APPLESCRIPT
}

click_window_button() {
  local button_title="$1"
  osascript <<APPLESCRIPT
tell application "System Events"
  repeat with proc in application processes
    set procName to name of proc
    if procName contains "1Context" then
      repeat with win in windows of proc
        try
          click button "$button_title" of win
          return
        end try
      end repeat
    end if
  end repeat
  error "Button not found: $button_title"
end tell
APPLESCRIPT
}
