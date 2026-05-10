#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
APP="${ONECONTEXT_EVIDENCE_APP:-/Applications/1Context.app}"
CLI="${ONECONTEXT_EVIDENCE_CLI:-$APP/Contents/MacOS/1context-cli}"
OUT_DIR="${ONECONTEXT_RELEASE_LOCKDOWN_EVIDENCE_DIR:-$ROOT/dist/release-lockdown-evidence/$STAMP}"
REDACT="${ONECONTEXT_RELEASE_LOCKDOWN_EVIDENCE_REDACT:-1}"
APPCAST_URL="${ONECONTEXT_RELEASE_LOCKDOWN_APPCAST_URL:-}"
FAILED_UPDATE_EVIDENCE_DIR="${ONECONTEXT_RELEASE_LOCKDOWN_FAILED_UPDATE_EVIDENCE_DIR:-}"

mkdir -p "$OUT_DIR"

redact_file() {
  local file="$1"
  if [[ "$REDACT" != "1" || ! -f "$file" ]]; then
    return 0
  fi
  python3 - "$file" "$HOME" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
home = sys.argv[2]
text = path.read_text(errors="replace")
if home:
    text = text.replace(home, "~")
    text = text.replace(home.replace("/", r"\/"), "~")
    username = Path(home).name
    if username:
        text = re.sub(rf"\b{re.escape(username)}\b", "<user>", text)
text = re.sub(r"/Users/[^/\\s:]+", "~", text)
text = re.sub(r"\\/Users\\/[^\\/\\s:]+", "~", text)
path.write_text(text)
PY
}

write_text() {
  local file="$1"
  shift
  printf '%s\n' "$@" > "$OUT_DIR/$file"
  redact_file "$OUT_DIR/$file"
}

capture_command() {
  local file="$1"
  shift
  {
    printf '$'
    printf ' %q' "$@"
    printf '\n\n'
    "$@"
    status=$?
    printf '\nexit=%s\n' "$status"
  } > "$OUT_DIR/$file" 2>&1 || true
  redact_file "$OUT_DIR/$file"
}

capture_shell() {
  local file="$1"
  local script="$2"
  {
    printf '$ %s\n\n' "$script"
    bash -lc "$script"
    status=$?
    printf '\nexit=%s\n' "$status"
  } > "$OUT_DIR/$file" 2>&1 || true
  redact_file "$OUT_DIR/$file"
}

info_plist="$APP/Contents/Info.plist"
repo_version="$(tr -d '[:space:]' < "$ROOT/VERSION" 2>/dev/null || true)"
git_commit="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || true)"
installed_version=""
if [[ -x "$CLI" ]]; then
  installed_version="$("$CLI" --version 2>/dev/null || true)"
fi
plist_version=""
if [[ -f "$info_plist" ]]; then
  plist_version="$(plutil -extract CFBundleShortVersionString raw "$info_plist" 2>/dev/null || true)"
fi

if [[ -z "$APPCAST_URL" && -f "$info_plist" ]]; then
  APPCAST_URL="$(plutil -extract SUFeedURL raw "$info_plist" 2>/dev/null || true)"
fi

write_text "manifest.txt" \
  "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  "repo_root=$ROOT" \
  "git_commit=$git_commit" \
  "repo_version=$repo_version" \
  "app=$APP" \
  "cli=$CLI" \
  "installed_cli_version=$installed_version" \
  "installed_plist_version=$plist_version" \
  "appcast_url=$APPCAST_URL" \
  "redaction=$REDACT"

write_text "version.txt" \
  "repo_version=$repo_version" \
  "git_commit=$git_commit" \
  "installed_cli_version=$installed_version" \
  "installed_plist_version=$plist_version"

if [[ -f "$info_plist" ]]; then
  capture_command "app-info-plist.txt" plutil -p "$info_plist"
  capture_shell "sparkle-info-plist.txt" "
for key in \
  SUFeedURL \
  SUPublicEDKey \
  SUEnableAutomaticChecks \
  SUAutomaticallyUpdate \
  SUAllowsAutomaticUpdates \
  SUScheduledCheckInterval \
  SUVerifyUpdateBeforeExtraction \
  OneContextUpdateOptionalPromptTitle \
  OneContextUpdateOptionalPromptBody \
  OneContextUpdateFailureTitle \
  OneContextUpdateFailureBody \
  OneContextUpdatePostInstallMessageEnabled \
  OneContextUpdatePostInstallTitle \
  OneContextUpdatePostInstallBody \
  OneContextUpdateShowReleaseNotesInUpdateWindow
do
  printf '%s=' \"\$key\"
  plutil -extract \"\$key\" raw '$info_plist' 2>/dev/null || printf 'missing'
  printf '\n'
done"
else
  write_text "app-info-plist.txt" "missing app Info.plist: $info_plist"
  write_text "sparkle-info-plist.txt" "missing app Info.plist: $info_plist"
fi

capture_command "sparkle-defaults-com.haptica.1context.txt" defaults read com.haptica.1context
capture_command "sparkle-defaults-com.haptica.1context.menu.txt" defaults read com.haptica.1context.menu

