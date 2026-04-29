#!/usr/bin/env bash
set -euo pipefail

MENU_LABEL="com.haptica.1context.menu"
RUNTIME_LABEL="com.haptica.1context"
DELETE_DATA=0

if [[ "${1:-}" == "--delete-data" ]]; then
  DELETE_DATA=1
fi

for label in "$MENU_LABEL" "$RUNTIME_LABEL"; do
  plist="$HOME/Library/LaunchAgents/$label.plist"
  launchctl bootout "gui/$(id -u)" "$plist" >/dev/null 2>&1 || true
  rm -f "$plist"
done

if [[ "$DELETE_DATA" == "1" ]]; then
  rm -rf \
    "$HOME/Library/Application Support/1Context" \
    "$HOME/Library/Logs/1Context" \
    "$HOME/.config/1context"
fi
