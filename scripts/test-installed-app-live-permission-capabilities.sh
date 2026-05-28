#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage:
  scripts/test-installed-app-live-permission-capabilities.sh

Environment:
  ONECONTEXT_APP                         Timestamped dev permission-test app bundle to verify. Required.
  ONECONTEXT_CAPABILITY_EVIDENCE_DIR     Defaults to dist/live-permission-capability-evidence/<timestamp>.
  ONECONTEXT_PERMISSION_PROBE_TIMEOUT    Per-capability timeout in seconds. Defaults to 5.
  ONECONTEXT_INCLUDE_BROWSER_EXTENSION=1 Include the dev browser extension probe. Defaults to skipped.

Runs the installed timestamped dev app executable in hidden one-shot probe mode,
using the app bundle's signed TCC identity. This is the only live TCC probe path
and intentionally rejects stable dev or production bundle identifiers.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

fail() {
  echo "live permission capability probe failed: $*" >&2
  exit 1
}

[[ -n "${ONECONTEXT_APP:-}" ]] || fail "ONECONTEXT_APP is required and must point at a timestamped dev permission-test app"
APP="$ONECONTEXT_APP"
APP="${APP%/}"
MAIN_EXE="$APP/Contents/MacOS/1Context"
INFO="$APP/Contents/Info.plist"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_CAPABILITY_EVIDENCE_DIR:-$ROOT/dist/live-permission-capability-evidence/$STAMP}"
REPORT="$EVIDENCE_DIR/report.json"
STDOUT="$EVIDENCE_DIR/stdout.txt"
STDERR="$EVIDENCE_DIR/stderr.txt"
TIMEOUT="${ONECONTEXT_PERMISSION_PROBE_TIMEOUT:-5}"

[[ -d "$APP" ]] || fail "app bundle not found: $APP"
[[ -x "$MAIN_EXE" ]] || fail "app executable not found or not executable: $MAIN_EXE"
[[ -f "$INFO" ]] || fail "Info.plist not found: $INFO"
mkdir -p "$EVIDENCE_DIR"

BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$INFO")"
[[ "$BUNDLE_ID" =~ ^com\.haptica\.1context\.dev\.permission\.[a-z0-9][a-z0-9-]{0,39}$ ]] \
  || fail "expected timestamped dev permission-test bundle id, got: $BUNDLE_ID"

printf '%s\n' "$BUNDLE_ID" >"$EVIDENCE_DIR/bundle-identifier.txt"
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$INFO" >"$EVIDENCE_DIR/version.txt"
codesign -dv --verbose=4 "$APP" >"$EVIDENCE_DIR/codesign.txt" 2>&1 || true
codesign -d --entitlements :- "$APP" >"$EVIDENCE_DIR/entitlements.plist" 2>"$EVIDENCE_DIR/entitlements.err" || true

args=(--permission-capability-probe --json "$REPORT" --timeout "$TIMEOUT")
if [[ "${ONECONTEXT_INCLUDE_BROWSER_EXTENSION:-0}" == "1" ]]; then
  args+=(--include-browser-extension)
fi

set +e
"$MAIN_EXE" "${args[@]}" >"$STDOUT" 2>"$STDERR"
status=$?
set -e

[[ -f "$REPORT" ]] || {
  [[ -s "$STDOUT" ]] && cat "$STDOUT" >&2
  [[ -s "$STDERR" ]] && cat "$STDERR" >&2
  fail "probe did not write a JSON report at $REPORT"
}

python3 - "$REPORT" "$status" <<'PY'
import json
import sys
from pathlib import Path

report = Path(sys.argv[1])
status = int(sys.argv[2])
payload = json.loads(report.read_text())
checks = payload.get("checks", [])
failed = [check for check in checks if check.get("status") == "failed"]
skipped = [check for check in checks if check.get("status") == "skipped"]
print(f"overall_status={payload.get('overallStatus')}")
for check in checks:
    print(f"{check.get('id')}={check.get('status')} :: {check.get('detail')}")
if failed or status != 0:
    print("", file=sys.stderr)
    print("Failed capability checks:", file=sys.stderr)
    for check in failed:
        print(f"- {check.get('title')}: {check.get('detail')}", file=sys.stderr)
    print(f"Evidence: {report}", file=sys.stderr)
    sys.exit(1)
if skipped:
    print("")
    print("Skipped capability checks:")
    for check in skipped:
        print(f"- {check.get('title')}: {check.get('detail')}")
PY

echo "Live permission capability probe passed."
echo "Evidence: $REPORT"
