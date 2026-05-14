#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${ONECONTEXT_APP:-/Applications/1Context.app}"
CLI="$APP/Contents/MacOS/1context-cli"
LABEL_RUNTIME="com.haptica.1context"
LABEL_MENU="com.haptica.1context.menu"
DURATION_SECONDS="${ONECONTEXT_STEADY_STATE_SECONDS:-120}"
INTERVAL_SECONDS="${ONECONTEXT_STEADY_STATE_INTERVAL_SECONDS:-5}"
STAMP="$(date -u +"%Y%m%dT%H%M%SZ")"
EVIDENCE_DIR="${ONECONTEXT_STEADY_STATE_EVIDENCE_DIR:-$ROOT/dist/steady-state-evidence/$STAMP}"
RUNTIME_LOG="$HOME/Library/Logs/1Context/1contextd.log"

mkdir -p "$EVIDENCE_DIR"

fail() {
  echo "steady-state failed: $*" >&2
  echo "$*" > "$EVIDENCE_DIR/failure.txt"
  exit 1
}

count_runtime_stops() {
  if [[ ! -f "$RUNTIME_LOG" ]]; then
    echo 0
    return
  fi
  grep -c "1Context runtime stopping signal=SIGTERM" "$RUNTIME_LOG" || true
}

capture_once() {
  local name="$1"
  local diagnose_exit=0
  "$CLI" diagnose > "$EVIDENCE_DIR/diagnose-$name.txt" 2>&1 || diagnose_exit=$?
  printf '%s\n' "$diagnose_exit" > "$EVIDENCE_DIR/diagnose-$name.exitcode"
  launchctl print "gui/$(id -u)/$LABEL_RUNTIME" > "$EVIDENCE_DIR/launchctl-runtime-$name.txt" 2>&1 || true
  launchctl print "gui/$(id -u)/$LABEL_MENU" > "$EVIDENCE_DIR/launchctl-menu-$name.txt" 2>&1 || true
  ps -axo pid=,command= |
    awk -v app="$APP/Contents/MacOS/1Context" 'index($0, app) > 0 { print }' \
      > "$EVIDENCE_DIR/processes-menu-$name.txt" || true
  return "$diagnose_exit"
}

assert_menu_alive() {
  local name="$1"
  local menu_launchctl="$EVIDENCE_DIR/launchctl-menu-$name.txt"
  local menu_processes="$EVIDENCE_DIR/processes-menu-$name.txt"
  grep -Eq "state = running|pid = [0-9]+" "$menu_launchctl" && return 0
  [[ -s "$menu_processes" ]] && return 0
  fail "menu app was not running in launchd or process list during $name"
}

assert_diagnose_healthy() {
  local name="$1"
  local diagnose_file="$EVIDENCE_DIR/diagnose-$name.txt"
  local runtime_launchctl="$EVIDENCE_DIR/launchctl-runtime-$name.txt"
  local menu_launchctl="$EVIDENCE_DIR/launchctl-menu-$name.txt"
  grep -q "Runtime:" "$diagnose_file" || fail "runtime diagnostics missing in $(basename "$diagnose_file")"
  grep -q "  Health: OK" "$diagnose_file" || fail "runtime health was not OK in $(basename "$diagnose_file")"
  grep -q "Local Web:" "$diagnose_file" || fail "local web block missing in $(basename "$diagnose_file")"
  grep -q "  Health: OK" "$diagnose_file" || fail "local web health was not OK in $(basename "$diagnose_file")"
  grep -q "  Setup Ready: yes" "$diagnose_file" || fail "setup was not ready in $(basename "$diagnose_file")"
  grep -Eq "state = running|pid = [0-9]+" "$runtime_launchctl" || fail "runtime launch agent was not running in $(basename "$runtime_launchctl")"
  grep -q "path =" "$menu_launchctl" || fail "menu launch agent was not loaded in $(basename "$menu_launchctl")"
  assert_menu_alive "$name"
}

[[ -x "$CLI" ]] || fail "missing CLI at $CLI"

app_version="$(defaults read "$APP/Contents/Info" CFBundleShortVersionString 2>/dev/null || true)"
cli_version="$("$CLI" version)"
[[ -n "$app_version" ]] || fail "could not read app version"
[[ "$app_version" == "$cli_version" ]] || fail "app version $app_version != CLI version $cli_version"
printf '%s\n' "$app_version" > "$EVIDENCE_DIR/version.txt"

start_stops="$(count_runtime_stops)"
printf '%s\n' "$start_stops" > "$EVIDENCE_DIR/runtime-sigterm-count-start.txt"

if ! capture_once "start"; then
  fail "CLI diagnose command failed at start; see diagnose-start.txt"
fi
assert_diagnose_healthy "start"

deadline=$((SECONDS + DURATION_SECONDS))
iteration=0
while (( SECONDS < deadline )); do
  sleep "$INTERVAL_SECONDS"
  iteration=$((iteration + 1))
  name="$(printf 'probe-%03d' "$iteration")"
  if ! capture_once "$name"; then
    fail "CLI diagnose command failed during $name; see diagnose-$name.txt"
  fi
  assert_diagnose_healthy "$name"
done

end_stops="$(count_runtime_stops)"
printf '%s\n' "$end_stops" > "$EVIDENCE_DIR/runtime-sigterm-count-end.txt"
if [[ "$end_stops" != "$start_stops" ]]; then
  fail "runtime SIGTERM count changed from $start_stops to $end_stops"
fi

if [[ -f "$RUNTIME_LOG" ]]; then
  tail -n 120 "$RUNTIME_LOG" > "$EVIDENCE_DIR/1contextd-tail.txt"
fi

cat > "$EVIDENCE_DIR/summary.txt" <<SUMMARY
1Context steady-state verification passed.
version=$app_version
duration_seconds=$DURATION_SECONDS
interval_seconds=$INTERVAL_SECONDS
probes=$iteration
runtime_sigterm_count=$end_stops
SUMMARY

cat "$EVIDENCE_DIR/summary.txt"
echo "evidence=$EVIDENCE_DIR"
