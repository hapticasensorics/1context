#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ -n "${ONECONTEXT_RELEASE_CHANNEL:-}" ]]; then
  eval "$("$ROOT/scripts/release-train.sh" manifest export-env --channel "$ONECONTEXT_RELEASE_CHANNEL")"
else
  eval "$("$ROOT/scripts/release-train.sh" manifest export-env)"
fi
MACOS_DIR="$ROOT/macos"
APP_DIR="$ROOT/dist/1Context.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_APP_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
FRAMEWORKS_DIR="$CONTENTS_DIR/Frameworks"
LAUNCH_DAEMONS_DIR="$CONTENTS_DIR/Library/LaunchDaemons"
SIGNING_MODE="${ONECONTEXT_SIGNING_MODE:-adhoc}"
IDENTITY="${CODESIGN_IDENTITY:-}"
CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-${ONECONTEXT_RELEASE_KEYCHAIN:-}}"
VERSION="$ONECONTEXT_RELEASE_VERSION"
ARCH="${ONECONTEXT_ARCH:-arm64}"
BUNDLE_IDENTIFIER="${ONECONTEXT_BUNDLE_IDENTIFIER:-com.haptica.1context}"
MENU_ICON_SOURCE="$MACOS_DIR/Sources/OneContextMenuBar/Resources/MenuBarIcon.png"
CADDY_VERSION="2.11.2"
CADDY_TOOL_ARCHIVE="$ROOT/release/tools/caddy/darwin-$ARCH/caddy-v$CADDY_VERSION-darwin-$ARCH.tar.gz"
CADDY_TOOL_SHA256="$CADDY_TOOL_ARCHIVE.sha256"
CADDY_TOOL_WORK_DIR="$ROOT/dist/release-tools/caddy/darwin-$ARCH"
CADDY_SOURCE=""
CADDY_NOTICE_SOURCE_DIR=""
RUNTIME_DEFAULTS_WORK_DIR="$ROOT/dist/runtime-defaults"
RUNTIME_DEFAULTS_RESOURCE_DIR="$RESOURCES_DIR/RuntimeDefaults"
WIKI_ENGINE_RESOURCE_DIR="$RESOURCES_DIR/WikiEngine"
SPARKLE_FEED_URL="$ONECONTEXT_SPARKLE_FEED_URL"
SPARKLE_PUBLIC_ED_KEY="${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-}"
UPDATE_OPTIONAL_PROMPT_TITLE="$ONECONTEXT_UPDATE_OPTIONAL_PROMPT_TITLE"
UPDATE_OPTIONAL_PROMPT_BODY="$ONECONTEXT_UPDATE_OPTIONAL_PROMPT_BODY"
UPDATE_FAILURE_TITLE="$ONECONTEXT_UPDATE_FAILURE_TITLE"
UPDATE_FAILURE_BODY="$ONECONTEXT_UPDATE_FAILURE_BODY"
UPDATE_POST_INSTALL_MESSAGE_ENABLED="$ONECONTEXT_UPDATE_POST_INSTALL_MESSAGE_ENABLED"
UPDATE_POST_INSTALL_TITLE="$ONECONTEXT_UPDATE_POST_INSTALL_TITLE"
UPDATE_POST_INSTALL_BODY="$ONECONTEXT_UPDATE_POST_INSTALL_BODY"
UPDATE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW="$ONECONTEXT_SPARKLE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW"

plist_escape() {
  local value="$1"
  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"
  value="${value//\"/&quot;}"
  value="${value//\'/&apos;}"
  printf '%s' "$value"
}

codesign_identity_available() {
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    security find-identity -v -p codesigning "$CODESIGN_KEYCHAIN" | grep -F "$IDENTITY" >/dev/null
  else
    security find-identity -v -p codesigning | grep -F "$IDENTITY" >/dev/null
  fi
}

codesign_release() {
  local args=(--force --options runtime --timestamp)
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    args+=(--keychain "$CODESIGN_KEYCHAIN")
  fi
  codesign "${args[@]}" "$@"
}

release_caddy_source() {
  if [[ ! -f "$CADDY_TOOL_ARCHIVE" || ! -f "$CADDY_TOOL_SHA256" ]]; then
    echo "Release-owned Caddy artifact is missing: $CADDY_TOOL_ARCHIVE" >&2
    exit 1
  fi

  local expected_sha
  local actual_sha
  expected_sha="$(awk '{print $1}' "$CADDY_TOOL_SHA256")"
  actual_sha="$(shasum -a 256 "$CADDY_TOOL_ARCHIVE" | awk '{print $1}')"
  if [[ -z "$expected_sha" || "$actual_sha" != "$expected_sha" ]]; then
    echo "Release-owned Caddy artifact checksum mismatch." >&2
    echo "Expected: $expected_sha" >&2
    echo "Actual:   $actual_sha" >&2
    exit 1
  fi

  rm -rf "$CADDY_TOOL_WORK_DIR"
  mkdir -p "$CADDY_TOOL_WORK_DIR"
  tar -xzf "$CADDY_TOOL_ARCHIVE" -C "$CADDY_TOOL_WORK_DIR"
  chmod 755 "$CADDY_TOOL_WORK_DIR/caddy"
  CADDY_SOURCE="$CADDY_TOOL_WORK_DIR/caddy"
  CADDY_NOTICE_SOURCE_DIR="$CADDY_TOOL_WORK_DIR"
}

