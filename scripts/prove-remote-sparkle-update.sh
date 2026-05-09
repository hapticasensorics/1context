#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${ONECONTEXT_INSTALLED_APP:-/Applications/1Context.app}"
APPCAST_URL="${ONECONTEXT_REMOTE_APPCAST_URL:-https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml}"
EXPECTED_NEW_VERSION="${ONECONTEXT_EXPECTED_NEW_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
EXPECTED_OLD_VERSION="${ONECONTEXT_EXPECTED_OLD_VERSION:-}"
KICK_MODE="${ONECONTEXT_UPDATE_PROOF_KICK_MODE:-relaunch}"
TIMEOUT_SECONDS="${ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS:-360}"
POLL_SECONDS="${ONECONTEXT_UPDATE_PROOF_POLL_SECONDS:-5}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

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

if [[ -z "$EXPECTED_OLD_VERSION" ]]; then
  EXPECTED_OLD_VERSION="$(installed_plist_version)"
fi
if [[ -z "$EXPECTED_OLD_VERSION" ]]; then
  echo "Could not determine installed old version from $APP." >&2
  exit 1
fi

EVIDENCE_DIR="${ONECONTEXT_REMOTE_UPDATE_EVIDENCE_DIR:-$ROOT/dist/remote-update-evidence/$EXPECTED_OLD_VERSION-to-$EXPECTED_NEW_VERSION-remote}"
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

capture_screenshot() {
  local output="$1"
  screencapture -x "$output" >/dev/null 2>&1 || true
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
  python3 - "$appcast" "$EXPECTED_NEW_VERSION" "$EXPECTED_OLD_VERSION" <<'PY'
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

appcast = Path(sys.argv[1])
expected_new = sys.argv[2]
expected_old = sys.argv[3]
ns = {"sparkle": "http://www.andymatuschak.org/xml-namespaces/sparkle"}
root = ET.parse(appcast).getroot()
item = root.find("./channel/item")
if item is None:
    raise SystemExit("appcast missing channel/item")
version = item.findtext("sparkle:version", namespaces=ns)
if version != expected_new:
    raise SystemExit(f"appcast version {version!r} != expected {expected_new!r}")
critical = item.find("sparkle:criticalUpdate", namespaces=ns)
if critical is None:
    raise SystemExit("appcast missing sparkle:criticalUpdate")
minimum_auto = item.findtext("sparkle:minimumAutoupdateVersion", namespaces=ns)
if minimum_auto != expected_old:
    raise SystemExit(f"minimumAutoupdateVersion {minimum_auto!r} != expected {expected_old!r}")
description = item.find("description")
if description is not None and (description.text or "").strip():
    raise SystemExit("appcast unexpectedly contains release notes description")
enclosure = item.find("enclosure")
if enclosure is None or not enclosure.attrib.get("{http://www.andymatuschak.org/xml-namespaces/sparkle}edSignature"):
    raise SystemExit("appcast enclosure missing sparkle:edSignature")
PY
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
  echo "kick_mode=$KICK_MODE"
  echo "timeout_seconds=$TIMEOUT_SECONDS"
  echo "poll_seconds=$POLL_SECONDS"
  echo "evidence_dir=$EVIDENCE_DIR"
} > "$EVIDENCE_DIR/environment.txt"

log "fetching live appcast"
curl --fail --location --silent "$APPCAST_URL" > "$EVIDENCE_DIR/live-appcast.xml"
validate_appcast "$EVIDENCE_DIR/live-appcast.xml"
"$ROOT/scripts/check-update-policy.sh" --appcast "$EVIDENCE_DIR/live-appcast.xml"

write_versions "$EVIDENCE_DIR/version-before.txt"
capture_windows "$EVIDENCE_DIR/windows-before.txt"
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
