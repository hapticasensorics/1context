#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${TMPDIR:-/tmp}/1context-uninstall-command-test-$$"
FIXTURE_HOME="$OUT_DIR/home"
APP="$OUT_DIR/Applications/1Context.app"
TEMP_COMMAND="${TMPDIR:-/tmp}/1context-uninstall-smoke-$$.command"

cleanup() {
  rm -rf "$OUT_DIR"
  rm -f "$TEMP_COMMAND"
}
trap cleanup EXIT

mkdir -p "$OUT_DIR"

if [[ -z "${BIN_DIR:-}" ]]; then
  swift build --package-path "$ROOT/macos" >/dev/null
  BIN_DIR="$(swift build --package-path "$ROOT/macos" --show-bin-path)"
fi
CLI="$BIN_DIR/1context"

write_fake_app() {
  rm -rf "$APP"
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Library/LaunchDaemons"
  cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.haptica.1context</string>
  <key>CFBundleShortVersionString</key>
  <string>9.9.9</string>
</dict>
</plist>
PLIST
  printf '#!/usr/bin/env bash\nexit 0\n' > "$APP/Contents/MacOS/1Context"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$APP/Contents/MacOS/1context-cli"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$APP/Contents/Resources/1context-local-web-proxy"
  chmod +x "$APP/Contents/MacOS/1Context" \
    "$APP/Contents/MacOS/1context-cli" \
    "$APP/Contents/Resources/1context-local-web-proxy"
  cat > "$APP/Contents/Library/LaunchDaemons/com.haptica.1context.local-web-proxy.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.haptica.1context.local-web-proxy</string>
</dict>
</plist>
PLIST
}

seed_fixture_state() {
  rm -rf "$FIXTURE_HOME" "$OUT_DIR/system"
  mkdir -p \
    "$FIXTURE_HOME/1Context" \
    "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup" \
    "$FIXTURE_HOME/Library/Logs/1Context" \
    "$FIXTURE_HOME/Library/Caches/1Context/render-cache" \
    "$FIXTURE_HOME/Library/Caches/com.haptica.1context" \
    "$FIXTURE_HOME/Library/Caches/com.haptica.1context.menu" \
    "$FIXTURE_HOME/Library/HTTPStorages/com.haptica.1context" \
    "$FIXTURE_HOME/Library/HTTPStorages/1context" \
    "$FIXTURE_HOME/Library/LaunchAgents" \
    "$FIXTURE_HOME/Library/Preferences" \
    "$FIXTURE_HOME/Library/Saved Application State/com.haptica.1context.savedState" \
    "$FIXTURE_HOME/Library/WebKit/com.haptica.1context" \
    "$FIXTURE_HOME/Not1Context"
  printf 'human wiki content\n' > "$FIXTURE_HOME/1Context/human-wiki-content.md"
  printf 'support state\n' > "$FIXTURE_HOME/Library/Application Support/1Context/config.json"
  printf 'log\n' > "$FIXTURE_HOME/Library/Logs/1Context/1contextd.log"
  printf 'cache\n' > "$FIXTURE_HOME/Library/Caches/1Context/render-cache/item"
  printf 'cookie\n' > "$FIXTURE_HOME/Library/HTTPStorages/com.haptica.1context/state"
  printf 'prefs\n' > "$FIXTURE_HOME/Library/Preferences/com.haptica.1context.plist"
  printf 'do not delete\n' > "$FIXTURE_HOME/Not1Context/sentinel.txt"
  printf '<?xml version="1.0"?><plist><dict/></plist>\n' \
    > "$FIXTURE_HOME/Library/LaunchAgents/com.haptica.1context.plist"
  printf '<?xml version="1.0"?><plist><dict/></plist>\n' \
    > "$FIXTURE_HOME/Library/LaunchAgents/com.haptica.1context.menu.plist"
  printf 'cert\n' > "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-root.crt"
  printf 'ABC123\n' > "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-root.sha1"
  printf 'DEF456\n' > "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-root.sha256"
  printf '{}\n' > "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-setup.json"
  printf 'temporary uninstall command\n' > "$TEMP_COMMAND"
}

