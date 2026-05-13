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
  local status_exit=0
  "$CLI" status > "$EVIDENCE_DIR/status-$name.txt" 2>&1 || status_exit=$?
  "$CLI" diagnose > "$EVIDENCE_DIR/diagnose-$name.txt" 2>&1 || true
  printf '%s\n' "$status_exit" > "$EVIDENCE_DIR/status-$name.exitcode"
  "$CLI" update > "$EVIDENCE_DIR/update-$name.txt" 2>&1 || true
  launchctl print "gui/$(id -u)/$LABEL_RUNTIME" > "$EVIDENCE_DIR/launchctl-runtime-$name.txt" 2>&1 || true
  launchctl print "gui/$(id -u)/$LABEL_MENU" > "$EVIDENCE_DIR/launchctl-menu-$name.txt" 2>&1 || true
  return "$status_exit"
}

assert_status_healthy() {
  local file="$1"
  local diagnose_file="${file/status-/diagnose-}"
  grep -q "1Context is running." "$file" || fail "CLI did not report running in $(basename "$file")"
  grep -q "Health: OK" "$file" || fail "runtime health was not OK in $(basename "$file")"
  grep -q "Menu Bar: running" "$file" || fail "menu bar was not running in $(basename "$file")"
  grep -q "Runtime:" "$diagnose_file" || fail "runtime diagnostics missing in $(basename "$diagnose_file")"
  grep -q "Local Web:" "$diagnose_file" || fail "local web block missing in $(basename "$diagnose_file")"
  grep -q "  Health: OK" "$diagnose_file" || fail "local web health was not OK in $(basename "$diagnose_file")"
  grep -q "  Setup Ready: yes" "$diagnose_file" || fail "setup was not ready in $(basename "$diagnose_file")"
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
  fail "CLI status command failed at start; see status-start.txt"
fi
assert_status_healthy "$EVIDENCE_DIR/status-start.txt"

deadline=$((SECONDS + DURATION_SECONDS))
iteration=0
while (( SECONDS < deadline )); do
  sleep "$INTERVAL_SECONDS"
  iteration=$((iteration + 1))
  name="$(printf 'probe-%03d' "$iteration")"
  if ! capture_once "$name"; then
    fail "CLI status command failed during $name; see status-$name.txt"
  fi
  assert_status_healthy "$EVIDENCE_DIR/status-$name.txt"
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
