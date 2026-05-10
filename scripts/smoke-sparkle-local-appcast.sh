#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH="${ONECONTEXT_ARCH:-arm64}"
BASE_VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
OLD_VERSION="${ONECONTEXT_SMOKE_OLD_VERSION:-$BASE_VERSION.900}"
NEW_VERSION="${ONECONTEXT_SMOKE_NEW_VERSION:-$BASE_VERSION.901}"
STAMP="$(date +%Y%m%d-%H%M%S)"
WORK_DIR="${ONECONTEXT_SPARKLE_SMOKE_DIR:-$ROOT/dist/sparkle-local-smoke/$STAMP}"
STATE_DIR="$WORK_DIR/state"
UPDATES_DIR="$WORK_DIR/updates"
EVIDENCE_DIR="$WORK_DIR/evidence"
INSTALL_APP="${ONECONTEXT_SMOKE_INSTALL_APP:-/Applications/1Context Sparkle Smoke.app}"
SMOKE_BUNDLE_IDENTIFIER="${ONECONTEXT_SMOKE_BUNDLE_IDENTIFIER:-com.haptica.1context.sparkle-smoke}"
EXPECT_POST_INSTALL_MESSAGE="${ONECONTEXT_SPARKLE_SMOKE_EXPECT_POST_INSTALL_MESSAGE:-0}"
POST_INSTALL_TITLE="${ONECONTEXT_SPARKLE_SMOKE_POST_INSTALL_TITLE:-1Context Improved!}"
POST_INSTALL_BODY="${ONECONTEXT_SPARKLE_SMOKE_POST_INSTALL_BODY:-}"
if [[ -z "$POST_INSTALL_BODY" ]]; then
  POST_INSTALL_BODY="Installed {version}."
fi
FAILURE_CASE="${ONECONTEXT_SPARKLE_SMOKE_FAILURE_CASE:-}"
FAILURE_TITLE="${ONECONTEXT_SPARKLE_SMOKE_FAILURE_TITLE:-Update failed.}"
FAILURE_BODY="${ONECONTEXT_SPARKLE_SMOKE_FAILURE_BODY:-Please contact support at paul@haptica.ai.}"
GENERATE_APPCAST="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_appcast"

if [[ -n "$FAILURE_CASE" && "$FAILURE_CASE" != "missing_asset" && "$FAILURE_CASE" != "bad_signature" ]]; then
  echo "ONECONTEXT_SPARKLE_SMOKE_FAILURE_CASE must be empty, missing_asset, or bad_signature." >&2
  exit 1
fi

