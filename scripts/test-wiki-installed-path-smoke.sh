#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${ONECONTEXT_APP_UNDER_TEST:-$ROOT/dist/1Context.app}"
RUNTIME_DEFAULTS="$APP/Contents/Resources/RuntimeDefaults/1Context"
RUNTIME_DEFAULTS_MANIFEST="$RUNTIME_DEFAULTS/.1context/runtime-defaults-manifest.json"
HOME_FIXTURE="$(mktemp -d /tmp/1ctx-wiki-installed-home-XXXXXX)"
PORT_FILE="$(mktemp /tmp/1ctx-wiki-installed-port-XXXXXX)"
SERVER_LOG="$(mktemp /tmp/1ctx-wiki-installed-server-XXXXXX.log)"
SERVER_PID=""

cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$HOME_FIXTURE"
  rm -f "$PORT_FILE" "$SERVER_LOG"
}
trap cleanup EXIT

assert_url_contains() {
  local url="$1"
  local pattern="$2"
  local out="$HOME_FIXTURE/http-response.txt"
  curl -fsS "$url" > "$out"
  grep -q "$pattern" "$out"
}

if [[ ! -d "$APP" ]]; then
  echo "Missing app bundle under test: $APP" >&2
  echo "Run ./scripts/package-macos-smoke.sh first." >&2
  exit 1
fi
if [[ ! -d "$RUNTIME_DEFAULTS" ]]; then
  echo "Packaged app is missing RuntimeDefaults/1Context." >&2
  exit 1
fi

test -f "$RUNTIME_DEFAULTS/user-wiki/wiki.toml"
test -f "$RUNTIME_DEFAULTS/user-wiki/site/.1context/route-manifest.json"
test -f "$RUNTIME_DEFAULTS_MANIFEST"
test -f "$APP/Contents/Resources/WikiEngine/tools/render-site.mjs"
test -d "$APP/Contents/Resources/WikiEngine/node_modules/gray-matter"
test -d "$APP/Contents/Resources/WikiEngine/node_modules/marked"
test ! -d "$APP/Contents/Resources/WikiEngine/node_modules/.bin"
test ! -f "$APP/Contents/Resources/WikiEngine/package-lock.json"
test ! -f "$APP/Contents/Resources/WikiEngine/node_modules/.package-lock.json"
test ! -d "$APP/Contents/Resources/memory-core"
test ! -e "$APP/Contents/Resources/memory-runtime"

python3 - "$RUNTIME_DEFAULTS/user-wiki/site/.1context/route-manifest.json" "$RUNTIME_DEFAULTS_MANIFEST" <<'PY'
import json
import re
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)
if payload.get("output") != "site://.":
    raise SystemExit("packaged route manifest must use portable site://. output")
routes = {entry.get("route") for entry in payload.get("routes", [])}
for expected in ["/for-you", "/your-context", "/projects", "/topics", "/topics/talk"]:
    if expected not in routes:
        raise SystemExit(f"missing packaged route: {expected}")

with open(sys.argv[2], "r", encoding="utf-8") as handle:
    manifest = json.load(handle)
if manifest.get("schema_version") != "1context.runtime-defaults-manifest.v1":
    raise SystemExit("runtime defaults manifest schema mismatch")
for key, value in (manifest.get("hashes") or {}).items():
    if key in {"runtime_defaults_source", "runtime_defaults_site", "wiki_engine"}:
        if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
            raise SystemExit(f"runtime defaults manifest has invalid hash: {key}")
render = manifest.get("render_result") or {}
if render.get("status") != "published":
    raise SystemExit("runtime defaults manifest did not record a successful render")
if render.get("route_count") != payload.get("route_count"):
    raise SystemExit("runtime defaults manifest route count does not match route manifest")
PY

while IFS= read -r -d '' directory; do
  mkdir -p "$HOME_FIXTURE/1Context/${directory#"$RUNTIME_DEFAULTS/"}"
done < <(find "$RUNTIME_DEFAULTS" -type d -print0)

while IFS= read -r -d '' source; do
  rel="${source#"$RUNTIME_DEFAULTS/"}"
  dest="$HOME_FIXTURE/1Context/$rel"
  mkdir -p "$(dirname "$dest")"
  cp -p "$source" "$dest"
done < <(find "$RUNTIME_DEFAULTS" -type f ! -name '.DS_Store' -print0)

SITE="$HOME_FIXTURE/1Context/user-wiki/site"
test -f "$SITE/topics.html"
test -f "$SITE/topics.talk.html"
if grep -R -I -n -E '/Users/|runtime-test|1context-private|memory-core/wiki|node_modules' "$HOME_FIXTURE/1Context" >/tmp/1ctx-installed-path-leaks.txt; then
  cat /tmp/1ctx-installed-path-leaks.txt >&2
  echo "installed-path runtime defaults contain forbidden local/source checkout fragments" >&2
  exit 1
fi

RENDER_PROOF="$HOME_FIXTURE/render-proof"
node "$APP/Contents/Resources/WikiEngine/tools/render-site.mjs" \
  --source-root "$HOME_FIXTURE/1Context/user-wiki/source" \
  --output "$RENDER_PROOF/site" \
  --result-json "$RENDER_PROOF/render-site-result.json" >/dev/null
python3 - "$RENDER_PROOF/render-site-result.json" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)
if payload.get("status") != "published":
    raise SystemExit("packaged WikiEngine render proof failed")
if payload.get("route_count", 0) < 5:
    raise SystemExit("packaged WikiEngine render proof route count is too small")
PY

mkdir -p "$HOME_FIXTURE/Library/Application Support/1Context/wiki-site/current"
cp -R "$SITE"/. "$HOME_FIXTURE/Library/Application Support/1Context/wiki-site/current/"

PORT_FILE="$PORT_FILE" node "$ROOT/scripts/serve-wiki-site.mjs" "$HOME_FIXTURE/Library/Application Support/1Context/wiki-site/current" >"$SERVER_LOG" 2>&1 &
SERVER_PID="$!"
for _ in $(seq 1 100); do
  [[ -s "$PORT_FILE" ]] && break
  sleep 0.05
done
if [[ ! -s "$PORT_FILE" ]]; then
  echo "installed-path server did not publish a port" >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
fi
BASE_URL="http://127.0.0.1:$(cat "$PORT_FILE")"

assert_url_contains "$BASE_URL/for-you" "For You"
assert_url_contains "$BASE_URL/topics/talk" "Talk"
assert_url_contains "$BASE_URL/your-context.md" "Your Context"

echo "wiki installed-path smoke passed."
