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
MANIFEST="$TMPDIR/manifest.txt"
tar -tzf "$ARCHIVE" | LC_ALL=C sort > "$MANIFEST"

archive_name="$(basename "$ARCHIVE")"
if [[ ! "$archive_name" =~ ^1context-([0-9]+\.[0-9]+\.[0-9]+)-macos-(arm64)\.tar\.gz$ ]]; then
  echo "Release archive name does not match expected version/arch pattern." >&2
  exit 1
fi
VERSION="${BASH_REMATCH[1]}"
ARCH="${BASH_REMATCH[2]}"
ROOT_NAME="1context-$VERSION-macos-$ARCH"

EXPECTED_MANIFEST="$TMPDIR/expected-manifest.txt"
cat > "$EXPECTED_MANIFEST" <<EOF
$ROOT_NAME/
$ROOT_NAME/1Context.app/
$ROOT_NAME/1Context.app/Contents/
$ROOT_NAME/1Context.app/Contents/Info.plist
$ROOT_NAME/1Context.app/Contents/MacOS/
$ROOT_NAME/1Context.app/Contents/MacOS/1Context
$ROOT_NAME/1Context.app/Contents/MacOS/1context-cli
$ROOT_NAME/1Context.app/Contents/MacOS/1contextd
$ROOT_NAME/1Context.app/Contents/Resources/
$ROOT_NAME/1Context.app/Contents/Resources/AppIcon.icns
$ROOT_NAME/1Context.app/Contents/Resources/MenuBarIcon.png
$ROOT_NAME/1Context.app/Contents/_CodeSignature/
$ROOT_NAME/1Context.app/Contents/_CodeSignature/CodeResources
$ROOT_NAME/bin/
$ROOT_NAME/bin/1context
$ROOT_NAME/scripts/
$ROOT_NAME/scripts/install-macos-launch-agents.sh
$ROOT_NAME/scripts/uninstall-macos-launch-agents.sh
EOF
LC_ALL=C sort -o "$EXPECTED_MANIFEST" "$EXPECTED_MANIFEST"

if ! diff -u "$EXPECTED_MANIFEST" "$MANIFEST"; then
  echo "Release archive manifest does not match expected contents." >&2
  exit 1
fi

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

if [[ "$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "$VERSION" ]]; then
  echo "Release app Info.plist version does not match archive version." >&2
  exit 1
fi

if [[ "$("$APP/Contents/MacOS/1context-cli" --version)" != "$VERSION" ]]; then
  echo "Release CLI version does not match archive version." >&2
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
