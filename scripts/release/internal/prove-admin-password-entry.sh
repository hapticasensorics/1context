#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/scripts/release/internal/lib-gui-evidence.sh"

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_ADMIN_PASSWORD_EVIDENCE_DIR:-$ROOT/dist/admin-password-entry/$STAMP}"
mkdir -p "$EVIDENCE_DIR"

exec > >(tee -a "$EVIDENCE_DIR/admin-password-entry.log") 2>&1

log() {
  printf '[admin-password-entry] %s\n' "$*"
}

fail() {
  log "failed: $*"
  exit 1
}

capture_all() {
  local label="$1"
  capture_windows "$EVIDENCE_DIR/windows-$label.txt"
  capture_accessibility "$EVIDENCE_DIR/accessibility-$label.txt"
  capture_screenshot "$EVIDENCE_DIR/desktop-$label.png"
}

prompt_visible_in() {
  local report="$1"
  grep -Eq 'Certificate Trust Settings|Enter your password to allow this|Update Settings' "$report"
}

wait_for_prompt() {
  local label="$1"
  local deadline
  deadline=$(($(date +%s) + 30))
  while true; do
    capture_all "$label"
    if prompt_visible_in "$EVIDENCE_DIR/accessibility-$label.txt"; then
      return 0
    fi
    if (( "$(date +%s)" >= deadline )); then
      return 1
    fi
    sleep 1
  done
}

wait_for_pid() {
  local pid="$1"
  local label="$2"
  local deadline
  deadline=$(($(date +%s) + 45))
  while kill -0 "$pid" >/dev/null 2>&1; do
    if (( "$(date +%s)" >= deadline )); then
      capture_all "$label-timeout"
      kill "$pid" >/dev/null 2>&1 || true
      wait "$pid" >/dev/null 2>&1 || true
      return 124
    fi
    sleep 1
  done
  wait "$pid"
}

run_installed_diagnose() {
  local output="$1"
  if [[ ! -x "/Applications/1Context.app/Contents/MacOS/1context-cli" ]]; then
    return 1
  fi
  "/Applications/1Context.app/Contents/MacOS/1context-cli" diagnose >"$output" 2>&1
}

approve_visible_prompt() {
  local label="$1"
  capture_all "$label-before"
  approve_admin_authorization_prompt >"$EVIDENCE_DIR/approve-$label.txt" 2>&1 || true
  sleep 2
  capture_all "$label-after"
  if prompt_visible_in "$EVIDENCE_DIR/accessibility-$label-after.txt"; then
    return 1
  fi
  return 0
}

write_result() {
  local status="$1"
  local detail="$2"
  cat >"$EVIDENCE_DIR/result.json" <<JSON
{
  "case": "admin_password_entry",
  "status": "$status",
  "detail": "$detail",
  "evidence_dir": "$EVIDENCE_DIR"
}
JSON
}

if [[ -z "${ONECONTEXT_UPDATE_RUNNER_ADMIN_PASSWORD:-}" ]]; then
  fail "ONECONTEXT_UPDATE_RUNNER_ADMIN_PASSWORD is required"
fi

log "capturing initial desktop state"
capture_all "initial"

if prompt_visible_in "$EVIDENCE_DIR/accessibility-initial.txt"; then
  log "using already-visible admin authorization prompt"
  approve_visible_prompt "existing-prompt" || fail "existing authorization prompt stayed visible after password entry"
else
  if run_installed_diagnose "$EVIDENCE_DIR/diagnose-no-prompt.txt" &&
    grep -q "  Setup Ready: yes" "$EVIDENCE_DIR/diagnose-no-prompt.txt"; then
    log "no admin authorization prompt is visible because installed setup is already ready"
    capture_all "final"
    write_result "skipped_no_prompt" "installed setup is already ready; no password sheet is available to approve"
    exit 0
  fi
  fail "no admin authorization prompt is visible; open 1Context setup and click Grant before running this isolated harness"
fi

capture_all "final"
write_result "passed" "visible admin authorization prompt was approved"

log "passed; evidence: $EVIDENCE_DIR"
