#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/lib-gui-evidence.sh"

APP="${ONECONTEXT_INSTALLED_APP:-/Applications/1Context.app}"
CLI="${ONECONTEXT_INSTALLED_CLI:-$APP/Contents/MacOS/1context-cli}"
EXPECTED_VERSION="${ONECONTEXT_EXPECTED_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
LABEL_RUNTIME="com.haptica.1context"
LABEL_MENU="com.haptica.1context.menu"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_LAUNCHAGENT_RECOVERY_EVIDENCE_DIR:-$ROOT/dist/launchagent-recovery-evidence/$STAMP}"
APP_SUPPORT_DIR="${ONECONTEXT_APP_SUPPORT_DIR:-$HOME/Library/Application Support/1Context}"
DESIRED_STATE_FILE="$APP_SUPPORT_DIR/desired-state"

fail() {
  echo "macOS LaunchAgent recovery harness failed: $*" >&2
  mkdir -p "$EVIDENCE_DIR"
  printf '%s\n' "$*" > "$EVIDENCE_DIR/failure.txt"
  exit 1
}

capture_status() {
  local name="$1"
  "$CLI" status > "$EVIDENCE_DIR/status-$name.txt" 2>&1 || true
  "$CLI" diagnose > "$EVIDENCE_DIR/diagnose-$name.txt" 2>&1 || true
  launchctl print "gui/$(id -u)/$LABEL_RUNTIME" > "$EVIDENCE_DIR/launchctl-runtime-$name.txt" 2>&1 || true
  launchctl print "gui/$(id -u)/$LABEL_MENU" > "$EVIDENCE_DIR/launchctl-menu-$name.txt" 2>&1 || true
  capture_menu "$EVIDENCE_DIR/menu-$name.txt"
  capture_screenshot "$EVIDENCE_DIR/desktop-$name.png"
  if [[ -f "$DESIRED_STATE_FILE" ]]; then
    tr -d '[:space:]' < "$DESIRED_STATE_FILE" > "$EVIDENCE_DIR/desired-state-$name.txt"
  else
    printf 'missing\n' > "$EVIDENCE_DIR/desired-state-$name.txt"
  fi
}

assert_healthy() {
  local name="$1"
  local status="$EVIDENCE_DIR/status-$name.txt"
  local diagnose="$EVIDENCE_DIR/diagnose-$name.txt"
  grep -q "1Context is running." "$status" || fail "$name did not report running"
  grep -q "Version: $EXPECTED_VERSION" "$status" || fail "$name did not report version $EXPECTED_VERSION"
  grep -q "Health: OK" "$status" || fail "$name health was not OK"
  grep -q "Menu Bar: running" "$status" || fail "$name menu bar was not running"
  grep -q "LaunchAgents:" "$diagnose" || fail "$name launch agent diagnostics were missing"
  grep -q "Runtime Version: $EXPECTED_VERSION" "$diagnose" || fail "$name runtime diagnostics did not report version $EXPECTED_VERSION"
  grep -q "  Health: OK" "$diagnose" || fail "$name local web health was not OK"
  grep -q "  Setup Ready: yes" "$diagnose" || fail "$name setup was not ready"
  grep -Fq "Version $EXPECTED_VERSION" "$EVIDENCE_DIR/menu-$name.txt" || fail "$name menu did not expose Settings version"
  grep -Fq "Check for Updates" "$EVIDENCE_DIR/menu-$name.txt" || fail "$name menu did not expose Check for Updates"
}

