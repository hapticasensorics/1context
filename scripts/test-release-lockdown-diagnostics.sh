#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLASSIFIER="$ROOT/scripts/classify-release-lockdown-diagnostics.py"
TMP_DIR="$(mktemp -d /tmp/1ctx-release-diagnostics-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

write_bundle() {
  local name="$1"
  local status="$2"
  local diagnose="$3"
  local update="$4"
  local dir="$TMP_DIR/$name"
  mkdir -p "$dir"
  printf '%s\n' "$status" > "$dir/status-debug.txt"
  printf '%s\n' "$diagnose" > "$dir/diagnose.txt"
  printf '%s\n' "$update" > "$dir/update-status.txt"
}

assert_state() {
  local name="$1"
  local expected="$2"
  shift 2
  python3 "$CLASSIFIER" --evidence-dir "$TMP_DIR/$name" "$@" > "$TMP_DIR/$name.out"
  grep -q "diagnostic_state=$expected" "$TMP_DIR/$name.out"
}

healthy_update='  Update Available: no
  Mandatory Update: no'

write_bundle \
  healthy \
  '1Context is running.
Health: OK' \
  'Runtime:
  Desired State: running
  Required Setup: ready' \
  "$healthy_update"
assert_state healthy healthy

write_bundle \
  needs_setup \
  '1Context is running.
Health: Needs Setup
Required Setup: Local web setup required: Local HTTPS helper' \
  'Runtime:
  Desired State: running
  Required Setup: needs setup' \
  "$healthy_update"
assert_state needs_setup needs_setup

write_bundle \
  needs_update \
  '1Context is running.
Health: OK' \
  'Runtime:
  Desired State: running
  Required Setup: ready' \
  '  Update Available: yes
  Mandatory Update: yes'
assert_state needs_update needs_update

write_bundle \
  stopped_by_user \
  '1Context is not running.' \
  'Runtime:
  Desired State: stopped
  Required Setup: ready' \
  "$healthy_update"
assert_state stopped_by_user stopped_by_user

write_bundle \
  failed_update \
  '1Context is running.
Health: OK' \
  'Runtime:
  Desired State: running
  Required Setup: ready' \
  "$healthy_update"
failed_dir="$TMP_DIR/failed-update-evidence"
mkdir -p "$failed_dir"
printf '%s\n' 'result=passed' 'failure_case=interrupted_download' > "$failed_dir/result.txt"
printf '%s\n' 'title=Update failed.' 'body=Please contact support at paul@haptica.ai.' > "$failed_dir/failure-message.txt"
assert_state failed_update failed_update --failed-update-dir "$failed_dir"

echo "Release lockdown diagnostic state checks passed."