if [[ -n "$APPCAST_URL" ]]; then
  if curl --fail --location --silent --show-error "$APPCAST_URL" > "$OUT_DIR/appcast.xml" 2> "$OUT_DIR/appcast-fetch.err"; then
    capture_command "appcast-policy.txt" "$ROOT/scripts/check-update-policy.sh" --appcast "$OUT_DIR/appcast.xml"
    capture_shell "appcast-summary.txt" "python3 - <<'PY'
from pathlib import Path
import xml.etree.ElementTree as ET

path = Path('$OUT_DIR/appcast.xml')
ns = {'sparkle': 'http://www.andymatuschak.org/xml-namespaces/sparkle'}
root = ET.parse(path).getroot()
item = root.find('./channel/item')
if item is None:
    raise SystemExit('missing channel/item')
print('title=' + (item.findtext('title') or ''))
print('sparkle_version=' + (item.findtext('sparkle:version', namespaces=ns) or ''))
print('critical_update=' + str(item.find('sparkle:criticalUpdate', ns) is not None).lower())
print('minimum_autoupdate_version=' + (item.findtext('sparkle:minimumAutoupdateVersion', namespaces=ns) or ''))
enclosure = item.find('enclosure')
print('enclosure_url=' + (enclosure.attrib.get('url', '') if enclosure is not None else ''))
print('has_release_notes=' + str(item.find('description') is not None).lower())
PY"
  else
    redact_file "$OUT_DIR/appcast-fetch.err"
    write_text "appcast-policy.txt" "appcast fetch failed"
    write_text "appcast-summary.txt" "appcast fetch failed"
  fi
else
  write_text "appcast-policy.txt" "missing appcast URL"
  write_text "appcast-summary.txt" "missing appcast URL"
fi

if [[ -x "$CLI" ]]; then
  capture_command "status-debug.txt" "$CLI" status --debug
  capture_command "diagnose.txt" "$CLI" diagnose
  capture_command "update-status.txt" "$CLI" update
  capture_command "setup-local-web-status.txt" "$CLI" setup local-web status
else
  write_text "status-debug.txt" "missing CLI: $CLI"
  write_text "diagnose.txt" "missing CLI: $CLI"
  write_text "update-status.txt" "missing CLI: $CLI"
  write_text "setup-local-web-status.txt" "missing CLI: $CLI"
fi

uid="$(id -u)"
capture_command "launchagent-runtime.txt" launchctl print "gui/$uid/com.haptica.1context"
capture_command "launchagent-menu.txt" launchctl print "gui/$uid/com.haptica.1context.menu"
capture_command "launchdaemon-local-web-proxy.txt" launchctl print system/com.haptica.1context.local-web-proxy

capture_shell "helper-state.txt" "
set -euo pipefail
for path in \
  '$APP/Contents/Resources/1context-local-web-proxy' \
  '$APP/Contents/Library/LaunchDaemons/com.haptica.1context.local-web-proxy.plist' \
  '$HOME/Library/Application Support/1Context/local-web/setup/bin/1context-local-web-proxy' \
  '$HOME/Library/Application Support/1Context/local-web/setup/local-web-root.sha256' \
  '$HOME/Library/Application Support/1Context/local-web/setup/local-web-setup.json'
do
  if [[ -e \"\$path\" ]]; then
    ls -la \"\$path\"
    if [[ -f \"\$path\" && \"\$path\" != *.json ]]; then
      shasum -a 256 \"\$path\" || true
    fi
  else
    echo \"missing \$path\"
  fi
done"

capture_shell "local-wiki-health.txt" "
set -euo pipefail
curl --fail --silent --show-error --max-time 5 https://wiki.1context.localhost/__1context/health || true
printf '\n--- api ---\n'
curl --fail --silent --show-error --max-time 5 https://wiki.1context.localhost/api/wiki/health || true"

capture_shell "recent-logs.txt" "
set -euo pipefail
for path in \
  '$HOME/Library/Logs/1Context/1contextd.log' \
  '$HOME/Library/Logs/1Context/menu.log' \
  '$HOME/Library/Logs/1Context/local-web-caddy.log' \
  '$HOME/Library/Logs/1Context/local-web-proxy.log'
do
  echo \"== \$path ==\"
  if [[ -f \"\$path\" ]]; then
    tail -n 120 \"\$path\"
  else
    echo missing
  fi
done"

if [[ -n "$FAILED_UPDATE_EVIDENCE_DIR" ]]; then
  capture_command \
    "diagnostic-state-summary.txt" \
    python3 "$ROOT/scripts/classify-release-lockdown-diagnostics.py" \
    --evidence-dir "$OUT_DIR" \
    --failed-update-dir "$FAILED_UPDATE_EVIDENCE_DIR"
else
  capture_command \
    "diagnostic-state-summary.txt" \
    python3 "$ROOT/scripts/classify-release-lockdown-diagnostics.py" \
    --evidence-dir "$OUT_DIR"
fi

write_text "result.txt" \
  "result=collected" \
  "evidence_dir=$OUT_DIR" \
  "repo_version=$repo_version" \
  "installed_cli_version=$installed_version" \
  "installed_plist_version=$plist_version" \
  "appcast_url=$APPCAST_URL" \
  "redaction=$REDACT"

printf '%s\n' "$OUT_DIR"
