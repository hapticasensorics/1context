#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
DMG="$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg"

export ONECONTEXT_SIGNING_MODE="${ONECONTEXT_SIGNING_MODE:-adhoc}"

"$ROOT/scripts/build-macos-app.sh"
"$ROOT/scripts/create-macos-dmg.sh" "$ROOT/dist/1Context.app" "$DMG" >/dev/null
ALLOW_UNNOTARIZED=1 "$ROOT/scripts/validate-macos-dmg.sh" "$DMG"
echo "$DMG"
