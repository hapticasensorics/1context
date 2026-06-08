#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
normalize_permission_test_id() {
  printf '%s' "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//'
}

if [[ -n "${ONECONTEXT_RELEASE_CHANNEL:-}" ]]; then
  eval "$("$ROOT/scripts/release-train.sh" manifest export-env --channel "$ONECONTEXT_RELEASE_CHANNEL")"
else
  eval "$("$ROOT/scripts/release-train.sh" manifest export-env)"
fi
MACOS_DIR="$ROOT/macos"
APP_IDENTITY="${ONECONTEXT_APP_IDENTITY:-}"
if [[ -z "$APP_IDENTITY" ]]; then
  if [[ "${ONECONTEXT_RELEASE_CHANNEL:-}" == "dev" ]]; then
    APP_IDENTITY="dev"
  else
    APP_IDENTITY="official"
  fi
fi
APP_IDENTITY_PLIST_VALUE="$APP_IDENTITY"
case "$APP_IDENTITY" in
  official)
    DEFAULT_APP_BUNDLE_NAME="1Context"
    DEFAULT_APP_DISPLAY_NAME="1Context"
    DEFAULT_BUNDLE_IDENTIFIER="com.haptica.1context"
    DEFAULT_PROXY_LABEL="com.haptica.1context.local-web-proxy"
    ;;
  dev)
    DEFAULT_APP_BUNDLE_NAME="1Context Dev"
    DEFAULT_APP_DISPLAY_NAME="1Context Dev"
    DEFAULT_BUNDLE_IDENTIFIER="com.haptica.1context.dev"
    DEFAULT_PROXY_LABEL="com.haptica.1context.dev.local-web-proxy"
    ;;
  *)
    echo "Unknown ONECONTEXT_APP_IDENTITY: $APP_IDENTITY" >&2
    exit 1
    ;;
