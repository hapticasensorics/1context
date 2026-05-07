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
GENERATE_APPCAST="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_appcast"

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
  echo "work_dir=$WORK_DIR"
} > "$EVIDENCE_DIR/environment.txt"

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
plutil -extract SUPublicEDKey raw "$OLD_APP/Contents/Info.plist" | grep -qx "$PUBLIC_KEY"
plutil -extract SUAutomaticallyUpdate raw "$OLD_APP/Contents/Info.plist" | grep -qx true

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

wait_for_version "$NEW_VERSION"
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
