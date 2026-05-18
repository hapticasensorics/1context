#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="${1:-$ROOT/runtime-test}"
LOCAL_USER_DATA_SOURCE="${2:-}"

USER_WIKI="$RUNTIME_TEST/1Context/user-wiki"
CONTEXT_ENGINE="$RUNTIME_TEST/1Context/context-engine"
APP_SUPPORT="$RUNTIME_TEST/Library/Application Support/1Context"
IMPORT_STATE="$APP_SUPPORT/setup/dev-user-data-import.toml"
LOGS="$RUNTIME_TEST/Library/Logs/1Context"
CACHES="$RUNTIME_TEST/Library/Caches/1Context"

if [[ -n "$LOCAL_USER_DATA_SOURCE" && ! -d "$LOCAL_USER_DATA_SOURCE" ]]; then
  echo "Missing local user-data source: $LOCAL_USER_DATA_SOURCE" >&2
  exit 1
fi

if [[ -n "$LOCAL_USER_DATA_SOURCE" ]]; then
  if [[ ! -d "$LOCAL_USER_DATA_SOURCE/1Context" && ! -d "$LOCAL_USER_DATA_SOURCE/Library" ]]; then
    echo "Local user-data source must be shaped like runtime-test/: expected 1Context/ or Library/ under $LOCAL_USER_DATA_SOURCE" >&2
    exit 1
  fi
fi

toml_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '%s' "$value"
}

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

ensure_dirs() {
  mkdir -p \
    "$USER_WIKI/source" \
    "$USER_WIKI/site/.1context" \
    "$CONTEXT_ENGINE/agents" \
    "$CONTEXT_ENGINE/agents/roles" \
    "$CONTEXT_ENGINE/agents/tools" \
    "$CONTEXT_ENGINE/agents/policies" \
    "$CONTEXT_ENGINE/jobs" \
    "$CONTEXT_ENGINE/prompts" \
    "$CONTEXT_ENGINE/prompts/shared" \
    "$CONTEXT_ENGINE/inbox" \
    "$CONTEXT_ENGINE/proposals" \
    "$CONTEXT_ENGINE/decisions" \
    "$CONTEXT_ENGINE/runs" \
    "$CONTEXT_ENGINE/artifacts" \
    "$CONTEXT_ENGINE/observations" \
    "$CONTEXT_ENGINE/ledgers" \
    "$CONTEXT_ENGINE/indexes" \
    "$APP_SUPPORT/wiki-site/current" \
    "$APP_SUPPORT/wiki-site/next" \
    "$APP_SUPPORT/wiki-site/previous" \
    "$APP_SUPPORT/indexes/lancedb" \
    "$APP_SUPPORT/local-web/caddy" \
    "$APP_SUPPORT/setup" \
    "$APP_SUPPORT/sockets" \
    "$APP_SUPPORT/staging" \
    "$APP_SUPPORT/run" \
    "$LOGS" \
    "$CACHES"
}

copy_runtime_tree_directories() {
  for runtime_root in "$ROOT/runtime/1Context" "$ROOT/runtime/Library"; do
    [[ -d "$runtime_root" ]] || continue
    while IFS= read -r -d '' directory; do
      mkdir -p "$RUNTIME_TEST/${directory#"$ROOT/runtime/"}"
    done < <(find "$runtime_root" -type d -print0)
  done
}

copy_runtime_defaults() {
  for runtime_root in "$ROOT/runtime/1Context" "$ROOT/runtime/Library"; do
    [[ -d "$runtime_root" ]] || continue
    while IFS= read -r -d '' source; do
      local rel="${source#"$ROOT/runtime/"}"
      local dest="$RUNTIME_TEST/$rel"
      mkdir -p "$(dirname "$dest")"
      [[ -f "$dest" ]] || cp -p "$source" "$dest"
    done < <(find "$runtime_root" -type f ! -name '.DS_Store' ! -name '.gitkeep' -print0 | sort -z)
  done
}

install_local_file() {
  local source="$1"
  local rel="$2"
  local dest="$RUNTIME_TEST/$rel"
  local status="installed"

  mkdir -p "$(dirname "$dest")"
  if [[ -f "$dest" ]]; then
    if cmp -s "$source" "$dest"; then
      status="unchanged"
    else
      status="skipped_modified"
    fi
  fi

  if [[ "$status" == "installed" ]]; then
    cp -p "$source" "$dest"
  fi

  local source_hash
  local installed_hash
  source_hash="$(sha256_file "$source")"
  installed_hash="$(sha256_file "$dest")"

  {
    printf '\n[[files]]\n'
    printf 'path = "%s"\n' "$(toml_escape "$rel")"
    printf 'source_hash = "%s"\n' "$source_hash"
    printf 'installed_hash = "%s"\n' "$installed_hash"
    printf 'status = "%s"\n' "$status"
  } >> "$STATE_TMP"

  printf '%s %s\n' "$status" "$rel"
}

import_local_user_data() {
  [[ -n "$LOCAL_USER_DATA_SOURCE" ]] || return 0

  while IFS= read -r -d '' source; do
    local rel="${source#"$LOCAL_USER_DATA_SOURCE/"}"
    case "$rel" in
      1Context/*|Library/*) ;;
      *)
        echo "Refusing non-runtime user-data path from fixture: $rel" >&2
        exit 1
        ;;
    esac
    install_local_file "$source" "$rel"
  done < <(find "$LOCAL_USER_DATA_SOURCE" -type f ! -name '.DS_Store' -print0 | sort -z)
}

copy_runtime_tree_directories
ensure_dirs

if [[ -n "$LOCAL_USER_DATA_SOURCE" ]]; then
  STATE_TMP="$(mktemp /tmp/1ctx-dev-user-data-XXXXXX)"
  trap 'rm -f "$STATE_TMP"' EXIT

  {
    printf 'schema_version = 1\n'
    printf 'imported_at = "%s"\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    printf 'source_root = "%s"\n' "$(toml_escape "$LOCAL_USER_DATA_SOURCE")"
  } > "$STATE_TMP"

  import_local_user_data
  cp -p "$STATE_TMP" "$IMPORT_STATE"
fi

copy_runtime_defaults

if [[ "${ONECONTEXT_SKIP_WIKI_MATERIALIZE:-0}" != "1" ]]; then
  python3 "$ROOT/scripts/materialize-wiki-pages.py" "$RUNTIME_TEST"
fi

printf 'runtime_test=%s\n' "$RUNTIME_TEST"
if [[ -n "$LOCAL_USER_DATA_SOURCE" ]]; then
  printf 'dev_user_data_import_state=%s\n' "$IMPORT_STATE"
fi
