#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_HOME="$(mktemp -d /tmp/1ctx-wiki-runtime-e2e-XXXXXX)"
PORT_FILE="$(mktemp /tmp/1ctx-wiki-runtime-e2e-port-XXXXXX)"
SERVER_LOG="$(mktemp /tmp/1ctx-wiki-runtime-e2e-server-XXXXXX.log)"
SERVER_PID=""

cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$RUNTIME_HOME"
  rm -f "$PORT_FILE" "$SERVER_LOG"
}
trap cleanup EXIT

render_site() {
  local source_root="$1"
  local output="$2"
  local result_json="$3"
  node "$ROOT/wiki-engine/tools/render-site.mjs" \
    --source-root "$source_root" \
    --output "$output" \
    --result-json "$result_json" >/tmp/1ctx-wiki-runtime-e2e-render.out
}

wait_for_server() {
  for _ in $(seq 1 100); do
    if [[ -s "$PORT_FILE" ]]; then
      return 0
    fi
    sleep 0.05
  done
  echo "wiki runtime e2e server did not publish a port" >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
}

assert_json_status() {
  local result_json="$1"
  local expected="$2"
  python3 - "$result_json" "$expected" <<'PY'
import json
import sys

path, expected = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as handle:
    payload = json.load(handle)
actual = payload.get("status")
if actual != expected:
    raise SystemExit(f"expected {path} status {expected!r}, got {actual!r}")
PY
}

assert_url_contains() {
  local url="$1"
  local pattern="$2"
  local out="$RUNTIME_HOME/http-response.txt"
  curl -fsS "$url" > "$out"
  grep -q "$pattern" "$out"
}

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_HOME" >/tmp/1ctx-wiki-runtime-e2e-init.out

SITE="$RUNTIME_HOME/1Context/user-wiki/site"
SOURCE="$RUNTIME_HOME/1Context/user-wiki/source"
TOPICS="$SOURCE/families/reference/topics/source/topics.md"

render_site "$SOURCE" "$SITE" "$RUNTIME_HOME/render-initial.json"
assert_json_status "$RUNTIME_HOME/render-initial.json" "published"

PORT_FILE="$PORT_FILE" node "$ROOT/scripts/serve-wiki-site.mjs" "$SITE" >"$SERVER_LOG" 2>&1 &
SERVER_PID="$!"
wait_for_server
PORT="$(cat "$PORT_FILE")"
BASE_URL="http://127.0.0.1:$PORT"

assert_url_contains "$BASE_URL/for-you" "For You"
assert_url_contains "$BASE_URL/topics/talk" "Talk"
assert_url_contains "$BASE_URL/topics.md" "Topics"

cat >> "$TOPICS" <<'EOF'

## Dev Runtime Refresh Proof

Dev runtime refresh marker.
EOF

render_site "$SOURCE" "$SITE" "$RUNTIME_HOME/render-refresh.json"
assert_json_status "$RUNTIME_HOME/render-refresh.json" "published"
assert_url_contains "$BASE_URL/topics" "Dev runtime refresh marker"

if render_site "$RUNTIME_HOME/missing-source" "$SITE" "$RUNTIME_HOME/render-failed.json"; then
  echo "render should fail when source root is missing" >&2
  exit 1
fi
assert_json_status "$RUNTIME_HOME/render-failed.json" "failed"
assert_url_contains "$BASE_URL/topics" "Dev runtime refresh marker"

echo "wiki runtime e2e proof passed."
