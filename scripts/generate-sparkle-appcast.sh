#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
DMG="${1:-$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg}"
UPDATES_DIR="${2:-$ROOT/dist/sparkle-updates}"
APPCAST_NAME="${SPARKLE_APPCAST_NAME:-appcast.xml}"
APPCAST="$UPDATES_DIR/$APPCAST_NAME"
SPARKLE_ACCOUNT="${SPARKLE_KEY_ACCOUNT:-com.haptica.1context.sparkle}"
GENERATE_APPCAST="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_appcast"

if [[ ! -f "$DMG" ]]; then
  echo "Usage: $0 dist/1Context-$VERSION-macos-$ARCH.dmg [updates-dir]" >&2
  exit 1
fi

if [[ ! -x "$GENERATE_APPCAST" ]]; then
  swift build --package-path "$ROOT/macos" -c release --arch "$ARCH" >/dev/null
fi
if [[ ! -x "$GENERATE_APPCAST" ]]; then
  echo "Sparkle generate_appcast tool was not found at $GENERATE_APPCAST." >&2
  exit 1
fi

mkdir -p "$UPDATES_DIR"
DMG_BASENAME="$(basename "$DMG")"
UPDATE_DMG="$UPDATES_DIR/$DMG_BASENAME"
ditto "$DMG" "$UPDATE_DMG"

if [[ -f "$ROOT/RELEASE_NOTES.md" ]]; then
  cp "$ROOT/RELEASE_NOTES.md" "$UPDATES_DIR/${DMG_BASENAME%.dmg}.md"
fi

ARGS=("-o" "$APPCAST" "--embed-release-notes")
ARGS+=("--account" "$SPARKLE_ACCOUNT")
if [[ -n "${SPARKLE_DOWNLOAD_URL_PREFIX:-}" ]]; then
  ARGS+=("--download-url-prefix" "$SPARKLE_DOWNLOAD_URL_PREFIX")
fi
if [[ -n "${SPARKLE_RELEASE_NOTES_URL_PREFIX:-}" ]]; then
  ARGS+=("--release-notes-url-prefix" "$SPARKLE_RELEASE_NOTES_URL_PREFIX")
fi
if [[ -n "${SPARKLE_LINK_URL:-}" ]]; then
  ARGS+=("--link" "$SPARKLE_LINK_URL")
fi
if [[ -n "${SPARKLE_ED_KEY_FILE:-}" && -z "${SPARKLE_PRIVATE_ED_KEY:-}" ]]; then
  ARGS+=("--ed-key-file" "$SPARKLE_ED_KEY_FILE")
fi

if [[ -n "${SPARKLE_PRIVATE_ED_KEY:-}" ]]; then
  printf '%s' "$SPARKLE_PRIVATE_ED_KEY" | "$GENERATE_APPCAST" "${ARGS[@]}" --ed-key-file - "$UPDATES_DIR"
else
  "$GENERATE_APPCAST" "${ARGS[@]}" "$UPDATES_DIR"
fi

if [[ ! -f "$APPCAST" ]]; then
  echo "Sparkle appcast was not generated: $APPCAST" >&2
  exit 1
fi
if ! grep -q "$DMG_BASENAME" "$APPCAST"; then
  echo "Sparkle appcast does not reference $DMG_BASENAME." >&2
  exit 1
fi
if ! grep -q 'sparkle:edSignature=' "$APPCAST"; then
  echo "Sparkle appcast is missing EdDSA update signatures." >&2
  echo "Build the DMG with ONECONTEXT_SPARKLE_PUBLIC_ED_KEY and sign the feed with the matching Sparkle EdDSA private key." >&2
  exit 1
fi

echo "$APPCAST"
