#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
APP_DIR="$ROOT/dist/1Context.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_APP_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
SIGNING_MODE="${ONECONTEXT_SIGNING_MODE:-adhoc}"
IDENTITY="${CODESIGN_IDENTITY:-}"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
MENU_ICON_SOURCE="$MACOS_DIR/Sources/OneContextMenuBar/Resources/MenuBarIcon.png"

swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH" --show-bin-path)"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_APP_DIR" "$RESOURCES_DIR"

cp "$BIN_DIR/OneContextMenuBar" "$MACOS_APP_DIR/1Context"
cp "$BIN_DIR/1context" "$MACOS_APP_DIR/1context-cli"
cp "$BIN_DIR/1contextd" "$MACOS_APP_DIR/1contextd"
cp "$MENU_ICON_SOURCE" "$RESOURCES_DIR/MenuBarIcon.png"
if [[ -d "$ROOT/memory-core" ]]; then
  COPYFILE_DISABLE=1 ditto \
    --norsrc \
    --noextattr \
    --noqtn \
    --noacl \
    "$ROOT/memory-core" \
    "$RESOURCES_DIR/memory-core"
  rm -rf \
    "$RESOURCES_DIR/memory-core/.venv" \
    "$RESOURCES_DIR/memory-core/.pytest_cache" \
    "$RESOURCES_DIR/memory-core/wiki-engine/node_modules"
  find "$RESOURCES_DIR/memory-core" -type d -name __pycache__ -prune -exec rm -rf {} +
  chmod +x "$RESOURCES_DIR/memory-core/bin/1context-memory-core"
fi

ICONSET="$ROOT/dist/AppIcon.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
sips -z 16 16 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_16x16.png" >/dev/null
sips -z 32 32 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
sips -z 32 32 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_32x32.png" >/dev/null
sips -z 64 64 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_128x128.png" >/dev/null
sips -z 256 256 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_256x256.png" >/dev/null
sips -z 512 512 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_512x512.png" >/dev/null
sips -z 1024 1024 "$MENU_ICON_SOURCE" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
iconutil -c icns "$ICONSET" -o "$RESOURCES_DIR/AppIcon.icns"
rm -rf "$ICONSET"

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
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
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

if [[ "$SIGNING_MODE" == "developer-id" ]]; then
  if [[ -z "$IDENTITY" ]]; then
    echo "Set CODESIGN_IDENTITY to the Developer ID Application identity for release signing." >&2
    exit 1
  fi

  if ! command -v codesign >/dev/null 2>&1 || ! security find-identity -v -p codesigning | grep -F "$IDENTITY" >/dev/null; then
    echo "Developer ID identity not found: $IDENTITY" >&2
    exit 1
  fi

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
elif command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - "$APP_DIR" >/dev/null
fi

echo "$APP_DIR"
