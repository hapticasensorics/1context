#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK_DIR="$(mktemp -d /tmp/1ctx-menu-lifecycle-XXXXXX)"

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

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

MENU_SOURCE="$ROOT/macos/Sources/OneContextMenuBar/main.swift"
MENU_OPEN_BODY="$WORK_DIR/menu-open-body.swift"
MENU_RENDER_BODY="$WORK_DIR/menu-render-body.swift"
awk '
  /func menuWillOpen\(_ menu: NSMenu\)/ { in_body = 1 }
  /func menuDidClose\(_ menu: NSMenu\)/ { in_body = 0 }
  in_body { print }
' "$MENU_SOURCE" > "$MENU_OPEN_BODY"
awk '
  /private func refreshMenuItems\(\)/ { in_body = 1 }
  /private func setRuntimeState\(_ newValue: RuntimeState/ { in_body = 0 }
  in_body { print }
' "$MENU_SOURCE" > "$MENU_RENDER_BODY"

assert_contains "runNativeUpdateFlow" "$MENU_SOURCE"
assert_contains "NativeUpdateSnapshot" "$MENU_SOURCE"
assert_contains "Uninstall 1Context..." "$MENU_SOURCE"
assert_contains "runBundledCLI(arguments: arguments)" "$MENU_SOURCE"
assert_contains "LocalWebSetupInstaller().install()" "$MENU_SOURCE"
assert_not_contains "runUpdateCommandInTerminal" "$MENU_SOURCE"
assert_not_contains "runLocalWebSetupCommandInTerminal" "$MENU_SOURCE"
assert_not_contains "runTerminalScript" "$MENU_SOURCE"
assert_not_contains "writeUpdaterScript" "$MENU_SOURCE"
assert_not_contains "tell application \"Terminal\"" "$MENU_SOURCE"
assert_not_contains 'do script \"/bin/zsh \" & quoted form of scriptPath' "$MENU_SOURCE"
assert_not_contains "/bin/zsh -lc" "$MENU_SOURCE"

assert_contains "startDesiredStateMonitor()" "$MENU_SOURCE"
assert_contains "startLocalWebEdge()" "$MENU_SOURCE"
assert_contains "scheduleLocalWebEdgeStartupRetries()" "$MENU_SOURCE"
assert_contains "self?.startLocalWebEdge()" "$MENU_SOURCE"
assert_contains "local-web.start failed" "$MENU_SOURCE"
assert_contains "registerMenuLaunchAgent()" "$MENU_SOURCE"
assert_contains "adoptLaunchRuntimeIntent()" "$MENU_SOURCE"
assert_contains "RuntimePermissions.writePrivateString(\"running\\n\", toFile: paths.desiredStatePath)" "$MENU_SOURCE"
assert_not_contains "app-launch-version" "$MENU_SOURCE"
assert_contains "controller.requestStart(startMenu: false)" "$MENU_SOURCE"
assert_contains "controller.requestStop()" "$MENU_SOURCE"
assert_contains "stopForAppQuit()" "$MENU_SOURCE"
assert_not_contains "RuntimeController().quit(stopMenu: false)" "$MENU_SOURCE"
assert_contains "startSetupReadinessPolling(message:" "$MENU_SOURCE"
assert_contains "pollSetupReadiness()" "$MENU_SOURCE"
assert_contains "completeLocalWebSetup(readiness:" "$MENU_SOURCE"
assert_contains "startRuntimeImmediatelyAfterSetup()" "$MENU_SOURCE"
assert_contains "readDesiredRuntimeIntentFromDisk()" "$MENU_SOURCE"
assert_contains "return state == \"stopped\" ? .stopped : .running" "$MENU_SOURCE"
assert_contains "startStopItem" "$MENU_SOURCE"
assert_not_contains "isRuntimeActionInFlight = true" "$MENU_SOURCE"
assert_not_contains "refreshRuntimeIntentForMenuOpen" "$MENU_SOURCE"
assert_not_contains "contentsOfFile" "$MENU_OPEN_BODY"
assert_not_contains "currentReadiness()" "$MENU_RENDER_BODY"

echo "1Context deterministic menu lifecycle checks passed."
