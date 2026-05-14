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

run_trust_command_with_gui_password() {
  local command_label="$1"
  shift
  log "starting certificate trust command: $command_label"
  "$@" >"$EVIDENCE_DIR/$command_label.out" 2>"$EVIDENCE_DIR/$command_label.err" &
  local command_pid="$!"

  if ! wait_for_prompt "$command_label-prompt"; then
    local status=0
    if wait_for_pid "$command_pid" "$command_label" >/dev/null 2>&1; then
      status=0
    else
      status=$?
    fi
    fail "certificate trust command '$command_label' did not show an authorization prompt; exit=$status"
  fi

  if ! approve_visible_prompt "$command_label"; then
    wait_for_pid "$command_pid" "$command_label" >/dev/null 2>&1 || true
    fail "authorization prompt stayed visible after password entry for '$command_label'"
  fi

  if ! wait_for_pid "$command_pid" "$command_label"; then
    local status="$?"
    capture_all "$command_label-failed"
    fail "certificate trust command '$command_label' failed after password entry; exit=$status"
  fi
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
  WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/onecontext-admin-password.XXXXXX")"
  CERT_PATH="$WORK_DIR/password-harness.crt"
  KEY_PATH="$WORK_DIR/password-harness.key"
  CERT_SUBJECT="/CN=1Context Password Harness $STAMP"
  log "creating temporary certificate"
  openssl req -x509 -newkey rsa:2048 -sha256 -days 1 -nodes \
    -keyout "$KEY_PATH" \
    -out "$CERT_PATH" \
    -subj "$CERT_SUBJECT" \
    >"$EVIDENCE_DIR/openssl.out" 2>"$EVIDENCE_DIR/openssl.err"

  run_trust_command_with_gui_password \
    "add-trusted-cert" \
    security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain "$CERT_PATH"

  run_trust_command_with_gui_password \
    "remove-trusted-cert" \
    security remove-trusted-cert -d "$CERT_PATH"
fi

capture_all "final"
cat >"$EVIDENCE_DIR/result.json" <<JSON
{
  "case": "admin_password_entry",
  "status": "passed",
  "evidence_dir": "$EVIDENCE_DIR"
}
JSON

log "passed; evidence: $EVIDENCE_DIR"
