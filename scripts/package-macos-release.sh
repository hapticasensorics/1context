#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-0.1.28}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
PACKAGE_DIR="$ROOT/dist/1context-$VERSION-macos-$ARCH"
ARCHIVE="$ROOT/dist/1context-$VERSION-macos-$ARCH.tar.gz"

if [[ "${NOTARIZE:-1}" != "1" && "${ALLOW_UNNOTARIZED:-0}" != "1" ]]; then
  echo "Release packaging requires notarization. Set ALLOW_UNNOTARIZED=1 for local-only builds." >&2
  exit 1
fi

if [[ "${NOTARIZE:-1}" == "1" ]]; then
  export ONECONTEXT_SIGNING_MODE="${ONECONTEXT_SIGNING_MODE:-developer-id}"
else
  export ONECONTEXT_SIGNING_MODE="${ONECONTEXT_SIGNING_MODE:-adhoc}"
fi
"$ROOT/scripts/build-macos-app.sh"
if [[ "${NOTARIZE:-1}" == "1" ]]; then
  "$ROOT/scripts/notarize-macos-app.sh" "$ROOT/dist/1Context.app"
fi

rm -rf "$PACKAGE_DIR" "$ARCHIVE"
mkdir -p "$PACKAGE_DIR/bin" "$PACKAGE_DIR/scripts"

ln -s ../1Context.app/Contents/MacOS/1context-cli "$PACKAGE_DIR/bin/1context"
cp "$ROOT/scripts/install-macos-launch-agents.sh" "$PACKAGE_DIR/scripts/"
cp "$ROOT/scripts/uninstall-macos-launch-agents.sh" "$PACKAGE_DIR/scripts/"
COPYFILE_DISABLE=1 ditto \
  --norsrc \
  --noextattr \
  --noqtn \
  --noacl \
  "$ROOT/dist/1Context.app" \
  "$PACKAGE_DIR/1Context.app"
xattr -cr "$PACKAGE_DIR" 2>/dev/null || true

COPYFILE_DISABLE=1 tar \
  --uid 0 \
  --gid 0 \
  --uname root \
  --gname wheel \
  --no-xattrs \
  --no-mac-metadata \
  -C "$ROOT/dist" \
  -czf "$ARCHIVE" \
  "$(basename "$PACKAGE_DIR")"
"$ROOT/scripts/validate-release-artifact.sh" "$ARCHIVE"
shasum -a 256 "$ARCHIVE"
echo "$ARCHIVE"
