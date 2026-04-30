#!/usr/bin/env bash
set -euo pipefail

ARCHIVE="${1:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

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
FULL_MANIFEST="$TMPDIR/full-manifest.txt"
MANIFEST="$TMPDIR/manifest.txt"
tar -tzf "$ARCHIVE" | LC_ALL=C sort > "$FULL_MANIFEST"

archive_name="$(basename "$ARCHIVE")"
if [[ ! "$archive_name" =~ ^1context-([0-9]+\.[0-9]+\.[0-9]+)-macos-(arm64)\.tar\.gz$ ]]; then
  echo "Release archive name does not match expected version/arch pattern." >&2
  exit 1
fi
VERSION="${BASH_REMATCH[1]}"
ARCH="${BASH_REMATCH[2]}"
ROOT_NAME="1context-$VERSION-macos-$ARCH"

if [[ -f "$ROOT/VERSION" ]] && [[ "$(tr -d '[:space:]' < "$ROOT/VERSION")" != "$VERSION" ]]; then
  echo "Release archive version does not match VERSION." >&2
  exit 1
fi

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
$ROOT_NAME/1Context.app/Contents/Resources/local-web/
$ROOT_NAME/1Context.app/Contents/Resources/local-web/caddy/
$ROOT_NAME/1Context.app/Contents/_CodeSignature/
$ROOT_NAME/1Context.app/Contents/_CodeSignature/CodeResources
$ROOT_NAME/bin/
$ROOT_NAME/bin/1context
$ROOT_NAME/scripts/
$ROOT_NAME/scripts/install-macos-launch-agents.sh
$ROOT_NAME/scripts/uninstall-macos-launch-agents.sh
EOF
grep -v "^$ROOT_NAME/1Context.app/Contents/Resources/memory-core\\(/\\|$\\)" "$FULL_MANIFEST" \
  | grep -v "^$ROOT_NAME/1Context.app/Contents/Resources/local-web/caddy/." \
  > "$MANIFEST"

if grep -Fxq "$ROOT_NAME/1Context.app/Contents/CodeResources" "$MANIFEST"; then
  echo "$ROOT_NAME/1Context.app/Contents/CodeResources" >> "$EXPECTED_MANIFEST"
fi
LC_ALL=C sort -o "$EXPECTED_MANIFEST" "$EXPECTED_MANIFEST"

if ! diff -u "$EXPECTED_MANIFEST" "$MANIFEST"; then
  echo "Release archive manifest does not match expected contents." >&2
  exit 1
fi

for required in \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/pyproject.toml" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/uv.lock" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/bin/1context-memory-core" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/src/onectx/memory_core_cli.py" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/src/onectx/wiki/render.py" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki-engine/package-lock.json" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki-engine/theme/js/enhance.js" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/10-for-you/10-for-you/source/for-you.md" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/10-for-you/20-your-context/generated/your-context.html" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/10-for-you/20-your-context/generated/your-context.talk.html" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/20-project/10-projects/generated/projects.html" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/20-project/10-projects/generated/projects.talk.html" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/30-topics/10-topics/generated/topics.html" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/30-topics/10-topics/generated/topics.talk.html"; do
  if ! grep -Fxq "$required" "$FULL_MANIFEST"; then
    echo "Release archive missing bundled memory-core file: $required" >&2
    exit 1
  fi
done

for required_pattern in \
  "^$ROOT_NAME/1Context\\.app/Contents/Resources/memory-core/wiki/menu/10-for-you/10-for-you/generated/for-you-[0-9]{4}-[0-9]{2}-[0-9]{2}\\.html$" \
  "^$ROOT_NAME/1Context\\.app/Contents/Resources/memory-core/wiki/menu/10-for-you/10-for-you/generated/for-you-[0-9]{4}-[0-9]{2}-[0-9]{2}\\.talk\\.html$"; do
  if ! grep -Eq "$required_pattern" "$FULL_MANIFEST"; then
    echo "Release archive missing bundled For You generated page matching: $required_pattern" >&2
    exit 1
  fi