pick_free_port() {
  python3 - <<'PY'
import socket
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

log() {
  printf '[sparkle-smoke] %s\n' "$*"
}

wait_for_version() {
  local expected="$1"
  local plist="$INSTALL_APP/Contents/Info.plist"
  local current=""
  for _ in {1..180}; do
    current="$(plutil -extract CFBundleShortVersionString raw "$plist" 2>/dev/null || true)"
    if [[ "$current" == "$expected" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "Expected installed app version $expected, got ${current:-missing}." >&2
  return 1
}

mkdir -p "$WORK_DIR" "$STATE_DIR" "$UPDATES_DIR" "$EVIDENCE_DIR"

if [[ ! -x "$GENERATE_APPCAST" ]]; then
  swift build --package-path "$ROOT/macos" -c release --arch "$ARCH" >/dev/null
fi

KEY_EXPORTS="$(uv run --with cryptography python - <<'PY'
import base64
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ed25519

private_key = ed25519.Ed25519PrivateKey.generate()
private_raw = private_key.private_bytes(
    encoding=serialization.Encoding.Raw,
    format=serialization.PrivateFormat.Raw,
    encryption_algorithm=serialization.NoEncryption(),
)
public_raw = private_key.public_key().public_bytes(
    encoding=serialization.Encoding.Raw,
    format=serialization.PublicFormat.Raw,
)
print(f"PRIVATE_KEY={base64.b64encode(private_raw).decode()}")
print(f"PUBLIC_KEY={base64.b64encode(public_raw).decode()}")
PY
)"
eval "$KEY_EXPORTS"

PORT="${ONECONTEXT_SPARKLE_SMOKE_PORT:-$(pick_free_port)}"
FEED_URL="http://127.0.0.1:$PORT/appcast.xml"
DOWNLOAD_PREFIX="http://127.0.0.1:$PORT/"

{
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "old_version=$OLD_VERSION"
  echo "new_version=$NEW_VERSION"
  echo "feed_url=$FEED_URL"
  echo "install_app=$INSTALL_APP"
  echo "bundle_identifier=$SMOKE_BUNDLE_IDENTIFIER"
  echo "expect_post_install_message=$EXPECT_POST_INSTALL_MESSAGE"
  echo "post_install_title=$POST_INSTALL_TITLE"
  echo "post_install_body=$POST_INSTALL_BODY"
  echo "failure_case=$FAILURE_CASE"
  echo "failure_title=$FAILURE_TITLE"
  echo "failure_body=$FAILURE_BODY"
  echo "work_dir=$WORK_DIR"
} > "$EVIDENCE_DIR/environment.txt"

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

capture_screenshot() {
  local output="$1"
  screencapture -x "$output" >/dev/null 2>&1 || true
}

click_post_install_ok() {
  osascript >/dev/null 2>&1 <<'APPLESCRIPT' || true
tell application "System Events"
  repeat with proc in application processes
    set procName to name of proc
    if procName contains "1Context" then
      repeat with win in windows of proc
        try
          click button "OK" of win
          return
        end try
      end repeat
    end if
  end repeat
end tell
APPLESCRIPT
}

wait_for_post_install_message() {
  local expected_body="${POST_INSTALL_BODY//\{version\}/$NEW_VERSION}"
  local deadline=$(( "$(date +%s)" + 45 ))
  local attempt=0
  while true; do
    attempt=$((attempt + 1))
    local accessibility="$EVIDENCE_DIR/post-install-accessibility-$attempt.txt"
    capture_accessibility "$accessibility"
    capture_screenshot "$EVIDENCE_DIR/post-install-desktop-$attempt.png"
    cp "$accessibility" "$EVIDENCE_DIR/post-install-accessibility.txt"
    cp "$EVIDENCE_DIR/post-install-desktop-$attempt.png" "$EVIDENCE_DIR/post-install-desktop.png"
    if grep -Fq "$POST_INSTALL_TITLE" "$accessibility" &&
      grep -Fq "$expected_body" "$accessibility"; then
      echo "title=$POST_INSTALL_TITLE" > "$EVIDENCE_DIR/post-install-message.txt"
      echo "body=$expected_body" >> "$EVIDENCE_DIR/post-install-message.txt"
      click_post_install_ok
      return 0
    fi
    if (( "$(date +%s)" >= deadline )); then
      echo "Timed out waiting for post-install message '$POST_INSTALL_TITLE' / '$expected_body'." >&2
      echo "Evidence: $EVIDENCE_DIR" >&2
      return 1
    fi
    sleep 1
  done
}

wait_for_failure_message() {
  local deadline=$(( "$(date +%s)" + 60 ))
  local attempt=0
  while true; do
    attempt=$((attempt + 1))
    local accessibility="$EVIDENCE_DIR/failure-accessibility-$attempt.txt"
    capture_accessibility "$accessibility"
    capture_screenshot "$EVIDENCE_DIR/failure-desktop-$attempt.png"
    cp "$accessibility" "$EVIDENCE_DIR/failure-accessibility.txt"
    cp "$EVIDENCE_DIR/failure-desktop-$attempt.png" "$EVIDENCE_DIR/failure-desktop.png"
    if grep -Fq "$FAILURE_TITLE" "$accessibility" &&
      grep -Fq "$FAILURE_BODY" "$accessibility"; then
      if grep -Eiq '404|not found|download|signature|Sparkle|installer|relaunch' "$accessibility"; then
        echo "Failure alert exposed technical update details. Evidence: $accessibility" >&2
        return 1
      fi
      echo "title=$FAILURE_TITLE" > "$EVIDENCE_DIR/failure-message.txt"
      echo "body=$FAILURE_BODY" >> "$EVIDENCE_DIR/failure-message.txt"
      click_post_install_ok
      return 0
    fi
    if (( "$(date +%s)" >= deadline )); then
      echo "Timed out waiting for failure message '$FAILURE_TITLE' / '$FAILURE_BODY'." >&2
      echo "Evidence: $EVIDENCE_DIR" >&2
      return 1
    fi
    sleep 1
  done
}

build_fixture_app() {
  local version="$1"
  local output_app="$2"
  log "building fixture app $version"
  ONECONTEXT_VERSION="$version" \
  ONECONTEXT_ARCH="$ARCH" \
  ONECONTEXT_SIGNING_MODE=adhoc \
  ONECONTEXT_BUNDLE_IDENTIFIER="$SMOKE_BUNDLE_IDENTIFIER" \
  ONECONTEXT_SPARKLE_FEED_URL="$FEED_URL" \
  ONECONTEXT_SPARKLE_PUBLIC_ED_KEY="$PUBLIC_KEY" \
  ONECONTEXT_SMOKE_FIXTURE=1 \
  ONECONTEXT_SMOKE_STATE_DIR="$STATE_DIR" \
  ONECONTEXT_UPDATE_POST_INSTALL_MESSAGE_ENABLED="$EXPECT_POST_INSTALL_MESSAGE" \
  ONECONTEXT_UPDATE_POST_INSTALL_TITLE="$POST_INSTALL_TITLE" \
  ONECONTEXT_UPDATE_POST_INSTALL_BODY="$POST_INSTALL_BODY" \
  ONECONTEXT_UPDATE_FAILURE_TITLE="$FAILURE_TITLE" \
  ONECONTEXT_UPDATE_FAILURE_BODY="$FAILURE_BODY" \
    "$ROOT/scripts/build-macos-app.sh" >/dev/null

  rm -rf "$output_app"
  COPYFILE_DISABLE=1 ditto --norsrc --noextattr --noqtn --noacl "$ROOT/dist/1Context.app" "$output_app"
}

build_dmg() {
  local version="$1"
  local app="$2"
  local dmg="$3"
  log "creating fixture DMG $version"
  ONECONTEXT_VERSION="$version" \
  ONECONTEXT_ARCH="$ARCH" \
    "$ROOT/scripts/create-macos-dmg.sh" "$app" "$dmg" >/dev/null
}

cleanup_server() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
}

cleanup_app() {
  pkill -f "$INSTALL_APP/Contents/MacOS/1Context" >/dev/null 2>&1 || true
  if [[ "${ONECONTEXT_SPARKLE_SMOKE_KEEP_APP:-0}" != "1" ]]; then
    rm -rf "$INSTALL_APP"
  fi
}

trap 'cleanup_server; cleanup_app' EXIT

OLD_APP="$WORK_DIR/old/1Context.app"
NEW_APP="$WORK_DIR/new/1Context.app"
OLD_DMG="$WORK_DIR/1Context-$OLD_VERSION-macos-$ARCH.dmg"
NEW_DMG="$WORK_DIR/1Context-$NEW_VERSION-macos-$ARCH.dmg"

build_fixture_app "$OLD_VERSION" "$OLD_APP"
build_dmg "$OLD_VERSION" "$OLD_APP" "$OLD_DMG"
build_fixture_app "$NEW_VERSION" "$NEW_APP"
build_dmg "$NEW_VERSION" "$NEW_APP" "$NEW_DMG"

log "generating mandatory local appcast"
SPARKLE_PRIVATE_ED_KEY="$PRIVATE_KEY" \
SPARKLE_DOWNLOAD_URL_PREFIX="$DOWNLOAD_PREFIX" \
ONECONTEXT_VERSION="$NEW_VERSION" \
ONECONTEXT_ARCH="$ARCH" \
ONECONTEXT_SPARKLE_MANDATORY=1 \
ONECONTEXT_SPARKLE_MINIMUM_AUTOUPDATE_VERSION="$OLD_VERSION" \
  "$ROOT/scripts/generate-sparkle-appcast.sh" "$NEW_DMG" "$UPDATES_DIR" > "$EVIDENCE_DIR/generated-appcast-path.txt"

APPCAST="$UPDATES_DIR/appcast.xml"
grep -q '<sparkle:criticalUpdate' "$APPCAST"
grep -q '<sparkle:minimumAutoupdateVersion>'"$OLD_VERSION"'</sparkle:minimumAutoupdateVersion>' "$APPCAST"
if [[ "$FAILURE_CASE" == "missing_asset" ]]; then
  rm -f "$UPDATES_DIR/$(basename "$NEW_DMG")"
elif [[ "$FAILURE_CASE" == "bad_signature" ]]; then
  SIGNED_UPDATE_DMG="$UPDATES_DIR/$(basename "$NEW_DMG")"
  cp "$SIGNED_UPDATE_DMG" "$EVIDENCE_DIR/dmg-before-signature-corruption.dmg"
  {
    echo "corrupted downloaded DMG bytes after appcast signing for bad_signature proof"
    echo "dmg=$(basename "$SIGNED_UPDATE_DMG")"
    echo "sha256_before=$(shasum -a 256 "$SIGNED_UPDATE_DMG" | awk '{ print $1 }')"
  } > "$EVIDENCE_DIR/signature-corruption.txt"
  printf '\n1context-bad-signature-fixture\n' >> "$SIGNED_UPDATE_DMG"
  echo "sha256_after=$(shasum -a 256 "$SIGNED_UPDATE_DMG" | awk '{ print $1 }')" >> "$EVIDENCE_DIR/signature-corruption.txt"
fi
plutil -extract SUPublicEDKey raw "$OLD_APP/Contents/Info.plist" | grep -qx "$PUBLIC_KEY"
plutil -extract SUAutomaticallyUpdate raw "$OLD_APP/Contents/Info.plist" | grep -qx true
plutil -extract SUVerifyUpdateBeforeExtraction raw "$OLD_APP/Contents/Info.plist" | grep -qx true

log "serving local appcast on $FEED_URL"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$UPDATES_DIR" \
  > "$EVIDENCE_DIR/http-server.log" 2>&1 &
SERVER_PID="$!"
sleep 1
curl --fail --silent "$FEED_URL" > "$EVIDENCE_DIR/appcast-served.xml"

log "installing old fixture to $INSTALL_APP"
cleanup_app
mkdir -p "$(dirname "$INSTALL_APP")"
COPYFILE_DISABLE=1 ditto --norsrc --noextattr --noqtn --noacl "$OLD_APP" "$INSTALL_APP"
wait_for_version "$OLD_VERSION"

log "launching old fixture and waiting for Sparkle update"
"$INSTALL_APP/Contents/MacOS/1Context" > "$EVIDENCE_DIR/app.log" 2>&1 &
APP_PID="$!"
echo "$APP_PID" > "$EVIDENCE_DIR/initial-app.pid"

if [[ -n "$FAILURE_CASE" ]]; then
  wait_for_failure_message
  wait_for_version "$OLD_VERSION"
  FAILED_CLI_VERSION="$("$INSTALL_APP/Contents/MacOS/1context-cli" --version)"
  echo "$FAILED_CLI_VERSION" > "$EVIDENCE_DIR/failed-cli-version.txt"
  if [[ "$FAILED_CLI_VERSION" != "$OLD_VERSION" ]]; then
    echo "Expected failed update to leave CLI version $OLD_VERSION, got $FAILED_CLI_VERSION." >&2
    exit 1
  fi
  {
    echo "result=passed"
    echo "failure_case=$FAILURE_CASE"
    echo "old_version=$OLD_VERSION"
    echo "attempted_new_version=$NEW_VERSION"
    echo "feed_url=$FEED_URL"
    echo "appcast=$APPCAST"
    echo "installed_cli_version=$FAILED_CLI_VERSION"
  } > "$EVIDENCE_DIR/result.txt"
  log "passed $FAILURE_CASE failure proof; evidence at $EVIDENCE_DIR"
  cleanup_app
  exit 0
fi

wait_for_version "$NEW_VERSION"
if [[ "$EXPECT_POST_INSTALL_MESSAGE" == "1" ]]; then
  wait_for_post_install_message
fi
UPDATED_CLI_VERSION="$("$INSTALL_APP/Contents/MacOS/1context-cli" --version)"
echo "$UPDATED_CLI_VERSION" > "$EVIDENCE_DIR/updated-cli-version.txt"
if [[ "$UPDATED_CLI_VERSION" != "$NEW_VERSION" ]]; then
  echo "Expected updated CLI version $NEW_VERSION, got $UPDATED_CLI_VERSION." >&2
  exit 1
fi

{
  echo "result=passed"
  echo "old_version=$OLD_VERSION"
  echo "new_version=$NEW_VERSION"
  echo "feed_url=$FEED_URL"
  echo "appcast=$APPCAST"
  echo "updated_cli_version=$UPDATED_CLI_VERSION"
} > "$EVIDENCE_DIR/result.txt"

log "passed; evidence at $EVIDENCE_DIR"
cleanup_app
