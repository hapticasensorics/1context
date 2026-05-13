#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/update-policy.py" validate "$@"
"$ROOT/scripts/release-manifest.py" validate "$@"
