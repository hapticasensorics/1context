#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage:
  scripts/test-installed-app-permission-capabilities.sh

Environment:
  ONECONTEXT_PACKAGE_APP                         App bundle to verify.
  ONECONTEXT_RELEASE_CHANNEL                     Channel used to infer the default app path.
  ONECONTEXT_EXPECT_PERMISSION_TEST=1            Require a dev permission-test bundle identity.
  ONECONTEXT_PERMISSION_EVIDENCE_DIR             Defaults to dist/permission-build-evidence.
  ONECONTEXT_REQUIRE_PERMISSION_EVIDENCE=0       Do not fail when build evidence is absent.
  ONECONTEXT_REQUIRE_LAUNCH_AGENTS=1             Fail if user LaunchAgent plists are absent.
  ONECONTEXT_RUN_DIAGNOSE=0                      Skip CLI diagnose field checks.
  ONECONTEXT_CAPABILITY_EVIDENCE_DIR             Defaults to a temp directory under /tmp.

This harness does not request or require live TCC grants. It verifies that the
installed app package has the signed identity, entitlements, usage strings,
diagnostic fields, and launchd identity wiring needed for grants to be usable.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

fail() {
  echo "permission capability package check failed: $*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "required tool not found: $1"
}

plist_value() {
  local plist="$1"
  local key="$2"
  /usr/libexec/PlistBuddy -c "Print :$key" "$plist" 2>/dev/null || true
}

require_plist_value() {
  local plist="$1"
  local key="$2"
  local value
  value="$(plist_value "$plist" "$key")"
  [[ -n "$value" ]] || fail "$plist is missing required Info.plist key: $key"
  printf '%s' "$value"
}

require_equal() {
  local label="$1"
  local actual="$2"
  local expected="$3"
  [[ "$actual" == "$expected" ]] || fail "$label mismatch. Expected '$expected', got '$actual'."
}

require_nonempty_plist_key() {
  local plist="$1"
  local key="$2"
  local value
  value="$(require_plist_value "$plist" "$key")"
  [[ -n "${value//[[:space:]]/}" ]] || fail "$key must not be blank in $plist"
}

grep_required() {
  local pattern="$1"
  local file="$2"
  local label="$3"
  grep -Eq "$pattern" "$file" || fail "$label missing from $file"
}

grep_literal_required() {
  local needle="$1"
  local file="$2"
  local label="$3"
  grep -Fq "$needle" "$file" || fail "$label missing from $file"
}

if [[ -z "${ONECONTEXT_PACKAGE_APP:-}" && -x "$ROOT/scripts/release-train.sh" ]]; then
  manifest_args=(manifest export-env)
  if [[ -n "${ONECONTEXT_RELEASE_CHANNEL:-}" ]]; then
    manifest_args+=(--channel "$ONECONTEXT_RELEASE_CHANNEL")
  fi
  eval "$("$ROOT/scripts/release-train.sh" "${manifest_args[@]}")"
fi

APP="${ONECONTEXT_PACKAGE_APP:-$ROOT/dist/${ONECONTEXT_EXPECTED_APP_BASENAME:-1Context.app}}"
APP="${APP%/}"
INFO="$APP/Contents/Info.plist"
MAIN_EXE="$APP/Contents/MacOS/1Context"
CLI_EXE="$APP/Contents/MacOS/1context-cli"
DAEMON_EXE="$APP/Contents/MacOS/1contextd"
WIKI_EXE="$APP/Contents/MacOS/onecontext-wiki"
LOCAL_WEB_PROXY_EXE="$APP/Contents/Resources/1context-local-web-proxy"

require_tool plutil
require_tool codesign
require_tool /usr/libexec/PlistBuddy

[[ -d "$APP" ]] || fail "app bundle not found: $APP"
[[ -f "$INFO" ]] || fail "Info.plist not found: $INFO"
plutil -lint "$INFO" >/dev/null

BUNDLE_ID="$(require_plist_value "$INFO" CFBundleIdentifier)"
BUNDLE_NAME="$(require_plist_value "$INFO" CFBundleName)"
DISPLAY_NAME="$(require_plist_value "$INFO" CFBundleDisplayName)"
APP_IDENTITY="$(require_plist_value "$INFO" OneContextAppIdentity)"
BROWSER_EXTENSION_ID="$(require_plist_value "$INFO" OneContextBrowserExtensionID)"
APP_VERSION="$(require_plist_value "$INFO" CFBundleShortVersionString)"

