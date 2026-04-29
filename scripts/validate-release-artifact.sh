#!/usr/bin/env bash
set -euo pipefail

ARCHIVE="${1:-}"

if [[ -z "$ARCHIVE" || ! -f "$ARCHIVE" ]]; then
  echo "Usage: $0 dist/1context-VERSION-macos-ARCH.tar.gz" >&2
  exit 1
fi

TMPDIR="$(mktemp -d /tmp/1ctx-artifact-XXXXXX)"
cleanup() {
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

LISTING="$TMPDIR/listing.txt"
tar -tvzf "$ARCHIVE" > "$LISTING"

if grep -Eq ' paulhan | staff ' "$LISTING"; then
  echo "Release archive contains local owner/group metadata." >&2
  exit 1
fi

if grep -Eq '(^|/)\._|(^|/)\.DS_Store' "$LISTING"; then
  echo "Release archive contains macOS metadata files." >&2
  exit 1
fi

tar -C "$TMPDIR" -xzf "$ARCHIVE"
apps=("$TMPDIR"/1context-*/1Context.app)
APP="${apps[0]}"
PACKAGE_ROOT="$(dirname "$APP")"

if [[ ${#apps[@]} -ne 1 || ! -d "$APP" ]]; then
  echo "Release archive does not contain 1Context.app." >&2
  exit 1
fi

if [[ ! -f "$APP/Contents/Resources/AppIcon.icns" ]]; then
  echo "Release app is missing AppIcon.icns." >&2
  exit 1
fi

if [[ "$(plutil -extract CFBundleIconFile raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "AppIcon" ]]; then
  echo "Release app Info.plist does not set CFBundleIconFile to AppIcon." >&2
  exit 1
fi

if [[ "$(readlink "$PACKAGE_ROOT/bin/1context" 2>/dev/null || true)" != "../1Context.app/Contents/MacOS/1context-cli" ]]; then
  echo "Release archive has an unexpected 1context symlink target." >&2
  exit 1
fi

codesign --verify --deep --strict "$APP" >/dev/null

if [[ "${ALLOW_UNNOTARIZED:-0}" != "1" ]]; then
  xcrun stapler validate "$APP" >/dev/null
  spctl --assess --type execute --verbose "$APP" >/dev/null
fi

for binary in "$APP/Contents/MacOS/1Context" "$APP/Contents/MacOS/1context-cli" "$APP/Contents/MacOS/1contextd"; do
  if [[ ! -x "$binary" ]]; then
    echo "Missing executable: $binary" >&2
    exit 1
  fi

  if [[ "$(lipo -archs "$binary")" != "arm64" ]]; then
    echo "Release binary is not arm64-only: $binary" >&2
    exit 1
  fi

  if [[ "$(stat -f "%Lp" "$binary")" != "755" ]]; then
    echo "Release binary has unexpected mode: $binary" >&2
    exit 1
  fi

  if strings "$binary" | grep -Eq '/Users/|paulhan|1context-public-launch|OneContextMac_.*\.bundle|could not load resource bundle'; then
    echo "Release binary contains local build paths or SwiftPM resource-bundle fallback text: $binary" >&2
    exit 1
  fi
done

if find "$TMPDIR" \( -name '._*' -o -name '.DS_Store' \) | grep -q .; then
  echo "Release archive extracted macOS metadata files." >&2
  exit 1
fi

echo "Release artifact validation passed."
