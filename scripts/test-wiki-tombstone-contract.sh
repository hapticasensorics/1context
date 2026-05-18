#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="$(mktemp -d /tmp/1ctx-wiki-tombstone-runtime-XXXXXX)"
OUT_DIR="$(mktemp -d /tmp/1ctx-wiki-tombstone-output-XXXXXX)"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$RUNTIME_TEST"
  rm -rf "$OUT_DIR"
}
trap cleanup EXIT

ONECONTEXT_SKIP_WIKI_MATERIALIZE=1 "$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" >/tmp/1ctx-wiki-tombstone-init.out

TOPICS_ROOT="$RUNTIME_TEST/1Context/user-wiki/source/families/reference/topics"
TOPICS_SOURCE="$TOPICS_ROOT/source/topics.md"
TOPICS_TALK="$TOPICS_ROOT/talk/topics.talk"
TOPICS_TOMBSTONE="$TOPICS_ROOT/source/topics.tombstone.toml"
mkdir -p "$(dirname "$TOPICS_TOMBSTONE")"
cat >"$TOPICS_TOMBSTONE" <<'EOF'
schema_version = 1
status = "tombstoned"
reason = "operator_removed_page"
created_at = "2026-05-14T00:00:00Z"
EOF

python3 "$ROOT/scripts/materialize-wiki-pages.py" "$RUNTIME_TEST" >/tmp/1ctx-wiki-tombstone-materialize.out

MATERIALIZE_STATE="$RUNTIME_TEST/Library/Application Support/1Context/setup/wiki-page-materialize.toml"
test -f "$MATERIALIZE_STATE"
grep -q 'id = "topics"' "$MATERIALIZE_STATE"
grep -q 'status = "tombstoned"' "$MATERIALIZE_STATE"
grep -q 'topics.tombstone.toml' "$MATERIALIZE_STATE"
test ! -f "$TOPICS_SOURCE"
test ! -d "$TOPICS_TALK"

SITE_OUT="$OUT_DIR/site"
RESULT_JSON="$OUT_DIR/render-result.json"
node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$RUNTIME_TEST/1Context/user-wiki/source" \
  --output "$SITE_OUT" \
  --result-json "$RESULT_JSON" \
  >/tmp/1ctx-wiki-tombstone-render.out

test ! -f "$SITE_OUT/topics.html"
test ! -f "$SITE_OUT/topics.md"
test ! -f "$SITE_OUT/topics.talk.html"
! grep -q '"/topics"' "$SITE_OUT/.1context/route-manifest.json"

PORT="$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"
PORT="$PORT" node "$ROOT/scripts/serve-wiki-site.mjs" "$SITE_OUT" >/tmp/1ctx-wiki-tombstone-server.out 2>&1 &
SERVER_PID=$!
for _ in {1..30}; do
  if curl -sS "http://127.0.0.1:$PORT/__not_ready__" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    cat /tmp/1ctx-wiki-tombstone-server.out >&2 || true
    exit 1
  fi
  sleep 0.1
done

STATUS="$(curl -sS -o "$OUT_DIR/topics.html" -w "%{http_code}" "http://127.0.0.1:$PORT/topics")"
test "$STATUS" = "404"
! grep -q 'Your Context' "$OUT_DIR/topics.html"

echo "wiki tombstone contract passed."
