#!/usr/bin/env bash
set -euo pipefail

DMG="${1:-}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

if [[ -z "$DMG" || ! -f "$DMG" ]]; then
  echo "Usage: $0 dist/1Context-VERSION-macos-ARCH.dmg" >&2
  exit 1
fi

dmg_name="$(basename "$DMG")"
if [[ ! "$dmg_name" =~ ^1Context-([0-9]+\.[0-9]+\.[0-9]+)-macos-(arm64)\.dmg$ ]]; then
  echo "DMG name does not match expected version/arch pattern." >&2
  exit 1
fi
VERSION="${BASH_REMATCH[1]}"
ARCH="${BASH_REMATCH[2]}"

EXPECTED_VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION" 2>/dev/null || true)}"
if [[ -n "$EXPECTED_VERSION" && "$EXPECTED_VERSION" != "$VERSION" ]]; then
  echo "DMG version does not match VERSION." >&2
  exit 1
fi

if [[ "${ALLOW_UNNOTARIZED:-0}" != "1" ]]; then
  if ! codesign --verify --strict "$DMG" >/dev/null 2>&1; then
    echo "DMG is not signed or has an invalid signature." >&2
    exit 1
  fi
  if ! xcrun stapler validate "$DMG" >/dev/null 2>&1; then
    echo "DMG does not have a valid stapled notarization ticket." >&2
    exit 1
  fi
  if ! spctl --assess --type open --context context:primary-signature --verbose "$DMG" >/dev/null 2>&1; then
    echo "Gatekeeper assessment failed for DMG." >&2
    exit 1
  fi
fi

TMPDIR="$(mktemp -d /tmp/1ctx-dmg-validate-XXXXXX)"
TMPDIR="$(cd "$TMPDIR" && pwd -P)"
MOUNT="$TMPDIR/mount"
mkdir -p "$MOUNT"

cleanup() {
  set +e
  if mount | grep -F " on $MOUNT " >/dev/null 2>&1; then
    for _ in 1 2 3 4 5; do
      hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 && break
      sleep 0.2
    done
  fi
  if mount | grep -F " on $MOUNT " >/dev/null 2>&1; then
    hdiutil detach "$MOUNT" -force -quiet >/dev/null 2>&1 || true
  fi
  if ! mount | grep -F " on $MOUNT " >/dev/null 2>&1; then
    rm -rf "$TMPDIR"
  else
    echo "Warning: leaving mounted DMG validation directory for inspection: $TMPDIR" >&2
  fi
}
trap cleanup EXIT

hdiutil attach "$DMG" \
  -mountpoint "$MOUNT" \
  -nobrowse \
  -readonly \
  -quiet

EXPECTED_APP_BASENAME="${ONECONTEXT_EXPECTED_APP_BASENAME:-1Context.app}"
APP="$MOUNT/$EXPECTED_APP_BASENAME"
if [[ ! -d "$APP" ]]; then
  echo "DMG does not contain $EXPECTED_APP_BASENAME." >&2
  exit 1
fi

if [[ "$(readlink "$MOUNT/Applications" 2>/dev/null || true)" != "/Applications" ]]; then
  echo "DMG does not contain an Applications symlink." >&2
  exit 1
fi

if find "$MOUNT" -maxdepth 1 -mindepth 1 \
  ! -name "$EXPECTED_APP_BASENAME" \
  ! -name "Applications" \
  ! -name ".background" \
  ! -name ".DS_Store" \
  | grep -q .; then
  echo "DMG contains unexpected top-level files." >&2
  find "$MOUNT" -maxdepth 1 -mindepth 1 -print >&2
  exit 1
fi

