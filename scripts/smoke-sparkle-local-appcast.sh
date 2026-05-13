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
RETRY_AFTER_FAILURE="${ONECONTEXT_SPARKLE_SMOKE_RETRY_AFTER_FAILURE:-0}"
PROVE_RUNTIME_SURVIVES_FAILURE="${ONECONTEXT_SPARKLE_SMOKE_PROVE_RUNTIME_SURVIVES_FAILURE:-0}"
SILENT_FAILURE_WATCH_SECONDS="${ONECONTEXT_SPARKLE_SMOKE_SILENT_FAILURE_WATCH_SECONDS:-25}"
GENERATE_APPCAST="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_appcast"

if [[ -n "$FAILURE_CASE" &&
  "$FAILURE_CASE" != "missing_asset" &&
  "$FAILURE_CASE" != "bad_signature" &&
  "$FAILURE_CASE" != "broken_appcast" &&
  "$FAILURE_CASE" != "interrupted_download" ]]; then
  echo "ONECONTEXT_SPARKLE_SMOKE_FAILURE_CASE must be empty, missing_asset, bad_signature, broken_appcast, or interrupted_download." >&2
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
RUNTIME_WIKI_PORT="${ONECONTEXT_SPARKLE_SMOKE_WIKI_PORT:-$(pick_free_port)}"
RUNTIME_WIKI_API_PORT="${ONECONTEXT_SPARKLE_SMOKE_WIKI_API_PORT:-$(pick_free_port)}"
if [[ "$RUNTIME_WIKI_PORT" == "$PORT" ]]; then
  RUNTIME_WIKI_PORT="$(pick_free_port)"
fi
if [[ "$RUNTIME_WIKI_API_PORT" == "$PORT" || "$RUNTIME_WIKI_API_PORT" == "$RUNTIME_WIKI_PORT" ]]; then
  RUNTIME_WIKI_API_PORT="$(pick_free_port)"
fi
RUNTIME_SOCKET_PATH="${ONECONTEXT_SPARKLE_SMOKE_SOCKET_PATH:-/tmp/1context-sparkle-smoke-$PORT.sock}"
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
  echo "retry_after_failure=$RETRY_AFTER_FAILURE"
  echo "prove_runtime_survives_failure=$PROVE_RUNTIME_SURVIVES_FAILURE"
  echo "silent_failure_watch_seconds=$SILENT_FAILURE_WATCH_SECONDS"
  echo "runtime_wiki_port=$RUNTIME_WIKI_PORT"
  echo "runtime_wiki_api_port=$RUNTIME_WIKI_API_PORT"
  echo "runtime_socket_path=$RUNTIME_SOCKET_PATH"
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

click_failure_try_again() {
  osascript >/dev/null 2>&1 <<'APPLESCRIPT' || true
tell application "System Events"
  repeat with proc in application processes
    set procName to name of proc
    if procName contains "1Context" then
      repeat with win in windows of proc
        try
          click button "Try Again" of win
          return
        end try
      end repeat
    end if
  end repeat
end tell
APPLESCRIPT
}

repair_failure_for_retry() {
  case "$FAILURE_CASE" in
    missing_asset)
      cp "$NEW_DMG" "$UPDATES_DIR/$(basename "$NEW_DMG")"
      {
        echo "restored missing DMG before clicking Try Again"
        echo "dmg=$UPDATES_DIR/$(basename "$NEW_DMG")"
        echo "sha256_repaired=$(shasum -a 256 "$UPDATES_DIR/$(basename "$NEW_DMG")" | awk '{ print $1 }')"
      } > "$EVIDENCE_DIR/retry-repair.txt"
      ;;
    broken_appcast)
      cp "$EVIDENCE_DIR/appcast-before-corruption.xml" "$APPCAST"
      {
        echo "repaired broken appcast before clicking Try Again"
        echo "appcast=$APPCAST"
        echo "sha256_repaired=$(shasum -a 256 "$APPCAST" | awk '{ print $1 }')"
      } > "$EVIDENCE_DIR/retry-repair.txt"
      ;;
    *)
      echo "Retry repair is not implemented for failure_case=$FAILURE_CASE." >&2
      return 1
      ;;
  esac
}

run_smoke_cli() {
  env \
    ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context" \
    ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context" \
    ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
    ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context" \
    ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context" \
    ONECONTEXT_SOCKET_PATH="$RUNTIME_SOCKET_PATH" \
    ONECONTEXT_WIKI_URL_MODE=high-port-http \
    ONECONTEXT_WIKI_PORT="$RUNTIME_WIKI_PORT" \
    ONECONTEXT_WIKI_API_PORT="$RUNTIME_WIKI_API_PORT" \
    "$INSTALL_APP/Contents/MacOS/1context-cli" "$@"
}

