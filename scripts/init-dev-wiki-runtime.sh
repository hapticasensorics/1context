#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="${1:-$ROOT/runtime-test}"
LOCAL_USER_DATA_SOURCE="${2:-}"

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

copy_missing_tree() {
  local source="$1"
  local destination="$2"

  [[ -d "$source" ]] || return 0
  mkdir -p "$destination"
  rsync -a --ignore-existing \
    --exclude='.DS_Store' \
    --exclude='.gitkeep' \
    "$source/" "$destination/"
}

ensure_runtime_dirs() {
  mkdir -p \
    "$RUNTIME_TEST/1Context/user-wiki/source" \
    "$RUNTIME_TEST/1Context/user-wiki/site/.1context" \
    "$RUNTIME_TEST/1Context/context-engine/indexes" \
    "$RUNTIME_TEST/1Context/context-engine/orchestrators" \
    "$RUNTIME_TEST/1Context/context-engine/packs" \
    "$RUNTIME_TEST/Library/Application Support/1Context/wiki-site/current" \
    "$RUNTIME_TEST/Library/Application Support/1Context/wiki-site/next" \
    "$RUNTIME_TEST/Library/Application Support/1Context/wiki-site/previous" \
    "$RUNTIME_TEST/Library/Application Support/1Context/indexes" \
    "$RUNTIME_TEST/Library/Application Support/1Context/local-web/caddy" \
    "$RUNTIME_TEST/Library/Application Support/1Context/setup" \
    "$RUNTIME_TEST/Library/Application Support/1Context/sockets" \
    "$RUNTIME_TEST/Library/Application Support/1Context/staging" \
    "$RUNTIME_TEST/Library/Application Support/1Context/run" \
    "$RUNTIME_TEST/Library/Logs/1Context" \
    "$RUNTIME_TEST/Library/Caches/1Context"
}

if ! command -v rsync >/dev/null 2>&1; then
  echo "rsync is required to initialize the dev wiki runtime." >&2
  exit 1
fi

if [[ -n "$LOCAL_USER_DATA_SOURCE" ]]; then
  if [[ ! -d "$LOCAL_USER_DATA_SOURCE/1Context" && ! -d "$LOCAL_USER_DATA_SOURCE/Library" ]]; then
    echo "Local fixture must contain 1Context/ or Library/: $LOCAL_USER_DATA_SOURCE" >&2
    exit 1
  fi
fi

ensure_runtime_dirs
copy_missing_tree "$ROOT/runtime/1Context" "$RUNTIME_TEST/1Context"
copy_missing_tree "$ROOT/runtime/Library" "$RUNTIME_TEST/Library"

if [[ -n "$LOCAL_USER_DATA_SOURCE" ]]; then
  copy_missing_tree "$LOCAL_USER_DATA_SOURCE/1Context" "$RUNTIME_TEST/1Context"
  copy_missing_tree "$LOCAL_USER_DATA_SOURCE/Library" "$RUNTIME_TEST/Library"
fi

if [[ "${ONECONTEXT_SKIP_WIKI_MATERIALIZE:-0}" != "1" ]]; then
  WIKI_CORE_BIN="$(resolve_wiki_core_bin)"
  "$WIKI_CORE_BIN" --root "$RUNTIME_TEST/1Context" page-create-all
fi

printf 'runtime_test=%s\n' "$RUNTIME_TEST"
