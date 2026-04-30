#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
WORK_DIR="$(mktemp -d /tmp/1ctx-upgrade-paths-XXXXXX)"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

version_part() {
  local version="$1"
  local index="$2"
  IFS='.' read -r major minor patch <<<"$version"
  case "$index" in
    major) printf '%s\n' "$major" ;;
    minor) printf '%s\n' "$minor" ;;
    patch) printf '%s\n' "$patch" ;;
  esac
}

previous_version() {
  local patch
  patch="$(version_part "$VERSION" patch)"
  if (( patch <= 0 )); then
    echo "Cannot infer previous patch version from $VERSION." >&2
    exit 1
  fi
  printf '%s.%s.%s\n' "$(version_part "$VERSION" major)" "$(version_part "$VERSION" minor)" "$((patch - 1))"
}

next_version() {
  local patch
  patch="$(version_part "$VERSION" patch)"
  printf '%s.%s.%s\n' "$(version_part "$VERSION" major)" "$(version_part "$VERSION" minor)" "$((patch + 1))"
}

assert_contains() {
  local needle="$1"
  local path="$2"
  if ! grep -Fq -- "$needle" "$path"; then
    echo "Expected '$path' to contain: $needle" >&2
    echo "--- $path ---" >&2
    cat "$path" >&2
    exit 1
  fi
}

assert_not_contains() {
  local needle="$1"
  local path="$2"
  if grep -Fq -- "$needle" "$path"; then
    echo "Expected '$path' not to contain: $needle" >&2
    echo "--- $path ---" >&2
    cat "$path" >&2
    exit 1
  fi
}

find_cli() {
  if [[ -n "${ONECONTEXT_UPGRADE_TEST_CLI:-}" ]]; then
    if [[ "$("$ONECONTEXT_UPGRADE_TEST_CLI" --version)" != "$VERSION" ]]; then
      echo "ONECONTEXT_UPGRADE_TEST_CLI does not match VERSION $VERSION." >&2
      exit 1
    fi
    printf '%s\n' "$ONECONTEXT_UPGRADE_TEST_CLI"
    return
  fi

  for candidate in \
    "$ROOT/macos/.build/release/1context" \
    "$ROOT/macos/.build/arm64-apple-macosx/release/1context" \
    "$ROOT/macos/.build/debug/1context" \
    "$ROOT/macos/.build/arm64-apple-macosx/debug/1context"
  do
    if [[ -x "$candidate" && "$("$candidate" --version)" == "$VERSION" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  swift build --package-path "$ROOT/macos" --product 1context >/dev/null
  for candidate in \
    "$ROOT/macos/.build/debug/1context" \
    "$ROOT/macos/.build/arm64-apple-macosx/debug/1context"
  do
    if [[ -x "$candidate" && "$("$candidate" --version)" == "$VERSION" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  echo "Could not find built 1context CLI." >&2
  exit 1
}

write_fake_installed_app() {
  local app="$1"
  local version_file="$2"
  mkdir -p "$app/Contents"
  cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleShortVersionString</key>
  <string>$(cat "$version_file")</string>
</dict>
</plist>
PLIST
}

start_update_server() {
  local version="$1"
  local server_dir="$WORK_DIR/server"
  local port_file="$WORK_DIR/http.port"
  mkdir -p "$server_dir"
  cat > "$server_dir/latest.json" <<JSON
{"version":"$version","notes_url":"https://example.invalid/1context/$version"}
JSON
  cat > "$WORK_DIR/http-server.py" <<PY
import functools
import http.server
import pathlib
import socketserver

directory = pathlib.Path("$server_dir")
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(directory))
with socketserver.TCPServer(("127.0.0.1", 0), handler) as httpd:
    pathlib.Path("$port_file").write_text(str(httpd.server_address[1]) + "\\n")
    httpd.serve_forever()
PY
  python3 "$WORK_DIR/http-server.py" > "$WORK_DIR/http.log" 2>&1 &
  SERVER_PID=$!
  for _ in {1..80}; do
    if [[ -s "$port_file" ]]; then
      cat "$port_file"
      return
    fi
    sleep 0.1
  done
  echo "Update test HTTP server did not start." >&2
  cat "$WORK_DIR/http.log" >&2
  exit 1
}

stop_update_server() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
    unset SERVER_PID
  fi
}

make_case_tools() {
  local case_dir="$1"
  local version_file="$2"
  local app_path="$3"
  local event_log="$4"
  local tap_dir="$case_dir/tap"
  local bin_dir="$case_dir/bin"
  mkdir -p "$tap_dir" "$bin_dir"

  cat > "$case_dir/installed-1context" <<SCRIPT
#!/usr/bin/env bash
case "\${1:-}" in
  --version|-v|version)
    cat "$version_file"
    ;;
  restart)
    printf 'installed-cli restart\\n' >> "$event_log"
    ;;
  *)
    printf 'installed-cli %s\\n' "\$*" >> "$event_log"
    ;;
