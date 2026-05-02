#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/notarize-macos-artifact.sh" "${1:-$ROOT/dist/1Context.app}"
