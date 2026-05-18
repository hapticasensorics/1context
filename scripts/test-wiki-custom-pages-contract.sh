#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="$(mktemp -d /tmp/1ctx-wiki-custom-runtime-XXXXXX)"
OUT_DIR="$(mktemp -d /tmp/1ctx-wiki-custom-output-XXXXXX)"
SERVER_LOG="$OUT_DIR/server.log"
PORT_FILE="$OUT_DIR/port"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$RUNTIME_TEST"
  rm -rf "$OUT_DIR"
}
trap cleanup EXIT

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" >/tmp/1ctx-wiki-custom-init.out

cat >>"$RUNTIME_TEST/1Context/user-wiki/wiki.toml" <<'TOML'

[[pages]]
id = "dummy-custom"
enabled = true
title = "Dummy Custom"
slug = "dummy-custom"
route = "/dummy-custom"
family_group = "custom"
family_group_title = "Custom"
family_id = "dummy-custom"
family_title = "Dummy Custom"
type = "context-page"
template = "pages/context-page.md"
talk_conventions_template = "talk/conventions.md"
summary = "Fixture custom page generated from the generic fallback template."
nav_order = 900
TOML

uv run python "$ROOT/scripts/materialize-wiki-pages.py" "$RUNTIME_TEST" >/tmp/1ctx-wiki-custom-materialize.out

CUSTOM_PAGE="$RUNTIME_TEST/1Context/user-wiki/source/families/custom/dummy-custom"
CUSTOM_SOURCE="$CUSTOM_PAGE/source/dummy-custom.md"
CUSTOM_TALK="$CUSTOM_PAGE/talk/dummy-custom.talk"
MATERIALIZE_STATE="$RUNTIME_TEST/Library/Application Support/1Context/setup/wiki-page-materialize.toml"

test -f "$CUSTOM_SOURCE"
test -f "$CUSTOM_TALK/_meta.yaml"
test -f "$CUSTOM_TALK/_conventions.md"
test -f "$CUSTOM_PAGE/templates/page.template.md"
test -f "$CUSTOM_PAGE/templates/talk/entry.template.md"

grep -q 'title: "Dummy Custom"' "$CUSTOM_SOURCE"
grep -q 'slug: "dummy-custom"' "$CUSTOM_SOURCE"
grep -q 'section: context' "$CUSTOM_SOURCE"
grep -q 'access: "private"' "$CUSTOM_SOURCE"
grep -q 'talk_url: "/dummy-custom/talk"' "$CUSTOM_SOURCE"
grep -q '# Dummy Custom' "$CUSTOM_SOURCE"
grep -q 'page_route: "/dummy-custom"' "$CUSTOM_TALK/_meta.yaml"
grep -q 'talk_route: "/dummy-custom/talk"' "$CUSTOM_TALK/_meta.yaml"
grep -q 'slug: "dummy-custom.talk"' "$CUSTOM_TALK/_meta.yaml"
if grep -R '{{' "$CUSTOM_SOURCE" "$CUSTOM_TALK" >/dev/null; then
  echo "custom page materialization left unresolved template placeholders" >&2
  exit 1
fi

printf '\nOperator edit: preserve custom page.\n' >>"$CUSTOM_SOURCE"
uv run python "$ROOT/scripts/materialize-wiki-pages.py" "$RUNTIME_TEST" >/tmp/1ctx-wiki-custom-rematerialize.out
grep -q 'Operator edit: preserve custom page.' "$CUSTOM_SOURCE"
grep -q 'path = "1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md"' "$MATERIALIZE_STATE"
grep -q 'status = "skipped_existing"' "$MATERIALIZE_STATE"

SITE_DIR="$OUT_DIR/site"
mkdir -p "$SITE_DIR"
(
  cd "$ROOT/wiki-engine"
  node tools/render-to-dir.mjs "$CUSTOM_SOURCE" "$SITE_DIR" >/tmp/1ctx-wiki-custom-render-source.out
  node tools/render-to-dir.mjs "$CUSTOM_TALK" "$SITE_DIR" >/tmp/1ctx-wiki-custom-render-talk.out
)

test -f "$SITE_DIR/dummy-custom.html"
test -f "$SITE_DIR/dummy-custom/index.html"
test -f "$SITE_DIR/dummy-custom.md"
test -f "$SITE_DIR/dummy-custom.talk.html"
test -f "$SITE_DIR/dummy-custom/talk/index.html"
test -f "$SITE_DIR/dummy-custom.talk.md"
grep -q '<h1>Dummy Custom</h1>' "$SITE_DIR/dummy-custom.html"
grep -q '<h1>Talk - Dummy Custom</h1>' "$SITE_DIR/dummy-custom.talk.html"
if grep -q '<h1>Your Context</h1>' "$SITE_DIR/dummy-custom.html"; then
  echo "custom page rendered with default-page article content" >&2
  exit 1
fi

PORT_FILE="$PORT_FILE" node "$ROOT/scripts/serve-wiki-site.mjs" "$SITE_DIR" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
for _ in {1..100}; do
  [[ -f "$PORT_FILE" ]] && break
  sleep 0.05
done
if [[ ! -f "$PORT_FILE" ]]; then
  echo "custom page route server did not start" >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
fi

BASE_URL="http://127.0.0.1:$(cat "$PORT_FILE")"
curl -fsS "$BASE_URL/dummy-custom" -o "$OUT_DIR/dummy-custom.html"
grep -q '<h1>Dummy Custom</h1>' "$OUT_DIR/dummy-custom.html"
curl -fsS "$BASE_URL/dummy-custom/talk" -o "$OUT_DIR/dummy-custom-talk.html"
grep -q '<h1>Talk - Dummy Custom</h1>' "$OUT_DIR/dummy-custom-talk.html"
curl -fsS "$BASE_URL/dummy-custom.md" -o "$OUT_DIR/dummy-custom.md"
grep -q 'Operator edit: preserve custom page.' "$OUT_DIR/dummy-custom.md"
curl -fsS "$BASE_URL/dummy-custom.talk.md" -o "$OUT_DIR/dummy-custom.talk.md"
grep -q 'Talk - Dummy Custom' "$OUT_DIR/dummy-custom.talk.md"
missing_status="$(curl -sS -o /tmp/1ctx-wiki-custom-missing.out -w '%{http_code}' "$BASE_URL/not-configured")"
if [[ "$missing_status" != "404" ]]; then
  echo "unconfigured custom route should return 404, got $missing_status" >&2
  cat /tmp/1ctx-wiki-custom-missing.out >&2 || true
  exit 1
fi
if grep -q '<h1>Your Context</h1>' /tmp/1ctx-wiki-custom-missing.out; then
  echo "unconfigured custom route fell back to Your Context" >&2
  exit 1
fi

echo "wiki custom pages contract passed."