done
for_you_talk_page="$(grep -E "^$ROOT_NAME/1Context\\.app/Contents/Resources/memory-core/wiki/menu/10-for-you/10-for-you/generated/for-you-[0-9]{4}-[0-9]{2}-[0-9]{2}\\.talk\\.html$" "$FULL_MANIFEST" | head -n 1)"

for required in \
  "$ROOT_NAME/1Context.app/Contents/Resources/local-web/caddy/caddy" \
  "$ROOT_NAME/1Context.app/Contents/Resources/local-web/caddy/caddy.version" \
  "$ROOT_NAME/1Context.app/Contents/Resources/local-web/caddy/THIRD_PARTY_NOTICES.txt"; do
  if ! grep -Fxq "$required" "$FULL_MANIFEST"; then
    echo "Release archive missing bundled Caddy file: $required" >&2
    exit 1
  fi
done

if grep -Fxq "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/src/onectx/wiki/server.py" "$FULL_MANIFEST" \
  || grep -Fxq "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/src/onectx/wiki/serve_main.py" "$FULL_MANIFEST"; then
  echo "Release archive still contains the deleted Python wiki server." >&2
  exit 1
fi

if grep -Eq "^$ROOT_NAME/1Context\\.app/Contents/Resources/memory-core/memory/runtime(/|$)" "$FULL_MANIFEST" \
  || grep -Eq "^$ROOT_NAME/1Context\\.app/Contents/Resources/memory-core/storage/lakestore/.*\\.lance(/|$)" "$FULL_MANIFEST"; then
  echo "Release archive contains local memory runtime or lakestore data." >&2
  exit 1
fi

for talk_page in \
  "$for_you_talk_page" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/10-for-you/20-your-context/generated/your-context.talk.html" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/20-project/10-projects/generated/projects.talk.html" \
  "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/30-topics/10-topics/generated/topics.talk.html"; do
  talk_copy="$TMPDIR/$(basename "$talk_page")"
  tar -xOz -f "$ARCHIVE" "$talk_page" > "$talk_copy"
  if ! grep -q 'class="opctx-tier-badge" data-tier="private" title="Only you">Private</span>' "$talk_copy"; then
    echo "Release archive has a talk page without private local chrome: $talk_page" >&2
    exit 1
  fi
done

FOR_YOU_SOURCE_COPY="$TMPDIR/for-you-source.md"
tar -xOz -f "$ARCHIVE" "$ROOT_NAME/1Context.app/Contents/Resources/memory-core/wiki/menu/10-for-you/10-for-you/source/for-you.md" > "$FOR_YOU_SOURCE_COPY"
if ! grep -q "How This Page Works" "$FOR_YOU_SOURCE_COPY"; then
  echo "Release archive is missing the visible For You template copy." >&2
  exit 1
fi

if grep -Eq "stub|empty: populated|<!-- empty" "$FOR_YOU_SOURCE_COPY"; then
  echo "Release archive still exposes raw For You stubs." >&2
  exit 1
fi

if [[ -z "$(tar -xOzf "$ARCHIVE" "$ROOT_NAME/1Context.app/Contents/Resources/local-web/caddy/caddy.version" | tr -d '[:space:]')" ]]; then
  echo "Bundled Caddy version file is empty." >&2
  exit 1
fi

if ! awk '{ if ($3 != "root" || $4 != "wheel") exit 1 }' "$LISTING"; then
  echo "Release archive contains unexpected owner/group metadata." >&2
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

for binary in "$APP/Contents/MacOS/1Context" "$APP/Contents/MacOS/1context-cli" "$APP/Contents/MacOS/1contextd" "$APP/Contents/Resources/local-web/caddy/caddy"; do
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

  if [[ "$binary" != "$APP/Contents/Resources/local-web/caddy/caddy" ]] \
    && strings "$binary" | grep -Eq '/Users/|/private/var/folders|/var/folders|/\.build/|OneContextMac_.*\.bundle|could not load resource bundle'; then
    echo "Release binary contains local build paths or SwiftPM resource-bundle fallback text: $binary" >&2
    exit 1
  fi
done

if find "$TMPDIR" \( -name '._*' -o -name '.DS_Store' \) | grep -q .; then
  echo "Release archive extracted macOS metadata files." >&2
  exit 1
fi

echo "Release artifact validation passed."
