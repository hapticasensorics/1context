#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
DMG="$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg"

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
  "$ROOT/scripts/notarize-macos-artifact.sh" "$ROOT/dist/1Context.app"
fi

if [[ "${RUN_PRODUCT_HTTPS_SMOKE:-0}" == "1" ]]; then
  ONECONTEXT_PRODUCT_HTTPS_SMOKE_INTERACTIVE=1 \
    "$ROOT/scripts/test-release-app-product-https.sh" "$ROOT/dist/1Context.app"
fi
"$ROOT/scripts/create-macos-dmg.sh" "$ROOT/dist/1Context.app" "$DMG" >/dev/null
if [[ "${NOTARIZE:-1}" == "1" ]]; then
  if [[ -z "${CODESIGN_IDENTITY:-}" ]]; then
    echo "Set CODESIGN_IDENTITY before notarizing the release DMG." >&2
    exit 1
  fi
  codesign --force --timestamp --sign "$CODESIGN_IDENTITY" "$DMG" >/dev/null
  codesign --verify --strict "$DMG" >/dev/null
  "$ROOT/scripts/notarize-macos-artifact.sh" "$DMG"
fi
"$ROOT/scripts/validate-macos-dmg.sh" "$DMG"
if [[ "${GENERATE_SPARKLE_APPCAST:-0}" == "1" ]]; then
  "$ROOT/scripts/generate-sparkle-appcast.sh" "$DMG"
fi
shasum -a 256 "$DMG"
echo "$DMG"
