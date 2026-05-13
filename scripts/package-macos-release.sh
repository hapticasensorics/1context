#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
DMG="$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg"
CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-${ONECONTEXT_RELEASE_KEYCHAIN:-}}"

if [[ "${ONECONTEXT_USE_RELEASE_POLICY:-1}" == "1" ]]; then
  eval "$("$ROOT/scripts/update-policy.py" export-env)"
fi

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
  codesign_args=(--force --timestamp --sign "$CODESIGN_IDENTITY")
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    codesign_args+=(--keychain "$CODESIGN_KEYCHAIN")
  fi
  codesign "${codesign_args[@]}" "$DMG" >/dev/null
  codesign --verify --strict "$DMG" >/dev/null
  "$ROOT/scripts/notarize-macos-artifact.sh" "$DMG"
fi
"$ROOT/scripts/validate-macos-dmg.sh" "$DMG"
if [[ "${GENERATE_SPARKLE_APPCAST:-0}" == "1" ]]; then
  "$ROOT/scripts/generate-sparkle-appcast.sh" "$DMG"
  "$ROOT/scripts/update-policy.py" validate --appcast "$ROOT/dist/sparkle-updates/appcast.xml"
  cp "$ROOT/dist/sparkle-updates/appcast.xml" "$ROOT/dist/appcast.xml"
fi
cp "$DMG" "$ROOT/dist/1Context.dmg"
shasum -a 256 "$DMG" > "$DMG.sha256"
shasum -a 256 "$ROOT/dist/1Context.dmg" > "$ROOT/dist/1Context.dmg.sha256"
cat "$DMG.sha256"
echo "$DMG"
