#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"

if [[ ! -d "$DIST" ]]; then
  exit 0
fi

find "$DIST" -mindepth 1 -maxdepth 1 \
  \( -name '1context-*-macos-*.tar.gz' \
    -o -name '1context-*-macos-*' \
    -o -name '1Context.app' \
    -o -name '1Context-notary.zip' \) \
  -exec rm -rf {} +

find "$DIST" \( -name '._*' -o -name '.DS_Store' \) -delete

echo "Cleaned local release artifacts from $DIST"
