#!/usr/bin/env bash
set -euo pipefail

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

mkdir -p "$HOME/Library/LaunchAgents"

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
</dict>
</plist>
PLIST

launchctl bootout "gui/$(id -u)" "$PLIST" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$(id -u)" "$PLIST" >/dev/null
launchctl kickstart -k "gui/$(id -u)/$LABEL" >/dev/null

"$CLI_PATH" start >/dev/null 2>&1 || true
