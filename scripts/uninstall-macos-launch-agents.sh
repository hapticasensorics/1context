#!/usr/bin/env bash
set -euo pipefail

MENU_LABEL="com.haptica.1context.menu"
RUNTIME_LABEL="com.haptica.1context"
DELETE_DATA=0
TEMP_DIR="${TMPDIR:-/tmp}"

if [[ "${1:-}" == "--delete-data" ]]; then
  DELETE_DATA=1
fi

safe_remove() {
  local target="$1"
  local home_prefix="$HOME/"

  if [[ -z "$target" || "$target" == "/" || "$target" == "$HOME" ]]; then
    echo "Refusing to delete unsafe path: $target" >&2
    exit 1
  fi

  if [[ "$target" != "$home_prefix"* ]]; then
    echo "Refusing to delete path outside home: $target" >&2
    exit 1
  fi

  case "$target" in
    "$HOME/1Context"|\
    "$HOME/Library/Application Support/1Context"|\
    "$HOME/Library/Logs/1Context"|\
    "$HOME/Library/Caches/1Context"|\
    "$HOME/Library/Caches/com.haptica.1context.menu"|\
    "$HOME/Library/HTTPStorages/1context"|\
    "$HOME/Library/HTTPStorages/1context.binarycookies"|\
    "$HOME/Library/HTTPStorages/com.haptica.1context.menu"|\
    "$HOME/Library/HTTPStorages/com.haptica.1context.menu.binarycookies"|\
    "$HOME/Library/Preferences/com.haptica.1context.plist"|\
    "$HOME/Library/Saved Application State/com.haptica.1context.menu.savedState"|\
    "$HOME/Library/WebKit/com.haptica.1context.menu")
      rm -rf "$target"
      ;;
    *)
      echo "Refusing to delete unapproved path: $target" >&2
      exit 1
      ;;
  esac
}

for label in "$MENU_LABEL" "$RUNTIME_LABEL"; do
  plist="$HOME/Library/LaunchAgents/$label.plist"
  launchctl bootout "gui/$(id -u)" "$plist" >/dev/null 2>&1 || true
  rm -f "$plist"
done

if [[ "$DELETE_DATA" == "1" ]]; then
  safe_remove "$HOME/1Context"
  safe_remove "$HOME/Library/Application Support/1Context"
  safe_remove "$HOME/Library/Logs/1Context"
  safe_remove "$HOME/Library/Caches/1Context"
  safe_remove "$HOME/Library/Caches/com.haptica.1context.menu"
  safe_remove "$HOME/Library/HTTPStorages/1context"
  safe_remove "$HOME/Library/HTTPStorages/1context.binarycookies"
  safe_remove "$HOME/Library/HTTPStorages/com.haptica.1context.menu"
  safe_remove "$HOME/Library/HTTPStorages/com.haptica.1context.menu.binarycookies"
  safe_remove "$HOME/Library/Preferences/com.haptica.1context.plist"
  safe_remove "$HOME/Library/Saved Application State/com.haptica.1context.menu.savedState"
  safe_remove "$HOME/Library/WebKit/com.haptica.1context.menu"
  rm -f "$TEMP_DIR"/1context-*.command
  rm -rf "$TEMP_DIR"/1context-update-*
fi
