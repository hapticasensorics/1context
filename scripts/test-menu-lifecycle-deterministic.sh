#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK_DIR="$(mktemp -d /tmp/1ctx-menu-lifecycle-XXXXXX)"

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

make_fake_app() {
  local app_path="$1"
  mkdir -p "$app_path/Contents/MacOS"
  cat > "$app_path/Contents/MacOS/1Context" <<'SCRIPT'
#!/usr/bin/env bash
exit 0
SCRIPT
  chmod +x "$app_path/Contents/MacOS/1Context"
}

make_shims() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"

  cat > "$bin_dir/launchctl" <<'SCRIPT'
#!/usr/bin/env bash
printf 'launchctl %s\n' "$*" >> "$ONECONTEXT_TEST_EVENT_LOG"
exit 0
SCRIPT
  chmod +x "$bin_dir/launchctl"

  cat > "$bin_dir/osascript" <<'SCRIPT'
#!/usr/bin/env bash
printf 'osascript %s\n' "$*" >> "$ONECONTEXT_TEST_EVENT_LOG"
exit 0
SCRIPT
  chmod +x "$bin_dir/osascript"
}

make_fake_cli() {
  local cli_path="$1"
  cat > "$cli_path" <<'SCRIPT'
#!/usr/bin/env bash
printf 'cli %s\n' "$*" >> "$ONECONTEXT_TEST_EVENT_LOG"
exit 0
SCRIPT
  chmod +x "$cli_path"
}

plist_program() {
  plutil -extract ProgramArguments.0 raw "$1"
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

run_install_case() {
  local case_name="$1"
  local desired_state="${2:-}"
  local case_dir="$WORK_DIR/$case_name"
  local home_dir="$case_dir/home"
  local fake_bin="$case_dir/bin"
  local event_log="$case_dir/events.log"
  local app_path="$case_dir/1Context.app"
  local cli_path="$case_dir/1context"
  local plist="$home_dir/Library/LaunchAgents/com.haptica.1context.menu.plist"

  mkdir -p "$case_dir" "$home_dir"
  : > "$event_log"
  make_shims "$fake_bin"
  make_fake_app "$app_path"
  make_fake_cli "$cli_path"

  if [[ -n "$desired_state" ]]; then
    mkdir -p "$home_dir/Library/Application Support/1Context"
    printf '%s\n' "$desired_state" > "$home_dir/Library/Application Support/1Context/desired-state"
  fi

  HOME="$home_dir" \
    PATH="$fake_bin:/usr/bin:/bin" \
    ONECONTEXT_TEST_EVENT_LOG="$event_log" \
    "$ROOT/scripts/install-macos-launch-agents.sh" "$app_path" "$cli_path"

  test -f "$plist"
  test "$(plist_program "$plist")" = "$app_path/Contents/MacOS/1Context"
  test "$(plutil -extract RunAtLoad raw "$plist")" = "true"
  test "$(plutil -extract KeepAlive raw "$plist")" = "false"
  assert_contains "launchctl bootstrap gui/$(id -u) $plist" "$event_log"
}

run_install_case "stopped" "stopped"
assert_not_contains "cli restart" "$WORK_DIR/stopped/events.log"
assert_not_contains "cli start" "$WORK_DIR/stopped/events.log"

run_install_case "default-running" ""
assert_contains "cli restart" "$WORK_DIR/default-running/events.log"

MENU_SOURCE="$ROOT/macos/Sources/OneContextMenuBar/main.swift"
UPDATER_BODY="$WORK_DIR/updater-body.swift"
MENU_OPEN_BODY="$WORK_DIR/menu-open-body.swift"
awk '
  /private func runUpdateCommandInTerminal\(\)/ { in_body = 1 }
  /private func runTerminalScript\(_ scriptPath: String\)/ { in_body = 0 }
  in_body { print }
' "$MENU_SOURCE" > "$UPDATER_BODY"
awk '
  /func menuWillOpen\(_ menu: NSMenu\)/ { in_body = 1 }
  /func menuDidClose\(_ menu: NSMenu\)/ { in_body = 0 }
  in_body { print }
' "$MENU_SOURCE" > "$MENU_OPEN_BODY"

assert_contains "appendingPathComponent(\"1context-cli\")" "$UPDATER_BODY"
assert_contains "if \\(shellQuote(cliExecutable)) update; then" "$UPDATER_BODY"
assert_contains "writeUpdaterScript(script)" "$UPDATER_BODY"
assert_contains 'trap '\''rm -f "$0"'\'' EXIT' "$UPDATER_BODY"
assert_contains 'do script \"/bin/zsh \" & quoted form of scriptPath' "$MENU_SOURCE"
assert_not_contains "/bin/zsh -lc" "$MENU_SOURCE"
assert_not_contains "NSWorkspace.shared.open" "$UPDATER_BODY"

assert_contains "startDesiredStateMonitor()" "$MENU_SOURCE"
assert_contains "startLocalWebEdge()" "$MENU_SOURCE"
assert_contains "scheduleLocalWebEdgeStartupRetries()" "$MENU_SOURCE"
assert_contains "self?.startLocalWebEdge()" "$MENU_SOURCE"
assert_contains "local-web.start failed" "$MENU_SOURCE"
assert_contains "readDesiredRuntimeIntentFromDisk()" "$MENU_SOURCE"
assert_contains "return state == \"stopped\" ? .stopped : .running" "$MENU_SOURCE"
assert_contains "startStopItem" "$MENU_SOURCE"
assert_not_contains "refreshRuntimeIntentForMenuOpen" "$MENU_SOURCE"
assert_not_contains "contentsOfFile" "$MENU_OPEN_BODY"

echo "1Context deterministic menu lifecycle checks passed."