PERMISSION_TEST_SUFFIX=""
EXPECTED_IDENTITY=""
EXPECTED_BUNDLE_NAME=""
EXPECTED_RUNTIME_LABEL=""
EXPECTED_MENU_LABEL=""
EXPECTED_PROXY_LABEL=""
EXPECTED_SIGNING_PATTERN='Authority=(Apple Development:|Developer ID Application:)'

if [[ "$BUNDLE_ID" =~ ^com\.haptica\.1context\.dev\.permission\.([a-z0-9][a-z0-9-]{0,39})$ ]]; then
  PERMISSION_TEST_SUFFIX="${BASH_REMATCH[1]}"
  EXPECTED_IDENTITY="dev-permission:$PERMISSION_TEST_SUFFIX"
  EXPECTED_BUNDLE_NAME="1Context Dev - $PERMISSION_TEST_SUFFIX"
  EXPECTED_RUNTIME_LABEL="$BUNDLE_ID"
  EXPECTED_MENU_LABEL="$BUNDLE_ID.menu"
  EXPECTED_PROXY_LABEL="$BUNDLE_ID.local-web-proxy"
elif [[ "$BUNDLE_ID" == "com.haptica.1context.dev" ]]; then
  [[ "${ONECONTEXT_EXPECT_PERMISSION_TEST:-0}" != "1" ]] || fail "expected a permission-test bundle, got stable dev bundle id: $BUNDLE_ID"
  EXPECTED_IDENTITY="dev"
  EXPECTED_BUNDLE_NAME="1Context Dev"
  EXPECTED_RUNTIME_LABEL="com.haptica.1context.dev"
  EXPECTED_MENU_LABEL="com.haptica.1context.dev.menu"
  EXPECTED_PROXY_LABEL="com.haptica.1context.dev.local-web-proxy"
elif [[ "$BUNDLE_ID" == "com.haptica.1context" ]]; then
  [[ "${ONECONTEXT_EXPECT_PERMISSION_TEST:-0}" != "1" ]] || fail "expected a permission-test bundle, got official bundle id: $BUNDLE_ID"
  EXPECTED_IDENTITY="official"
  EXPECTED_BUNDLE_NAME="1Context"
  EXPECTED_RUNTIME_LABEL="com.haptica.1context"
  EXPECTED_MENU_LABEL="com.haptica.1context.menu"
  EXPECTED_PROXY_LABEL="com.haptica.1context.local-web-proxy"
else
  fail "unsupported bundle identifier for permission capability verification: $BUNDLE_ID"
fi

require_equal "OneContextAppIdentity" "$APP_IDENTITY" "$EXPECTED_IDENTITY"
require_equal "CFBundleName" "$BUNDLE_NAME" "$EXPECTED_BUNDLE_NAME"
require_equal "CFBundleDisplayName" "$DISPLAY_NAME" "$EXPECTED_BUNDLE_NAME"
require_equal "OneContextBrowserExtensionID" "$BROWSER_EXTENSION_ID" "ijkabgddnhgkapedaloabgpcmpdhdhpb"

for usage_key in \
  NSAppleEventsUsageDescription \
  NSAudioCaptureUsageDescription \
  NSMicrophoneUsageDescription; do
  require_nonempty_plist_key "$INFO" "$usage_key"
done
APPLE_EVENTS_USAGE="$(require_plist_value "$INFO" NSAppleEventsUsageDescription)"
SYSTEM_AUDIO_USAGE="$(require_plist_value "$INFO" NSAudioCaptureUsageDescription)"
MICROPHONE_USAGE="$(require_plist_value "$INFO" NSMicrophoneUsageDescription)"

LAUNCH_DAEMON_PLIST="$APP/Contents/Library/LaunchDaemons/$EXPECTED_PROXY_LABEL.plist"
[[ -f "$LAUNCH_DAEMON_PLIST" ]] || fail "packaged local-web LaunchDaemon plist is missing: $LAUNCH_DAEMON_PLIST"
plutil -lint "$LAUNCH_DAEMON_PLIST" >/dev/null
require_equal "local-web LaunchDaemon Label" "$(require_plist_value "$LAUNCH_DAEMON_PLIST" Label)" "$EXPECTED_PROXY_LABEL"
require_equal "local-web LaunchDaemon BundleProgram" "$(require_plist_value "$LAUNCH_DAEMON_PLIST" BundleProgram)" "Contents/Resources/1context-local-web-proxy"

