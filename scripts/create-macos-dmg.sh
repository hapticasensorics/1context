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

OUTPUT_DMG="$TMPDIR/$(basename "$DMG")"

create_dmg() {
  hdiutil create \
    -volname "$VOLUME_NAME" \
    -srcfolder "$STAGING" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -ov \
    "$OUTPUT_DMG" >/dev/null
}

create_status=1
for attempt in 1 2 3; do
  rm -f "$OUTPUT_DMG"
  if create_dmg; then
    create_status=0
    break
  fi
  create_status=$?
  if [[ "$attempt" == "3" ]]; then
    break
  fi
  echo "hdiutil create failed, retrying attempt $((attempt + 1)) of 3..." >&2
  sleep "$attempt"
done

if [[ "$create_status" != "0" ]]; then
  exit "$create_status"
fi

mkdir -p "$(dirname "$DMG")"
rm -f "$DMG"
mv "$OUTPUT_DMG" "$DMG"

echo "$DMG"
