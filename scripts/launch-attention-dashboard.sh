#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SESSION_PATH="${1:-$ROOT_DIR/target/attention-bundle-shots/bundle_shot_20260525T115427Z/attention-dashboard-session.json}"
APP_PATH="$ROOT_DIR/target/1Context Attention Dashboard.app"
BIN_PATH="$ROOT_DIR/target/debug/onecontext-attention-dashboard"

cargo build -p onecontext-attention-dashboard --manifest-path "$ROOT_DIR/Cargo.toml"

mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources"

if [[ ! -f "$APP_PATH/Contents/Info.plist" ]]; then
  cat > "$APP_PATH/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>1Context Attention Dashboard</string>
  <key>CFBundleExecutable</key>
  <string>onecontext-attention-dashboard</string>
  <key>CFBundleIdentifier</key>
  <string>com.haptica.1context.attention-dashboard.dev</string>
  <key>CFBundleName</key>
  <string>1Context Attention Dashboard</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST
fi

cp "$BIN_PATH" "$APP_PATH/Contents/MacOS/onecontext-attention-dashboard"
chmod +x "$APP_PATH/Contents/MacOS/onecontext-attention-dashboard"
/usr/bin/codesign --force --deep --sign - "$APP_PATH"
/usr/bin/codesign --verify --deep --strict "$APP_PATH"

open -n "$APP_PATH" --args --session "$SESSION_PATH"