EVIDENCE_OUT="${ONECONTEXT_CAPABILITY_EVIDENCE_DIR:-$(mktemp -d /tmp/1ctx-permission-capability-XXXXXX)}"
mkdir -p "$EVIDENCE_OUT"

verify_signed_entitled_target() {
  local target="$1"
  local label="$2"
  local entitlements="$EVIDENCE_OUT/$label.entitlements.plist"
  local signature="$EVIDENCE_OUT/$label.codesign.txt"

  [[ -e "$target" ]] || fail "$label is missing from app package: $target"
  [[ -x "$target" || "$target" == *.app ]] || fail "$label is not executable: $target"
  codesign --verify --strict "$target" >/dev/null 2>"$EVIDENCE_OUT/$label.verify.err" \
    || fail "$label does not pass strict codesign verification. See $EVIDENCE_OUT/$label.verify.err"
  codesign -dv --verbose=4 "$target" >"$signature" 2>&1 \
    || fail "could not read codesign metadata for $label"
  grep_required 'flags=.*\(runtime\)' "$signature" "$label hardened runtime flag"

  codesign -d --entitlements :- "$target" >"$entitlements" 2>"$EVIDENCE_OUT/$label.entitlements.err" \
    || fail "could not read entitlements for $label"
  plutil -lint "$entitlements" >/dev/null \
    || fail "$label entitlements are not a valid plist: $entitlements"
  require_equal "$label audio-input entitlement" "$(plist_value "$entitlements" "com.apple.security.device.audio-input")" "true"
  require_equal "$label Apple Events entitlement" "$(plist_value "$entitlements" "com.apple.security.automation.apple-events")" "true"

  if [[ -n "$PERMISSION_TEST_SUFFIX" ]]; then
    grep_required "$EXPECTED_SIGNING_PATTERN" "$signature" "$label Apple Development or Developer ID authority"
  fi
}

verify_signed_entitled_target "$APP" app
verify_signed_entitled_target "$MAIN_EXE" main
verify_signed_entitled_target "$CLI_EXE" cli
verify_signed_entitled_target "$DAEMON_EXE" daemon

for helper in "$WIKI_EXE" "$LOCAL_WEB_PROXY_EXE"; do
  [[ -x "$helper" ]] || fail "packaged helper is missing or not executable: $helper"
  codesign --verify --strict "$helper" >/dev/null 2>"$EVIDENCE_OUT/$(basename "$helper").verify.err" \
    || fail "packaged helper does not pass strict codesign verification: $helper"
done

DESIGNATED_REQUIREMENT="$EVIDENCE_OUT/app.designated-requirement.txt"
codesign -dr - "$APP" >"$DESIGNATED_REQUIREMENT" 2>&1 \
  || fail "could not read app designated requirement"
grep_required "identifier \"$BUNDLE_ID\"" "$DESIGNATED_REQUIREMENT" "app designated requirement bundle identifier"

PERMISSION_EVIDENCE_DIR="${ONECONTEXT_PERMISSION_EVIDENCE_DIR:-$ROOT/dist/permission-build-evidence}"
REQUIRE_PERMISSION_EVIDENCE="${ONECONTEXT_REQUIRE_PERMISSION_EVIDENCE:-1}"
PERMISSION_EVIDENCE_FILE=""
if [[ -d "$PERMISSION_EVIDENCE_DIR" ]]; then
  while IFS= read -r candidate; do
    if grep -Fq "bundle_identifier=$BUNDLE_ID" "$candidate"; then
      PERMISSION_EVIDENCE_FILE="$candidate"
      break
    fi
  done < <(find "$PERMISSION_EVIDENCE_DIR" -maxdepth 1 -type f -name '*.txt' -print | sort)
fi

if [[ -z "$PERMISSION_EVIDENCE_FILE" ]]; then
  if [[ "$REQUIRE_PERMISSION_EVIDENCE" == "1" ]]; then
    fail "permission identity evidence for $BUNDLE_ID not found under $PERMISSION_EVIDENCE_DIR"
  fi
