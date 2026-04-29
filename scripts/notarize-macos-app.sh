#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="${1:-$ROOT/dist/1Context.app}"
PROFILE="${NOTARYTOOL_PROFILE:-1context-notary}"
ZIP_PATH="$ROOT/dist/1Context-notary.zip"

if [[ ! -d "$APP_DIR" ]]; then
  echo "App not found: $APP_DIR" >&2
  echo "Build it first with: ./scripts/build-macos-app.sh" >&2
  exit 1
fi

COPYFILE_DISABLE=1 ditto \
  --norsrc \
  --noextattr \
  --noqtn \
  --noacl \
  -c \
  -k \
  --keepParent \
  "$APP_DIR" \
  "$ZIP_PATH"

if [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]]; then
  xcrun notarytool submit "$ZIP_PATH" \
    --apple-id "$APPLE_ID" \
    --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_SPECIFIC_PASSWORD" \
    --wait
else
  xcrun notarytool submit "$ZIP_PATH" --keychain-profile "$PROFILE" --wait
fi

xcrun stapler staple "$APP_DIR"
xcrun stapler validate "$APP_DIR"
spctl --assess --type execute --verbose "$APP_DIR"

echo "$APP_DIR"
