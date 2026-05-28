#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCES="${ONECONTEXT_MEMORY_E2E_SOURCES:-codex,claude,imessage}"
MAX_EVENTS="${ONECONTEXT_MEMORY_E2E_MAX_EVENTS:-75}"
MAX_LINES="${ONECONTEXT_MEMORY_E2E_MAX_LINES:-20000}"
CONTEXT_ENGINE_ROOT="${ONECONTEXT_MEMORY_E2E_CONTEXT_ENGINE_ROOT:-$HOME/1Context-Dev/context-engine}"
RUN_DIR="${ONECONTEXT_MEMORY_E2E_RUN_DIR:-$HOME/Library/Application Support/1Context Dev/run}"
VIEWER_URL="${ONECONTEXT_MEMORY_E2E_VIEWER_URL:-http://localhost:39291/memory}"
HEALTH_URL="${ONECONTEXT_MEMORY_E2E_HEALTH_URL:-http://localhost:39291/api/wiki/health}"
APP_PATH="${ONECONTEXT_MEMORY_E2E_APP_PATH:-/Applications/1Context Dev.app}"
BUILD_APP="${ONECONTEXT_MEMORY_E2E_BUILD_APP:-1}"

usage() {
  cat <<'USAGE'
Usage: scripts/test-memory-local-web-e2e.sh

End-to-end dev harness for the local memory viewer:
  1. starts/provisions the dev memory DB with the current schema,
  2. runs one bounded onecontext-memoryd ingest tick to write sample local adapter data,
  3. optionally builds/installs/opens 1Context Dev when ONECONTEXT_MEMORY_E2E_BUILD_APP=1,
  4. opens the local /memory viewer.

Environment:
  ONECONTEXT_MEMORY_E2E_SOURCES       default: codex,claude,imessage
  ONECONTEXT_MEMORY_E2E_MAX_EVENTS    default: 75
  ONECONTEXT_MEMORY_E2E_MAX_LINES     default: 20000
  ONECONTEXT_MEMORY_E2E_BUILD_APP     default: 1
  ONECONTEXT_MEMORY_E2E_VIEWER_URL    default: http://localhost:39291/memory
  ONECONTEXT_MEMORY_E2E_HEALTH_URL    default: http://localhost:39291/api/wiki/health
USAGE
}

wait_for_local_web() {
  echo "==> Waiting for local web API: $HEALTH_URL"
  for _ in $(seq 1 40); do
    if curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  echo "local web API did not become healthy at $HEALTH_URL" >&2
  return 1
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

cd "$ROOT"

echo "==> Starting dev memory DB"
"$ROOT/scripts/memory-db-dev.sh" start
"$ROOT/scripts/memory-db-dev.sh" provision
MEMORY_DB_URL="$("$ROOT/scripts/memory-db-dev.sh" url)"

echo "==> Building onecontext-memoryd"
cargo build -q -p onecontext-memory-db --bin onecontext-memoryd

echo "==> Writing bounded sample data through onecontext-memoryd"
mkdir -p "$CONTEXT_ENGINE_ROOT" "$RUN_DIR"
ONECONTEXT_MEMORY_DB_URL="$MEMORY_DB_URL" \
  "$ROOT/target/debug/onecontext-memoryd" daemon \
  --once \
  --context-engine-root "$CONTEXT_ENGINE_ROOT" \
  --run-dir "$RUN_DIR" \
  --sources "$SOURCES" \
  --max-events "$MAX_EVENTS" \
  --max-lines "$MAX_LINES" \
  --no-audit-spool

if [[ "$BUILD_APP" == "1" ]]; then
  echo "==> Building and opening 1Context Dev"
  "$ROOT/scripts/release-train.sh" build --channel dev
  ditto --norsrc --noqtn "$ROOT/dist/1Context Dev.app" "$APP_PATH"
  open -na "$APP_PATH"
else
  echo "==> Skipping app build; expecting local web to already be running"
fi

wait_for_local_web

echo "==> Opening memory viewer: $VIEWER_URL"
open "$VIEWER_URL"