resolve_caddy_source() {
  if [[ "$ONECONTEXT_RELEASE_CHANNEL" == "dev" ]]; then
    if [[ -n "${ONECONTEXT_CADDY_PATH:-}" ]]; then
      CADDY_SOURCE="$ONECONTEXT_CADDY_PATH"
      CADDY_NOTICE_SOURCE_DIR="$(dirname "$ONECONTEXT_CADDY_PATH")"
      return
    fi
    if [[ -f "$CADDY_TOOL_ARCHIVE" && -f "$CADDY_TOOL_SHA256" ]]; then
      release_caddy_source
      return
    fi
    local host_caddy
    host_caddy="$(type -P caddy 2>/dev/null || true)"
    if [[ -n "$host_caddy" ]]; then
      CADDY_SOURCE="$host_caddy"
      CADDY_NOTICE_SOURCE_DIR="$(dirname "$host_caddy")"
      return
    fi
    echo "Dev app build requires Caddy. Set ONECONTEXT_CADDY_PATH or add the release-owned Caddy artifact." >&2
    exit 1
  fi

  if [[ -n "${ONECONTEXT_CADDY_PATH:-}" ]]; then
    echo "Non-dev release channels must use the release-owned Caddy artifact, not ONECONTEXT_CADDY_PATH." >&2
    exit 1
  fi
  release_caddy_source
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
resolve_caddy_source
if [[ -z "$CADDY_SOURCE" || ! -x "$CADDY_SOURCE" ]]; then
  echo "App build requires an executable Caddy source: $CADDY_SOURCE" >&2
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
if [[ -n "$CADDY_NOTICE_SOURCE_DIR" ]]; then
  for notice in LICENSE AUTHORS README.md sbom.spdx.json; do
    if [[ -f "$CADDY_NOTICE_SOURCE_DIR/$notice" ]]; then
      cp "$CADDY_NOTICE_SOURCE_DIR/$notice" "$CADDY_BUNDLE_DIR/$notice"
    fi
  done
fi

rm -rf "$RUNTIME_DEFAULTS_WORK_DIR" "$RUNTIME_DEFAULTS_RESOURCE_DIR" "$WIKI_ENGINE_RESOURCE_DIR"
mkdir -p "$RUNTIME_DEFAULTS_WORK_DIR" "$RUNTIME_DEFAULTS_RESOURCE_DIR" "$WIKI_ENGINE_RESOURCE_DIR"
npm ci --omit=dev --ignore-scripts --prefix "$ROOT/wiki-engine" >/dev/null
rsync -a \
  --exclude '.gitkeep' \
  --exclude 'README.md' \
  "$ROOT/runtime/1Context/" \
  "$RUNTIME_DEFAULTS_WORK_DIR/1Context/"
python3 "$ROOT/wiki-engine/tools/materialize-wiki-pages.py" "$RUNTIME_DEFAULTS_WORK_DIR" >/dev/null
node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$RUNTIME_DEFAULTS_WORK_DIR/1Context/user-wiki/source" \
  --output "$RUNTIME_DEFAULTS_WORK_DIR/1Context/user-wiki/site" \
  --result-json "$RUNTIME_DEFAULTS_WORK_DIR/render-site-result.json" >/dev/null
python3 "$ROOT/wiki-engine/tools/write-runtime-defaults-manifest.py" \
  --runtime-defaults-root "$RUNTIME_DEFAULTS_WORK_DIR/1Context" \
  --wiki-engine-root "$ROOT/wiki-engine" \
  --render-result "$RUNTIME_DEFAULTS_WORK_DIR/render-site-result.json" \
  --version "$VERSION" \
  --output "$RUNTIME_DEFAULTS_WORK_DIR/1Context/.1context/runtime-defaults-manifest.json"
ditto "$RUNTIME_DEFAULTS_WORK_DIR/1Context" "$RUNTIME_DEFAULTS_RESOURCE_DIR/1Context"
rsync -a \
  --exclude 'package-lock.json' \
  --exclude 'node_modules/.bin' \
  --exclude 'node_modules/.package-lock.json' \
  --exclude 'node_modules/*/bin' \
  --exclude 'tools/materialize-wiki-pages.py' \
  --exclude 'tools/serve-site.mjs' \
  --exclude 'tools/write-runtime-defaults-manifest.py' \
  --exclude 'README.md' \
  "$ROOT/wiki-engine/" \
  "$WIKI_ENGINE_RESOURCE_DIR/"

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
if [[ -n "$SPARKLE_PUBLIC_ED_KEY" ]]; then
  if [[ -z "$SPARKLE_FEED_URL" ]]; then
    echo "release/release.toml must provide public_appcast_url to configure Sparkle." >&2
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
  <key>SUAutomaticallyUpdate</key>
  <true/>
  <key>SUAllowsAutomaticUpdates</key>
  <true/>
  <key>SUVerifyUpdateBeforeExtraction</key>
  <true/>
  <key>SUScheduledCheckInterval</key>
  <integer>3600</integer>
PLIST
)"
fi