if [[ -n "${ONECONTEXT_BUNDLE_IDENTIFIER:-}" ]]; then
  if [[ "$(plutil -extract CFBundleIdentifier raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "$ONECONTEXT_BUNDLE_IDENTIFIER" ]]; then
    echo "DMG app Info.plist bundle identifier does not match ONECONTEXT_BUNDLE_IDENTIFIER." >&2
    exit 1
  fi
fi

if [[ "$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "$VERSION" ]]; then
  echo "DMG app Info.plist version does not match DMG version." >&2
  exit 1
fi
if [[ -n "${ONECONTEXT_SPARKLE_FEED_URL:-}" || -n "${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-}" ]]; then
  if [[ "$(plutil -extract SUFeedURL raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "${ONECONTEXT_SPARKLE_FEED_URL:-}" ]]; then
    echo "DMG app Info.plist Sparkle feed URL does not match ONECONTEXT_SPARKLE_FEED_URL." >&2
    exit 1
  fi
  if [[ "$(plutil -extract SUPublicEDKey raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-}" ]]; then
    echo "DMG app Info.plist Sparkle public key does not match ONECONTEXT_SPARKLE_PUBLIC_ED_KEY." >&2
    exit 1
  fi
  if [[ "$(plutil -extract SUEnableAutomaticChecks raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "true" ]]; then
    echo "DMG app Info.plist does not enable Sparkle automatic checks." >&2
    exit 1
  fi
  if [[ "$(plutil -extract SUAutomaticallyUpdate raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "true" ]]; then
    echo "DMG app Info.plist does not enable Sparkle automatic downloads and installs." >&2
    exit 1
  fi
  if [[ "$(plutil -extract SUAllowsAutomaticUpdates raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "true" ]]; then
    echo "DMG app Info.plist does not allow Sparkle automatic updates." >&2
    exit 1
  fi
  if [[ "$(plutil -extract SUScheduledCheckInterval raw "$APP/Contents/Info.plist" 2>/dev/null || true)" != "3600" ]]; then
    echo "DMG app Info.plist does not set the aggressive Sparkle check interval." >&2
    exit 1
  fi
fi

if [[ "$("$APP/Contents/MacOS/1context-cli" --version)" != "$VERSION" ]]; then
  echo "DMG CLI version does not match DMG version." >&2
  exit 1
fi

codesign --verify --deep --strict "$APP" >/dev/null
if [[ ! -d "$APP/Contents/Frameworks/Sparkle.framework" ]]; then
  echo "DMG app is missing Sparkle.framework." >&2
  exit 1
fi
codesign --verify --deep --strict "$APP/Contents/Frameworks/Sparkle.framework" >/dev/null
for sparkle_component in \
  "$APP/Contents/Frameworks/Sparkle.framework/Versions/B/Autoupdate" \
  "$APP/Contents/Frameworks/Sparkle.framework/Versions/B/Updater.app" \
  "$APP/Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices/Downloader.xpc" \
  "$APP/Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices/Installer.xpc"; do
  if [[ ! -e "$sparkle_component" ]]; then
    echo "DMG app is missing Sparkle update helper component: $sparkle_component" >&2
    exit 1
  fi
  codesign --verify --deep --strict "$sparkle_component" >/dev/null
done
if ! otool -L "$APP/Contents/MacOS/1Context" | grep -q '@rpath/Sparkle.framework/Versions/B/Sparkle'; then
  echo "DMG menu app is not linked to Sparkle.framework." >&2
  exit 1
fi
if ! otool -l "$APP/Contents/MacOS/1Context" | grep -q '@executable_path/../Frameworks'; then
  echo "DMG menu app is missing the Sparkle framework rpath." >&2
  exit 1
fi

if [[ "${ALLOW_UNNOTARIZED:-0}" != "1" ]]; then
  if ! xcrun stapler validate "$APP" >/dev/null 2>&1; then
    echo "DMG app does not have a valid stapled notarization ticket." >&2
    exit 1
  fi
  if ! spctl --assess --type execute --verbose "$APP" >/dev/null 2>&1; then
    echo "Gatekeeper assessment failed for app inside DMG." >&2
    exit 1
  fi
fi

for binary in \
  "$APP/Contents/MacOS/1Context" \
  "$APP/Contents/MacOS/1context-cli" \
  "$APP/Contents/Resources/1context-local-web-proxy" \
  "$APP/Contents/MacOS/1contextd" \
  "$APP/Contents/MacOS/onecontext-memoryd" \
  "$APP/Contents/MacOS/onecontext-capture-dashboard" \
  "$APP/Contents/MacOS/onecontext-agent-harness" \
  "$APP/Contents/MacOS/onecontext-codex-adapter" \
  "$APP/Contents/Resources/local-web/caddy/caddy"; do
  if [[ ! -x "$binary" ]]; then
    echo "Missing executable in DMG app: $binary" >&2
    exit 1
  fi

  if [[ "$(lipo -archs "$binary")" != "$ARCH" ]]; then
    echo "DMG binary does not match expected arch: $binary" >&2
    exit 1
  fi
done

echo "DMG validation passed."
