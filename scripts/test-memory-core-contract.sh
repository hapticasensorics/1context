#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
STATE_DIR="$(mktemp -d /tmp/1ctx-memory-core-contract-XXXXXX)"
FIXTURE="$ROOT/scripts/fixtures/memory-core-fixture.sh"

cleanup() {
  rm -rf "$STATE_DIR"
}
trap cleanup EXIT

if [[ ! -x "$FIXTURE" ]]; then
  echo "memory-core fixture is not executable: $FIXTURE" >&2
  exit 1
fi

if [[ -z "${BIN_DIR:-}" ]]; then
  swift build --package-path "$MACOS_DIR" >/dev/null
  BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"
fi

export ONECONTEXT_APP_SUPPORT_DIR="$STATE_DIR/Application Support/1Context"
export ONECONTEXT_USER_CONTENT_DIR="$STATE_DIR/1Context"
export ONECONTEXT_LAUNCH_AGENT_DISABLED=1
export ONECONTEXT_LOG_DIR="$STATE_DIR/Logs/1Context"
export ONECONTEXT_CACHE_DIR="$STATE_DIR/Caches/1Context"
export ONECONTEXT_UPDATE_STATE_DIR="$STATE_DIR/Application Support/1Context/update"
export ONECONTEXT_NO_UPDATE_CHECK=1

CLI="$BIN_DIR/1context"

json_file="$STATE_DIR/result.json"

assert_json() {
  "$CLI" memory-core run -- "$@" >"$json_file"
  /usr/bin/python3 -m json.tool <"$json_file" >/dev/null
}

"$CLI" memory-core status | grep -q "Health: not configured"
"$CLI" memory-core configure --executable "$FIXTURE" | grep -q "Health: ok"
test "$(stat -f "%Lp" "$ONECONTEXT_APP_SUPPORT_DIR/memory-core")" = "700"
test "$(stat -f "%Lp" "$ONECONTEXT_APP_SUPPORT_DIR/memory-core/config.json")" = "600"
"$CLI" memory-core doctor | grep -q "Health: ok"

assert_json status --json
grep -q '"capabilities"' "$json_file"

assert_json storage init --json
grep -q '"created"' "$json_file"

assert_json wiki list --json
grep -q '"wikis"' "$json_file"

assert_json wiki ensure --json
grep -q '"ensured"' "$json_file"

assert_json wiki render --json
grep -q '"rendered"' "$json_file"

assert_json wiki routes --json
grep -q '"routes"' "$json_file"

assert_json memory tick --wiki-only --json
grep -q '"wiki-only"' "$json_file"

assert_json memory replay-dry-run --json
grep -q '"dry_run"' "$json_file"

assert_json memory cycles list --json
grep -q '"cycles"' "$json_file"

assert_json memory cycles show --json
grep -q '"cycle"' "$json_file"

assert_json memory cycles validate --json
grep -q '"valid"' "$json_file"

if "$CLI" memory-core run -- hired-agent run >"$STATE_DIR/disallowed.out" 2>&1; then
  echo "disallowed memory-core command should fail" >&2
  exit 1
fi
grep -q "not allowed" "$STATE_DIR/disallowed.out"

"$CLI" memory-core configure --clear | grep -q "Health: not configured"

echo "Memory core contract tests passed."