run_fixture_cli() {
  env \
    ONECONTEXT_UNINSTALL_HOME_DIR="$FIXTURE_HOME" \
    ONECONTEXT_ALLOW_UNINSTALL_HOME_OVERRIDE=1 \
    ONECONTEXT_APP_SUPPORT_DIR="$FIXTURE_HOME/Library/Application Support/1Context" \
    ONECONTEXT_USER_CONTENT_DIR="$FIXTURE_HOME/1Context" \
    ONECONTEXT_LOG_DIR="$FIXTURE_HOME/Library/Logs/1Context" \
    ONECONTEXT_CACHE_DIR="$FIXTURE_HOME/Library/Caches/1Context" \
    ONECONTEXT_PREFERENCES_PATH="$FIXTURE_HOME/Library/Preferences/com.haptica.1context.plist" \
    ONECONTEXT_SOCKET_PATH="$FIXTURE_HOME/Library/Application Support/1Context/run/1context.sock" \
    ONECONTEXT_LAUNCH_AGENT_DISABLED=1 \
    ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES=1 \
    ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR="$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup" \
    ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR="$FIXTURE_HOME/Library/Logs/1Context" \
    ONECONTEXT_LOCAL_WEB_SKIP_SERVICE_MANAGEMENT=1 \
    ONECONTEXT_LOCAL_WEB_SKIP_KEYCHAIN_MUTATION=1 \
    "$CLI" "$@"
}

assert_exists() {
  if [[ ! -e "$1" ]]; then
    echo "Expected path to exist: $1" >&2
    exit 1
  fi
}

assert_missing() {
  if [[ -e "$1" ]]; then
    echo "Expected path to be removed: $1" >&2
    exit 1
  fi
}

"$CLI" --help | grep -q "1context uninstall \\[--delete-data\\] \\[--keep-app\\]"
"$CLI" --help | grep -q "1context setup local-web <status|install|repair|uninstall>"
grep -q "Uninstall 1Context..." "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q "runBundledCLI(arguments: arguments)" "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q '"uninstall", "--menu-process"' "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q "1Context was moved to Trash." "$ROOT/macos/Sources/OneContextMenuBar/main.swift"
grep -q "AppBundleTrasher" "$ROOT/macos/Sources/OneContextCLI/main.swift"
grep -q "Application bundle" "$ROOT/macos/Sources/OneContextCLI/main.swift"

if "$CLI" uninstall --definitely-not-real >"$OUT_DIR/uninstall-unknown.out" 2>&1; then
  echo "Expected uninstall with an unknown option to fail before cleanup." >&2
  exit 1
fi
grep -q "Unknown argument: --definitely-not-real" "$OUT_DIR/uninstall-unknown.out"

write_fake_app
seed_fixture_state
run_fixture_cli uninstall --keep-app >"$OUT_DIR/uninstall-preserve.out"
grep -q "Preserved user data" "$OUT_DIR/uninstall-preserve.out"
grep -q "Preserved application bundle" "$OUT_DIR/uninstall-preserve.out"
assert_exists "$FIXTURE_HOME/1Context/human-wiki-content.md"
assert_exists "$APP/Contents/Info.plist"
assert_missing "$FIXTURE_HOME/Library/LaunchAgents/com.haptica.1context.plist"
assert_missing "$FIXTURE_HOME/Library/LaunchAgents/com.haptica.1context.menu.plist"
assert_missing "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-root.crt"
assert_missing "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-root.sha1"
assert_missing "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-root.sha256"
assert_missing "$FIXTURE_HOME/Library/Application Support/1Context/local-web/setup/local-web-setup.json"

seed_fixture_state
run_fixture_cli uninstall --delete-data --keep-app >"$OUT_DIR/uninstall-delete-data.out"
grep -q "Removed: User data" "$OUT_DIR/uninstall-delete-data.out"
assert_missing "$FIXTURE_HOME/1Context"
assert_missing "$FIXTURE_HOME/Library/Application Support/1Context"
assert_missing "$FIXTURE_HOME/Library/Logs/1Context"
assert_missing "$FIXTURE_HOME/Library/Caches/1Context"
assert_missing "$FIXTURE_HOME/Library/Caches/com.haptica.1context"
assert_missing "$FIXTURE_HOME/Library/HTTPStorages/com.haptica.1context"
assert_missing "$FIXTURE_HOME/Library/Preferences/com.haptica.1context.plist"
assert_missing "$FIXTURE_HOME/Library/Saved Application State/com.haptica.1context.savedState"
assert_missing "$FIXTURE_HOME/Library/WebKit/com.haptica.1context"
assert_missing "$TEMP_COMMAND"
assert_exists "$FIXTURE_HOME/Not1Context/sentinel.txt"
assert_exists "$APP/Contents/Info.plist"

echo "macOS uninstall command smoke passed."