capture_runtime_survival_status() {
  local name="$1"
  run_smoke_cli status --debug > "$EVIDENCE_DIR/runtime-status-$name.txt" 2>&1
}

wait_for_runtime_survival_status() {
  local name="$1"
  local expected_version="$2"
  local status_file="$EVIDENCE_DIR/runtime-status-$name.txt"
  for _ in {1..80}; do
    if capture_runtime_survival_status "$name" &&
      grep -q "1Context is running." "$status_file" &&
      grep -q "Version: $expected_version" "$status_file" &&
      grep -q "Health: OK" "$status_file" &&
      grep -q "Socket: responding" "$status_file"; then
      cat "$STATE_DIR/Application Support/1Context/run/1contextd.pid" > "$EVIDENCE_DIR/runtime-pid-$name.txt"
      return 0
    fi
    sleep 0.25
  done
  echo "Timed out waiting for smoke runtime $name status $expected_version." >&2
  echo "Evidence: $status_file" >&2
  return 1
}

start_runtime_survival_proof() {
  log "starting old fixture runtime for failed-update survival proof"
  run_smoke_cli start --debug > "$EVIDENCE_DIR/runtime-start.txt" 2>&1
  wait_for_runtime_survival_status before "$OLD_VERSION"
}

finish_runtime_survival_proof() {
  wait_for_runtime_survival_status after "$OLD_VERSION"
  local before_pid
  local after_pid
  before_pid="$(tr -d '[:space:]' < "$EVIDENCE_DIR/runtime-pid-before.txt")"
  after_pid="$(tr -d '[:space:]' < "$EVIDENCE_DIR/runtime-pid-after.txt")"
  if [[ -z "$before_pid" || "$before_pid" != "$after_pid" ]]; then
    echo "Expected failed update to leave runtime PID unchanged, before=$before_pid after=$after_pid." >&2
    return 1
  fi
  {
    echo "runtime_survived=1"
    echo "runtime_pid_before=$before_pid"
    echo "runtime_pid_after=$after_pid"
    echo "runtime_version=$OLD_VERSION"
    echo "desired_state=$(tr -d '[:space:]' < "$STATE_DIR/Application Support/1Context/desired-state")"
    echo "socket=$RUNTIME_SOCKET_PATH"
    echo "status_before=$EVIDENCE_DIR/runtime-status-before.txt"
    echo "status_after=$EVIDENCE_DIR/runtime-status-after.txt"
  } > "$EVIDENCE_DIR/runtime-survival.txt"
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
      grep -Fq "button	Try Again" "$accessibility"
      grep -Fq "button	OK" "$accessibility"
      if grep -Eiq '404|not found|download|signature|Sparkle|installer|relaunch' "$accessibility"; then
        echo "Failure alert exposed technical update details. Evidence: $accessibility" >&2
        return 1
      fi
      echo "title=$FAILURE_TITLE" > "$EVIDENCE_DIR/failure-message.txt"
      echo "body=$FAILURE_BODY" >> "$EVIDENCE_DIR/failure-message.txt"
      echo "buttons=Try Again, OK" >> "$EVIDENCE_DIR/failure-message.txt"
      if [[ "$RETRY_AFTER_FAILURE" == "1" ]]; then
        repair_failure_for_retry
        echo "action=try_again" >> "$EVIDENCE_DIR/failure-message.txt"
        click_failure_try_again
      else
        echo "action=ok" >> "$EVIDENCE_DIR/failure-message.txt"
        click_post_install_ok
      fi
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

watch_for_no_failure_message() {
  local deadline=$(( "$(date +%s)" + SILENT_FAILURE_WATCH_SECONDS ))
  local attempt=0
  while true; do
    attempt=$((attempt + 1))
    local accessibility="$EVIDENCE_DIR/silent-failure-accessibility-$attempt.txt"
    capture_accessibility "$accessibility"
    capture_screenshot "$EVIDENCE_DIR/silent-failure-desktop-$attempt.png"
    cp "$accessibility" "$EVIDENCE_DIR/silent-failure-accessibility.txt"
    cp "$EVIDENCE_DIR/silent-failure-desktop-$attempt.png" "$EVIDENCE_DIR/silent-failure-desktop.png"
    if grep -Fq "$FAILURE_TITLE" "$accessibility" ||
      grep -Fq "$FAILURE_BODY" "$accessibility"; then
      echo "Automatic check-only failure unexpectedly showed support UI. Evidence: $accessibility" >&2
      return 1
    fi
    if (( "$(date +%s)" >= deadline )); then
      return 0
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
  if [[ -x "$INSTALL_APP/Contents/MacOS/1context-cli" ]]; then
    run_smoke_cli quit >/dev/null 2>&1 || true
  fi
  rm -f "$RUNTIME_SOCKET_PATH"
  pkill -f "$INSTALL_APP/Contents/MacOS/1Context" >/dev/null 2>&1 || true
  if [[ "${ONECONTEXT_SPARKLE_SMOKE_KEEP_APP:-0}" != "1" ]]; then
    rm -rf "$INSTALL_APP"
  fi
}

start_update_server() {
  if [[ "$FAILURE_CASE" == "interrupted_download" ]]; then
    local interrupted_dmg
    local interrupted_dmg_path
    local declared_bytes
    local sent_bytes
    interrupted_dmg="$(basename "$NEW_DMG")"
    interrupted_dmg_path="$UPDATES_DIR/$interrupted_dmg"
    declared_bytes="$(wc -c < "$interrupted_dmg_path" | tr -d '[:space:]')"
    sent_bytes="${ONECONTEXT_SPARKLE_SMOKE_INTERRUPTED_BYTES:-65536}"
    if (( sent_bytes >= declared_bytes )); then
      sent_bytes=$((declared_bytes / 2))
    fi
    if (( sent_bytes < 1 )); then
      sent_bytes=1
    fi
    {
      echo "interrupted DMG response after appcast signing for interrupted_download proof"
      echo "dmg=$interrupted_dmg"
      echo "declared_bytes=$declared_bytes"
      echo "sent_bytes=$sent_bytes"
      echo "sha256=$(shasum -a 256 "$interrupted_dmg_path" | awk '{ print $1 }')"
    } > "$EVIDENCE_DIR/download-interruption.txt"
    python3 - "$UPDATES_DIR" "$PORT" "$interrupted_dmg" "$sent_bytes" \
      > "$EVIDENCE_DIR/http-server.log" 2>&1 <<'PY' &
import functools
import http.server
import os
import socket
import socketserver
import sys

updates_dir = sys.argv[1]
port = int(sys.argv[2])
interrupted_name = sys.argv[3]
sent_bytes = int(sys.argv[4])

class InterruptedDownloadHandler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        requested = os.path.basename(self.path.split("?", 1)[0])
        if requested == interrupted_name:
            target = os.path.join(updates_dir, interrupted_name)
            declared_bytes = os.path.getsize(target)
            with open(target, "rb") as file:
                payload = file.read(min(sent_bytes, declared_bytes))
            self.send_response(200)
            self.send_header("Content-Type", "application/x-apple-diskimage")
            self.send_header("Content-Length", str(declared_bytes))
            self.send_header("Accept-Ranges", "none")
            self.end_headers()
            self.wfile.write(payload)
            self.wfile.flush()
            print(
                f"interrupted_download path={self.path} declared_bytes={declared_bytes} sent_bytes={len(payload)}",
                flush=True,
            )
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.connection.close()
            return
        return super().do_GET()

handler = functools.partial(InterruptedDownloadHandler, directory=updates_dir)
with socketserver.TCPServer(("127.0.0.1", port), handler) as httpd:
    httpd.serve_forever()
PY
    SERVER_PID="$!"
  else
    python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$UPDATES_DIR" \
      > "$EVIDENCE_DIR/http-server.log" 2>&1 &
    SERVER_PID="$!"
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
elif [[ "$FAILURE_CASE" == "broken_appcast" ]]; then
  cp "$APPCAST" "$EVIDENCE_DIR/appcast-before-corruption.xml"
  {
    echo "corrupted appcast XML after mandatory appcast validation for broken_appcast proof"
    echo "sha256_before=$(shasum -a 256 "$APPCAST" | awk '{ print $1 }')"
  } > "$EVIDENCE_DIR/appcast-corruption.txt"
  printf '%s\n' '<rss><channel><item><title>broken 1Context appcast' > "$APPCAST"
  echo "sha256_after=$(shasum -a 256 "$APPCAST" | awk '{ print $1 }')" >> "$EVIDENCE_DIR/appcast-corruption.txt"
fi
plutil -extract SUPublicEDKey raw "$OLD_APP/Contents/Info.plist" | grep -qx "$PUBLIC_KEY"
plutil -extract SUAutomaticallyUpdate raw "$OLD_APP/Contents/Info.plist" | grep -qx true
plutil -extract SUVerifyUpdateBeforeExtraction raw "$OLD_APP/Contents/Info.plist" | grep -qx true

log "serving local appcast on $FEED_URL"
start_update_server
sleep 1
curl --fail --silent "$FEED_URL" > "$EVIDENCE_DIR/appcast-served.xml"

log "installing old fixture to $INSTALL_APP"
cleanup_app
mkdir -p "$(dirname "$INSTALL_APP")"
COPYFILE_DISABLE=1 ditto --norsrc --noextattr --noqtn --noacl "$OLD_APP" "$INSTALL_APP"
wait_for_version "$OLD_VERSION"
if [[ -n "$FAILURE_CASE" && "$PROVE_RUNTIME_SURVIVES_FAILURE" == "1" ]]; then
  start_runtime_survival_proof
fi

log "launching old fixture and waiting for Sparkle update"
"$INSTALL_APP/Contents/MacOS/1Context" > "$EVIDENCE_DIR/app.log" 2>&1 &
APP_PID="$!"
echo "$APP_PID" > "$EVIDENCE_DIR/initial-app.pid"

if [[ -n "$FAILURE_CASE" ]]; then
  if [[ "$FAILURE_CASE" == "broken_appcast" ]]; then
    if [[ "$RETRY_AFTER_FAILURE" == "1" ]]; then
      echo "broken_appcast is an automatic check-only failure and should not expose Try Again UI." >&2
      exit 1
    fi
    watch_for_no_failure_message
    wait_for_version "$OLD_VERSION"
    FAILED_CLI_VERSION="$("$INSTALL_APP/Contents/MacOS/1context-cli" --version)"
    echo "$FAILED_CLI_VERSION" > "$EVIDENCE_DIR/silent-failure-cli-version.txt"
    if [[ "$FAILED_CLI_VERSION" != "$OLD_VERSION" ]]; then
      echo "Expected broken appcast check-only failure to leave CLI version $OLD_VERSION, got $FAILED_CLI_VERSION." >&2
      exit 1
    fi
    {
      echo "result=passed"
      echo "failure_case=$FAILURE_CASE"
      echo "observed=no support alert for automatic check-only failure"
      echo "old_version=$OLD_VERSION"
      echo "attempted_new_version=$NEW_VERSION"
      echo "feed_url=$FEED_URL"
      echo "appcast=$APPCAST"
      echo "installed_cli_version=$FAILED_CLI_VERSION"
      echo "watch_seconds=$SILENT_FAILURE_WATCH_SECONDS"
    } > "$EVIDENCE_DIR/result.txt"
    log "passed $FAILURE_CASE silent check-only proof; evidence at $EVIDENCE_DIR"
    cleanup_app
    exit 0
  fi
  wait_for_failure_message
  if [[ "$PROVE_RUNTIME_SURVIVES_FAILURE" == "1" ]]; then
    finish_runtime_survival_proof
  fi
  if [[ "$RETRY_AFTER_FAILURE" == "1" ]]; then
    wait_for_version "$NEW_VERSION"
    RETRIED_CLI_VERSION="$("$INSTALL_APP/Contents/MacOS/1context-cli" --version)"
    echo "$RETRIED_CLI_VERSION" > "$EVIDENCE_DIR/retried-cli-version.txt"
    if [[ "$RETRIED_CLI_VERSION" != "$NEW_VERSION" ]]; then
      echo "Expected retry to update CLI to $NEW_VERSION, got $RETRIED_CLI_VERSION." >&2
      exit 1
    fi
    {
      echo "result=passed"
      echo "failure_case=$FAILURE_CASE"
      echo "retry_after_failure=1"
      echo "old_version=$OLD_VERSION"
      echo "new_version=$NEW_VERSION"
      echo "feed_url=$FEED_URL"
      echo "appcast=$APPCAST"
      echo "retried_cli_version=$RETRIED_CLI_VERSION"
      if [[ "$PROVE_RUNTIME_SURVIVES_FAILURE" == "1" ]]; then
        cat "$EVIDENCE_DIR/runtime-survival.txt"
      fi
    } > "$EVIDENCE_DIR/result.txt"
    log "passed $FAILURE_CASE retry proof; evidence at $EVIDENCE_DIR"
  else
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
      if [[ "$PROVE_RUNTIME_SURVIVES_FAILURE" == "1" ]]; then
        cat "$EVIDENCE_DIR/runtime-survival.txt"
      fi
    } > "$EVIDENCE_DIR/result.txt"
    log "passed $FAILURE_CASE failure proof; evidence at $EVIDENCE_DIR"
  fi
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