else
  grep_required "^app_path=.*$(basename "$APP" | sed -E 's/[][(){}.^$*+?|\\/]/\\&/g')$" "$PERMISSION_EVIDENCE_FILE" "permission evidence app path"
  grep_required "^bundle_identifier=$BUNDLE_ID$" "$PERMISSION_EVIDENCE_FILE" "permission evidence bundle identifier"
  grep_required "^app_display_name=$DISPLAY_NAME$" "$PERMISSION_EVIDENCE_FILE" "permission evidence app display name"
  grep_required "^signing_mode=(apple-development|developer-id|adhoc)$" "$PERMISSION_EVIDENCE_FILE" "permission evidence signing mode"
  grep_required "^version=$APP_VERSION$" "$PERMISSION_EVIDENCE_FILE" "permission evidence app version"
  grep_required "identifier \"$BUNDLE_ID\"" "$PERMISSION_EVIDENCE_FILE" "permission evidence designated requirement"
  grep_required "com.apple.security.device.audio-input" "$PERMISSION_EVIDENCE_FILE" "permission evidence audio entitlement"
  grep_required "com.apple.security.automation.apple-events" "$PERMISSION_EVIDENCE_FILE" "permission evidence automation entitlement"
  grep_literal_required "$APPLE_EVENTS_USAGE" "$PERMISSION_EVIDENCE_FILE" "permission evidence Apple Events usage string"
  grep_literal_required "$SYSTEM_AUDIO_USAGE" "$PERMISSION_EVIDENCE_FILE" "permission evidence system-audio usage string"
  grep_literal_required "$MICROPHONE_USAGE" "$PERMISSION_EVIDENCE_FILE" "permission evidence microphone usage string"
  if [[ -n "$PERMISSION_TEST_SUFFIX" ]]; then
    grep_required "^signing_mode=(apple-development|developer-id)$" "$PERMISSION_EVIDENCE_FILE" "permission-test evidence non-adhoc signing mode"
    grep_required "dev\\.permission\\.$PERMISSION_TEST_SUFFIX" "$PERMISSION_EVIDENCE_FILE" "permission-test evidence suffix"
  fi
fi

check_launch_agent_plist() {
  local label="$1"
  local expected_program_basename="$2"
  local plist="$HOME/Library/LaunchAgents/$label.plist"

  if [[ ! -f "$plist" ]]; then
    if [[ "${ONECONTEXT_REQUIRE_LAUNCH_AGENTS:-0}" == "1" ]]; then
      fail "LaunchAgent plist is missing for $label. Launch the app once or disable ONECONTEXT_REQUIRE_LAUNCH_AGENTS."
    fi
    return
  fi

  plutil -lint "$plist" >/dev/null
  require_equal "LaunchAgent $label Label" "$(require_plist_value "$plist" Label)" "$label"
  require_equal "LaunchAgent $label ONECONTEXT_APP_IDENTITY" \
    "$(plist_value "$plist" "EnvironmentVariables:ONECONTEXT_APP_IDENTITY")" \
    "$EXPECTED_IDENTITY"

  local program
  program="$(plist_value "$plist" "ProgramArguments:0")"
  [[ "$program" == *"/$EXPECTED_BUNDLE_NAME.app/Contents/MacOS/$expected_program_basename" ]] \
    || fail "LaunchAgent $label ProgramArguments:0 points at '$program', expected $EXPECTED_BUNDLE_NAME.app/Contents/MacOS/$expected_program_basename"
}

check_launch_agent_plist "$EXPECTED_RUNTIME_LABEL" "1contextd"
check_launch_agent_plist "$EXPECTED_MENU_LABEL" "1Context"

if [[ -n "$PERMISSION_TEST_SUFFIX" ]]; then
  for stable_label in com.haptica.1context.dev com.haptica.1context.dev.menu; do
    stable_plist="$HOME/Library/LaunchAgents/$stable_label.plist"
    if [[ -f "$stable_plist" ]]; then
      stable_program="$(plist_value "$stable_plist" "ProgramArguments:0")"
      [[ "$stable_program" != *"/$EXPECTED_BUNDLE_NAME.app/" ]] \
        || fail "stable dev LaunchAgent $stable_label points at permission-test app $EXPECTED_BUNDLE_NAME; it would relaunch with the wrong identity"
    fi
  done
fi

