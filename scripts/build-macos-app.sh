#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
APP_DIR="$ROOT/dist/1Context.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_APP_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
IDENTITY="${CODESIGN_IDENTITY:-Developer ID Application: Paul Han (NBY9V4M69J)}"
VERSION="${ONECONTEXT_VERSION:-0.1.24}"
ARCH="${ONECONTEXT_ARCH:-arm64}"

swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH" --show-bin-path)"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_APP_DIR" "$RESOURCES_DIR"

cp "$BIN_DIR/OneContextMenuBar" "$MACOS_APP_DIR/1Context"
cp "$BIN_DIR/1context" "$MACOS_APP_DIR/1context-cli"
cp "$BIN_DIR/1contextd" "$MACOS_APP_DIR/1contextd"
cp "$MACOS_DIR/Sources/OneContextMenuBar/Resources/MenuBarIcon.png" "$RESOURCES_DIR/MenuBarIcon.png"

cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>1Context</string>
  <key>CFBundleIdentifier</key>
  <string>com.haptica.1context.menu</string>
  <key>CFBundleName</key>
  <string>1Context</string>
  <key>CFBundleDisplayName</key>
  <string>1Context</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
PLIST

if command -v codesign >/dev/null 2>&1 && security find-identity -v -p codesigning | grep -F "$IDENTITY" >/dev/null; then
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1context-cli" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1contextd" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1Context" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$APP_DIR" >/dev/null
elif [[ "${REQUIRE_DEVELOPER_ID:-0}" == "1" ]]; then
  echo "Developer ID identity not found: $IDENTITY" >&2
  exit 1
elif command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - "$APP_DIR" >/dev/null
fi

echo "$APP_DIR"