esac
SCRIPT
  chmod +x "$case_dir/installed-1context"

  cat > "$bin_dir/brew" <<SCRIPT
#!/usr/bin/env bash
printf 'brew %s\\n' "\$*" >> "$event_log"
if [[ "\$1" == "--repo" && "\${2:-}" == "hapticasensorics/tap" ]]; then
  printf '%s\\n' "$tap_dir"
  exit 0
fi
if [[ "\$1" == "tap" && "\${2:-}" == "hapticasensorics/tap" ]]; then
  exit 0
fi
if [[ "\$1" == "upgrade" && "\${2:-}" == "--cask" && "\${3:-}" == "hapticasensorics/tap/1context" ]]; then
  printf '%s\\n' "\$ONECONTEXT_TEST_TARGET_VERSION" > "$version_file"
  mkdir -p "$app_path/Contents"
  cat > "$app_path/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleShortVersionString</key>
  <string>\$ONECONTEXT_TEST_TARGET_VERSION</string>
</dict>
</plist>
PLIST
  exit 0
fi
echo "unexpected brew invocation: \$*" >&2
exit 2
SCRIPT
  chmod +x "$bin_dir/brew"

  cat > "$bin_dir/git" <<SCRIPT
#!/usr/bin/env bash
printf 'git %s\\n' "\$*" >> "$event_log"
exit 0
SCRIPT
  chmod +x "$bin_dir/git"
}

run_cli_upgrade_case() {
  local name="$1"
  local from_version="$2"
  local target_version="$3"
  local cli="$4"
  local case_dir="$WORK_DIR/$name"
  local state_dir="$case_dir/state"
  local event_log="$case_dir/events.log"
  local version_file="$case_dir/installed-version"
  local app_path="$case_dir/1Context.app"
  mkdir -p "$case_dir" "$state_dir/Application Support/1Context"
  printf '%s\n' "$from_version" > "$version_file"
  printf 'stopped\n' > "$state_dir/Application Support/1Context/desired-state"
  : > "$event_log"
  write_fake_installed_app "$app_path" "$version_file"
  make_case_tools "$case_dir" "$version_file" "$app_path" "$event_log"

  local port
  port="$(start_update_server "$target_version")"
  local output="$case_dir/update.out"
  ONECONTEXT_TEST_CURRENT_VERSION="$from_version" \
    ONECONTEXT_TEST_TARGET_VERSION="$target_version" \
    ONECONTEXT_TEST_BREW_EXECUTABLE="$case_dir/bin/brew" \
    ONECONTEXT_TEST_GIT_EXECUTABLE="$case_dir/bin/git" \
    ONECONTEXT_TEST_INSTALLED_CLI="$case_dir/installed-1context" \
    ONECONTEXT_TEST_APP_BUNDLE_PATH="$app_path" \
    ONECONTEXT_APP_SUPPORT_DIR="$state_dir/Application Support/1Context" \
    ONECONTEXT_USER_CONTENT_DIR="$state_dir/1Context" \
    ONECONTEXT_LOG_DIR="$state_dir/Logs/1Context" \
    ONECONTEXT_CACHE_DIR="$state_dir/Caches/1Context" \
    ONECONTEXT_UPDATE_STATE_DIR="$state_dir/Application Support/1Context/update" \
    ONECONTEXT_UPDATE_URL="http://127.0.0.1:$port/latest.json" \
    "$cli" update > "$output"
  stop_update_server

  assert_contains "1Context $target_version is available. You have $from_version." "$output"
  assert_contains "Updating 1Context..." "$output"
  assert_contains "brew --repo hapticasensorics/tap" "$event_log"
  assert_contains "git -C $case_dir/tap fetch --quiet --no-tags origin main:refs/remotes/origin/main" "$event_log"
  assert_contains "git -C $case_dir/tap merge --quiet --ff-only refs/remotes/origin/main" "$event_log"
  assert_contains "brew upgrade --cask hapticasensorics/tap/1context" "$event_log"
  test "$(cat "$version_file")" = "$target_version"
  test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app_path/Contents/Info.plist")" = "$target_version"
  assert_not_contains "installed-cli restart" "$event_log"
}

