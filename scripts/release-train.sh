#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNNER_DIR="$ROOT/release/runner"

if [[ ! -d "$RUNNER_DIR/node_modules" ]]; then
  npm --prefix "$RUNNER_DIR" ci
fi

exec npm --prefix "$RUNNER_DIR" run --silent release -- "$@"
