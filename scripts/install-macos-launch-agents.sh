#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" == "0" || -n "${SUDO_USER:-}" ]]; then
  echo "Run 1Context as your normal macOS user, not with sudo or as root." >&2
  exit 1
fi

APP_PATH="${ONECONTEXT_APP_PATH:-${1:-}}"
CLI_PATH="${ONECONTEXT_CLI_PATH:-${2:-1context}}"
LABEL="com.haptica.1context.menu"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

if [[ -z "$APP_PATH" ]]; then
  echo "Usage: $0 /path/to/1Context.app [/path/to/1context]" >&2
  exit 1
fi

EXECUTABLE="$APP_PATH/Contents/MacOS/1Context"
if [[ ! -x "$EXECUTABLE" ]]; then
  echo "1Context app executable not found: $EXECUTABLE" >&2
  exit 1
fi

xml_escape() {
  printf '%s' "$1" |
    sed \
      -e 's/&/\&amp;/g' \
      -e 's/</\&lt;/g' \
      -e 's/>/\&gt;/g' \
      -e 's/"/\&quot;/g' \
      -e "s/'/\&apos;/g"
}

EXECUTABLE_XML="$(xml_escape "$EXECUTABLE")"
MENU_LOG="$HOME/Library/Logs/1Context/menu.log"
MENU_LOG_XML="$(xml_escape "$MENU_LOG")"
DESIRED_STATE="$HOME/Library/Application Support/1Context/desired-state"

mkdir -p "$HOME/Library/LaunchAgents"
mkdir -p "$HOME/Library/Logs/1Context"
chmod 700 "$HOME/Library/Logs/1Context"
touch "$MENU_LOG"
chmod 600 "$MENU_LOG"

osascript -e 'tell application id "com.haptica.1context.menu" to quit' >/dev/null 2>&1 || true
launchctl bootout "gui/$(id -u)/$LABEL" >/dev/null 2>&1 || true
launchctl bootout "gui/$(id -u)" "$PLIST" >/dev/null 2>&1 || true

cat > "$PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$EXECUTABLE_XML</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>ThrottleInterval</key>
  <integer>1</integer>
  <key>StandardOutPath</key>
  <string>$MENU_LOG_XML</string>
  <key>StandardErrorPath</key>
  <string>$MENU_LOG_XML</string>
</dict>
</plist>
PLIST

launchctl bootstrap "gui/$(id -u)" "$PLIST" >/dev/null

if [[ -f "$DESIRED_STATE" ]] && [[ "$(tr -d '[:space:]' < "$DESIRED_STATE")" == "stopped" ]]; then
  exit 0
fi

if ! "$CLI_PATH" restart >/dev/null 2>&1 && ! "$CLI_PATH" start >/dev/null 2>&1; then
  echo "1Context installed, but the runtime did not start. Run '1context diagnose'." >&2
  exit 1
fi
