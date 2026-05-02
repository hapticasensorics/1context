#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
APP="${1:-$ROOT/dist/1Context.app}"
DMG="${2:-$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg}"
VOLUME_NAME="${ONECONTEXT_DMG_VOLUME_NAME:-1Context}"

if [[ ! -d "$APP" ]]; then
  echo "App not found: $APP" >&2
  echo "Build one first with: ./scripts/build-macos-app.sh" >&2
  exit 1
fi

TMPDIR="$(mktemp -d /tmp/1ctx-dmg-XXXXXX)"
cleanup() {
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

STAGING="$TMPDIR/staging"
mkdir -p "$STAGING"

COPYFILE_DISABLE=1 ditto \
  --norsrc \
  --noextattr \
  --noqtn \
  --noacl \
  "$APP" \
  "$STAGING/1Context.app"

ln -s /Applications "$STAGING/Applications"

rm -f "$DMG"
hdiutil create \
  -volname "$VOLUME_NAME" \
  -srcfolder "$STAGING" \
  -format UDZO \
  -imagekey zlib-level=9 \
  -ov \
  "$DMG" >/dev/null

echo "$DMG"
