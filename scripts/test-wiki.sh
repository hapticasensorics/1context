#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="$(mktemp -d /tmp/1ctx-wiki-runtime-XXXXXX)"
OUT_DIR="$(mktemp -d /tmp/1ctx-wiki-render-XXXXXX)"
ARTIFACT_DIR="${ONECONTEXT_WIKI_ARTIFACT_DIR:-$(mktemp -d /tmp/1ctx-wiki-artifacts-XXXXXX)}"

resolve_wiki_core_bin() {
  if [[ -n "${ONECONTEXT_WIKI_CORE_BIN:-}" ]]; then
    printf '%s\n' "$ONECONTEXT_WIKI_CORE_BIN"
    return
  fi

  local debug_bin="$ROOT/target/debug/onecontext-wiki"
  if [[ ! -x "$debug_bin" ]] || find "$ROOT/crates" -name '*.rs' -newer "$debug_bin" -print -quit | grep -q .; then
    cargo build --package onecontext-wiki-daemon >/dev/null
  fi
  printf '%s\n' "$debug_bin"
}

cleanup() {
  rm -rf "$RUNTIME_TEST" "$OUT_DIR"
  if [[ -z "${ONECONTEXT_WIKI_ARTIFACT_DIR:-}" ]]; then
    rm -rf "$ARTIFACT_DIR"
  fi
}
trap cleanup EXIT

mkdir -p "$ARTIFACT_DIR"

if [[ ! -d "$ROOT/wiki-engine/node_modules" ]]; then
  npm ci --prefix "$ROOT/wiki-engine" >/dev/null
fi

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" >"$ARTIFACT_DIR/init-runtime.out"

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
talk_curator_template = "talk/curators/topics.md"
summary = "Fixture custom page created from the context-page template."
nav_order = 900
TOML

WIKI_CORE_BIN="$(resolve_wiki_core_bin)"
"$WIKI_CORE_BIN" --root "$RUNTIME_TEST/1Context" page-create dummy-custom \
  >"$ARTIFACT_DIR/create-custom-page.json"

CUSTOM_SOURCE="$RUNTIME_TEST/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md"
CUSTOM_TALK_META="$RUNTIME_TEST/1Context/user-wiki/source/families/custom/dummy-custom/talk/dummy-custom.talk/_meta.yaml"
test -f "$CUSTOM_SOURCE"
test -f "$CUSTOM_TALK_META"
grep -q 'title: "Dummy Custom"' "$CUSTOM_SOURCE"
grep -q 'talk_route: "/dummy-custom/talk"' "$CUSTOM_TALK_META"
if grep -R '{{' "$CUSTOM_SOURCE" "$CUSTOM_TALK_META" >/dev/null; then
  echo "custom page creation left unresolved template placeholders" >&2
  exit 1
fi

SITE_DIR="$OUT_DIR/site"
RESULT_JSON="$OUT_DIR/render-result.json"
node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$RUNTIME_TEST/1Context/user-wiki/source" \
  --output "$SITE_DIR" \
  --result-json "$RESULT_JSON" \
  >"$ARTIFACT_DIR/render-site.out"

python3 - "$RESULT_JSON" "$SITE_DIR/.1context/route-manifest.json" "$SITE_DIR" <<'PY'
import json
import sys
from pathlib import Path

result = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
manifest = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
site = Path(sys.argv[3])

if result.get("status") != "published":
    raise SystemExit(f"wiki render did not publish: {result.get('status')}")

routes = {entry.get("route") for entry in manifest.get("routes", [])}
for route in [
    "/for-you",
    "/for-you/talk",
    "/your-context",
    "/your-context/talk",
    "/projects",
    "/projects/talk",
    "/topics",
    "/topics/talk",
    "/dummy-custom",
    "/dummy-custom/talk",
]:
    if route not in routes:
        raise SystemExit(f"missing rendered wiki route: {route}")

dot_talk_routes = sorted(route for route in routes if ".talk" in route)
if dot_talk_routes:
    raise SystemExit(f"dot-talk routes should not be emitted: {dot_talk_routes}")

for path in ["dummy-custom.html", "dummy-custom.md", "dummy-custom.talk.html", "dummy-custom.talk.md"]:
    if not (site / path).is_file():
        raise SystemExit(f"missing rendered file: {path}")

if int(result.get("markdown_twin_count") or 0) < 10:
    raise SystemExit("render result did not include expected markdown twins")
PY

echo "wiki_artifacts=$ARTIFACT_DIR"
