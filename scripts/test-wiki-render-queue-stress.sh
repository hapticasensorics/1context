#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="$(mktemp /tmp/1ctx-wiki-render-queue-stress-XXXXXX.log)"

cleanup() {
  rm -f "$LOG"
}
trap cleanup EXIT

swift test --package-path "$ROOT/macos" --filter WikiRenderQueueTests | tee "$LOG"

grep -q "testRapidRequestsAreSingleFlightAndCoalesced" "$LOG"
grep -q "testFailureBackoffDelaysAutomaticButManualRunsImmediately" "$LOG"
grep -q "testRecordsDurationsDirtyPagesAndSkipReason" "$LOG"
grep -q "Executed 3 tests, with 0 failures" "$LOG"

echo "wiki render queue stress proof passed."