DIAGNOSE_FILE="$EVIDENCE_OUT/diagnose.txt"
if [[ "${ONECONTEXT_RUN_DIAGNOSE:-1}" == "1" ]]; then
  [[ -x "$CLI_EXE" ]] || fail "diagnose CLI is missing or not executable: $CLI_EXE"
  "$CLI_EXE" diagnose >"$DIAGNOSE_FILE" 2>&1 \
    || fail "1context diagnose failed for packaged app. See $DIAGNOSE_FILE"
  grep_required "App Identity: $EXPECTED_IDENTITY" "$DIAGNOSE_FILE" "diagnose app identity"
  grep_required "App Readiness:" "$DIAGNOSE_FILE" "diagnose app readiness section"
  grep_required "Required Setup:" "$DIAGNOSE_FILE" "diagnose required setup field"
  grep_required "Setup:" "$DIAGNOSE_FILE" "diagnose setup section"
  grep_required "Screen & System Audio Recording: (Granted|Required|Needs Relaunch|Unavailable)" "$DIAGNOSE_FILE" "diagnose screen/system-audio status"
  grep_required "Accessibility: (Granted|Required|Needs Relaunch|Unavailable)" "$DIAGNOSE_FILE" "diagnose accessibility status"
  grep_required "Input Monitoring: (Granted|Required|Needs Relaunch|Unavailable)" "$DIAGNOSE_FILE" "diagnose input monitoring status"
  grep_required "Browser Extension Permissions: (Granted|Required|Needs Relaunch|Unavailable)" "$DIAGNOSE_FILE" "diagnose browser extension status"
  grep_required "Microphone: (Granted|Required|Needs Relaunch|Unavailable)" "$DIAGNOSE_FILE" "diagnose microphone status"
  grep_required "Automation: (Granted|Required|Needs Relaunch|Unavailable)" "$DIAGNOSE_FILE" "diagnose automation status"
  grep_required "Full Disk Access: (Granted|Required|Needs Relaunch|Unavailable)" "$DIAGNOSE_FILE" "diagnose full disk access status"
  if ! grep -Eq "Screen & System Audio Recording: Granted" "$DIAGNOSE_FILE"; then
    grep_required "signed capture process|pixels and system audio" "$DIAGNOSE_FILE" "diagnose screen/system-audio proof detail"
  fi
  if ! grep -Eq "Input Monitoring: Granted" "$DIAGNOSE_FILE"; then
    grep_required "Input Monitoring Detail:" "$DIAGNOSE_FILE" "diagnose input monitoring proof detail"
  fi
  if ! grep -Eq "Browser Extension Permissions: Granted" "$DIAGNOSE_FILE"; then
    grep_required "installed browser extension id|browser extension id|browser extension native-message proof" "$DIAGNOSE_FILE" "diagnose browser extension proof detail"
  fi
  if ! grep -Eq "Microphone: Granted" "$DIAGNOSE_FILE"; then
    grep_required "signed app can open the audio capture path|microphone" "$DIAGNOSE_FILE" "diagnose microphone proof detail"
  fi
  if ! grep -Eq "Automation: Granted" "$DIAGNOSE_FILE"; then
    grep_required "Apple Events proof|Automation Detail:" "$DIAGNOSE_FILE" "diagnose automation proof detail"
  fi
  grep_required "LaunchAgents:" "$DIAGNOSE_FILE" "diagnose launch agent section"
  grep_required "$EXPECTED_RUNTIME_LABEL:" "$DIAGNOSE_FILE" "diagnose runtime LaunchAgent label"
  grep_required "$EXPECTED_MENU_LABEL:" "$DIAGNOSE_FILE" "diagnose menu LaunchAgent label"
fi

SUMMARY="$EVIDENCE_OUT/permission-capability-summary.json"
python3 - "$SUMMARY" "$APP" "$BUNDLE_ID" "$EXPECTED_IDENTITY" "$EXPECTED_RUNTIME_LABEL" "$EXPECTED_MENU_LABEL" "$EXPECTED_PROXY_LABEL" "${PERMISSION_EVIDENCE_FILE:-}" "$DIAGNOSE_FILE" <<'PY'
import json
import sys
from pathlib import Path

summary, app, bundle_id, identity, runtime_label, menu_label, proxy_label, evidence, diagnose = sys.argv[1:]
payload = {
    "schema": "1context.permission-capability-package-check.v1",
    "status": "passed",
    "app": app,
    "bundle_identifier": bundle_id,
    "app_identity": identity,
    "launch_agents": {
        "runtime": runtime_label,
        "menu": menu_label,
    },
    "local_web_proxy_launch_daemon": proxy_label,
    "permission_identity_evidence": evidence or None,
    "diagnose_evidence": diagnose if Path(diagnose).exists() else None,
}
Path(summary).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "Permission capability package check passed."
echo "Evidence: $SUMMARY"
