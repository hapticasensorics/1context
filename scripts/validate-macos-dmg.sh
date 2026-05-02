#!/usr/bin/env bash
set -euo pipefail

DMG="${1:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

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

if [[ -f "$ROOT/VERSION" ]] && [[ "$(tr -d '[:space:]' < "$ROOT/VERSION")" != "$VERSION" ]]; then
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
MOUNT="$TMPDIR/mount"
mkdir -p "$MOUNT"

cleanup() {
  hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

hdiutil attach "$DMG" \
  -mountpoint "$MOUNT" \
  -nobrowse \
  -readonly \
  -quiet

APP="$MOUNT/1Context.app"
if [[ ! -d "$APP" ]]; then
  echo "DMG does not contain 1Context.app." >&2
  exit 1
fi

if [[ "$(readlink "$MOUNT/Applications" 2>/dev/null || true)" != "/Applications" ]]; then
  echo "DMG does not contain an Applications symlink." >&2
  exit 1
fi

if find "$MOUNT" -maxdepth 1 -mindepth 1 \
  ! -name "1Context.app" \
  ! -name "Applications" \
  ! -name ".background" \
  ! -name ".DS_Store" \
  | grep -q .; then
  echo "DMG contains unexpected top-level files." >&2
  find "$MOUNT" -maxdepth 1 -mindepth 1 -print >&2
  exit 1
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
