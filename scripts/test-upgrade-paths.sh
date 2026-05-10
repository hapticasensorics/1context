#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d /tmp/1ctx-upgrade-paths-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "Missing expected upgrade-path file: $path" >&2
    exit 1
  fi
}

require_file "$ROOT/scripts/prove-remote-sparkle-update.sh"
require_file "$ROOT/scripts/self-hosted-update-proof.sh"
require_file "$ROOT/scripts/self-hosted-mac-runner-enroll.sh"
require_file "$ROOT/scripts/smoke-sparkle-local-appcast.sh"
require_file "$ROOT/scripts/collect-release-lockdown-evidence.sh"

bash -n \
  "$ROOT/scripts/prove-remote-sparkle-update.sh" \
  "$ROOT/scripts/self-hosted-update-proof.sh" \
  "$ROOT/scripts/self-hosted-mac-runner-enroll.sh" \
  "$ROOT/scripts/smoke-sparkle-local-appcast.sh" \
  "$ROOT/scripts/collect-release-lockdown-evidence.sh"

if command -v shellcheck >/dev/null 2>&1; then
  shellcheck \
    "$ROOT/scripts/prove-remote-sparkle-update.sh" \
    "$ROOT/scripts/self-hosted-update-proof.sh" \
    "$ROOT/scripts/self-hosted-mac-runner-enroll.sh" \
    "$ROOT/scripts/smoke-sparkle-local-appcast.sh" \
    "$ROOT/scripts/collect-release-lockdown-evidence.sh"
fi

if ONECONTEXT_INSTALLED_APP="$TMP_DIR/Nope.app" \
  ONECONTEXT_OLD_VERSION=0.1.60 \
  ONECONTEXT_NEW_VERSION=0.1.61 \
  ONECONTEXT_STAGING_APPCAST_URL=http://127.0.0.1:9/appcast.xml \
  "$ROOT/scripts/self-hosted-update-proof.sh" > "$TMP_DIR/destructive-guard.out" 2>&1; then
  echo "self-hosted update proof should refuse to run without the destructive guard." >&2
  exit 1
fi
grep -q "Refusing to mutate" "$TMP_DIR/destructive-guard.out"

mkdir -p "$TMP_DIR/Fake.app"
if ONECONTEXT_INSTALLED_APP="$TMP_DIR/Fake.app" \
  ONECONTEXT_EXPECTED_OLD_VERSION=0.1.60 \
  ONECONTEXT_EXPECTED_NEW_VERSION=0.1.61 \
  ONECONTEXT_EXPECTED_UPDATE_CLASS=surprise \
  "$ROOT/scripts/prove-remote-sparkle-update.sh" > "$TMP_DIR/update-class.out" 2>&1; then
  echo "remote Sparkle proof should reject unknown update classes." >&2
  exit 1
fi
grep -q "must be mandatory or optional" "$TMP_DIR/update-class.out"

grep -q "assert_no_unwanted_update_ui" "$ROOT/scripts/prove-remote-sparkle-update.sh"
grep -q "Installed old app SUFeedURL does not match the proof appcast" "$ROOT/scripts/self-hosted-update-proof.sh"
grep -q "ONECONTEXT_RELEASE_LOCKDOWN_EVIDENCE_REDACT" "$ROOT/scripts/collect-release-lockdown-evidence.sh"

echo "1Context upgrade path checks passed."
