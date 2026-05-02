#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
APP_DIR="$ROOT/dist/1Context.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_APP_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
FRAMEWORKS_DIR="$CONTENTS_DIR/Frameworks"
LAUNCH_DAEMONS_DIR="$CONTENTS_DIR/Library/LaunchDaemons"
SIGNING_MODE="${ONECONTEXT_SIGNING_MODE:-adhoc}"
IDENTITY="${CODESIGN_IDENTITY:-}"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
MENU_ICON_SOURCE="$MACOS_DIR/Sources/OneContextMenuBar/Resources/MenuBarIcon.png"
CADDY_SOURCE="${ONECONTEXT_CADDY_PATH:-$(command -v caddy 2>/dev/null || true)}"
SPARKLE_FEED_URL="${ONECONTEXT_SPARKLE_FEED_URL:-}"
SPARKLE_PUBLIC_ED_KEY="${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-}"

plist_escape() {
  local value="$1"
  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"
  value="${value//\"/&quot;}"
  value="${value//\'/&apos;}"
  printf '%s' "$value"
}

swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH" --show-bin-path)"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_APP_DIR" "$RESOURCES_DIR" "$FRAMEWORKS_DIR" "$LAUNCH_DAEMONS_DIR"

cp "$BIN_DIR/OneContextMenuBar" "$MACOS_APP_DIR/1Context"
cp "$BIN_DIR/1context" "$MACOS_APP_DIR/1context-cli"
cp "$BIN_DIR/1contextd" "$MACOS_APP_DIR/1contextd"
cp "$BIN_DIR/1context-local-web-proxy" "$RESOURCES_DIR/1context-local-web-proxy"
cp "$MENU_ICON_SOURCE" "$RESOURCES_DIR/MenuBarIcon.png"
if [[ ! -d "$BIN_DIR/Sparkle.framework" ]]; then
  echo "SwiftPM did not build Sparkle.framework beside OneContextMenuBar." >&2
  exit 1
fi
ditto "$BIN_DIR/Sparkle.framework" "$FRAMEWORKS_DIR/Sparkle.framework"
CADDY_BUNDLE_DIR="$RESOURCES_DIR/local-web/caddy"
if [[ -z "$CADDY_SOURCE" || ! -x "$CADDY_SOURCE" ]]; then
  echo "Release app build requires a Caddy binary. Install caddy or set ONECONTEXT_CADDY_PATH." >&2
  exit 1
fi
mkdir -p "$CADDY_BUNDLE_DIR"
cp "$CADDY_SOURCE" "$CADDY_BUNDLE_DIR/caddy"
chmod 755 "$CADDY_BUNDLE_DIR/caddy"
"$CADDY_BUNDLE_DIR/caddy" version > "$CADDY_BUNDLE_DIR/caddy.version"
cat > "$CADDY_BUNDLE_DIR/THIRD_PARTY_NOTICES.txt" <<'EOF'
Caddy
Homepage: https://caddyserver.com/
Source: https://github.com/caddyserver/caddy
License: Apache-2.0
Bundled by 1Context as the local web edge server so users do not need to
install or manage a separate Caddy dependency.
EOF
if command -v brew >/dev/null 2>&1; then
  CADDY_PREFIX="$(brew --prefix caddy 2>/dev/null || true)"
  if [[ -n "$CADDY_PREFIX" ]]; then
    for notice in LICENSE AUTHORS README.md sbom.spdx.json; do
      if [[ -f "$CADDY_PREFIX/$notice" ]]; then
        cp "$CADDY_PREFIX/$notice" "$CADDY_BUNDLE_DIR/$notice"
      fi
    done
  fi
fi
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
    "$RESOURCES_DIR/memory-core/memory/runtime" \
    "$RESOURCES_DIR/memory-core/storage/lakestore/artifacts.lance" \
    "$RESOURCES_DIR/memory-core/storage/lakestore/documents.lance" \
    "$RESOURCES_DIR/memory-core/storage/lakestore/events.lance" \
    "$RESOURCES_DIR/memory-core/storage/lakestore/evidence.lance" \
    "$RESOURCES_DIR/memory-core/storage/lakestore/sessions.lance" \
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

SPARKLE_PLIST_KEYS=""
if [[ -n "$SPARKLE_FEED_URL" || -n "$SPARKLE_PUBLIC_ED_KEY" ]]; then
  if [[ -z "$SPARKLE_FEED_URL" || -z "$SPARKLE_PUBLIC_ED_KEY" ]]; then
    echo "Set both ONECONTEXT_SPARKLE_FEED_URL and ONECONTEXT_SPARKLE_PUBLIC_ED_KEY to configure Sparkle." >&2
    exit 1
  fi
  SPARKLE_FEED_URL_ESCAPED="$(plist_escape "$SPARKLE_FEED_URL")"
  SPARKLE_PUBLIC_ED_KEY_ESCAPED="$(plist_escape "$SPARKLE_PUBLIC_ED_KEY")"
  SPARKLE_PLIST_KEYS="$(cat <<PLIST
  <key>SUFeedURL</key>
  <string>$SPARKLE_FEED_URL_ESCAPED</string>
  <key>SUPublicEDKey</key>
  <string>$SPARKLE_PUBLIC_ED_KEY_ESCAPED</string>
  <key>SUEnableAutomaticChecks</key>
  <true/>
PLIST
)"
fi

cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>1Context</string>
  <key>CFBundleIdentifier</key>
  <string>com.haptica.1context</string>
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
$SPARKLE_PLIST_KEYS
</dict>
</plist>
PLIST

cat > "$LAUNCH_DAEMONS_DIR/com.haptica.1context.local-web-proxy.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.haptica.1context.local-web-proxy</string>
  <key>BundleProgram</key>
  <string>Contents/Resources/1context-local-web-proxy</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
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
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/Autoupdate" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/Updater.app" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/XPCServices/Downloader.xpc" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/XPCServices/Installer.xpc" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework" >/dev/null
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --sign "$IDENTITY" \
    "$CADDY_BUNDLE_DIR/caddy" >/dev/null
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
    "$RESOURCES_DIR/1context-local-web-proxy" >/dev/null
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
  codesign --force --deep --sign - "$FRAMEWORKS_DIR/Sparkle.framework" >/dev/null
  codesign --force --sign - "$CADDY_BUNDLE_DIR/caddy" >/dev/null
  codesign --force --sign - "$MACOS_APP_DIR/1context-cli" >/dev/null
  codesign --force --sign - "$MACOS_APP_DIR/1contextd" >/dev/null
  codesign --force --sign - "$RESOURCES_DIR/1context-local-web-proxy" >/dev/null
  codesign --force --sign - "$MACOS_APP_DIR/1Context" >/dev/null
  codesign --force --sign - "$APP_DIR" >/dev/null
fi

echo "$APP_DIR"