wait_for_healthy() {
  local name="$1"
  local deadline=$(( "$(date +%s)" + 45 ))
  local attempt=0
  while true; do
    attempt=$((attempt + 1))
    "$CLI" status > "$EVIDENCE_DIR/status-$name-attempt-$attempt.txt" 2>&1 || true
    "$CLI" diagnose > "$EVIDENCE_DIR/diagnose-$name-attempt-$attempt.txt" 2>&1 || true
    if grep -q "1Context is running." "$EVIDENCE_DIR/status-$name-attempt-$attempt.txt" &&
      grep -q "Version: $EXPECTED_VERSION" "$EVIDENCE_DIR/status-$name-attempt-$attempt.txt" &&
      grep -q "Health: OK" "$EVIDENCE_DIR/status-$name-attempt-$attempt.txt" &&
      grep -q "Menu Bar: running" "$EVIDENCE_DIR/status-$name-attempt-$attempt.txt" &&
      grep -q "Runtime Version: $EXPECTED_VERSION" "$EVIDENCE_DIR/diagnose-$name-attempt-$attempt.txt" &&
      grep -q "  Health: OK" "$EVIDENCE_DIR/diagnose-$name-attempt-$attempt.txt" &&
      grep -q "  Setup Ready: yes" "$EVIDENCE_DIR/diagnose-$name-attempt-$attempt.txt"; then
      cp "$EVIDENCE_DIR/status-$name-attempt-$attempt.txt" "$EVIDENCE_DIR/status-$name.txt"
      cp "$EVIDENCE_DIR/diagnose-$name-attempt-$attempt.txt" "$EVIDENCE_DIR/diagnose-$name.txt"
      capture_status "$name"
      assert_healthy "$name"
      return
    fi
    if (( "$(date +%s)" >= deadline )); then
      cp "$EVIDENCE_DIR/status-$name-attempt-$attempt.txt" "$EVIDENCE_DIR/status-$name.txt"
      fail "Timed out waiting for healthy state during $name"
    fi
    sleep 2
  done
}

[[ -d "$APP" ]] || fail "Installed app not found: $APP"
[[ -x "$CLI" ]] || fail "Installed CLI not found or not executable: $CLI"

mkdir -p "$EVIDENCE_DIR"

installed_version="$("$CLI" --version 2>/dev/null || true)"
if [[ "$installed_version" != "$EXPECTED_VERSION" ]]; then
  fail "Installed CLI version is $installed_version, expected $EXPECTED_VERSION"
fi

{
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "app=$APP"
  echo "cli=$CLI"
  echo "expected_version=$EXPECTED_VERSION"
  echo "installed_version=$installed_version"
  echo "evidence_dir=$EVIDENCE_DIR"
} > "$EVIDENCE_DIR/environment.txt"

"$CLI" start > "$EVIDENCE_DIR/start-before.txt" 2>&1 || true
open "$APP"
wait_for_healthy "before"

osascript -e 'tell application id "com.haptica.1context.menu" to quit' > "$EVIDENCE_DIR/menu-quit.txt" 2>&1 || true
sleep 3
open "$APP"
wait_for_healthy "after-app-relaunch"

"$CLI" start > "$EVIDENCE_DIR/start-before-login-style.txt" 2>&1 || true
if [[ -f "$DESIRED_STATE_FILE" ]]; then
  tr -d '[:space:]' < "$DESIRED_STATE_FILE" > "$EVIDENCE_DIR/desired-state-before-login-style.txt"
fi
grep -qx "running" "$EVIDENCE_DIR/desired-state-before-login-style.txt" || fail "desired state was not running before login-style recovery"

launchctl bootout "gui/$(id -u)/$LABEL_MENU" > "$EVIDENCE_DIR/bootout-menu.txt" 2>&1 || true
launchctl bootout "gui/$(id -u)/$LABEL_RUNTIME" > "$EVIDENCE_DIR/bootout-runtime.txt" 2>&1 || true
sleep 3
capture_status "after-bootout"

open "$APP"
wait_for_healthy "after-login-style-recovery"
grep -qx "running" "$EVIDENCE_DIR/desired-state-after-login-style-recovery.txt" || fail "desired state did not remain running after login-style recovery"

{
  echo "result=passed"
  echo "version=$EXPECTED_VERSION"
  echo "evidence_dir=$EVIDENCE_DIR"
  echo "proved=app_relaunch"
  echo "proved=login_style_launchagent_recovery"
  echo "desired_state=running"
} > "$EVIDENCE_DIR/result.txt"

printf '%s\n' "$EVIDENCE_DIR"