plist_bool() {
  if [[ "$1" == "1" || "$1" == "true" || "$1" == "yes" ]]; then
    printf '<true/>'
  else
    printf '<false/>'
  fi
}

UPDATE_OPTIONAL_PROMPT_TITLE_ESCAPED="$(plist_escape "$UPDATE_OPTIONAL_PROMPT_TITLE")"
UPDATE_OPTIONAL_PROMPT_BODY_ESCAPED="$(plist_escape "$UPDATE_OPTIONAL_PROMPT_BODY")"
UPDATE_FAILURE_TITLE_ESCAPED="$(plist_escape "$UPDATE_FAILURE_TITLE")"
UPDATE_FAILURE_BODY_ESCAPED="$(plist_escape "$UPDATE_FAILURE_BODY")"
UPDATE_POST_INSTALL_TITLE_ESCAPED="$(plist_escape "$UPDATE_POST_INSTALL_TITLE")"
UPDATE_POST_INSTALL_BODY_ESCAPED="$(plist_escape "$UPDATE_POST_INSTALL_BODY")"
UPDATE_POST_INSTALL_MESSAGE_ENABLED_PLIST="$(plist_bool "$UPDATE_POST_INSTALL_MESSAGE_ENABLED")"
UPDATE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW_PLIST="$(plist_bool "$UPDATE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW")"
UPDATE_POLICY_PLIST_KEYS="$(cat <<PLIST
  <key>OneContextUpdateOptionalPromptTitle</key>
  <string>$UPDATE_OPTIONAL_PROMPT_TITLE_ESCAPED</string>
  <key>OneContextUpdateOptionalPromptBody</key>
  <string>$UPDATE_OPTIONAL_PROMPT_BODY_ESCAPED</string>
  <key>OneContextUpdateFailureTitle</key>
  <string>$UPDATE_FAILURE_TITLE_ESCAPED</string>
  <key>OneContextUpdateFailureBody</key>
  <string>$UPDATE_FAILURE_BODY_ESCAPED</string>
  <key>OneContextUpdatePostInstallMessageEnabled</key>
  $UPDATE_POST_INSTALL_MESSAGE_ENABLED_PLIST
  <key>OneContextUpdatePostInstallTitle</key>
  <string>$UPDATE_POST_INSTALL_TITLE_ESCAPED</string>
  <key>OneContextUpdatePostInstallBody</key>
  <string>$UPDATE_POST_INSTALL_BODY_ESCAPED</string>
  <key>OneContextUpdateShowReleaseNotesInUpdateWindow</key>
  $UPDATE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW_PLIST
PLIST
)"

cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>1Context</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_IDENTIFIER</string>
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
$UPDATE_POLICY_PLIST_KEYS
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

  if ! command -v codesign >/dev/null 2>&1 || ! codesign_identity_available; then
    echo "Developer ID identity not found: $IDENTITY" >&2
    if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
      echo "Checked keychain: $CODESIGN_KEYCHAIN" >&2
    fi
    exit 1
  fi

  codesign_release \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/Autoupdate" >/dev/null
  codesign_release \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/Updater.app" >/dev/null
  codesign_release \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/XPCServices/Downloader.xpc" >/dev/null
  codesign_release \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework/Versions/B/XPCServices/Installer.xpc" >/dev/null
  codesign_release \
    --sign "$IDENTITY" \
    "$FRAMEWORKS_DIR/Sparkle.framework" >/dev/null
  codesign_release \
    --sign "$IDENTITY" \
    "$CADDY_BUNDLE_DIR/caddy" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1context-cli" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1contextd" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$RESOURCES_DIR/1context-local-web-proxy" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1Context" >/dev/null
  codesign_release \
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

if [[ "$ONECONTEXT_RELEASE_CHANNEL" != "dev" ]]; then
  homebrew_path_report="$(mktemp /tmp/onecontext-homebrew-paths.XXXXXX)"
  if grep -R -a -n -E '/opt/homebrew|/usr/local/Cellar|/Cellar/caddy' "$APP_DIR" >"$homebrew_path_report"; then
    cat "$homebrew_path_report" >&2
    rm -f "$homebrew_path_report"
    echo "Non-dev app bundle contains Homebrew or host Caddy paths." >&2
    exit 1
  fi
  rm -f "$homebrew_path_report"
  "$ROOT/macos/tools/audit-app-dependencies.sh" "$APP_DIR"
fi

echo "$APP_DIR"