esac
APP_BUNDLE_NAME="${ONECONTEXT_APP_BUNDLE_NAME:-$DEFAULT_APP_BUNDLE_NAME}"
APP_DISPLAY_NAME="${ONECONTEXT_APP_DISPLAY_NAME:-$DEFAULT_APP_DISPLAY_NAME}"
APP_DIR="$ROOT/dist/$APP_BUNDLE_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_APP_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
FRAMEWORKS_DIR="$CONTENTS_DIR/Frameworks"
LAUNCH_DAEMONS_DIR="$CONTENTS_DIR/Library/LaunchDaemons"
SIGNING_MODE="${ONECONTEXT_SIGNING_MODE:-${ONECONTEXT_RELEASE_CHANNEL_SIGNING_MODE:-adhoc}}"
IDENTITY="${CODESIGN_IDENTITY:-}"
CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-${ONECONTEXT_RELEASE_KEYCHAIN:-}}"
VERSION="$ONECONTEXT_RELEASE_VERSION"
ARCH="${ONECONTEXT_ARCH:-arm64}"
BUNDLE_IDENTIFIER="${ONECONTEXT_BUNDLE_IDENTIFIER:-$DEFAULT_BUNDLE_IDENTIFIER}"
LOCAL_WEB_PROXY_LABEL="${ONECONTEXT_LOCAL_WEB_PROXY_LABEL:-$DEFAULT_PROXY_LABEL}"
if [[ -n "${ONECONTEXT_PERMISSION_TEST_ID:-}" ]]; then
  if [[ "$APP_IDENTITY" != "dev" ]]; then
    echo "ONECONTEXT_PERMISSION_TEST_ID is only supported for the dev app identity." >&2
    exit 1
  fi
  if [[ "$SIGNING_MODE" != "apple-development" && "$SIGNING_MODE" != "developer-id" ]]; then
    echo "ONECONTEXT_PERMISSION_TEST_ID requires Apple Development or Developer ID signing so TCC identity can be tested honestly." >&2
    exit 1
  fi
  PERMISSION_TEST_ID="$(normalize_permission_test_id "$ONECONTEXT_PERMISSION_TEST_ID")"
  if [[ -z "$PERMISSION_TEST_ID" || ${#PERMISSION_TEST_ID} -gt 40 ]]; then
    echo "ONECONTEXT_PERMISSION_TEST_ID must normalize to 1-40 lowercase letters, digits, or hyphens." >&2
    exit 1
  fi
  APP_BUNDLE_NAME="1Context Dev - $PERMISSION_TEST_ID"
  APP_DISPLAY_NAME="1Context Dev - $PERMISSION_TEST_ID"
  APP_DIR="$ROOT/dist/$APP_BUNDLE_NAME.app"
  CONTENTS_DIR="$APP_DIR/Contents"
  MACOS_APP_DIR="$CONTENTS_DIR/MacOS"
  RESOURCES_DIR="$CONTENTS_DIR/Resources"
  FRAMEWORKS_DIR="$CONTENTS_DIR/Frameworks"
  LAUNCH_DAEMONS_DIR="$CONTENTS_DIR/Library/LaunchDaemons"
  BUNDLE_IDENTIFIER="com.haptica.1context.dev.permission.$PERMISSION_TEST_ID"
  LOCAL_WEB_PROXY_LABEL="com.haptica.1context.dev.permission.$PERMISSION_TEST_ID.local-web-proxy"
  APP_IDENTITY_PLIST_VALUE="dev-permission:$PERMISSION_TEST_ID"
fi
LOCAL_WEB_PROXY_PLIST_NAME="$LOCAL_WEB_PROXY_LABEL.plist"
MENU_ICON_SOURCE="$MACOS_DIR/Sources/OneContextMenuBar/Resources/MenuBarIcon.png"
CADDY_VERSION="2.11.2"
CADDY_TOOL_ARCHIVE_NAME="caddy-v$CADDY_VERSION-darwin-$ARCH.tar.gz"
CADDY_TOOL_PIN_DIR="$ROOT/release/tools/caddy/darwin-$ARCH"
CADDY_TOOL_SHA256="$CADDY_TOOL_PIN_DIR/$CADDY_TOOL_ARCHIVE_NAME.sha256"
CADDY_TOOL_ARCHIVE="$ROOT/dist/release-tools/caddy/downloads/darwin-$ARCH/$CADDY_TOOL_ARCHIVE_NAME"
CADDY_TOOL_URL="https://github.com/caddyserver/caddy/releases/download/v$CADDY_VERSION/$CADDY_TOOL_ARCHIVE_NAME"
CADDY_TOOL_WORK_DIR="$ROOT/dist/release-tools/caddy/darwin-$ARCH"
CADDY_SOURCE=""
CADDY_NOTICE_SOURCE_DIR=""
RUNTIME_DEFAULTS_WORK_DIR="$ROOT/dist/runtime-defaults"
RUNTIME_DEFAULTS_RESOURCE_DIR="$RESOURCES_DIR/RuntimeDefaults"
WIKI_ENGINE_RESOURCE_DIR="$RESOURCES_DIR/WikiEngine"
MANAGED_POSTGRES_SOURCE_DIR="${ONECONTEXT_MANAGED_POSTGRES_SOURCE_DIR:-$ROOT/release/managed-postgres/runtime/macos-arm64}"
MANAGED_POSTGRES_RESOURCE_DIR="$RESOURCES_DIR/managed-postgres/macos-arm64"
INCLUDE_MANAGED_POSTGRES="${ONECONTEXT_INCLUDE_MANAGED_POSTGRES:-true}"
if [[ "$INCLUDE_MANAGED_POSTGRES" == "auto" ]]; then
  INCLUDE_MANAGED_POSTGRES="true"
fi
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

default_codesign_identity() {
  local pattern="$1"
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    security find-identity -v -p codesigning "$CODESIGN_KEYCHAIN"
  else
    security find-identity -v -p codesigning
  fi | awk -F'"' -v pattern="$pattern" '$2 ~ pattern { print $2; exit }'
}

codesign_release() {
  local args=(--force --options runtime)
  if [[ "$SIGNING_MODE" == "developer-id" ]]; then
    args+=(--timestamp)
  fi
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    args+=(--keychain "$CODESIGN_KEYCHAIN")
  fi
  codesign "${args[@]}" "$@"
}

codesign_adhoc_runtime() {
  local entitlements_arg=()
  if [[ "${1:-}" == "--entitlements" ]]; then
    entitlements_arg=(--entitlements "$2")
    shift 2
  fi
  codesign --force --options runtime "${entitlements_arg[@]}" --sign - "$@"
}

validate_permission_codesign() {
  local app="$1"
  local entitlements
  codesign --verify --strict "$app" >/dev/null
  entitlements="$(codesign -d --entitlements :- "$app" 2>/dev/null || true)"
  if ! grep -q "com.apple.security.device.audio-input" <<<"$entitlements"; then
    echo "Signed app is missing com.apple.security.device.audio-input entitlement." >&2
    exit 1
  fi
  if ! grep -q "com.apple.security.automation.apple-events" <<<"$entitlements"; then
    echo "Signed app is missing com.apple.security.automation.apple-events entitlement." >&2
    exit 1
  fi
  if [[ -z "$(/usr/libexec/PlistBuddy -c 'Print :NSAppleEventsUsageDescription' "$CONTENTS_DIR/Info.plist" 2>/dev/null || true)" ]]; then
    echo "Info.plist is missing NSAppleEventsUsageDescription." >&2
    exit 1
  fi
  if [[ -z "$(/usr/libexec/PlistBuddy -c 'Print :NSAudioCaptureUsageDescription' "$CONTENTS_DIR/Info.plist" 2>/dev/null || true)" ]]; then
    echo "Info.plist is missing NSAudioCaptureUsageDescription." >&2
    exit 1
  fi
  if [[ -z "$(/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$CONTENTS_DIR/Info.plist" 2>/dev/null || true)" ]]; then
    echo "Info.plist is missing NSMicrophoneUsageDescription." >&2
    exit 1
  fi
}

managed_postgres_sbom_required() {
  [[ "$INCLUDE_MANAGED_POSTGRES" != "auto" ]]
}

verify_managed_postgres_bundle() {
  local target="$1"
  local verify_script="$ROOT/scripts/verify-managed-postgres-bundle.sh"
  local verify_args=()
  if managed_postgres_sbom_required; then
    verify_args+=(--require-sbom)
  fi
  "$verify_script" "${verify_args[@]}" "$target"
}

is_mach_o_file() {
  file -b "$1" 2>/dev/null | grep -q 'Mach-O'
}

sign_managed_postgres_release() {
  [[ -d "$MANAGED_POSTGRES_RESOURCE_DIR" ]] || return 0
  while IFS= read -r mach_o; do
    is_mach_o_file "$mach_o" || continue
    codesign_release --sign "$IDENTITY" "$mach_o" >/dev/null
  done < <(find "$MANAGED_POSTGRES_RESOURCE_DIR/bin" "$MANAGED_POSTGRES_RESOURCE_DIR/lib" -type f -print 2>/dev/null)
  verify_managed_postgres_bundle "$MANAGED_POSTGRES_RESOURCE_DIR" >/dev/null
}

sign_managed_postgres_adhoc() {
  [[ -d "$MANAGED_POSTGRES_RESOURCE_DIR" ]] || return 0
  while IFS= read -r mach_o; do
    is_mach_o_file "$mach_o" || continue
    codesign_adhoc_runtime "$mach_o" >/dev/null
  done < <(find "$MANAGED_POSTGRES_RESOURCE_DIR/bin" "$MANAGED_POSTGRES_RESOURCE_DIR/lib" -type f -print 2>/dev/null)
  verify_managed_postgres_bundle "$MANAGED_POSTGRES_RESOURCE_DIR" >/dev/null
}

write_permission_identity_evidence() {
  local app="$1"
  local evidence_dir="$ROOT/dist/permission-build-evidence"
  local safe_name
  safe_name="$(basename "$app" .app | tr '[:upper:] ' '[:lower:]-' | sed -E 's/[^a-z0-9.-]+/-/g')"
  mkdir -p "$evidence_dir"
  {
    echo "app_path=$app"
    echo "bundle_identifier=$BUNDLE_IDENTIFIER"
    echo "app_display_name=$APP_DISPLAY_NAME"
    echo "app_identity=$APP_IDENTITY"
    echo "signing_mode=$SIGNING_MODE"
    echo "version=$VERSION"
    echo
    echo "[designated_requirement]"
    codesign -dr - "$app"
    echo
    echo "[entitlements]"
    codesign -d --entitlements :- "$app"
    echo
    echo "[usage_strings]"
    /usr/libexec/PlistBuddy -c 'Print :NSAppleEventsUsageDescription' "$CONTENTS_DIR/Info.plist"
    /usr/libexec/PlistBuddy -c 'Print :NSAudioCaptureUsageDescription' "$CONTENTS_DIR/Info.plist"
    /usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$CONTENTS_DIR/Info.plist"
  } >"$evidence_dir/$safe_name.txt" 2>&1
}

if [[ -z "$IDENTITY" && "$SIGNING_MODE" == "apple-development" ]]; then
  IDENTITY="$(default_codesign_identity "Apple Development:")"
elif [[ -z "$IDENTITY" && "$SIGNING_MODE" == "developer-id" ]]; then
  IDENTITY="$(default_codesign_identity "Developer ID Application:")"
fi

release_caddy_source() {
  if [[ ! -f "$CADDY_TOOL_SHA256" ]]; then
    echo "Pinned Caddy checksum is missing: $CADDY_TOOL_SHA256" >&2
    exit 1
  fi

  if [[ ! -f "$CADDY_TOOL_ARCHIVE" ]]; then
    mkdir -p "$(dirname "$CADDY_TOOL_ARCHIVE")"
    curl -fsSL "$CADDY_TOOL_URL" -o "$CADDY_TOOL_ARCHIVE"
  fi

  local expected_sha
  local actual_sha
  expected_sha="$(awk '{print $1}' "$CADDY_TOOL_SHA256")"
  actual_sha="$(shasum -a 256 "$CADDY_TOOL_ARCHIVE" | awk '{print $1}')"
  if [[ -z "$expected_sha" || "$actual_sha" != "$expected_sha" ]]; then
    echo "Pinned Caddy artifact checksum mismatch." >&2
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
    if [[ -f "$CADDY_TOOL_SHA256" ]]; then
      release_caddy_source
      return
    fi
    echo "Dev app build requires Caddy. Set ONECONTEXT_CADDY_PATH, install caddy on PATH, or allow the pinned Caddy download." >&2
    exit 1
  fi

  if [[ -n "${ONECONTEXT_CADDY_PATH:-}" ]]; then
    echo "Non-dev release channels must use the release-owned Caddy artifact, not ONECONTEXT_CADDY_PATH." >&2
    exit 1
  fi
  release_caddy_source
}

package_managed_postgres() {
  local source_dir="$MANAGED_POSTGRES_SOURCE_DIR"
  local mode="$INCLUDE_MANAGED_POSTGRES"

  case "$mode" in
    1|true|yes|auto) ;;
    0|false|no)
      if [[ "$ONECONTEXT_RELEASE_CHANNEL" == "dev" && "${ONECONTEXT_ALLOW_UNBUNDLED_STORAGE_TESTS:-}" == "1" ]]; then
        echo "Skipping managed Postgres only because ONECONTEXT_ALLOW_UNBUNDLED_STORAGE_TESTS=1 is set for a dev build." >&2
        return
      fi
      echo "App builds must include Perception DB Ultra Max managed Postgres; ONECONTEXT_INCLUDE_MANAGED_POSTGRES=$mode is not allowed." >&2
      exit 1
      ;;
    *)
      echo "Unsupported ONECONTEXT_INCLUDE_MANAGED_POSTGRES value: $mode" >&2
      exit 1
      ;;
  esac

  if [[ ! -f "$source_dir/manifest.json" ]]; then
    if [[ "$mode" == "auto" ]]; then
      return
    fi
    echo "Managed Postgres packaging was requested but no staged bundle exists at $source_dir." >&2
    exit 1
  fi

  if ! verify_managed_postgres_bundle "$source_dir" >/dev/null; then
    if [[ "$mode" == "auto" ]]; then
      echo "Skipping invalid staged managed Postgres bundle at $source_dir." >&2
      return
    fi
    echo "Managed Postgres packaging was requested but the staged bundle is invalid." >&2
    exit 1
  fi

  mkdir -p "$(dirname "$MANAGED_POSTGRES_RESOURCE_DIR")"
  ditto "$source_dir" "$MANAGED_POSTGRES_RESOURCE_DIR"
  verify_managed_postgres_bundle "$MANAGED_POSTGRES_RESOURCE_DIR" >/dev/null
}

swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" -c release --arch "$ARCH" --show-bin-path)"
cargo build --release --package onecontext-wiki-daemon
cargo build --release --package onecontext-memory-db --bin onecontext-memoryd
cargo build --release --package onecontext-capture-dashboard --bin onecontext-capture-dashboard
cargo build --release --package onecontext-agent-harness-daemon --bin onecontext-agent-harness
cargo build --release --package onecontext-codex-adapter --bin onecontext-codex-adapter
cargo build --release --package onecontext-context-engine --bin onecontext-context-engine
WIKI_CORE_BIN="$ROOT/target/release/onecontext-wiki"
MEMORYD_BIN="$ROOT/target/release/onecontext-memoryd"
CAPTURE_DASHBOARD_BIN="$ROOT/target/release/onecontext-capture-dashboard"
AGENT_HARNESS_BIN="$ROOT/target/release/onecontext-agent-harness"
CODEX_ADAPTER_BIN="$ROOT/target/release/onecontext-codex-adapter"
CONTEXT_ENGINE_BIN="$ROOT/target/release/onecontext-context-engine"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_APP_DIR" "$RESOURCES_DIR" "$FRAMEWORKS_DIR" "$LAUNCH_DAEMONS_DIR"

cp "$BIN_DIR/OneContextMenuBar" "$MACOS_APP_DIR/1Context"
cp "$BIN_DIR/1context" "$MACOS_APP_DIR/1context-cli"
cp "$BIN_DIR/1contextd" "$MACOS_APP_DIR/1contextd"
cp "$WIKI_CORE_BIN" "$MACOS_APP_DIR/onecontext-wiki"
cp "$MEMORYD_BIN" "$MACOS_APP_DIR/onecontext-memoryd"
cp "$CAPTURE_DASHBOARD_BIN" "$MACOS_APP_DIR/onecontext-capture-dashboard"
cp "$AGENT_HARNESS_BIN" "$MACOS_APP_DIR/onecontext-agent-harness"
cp "$CODEX_ADAPTER_BIN" "$MACOS_APP_DIR/onecontext-codex-adapter"
cp "$CONTEXT_ENGINE_BIN" "$MACOS_APP_DIR/onecontext-context-engine"
cp "$BIN_DIR/1context-local-web-proxy" "$RESOURCES_DIR/1context-local-web-proxy"
cp "$MENU_ICON_SOURCE" "$RESOURCES_DIR/MenuBarIcon.png"
if [[ ! -d "$BIN_DIR/Sparkle.framework" ]]; then
  echo "SwiftPM did not build Sparkle.framework beside OneContextMenuBar." >&2
  exit 1
fi
ditto "$BIN_DIR/Sparkle.framework" "$FRAMEWORKS_DIR/Sparkle.framework"
package_managed_postgres
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
"$WIKI_CORE_BIN" --root "$RUNTIME_DEFAULTS_WORK_DIR/1Context" page-create-all >/dev/null
rm -rf "$RUNTIME_DEFAULTS_WORK_DIR/1Context/context-engine/runs"
mkdir -p "$RUNTIME_DEFAULTS_WORK_DIR/1Context/context-engine/runs"
"$WIKI_CORE_BIN" --root "$RUNTIME_DEFAULTS_WORK_DIR/1Context" publish \
  --wiki-engine "$ROOT/wiki-engine" \
  --node node \
  --trigger runtime-defaults \
  --force >/dev/null
cp "$RUNTIME_DEFAULTS_WORK_DIR/1Context/context-engine/runs/wiki-publish-result.json" \
  "$RUNTIME_DEFAULTS_WORK_DIR/render-site-result.json"
rm -rf "$RUNTIME_DEFAULTS_WORK_DIR/1Context/context-engine/runs"
mkdir -p "$RUNTIME_DEFAULTS_WORK_DIR/1Context/context-engine/runs"
ditto "$RUNTIME_DEFAULTS_WORK_DIR/1Context" "$RUNTIME_DEFAULTS_RESOURCE_DIR/1Context"
rsync -a \
  --exclude 'package-lock.json' \
  --exclude '__pycache__' \
  --exclude '*.pyc' \
  --exclude 'node_modules/.bin' \
  --exclude 'node_modules/.package-lock.json' \
  --exclude 'node_modules/*/bin' \
  --exclude 'tools/serve-site.mjs' \
  --exclude 'tools/write-runtime-defaults-manifest.py' \
  --exclude 'README.md' \
  "$ROOT/wiki-engine/" \
  "$WIKI_ENGINE_RESOURCE_DIR/"

MANIFEST_WRITTEN=0
write_runtime_defaults_manifest() {
  local git_commit
  git_commit="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || printf 'unknown')"

  local args=(
    --runtime-defaults-root "$RUNTIME_DEFAULTS_RESOURCE_DIR/1Context"
    --wiki-engine-root "$WIKI_ENGINE_RESOURCE_DIR"
    --wiki-core-bin "$MACOS_APP_DIR/onecontext-wiki"
    --manifest-writer "$ROOT/wiki-engine/tools/write-runtime-defaults-manifest.py"
    --manifest-writer-display-path "wiki-engine/tools/write-runtime-defaults-manifest.py"
    --render-result "$RUNTIME_DEFAULTS_WORK_DIR/render-site-result.json"
    --version "$VERSION"
    --git-commit "$git_commit"
  )
  if ! git -C "$ROOT" diff --quiet --ignore-submodules -- 2>/dev/null \
    || ! git -C "$ROOT" diff --cached --quiet --ignore-submodules -- 2>/dev/null; then
    args+=(--git-dirty)
  fi
  args+=(
    --output "$RUNTIME_DEFAULTS_RESOURCE_DIR/1Context/.1context/runtime-defaults-manifest.json"
  )
  python3 "$ROOT/wiki-engine/tools/write-runtime-defaults-manifest.py" "${args[@]}"
  MANIFEST_WRITTEN=1
}

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
  <string>$(plist_escape "$APP_DISPLAY_NAME")</string>
  <key>CFBundleDisplayName</key>
  <string>$(plist_escape "$APP_DISPLAY_NAME")</string>
  <key>OneContextAppIdentity</key>
  <string>$APP_IDENTITY_PLIST_VALUE</string>
  <key>OneContextBrowserExtensionID</key>
  <string>ijkabgddnhgkapedaloabgpcmpdhdhpb</string>
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
  <key>NSAppleEventsUsageDescription</key>
  <string>1Context uses Automation to gather high-signal app context when you enable app-specific remembering lanes.</string>
  <key>NSAudioCaptureUsageDescription</key>
  <string>1Context uses system audio capture to remember audio context from active desktop work when audio remembering is enabled.</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>1Context uses the microphone to remember spoken context when audio remembering is enabled.</string>
$UPDATE_POLICY_PLIST_KEYS
$SPARKLE_PLIST_KEYS
</dict>
</plist>
PLIST

cat > "$LAUNCH_DAEMONS_DIR/$LOCAL_WEB_PROXY_PLIST_NAME" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$(plist_escape "$LOCAL_WEB_PROXY_LABEL")</string>
  <key>BundleProgram</key>
  <string>Contents/Resources/1context-local-web-proxy</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
PLIST

if [[ "$SIGNING_MODE" == "developer-id" || "$SIGNING_MODE" == "apple-development" ]]; then
  if [[ -z "$IDENTITY" ]]; then
    if [[ "$SIGNING_MODE" == "apple-development" ]]; then
      echo "Set CODESIGN_IDENTITY to the Apple Development identity for dev signing." >&2
    else
      echo "Set CODESIGN_IDENTITY to the Developer ID Application identity for release signing." >&2
    fi
    exit 1
  fi

  if ! command -v codesign >/dev/null 2>&1 || ! codesign_identity_available; then
    echo "Code signing identity not found: $IDENTITY" >&2
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
    --identifier "$BUNDLE_IDENTIFIER.caddy" \
    --sign "$IDENTITY" \
    "$CADDY_BUNDLE_DIR/caddy" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.cli" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1context-cli" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.daemon" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1contextd" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.wiki" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/onecontext-wiki" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.memoryd" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/onecontext-memoryd" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.capture-dashboard" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/onecontext-capture-dashboard" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.agent-harness" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/onecontext-agent-harness" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.codex-adapter" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/onecontext-codex-adapter" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.context-engine" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/onecontext-context-engine" >/dev/null
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --identifier "$BUNDLE_IDENTIFIER.local-web-proxy" \
    --sign "$IDENTITY" \
    "$RESOURCES_DIR/1context-local-web-proxy" >/dev/null
  sign_managed_postgres_release
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$MACOS_APP_DIR/1Context" >/dev/null
  write_runtime_defaults_manifest
  codesign_release \
    --entitlements "$MACOS_DIR/entitlements.plist" \
    --sign "$IDENTITY" \
    "$APP_DIR" >/dev/null
elif command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$FRAMEWORKS_DIR/Sparkle.framework" >/dev/null
  codesign_adhoc_runtime "$CADDY_BUNDLE_DIR/caddy" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/1context-cli" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/1contextd" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/onecontext-wiki" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/onecontext-memoryd" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/onecontext-capture-dashboard" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/onecontext-agent-harness" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/onecontext-codex-adapter" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/onecontext-context-engine" >/dev/null
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$RESOURCES_DIR/1context-local-web-proxy" >/dev/null
  sign_managed_postgres_adhoc
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$MACOS_APP_DIR/1Context" >/dev/null
  write_runtime_defaults_manifest
  codesign_adhoc_runtime --entitlements "$MACOS_DIR/entitlements.plist" "$APP_DIR" >/dev/null
fi
if [[ "$MANIFEST_WRITTEN" != "1" ]]; then
  write_runtime_defaults_manifest
fi
if command -v codesign >/dev/null 2>&1; then
  validate_permission_codesign "$APP_DIR"
  write_permission_identity_evidence "$APP_DIR"
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
