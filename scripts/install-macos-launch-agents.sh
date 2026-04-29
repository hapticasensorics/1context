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
MENU_LOCK="$HOME/Library/Application Support/1Context/run/1context-menu.lock"

menu_environment_xml() {
  if [[ "${ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES:-0}" != "1" ]]; then
    return 0
  fi

  cat <<XML
  <key>EnvironmentVariables</key>
  <dict>
    <key>ONECONTEXT_APP_SUPPORT_DIR</key>
    <string>$(xml_escape "${ONECONTEXT_APP_SUPPORT_DIR:-$HOME/Library/Application Support/1Context}")</string>
    <key>ONECONTEXT_USER_CONTENT_DIR</key>
    <string>$(xml_escape "${ONECONTEXT_USER_CONTENT_DIR:-$HOME/1Context}")</string>
    <key>ONECONTEXT_LOG_DIR</key>
    <string>$(xml_escape "${ONECONTEXT_LOG_DIR:-$HOME/Library/Logs/1Context}")</string>
    <key>ONECONTEXT_CACHE_DIR</key>
    <string>$(xml_escape "${ONECONTEXT_CACHE_DIR:-$HOME/Library/Caches/1Context}")</string>
    <key>ONECONTEXT_UPDATE_STATE_DIR</key>
    <string>$(xml_escape "${ONECONTEXT_UPDATE_STATE_DIR:-$HOME/Library/Application Support/1Context/update}")</string>
    <key>ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES</key>
    <string>1</string>
    <key>ONECONTEXT_AGENT_ALLOW_ENV_OVERRIDES</key>
    <string>$(xml_escape "${ONECONTEXT_AGENT_ALLOW_ENV_OVERRIDES:-0}")</string>
    <key>ONECONTEXT_CLAUDE_SETTINGS_PATH</key>
    <string>$(xml_escape "${ONECONTEXT_CLAUDE_SETTINGS_PATH:-}")</string>
  </dict>
XML
}

wait_for_old_menu() {
  local lock_pid=""

  for _ in {1..50}; do
    lock_pid=""
    if [[ -f "$MENU_LOCK" ]]; then
      lock_pid="$(tr -dc '0-9' < "$MENU_LOCK" | head -c 20 || true)"
    fi

    if ! pgrep -f "$EXECUTABLE" >/dev/null 2>&1 &&
      { [[ -z "$lock_pid" ]] || ! kill -0 "$lock_pid" >/dev/null 2>&1; }; then
      return 0
    fi

    sleep 0.1
  done

  echo "Timed out waiting for old 1Context menu instance to exit." >&2
  return 1
}

install_agent_hooks() {
  if "$CLI_PATH" agent integrations install >/dev/null 2>&1; then
    return 0
  fi

  echo "1Context installed, but Claude hook setup needs attention. Run '1context agent integrations status'." >&2
  return 0
}

mkdir -p "$HOME/Library/LaunchAgents"
mkdir -p "$HOME/Library/Logs/1Context"
chmod 700 "$HOME/Library/Logs/1Context"
touch "$MENU_LOG"
chmod 600 "$MENU_LOG"

launchctl bootout "gui/$(id -u)/$LABEL" >/dev/null 2>&1 || true
launchctl bootout "gui/$(id -u)" "$PLIST" >/dev/null 2>&1 || true
osascript -e 'tell application id "com.haptica.1context.menu" to quit' >/dev/null 2>&1 || true
wait_for_old_menu

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
$(menu_environment_xml)
</dict>
</plist>
PLIST

launchctl bootstrap "gui/$(id -u)" "$PLIST" >/dev/null
install_agent_hooks

if [[ -f "$DESIRED_STATE" ]] && [[ "$(tr -d '[:space:]' < "$DESIRED_STATE")" == "stopped" ]]; then
  exit 0
fi

if ! "$CLI_PATH" restart >/dev/null 2>&1 && ! "$CLI_PATH" start >/dev/null 2>&1; then
  echo "1Context installed, but the runtime did not start. Run '1context diagnose'." >&2
  exit 1
fi