run_cli_noop_case() {
  local cli="$1"
  local case_dir="$WORK_DIR/noop"
  local state_dir="$case_dir/state"
  local event_log="$case_dir/events.log"
  local version_file="$case_dir/installed-version"
  local app_path="$case_dir/1Context.app"
  mkdir -p "$case_dir" "$state_dir/Application Support/1Context"
  printf '%s\n' "$VERSION" > "$version_file"
  printf 'stopped\n' > "$state_dir/Application Support/1Context/desired-state"
  : > "$event_log"
  write_fake_installed_app "$app_path" "$version_file"
  make_case_tools "$case_dir" "$version_file" "$app_path" "$event_log"

  local port
  port="$(start_update_server "$VERSION")"
  local output="$case_dir/update.out"
  ONECONTEXT_TEST_CURRENT_VERSION="$VERSION" \
    ONECONTEXT_TEST_TARGET_VERSION="$VERSION" \
    ONECONTEXT_TEST_BREW_EXECUTABLE="$case_dir/bin/brew" \
    ONECONTEXT_TEST_GIT_EXECUTABLE="$case_dir/bin/git" \
    ONECONTEXT_TEST_INSTALLED_CLI="$case_dir/installed-1context" \
    ONECONTEXT_TEST_APP_BUNDLE_PATH="$app_path" \
    ONECONTEXT_APP_SUPPORT_DIR="$state_dir/Application Support/1Context" \
    ONECONTEXT_USER_CONTENT_DIR="$state_dir/1Context" \
    ONECONTEXT_LOG_DIR="$state_dir/Logs/1Context" \
    ONECONTEXT_CACHE_DIR="$state_dir/Caches/1Context" \
    ONECONTEXT_UPDATE_STATE_DIR="$state_dir/Application Support/1Context/update" \
    ONECONTEXT_UPDATE_URL="http://127.0.0.1:$port/latest.json" \
    "$cli" update > "$output"
  stop_update_server

  assert_contains "1Context up to date." "$output"
  assert_not_contains "brew upgrade --cask hapticasensorics/tap/1context" "$event_log"
}

run_gui_upgrade_bridge_checks() {
  local source="$ROOT/macos/Sources/OneContextMenuBar/main.swift"
  local gui_body="$WORK_DIR/gui-upgrade-body.swift"
  awk '
    /private func runUpdateCommandInTerminal\(\)/ { in_body = 1 }
    /private func writeUpdaterScript\(_ script: String\)/ { in_body = 0 }
    in_body { print }
  ' "$source" > "$gui_body"

  assert_contains 'updateTitle = "Please Update"' "$source"
  assert_contains 'updateAction = #selector(openUpgradeCommand)' "$source"
  assert_contains '@objc private func openUpgradeCommand()' "$source"
  assert_contains 'guard confirmUpdate() else { return }' "$source"
  assert_contains 'runUpdateCommandInTerminal()' "$source"
  assert_contains 'if result.updateAvailable {' "$source"
  assert_contains 'if self.confirmUpdate() {' "$source"
  assert_contains 'self.runUpdateCommandInTerminal()' "$source"
  assert_contains 'appendingPathComponent("1context-cli")' "$gui_body"
  assert_contains 'if \(shellQuote(cliExecutable)) update; then' "$gui_body"
  assert_contains '--update-success-alert' "$gui_body"
  assert_contains 'display dialog "Could not update 1Context."' "$gui_body"
  assert_contains 'do script \"/bin/zsh \" & quoted form of scriptPath' "$source"
  assert_not_contains '/opt/homebrew/bin/1context update' "$gui_body"
  assert_not_contains 'brew upgrade --cask' "$gui_body"
}

CLI="$(find_cli)"
test "$("$CLI" --version)" = "$VERSION"

run_cli_upgrade_case "previous-to-current" "$(previous_version)" "$VERSION" "$CLI"
run_cli_upgrade_case "current-to-next" "$VERSION" "$(next_version)" "$CLI"
run_cli_noop_case "$CLI"
run_gui_upgrade_bridge_checks

echo "1Context upgrade path checks passed."
