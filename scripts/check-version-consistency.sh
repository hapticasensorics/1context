#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "VERSION must be SemVer-like X.Y.Z, got: $VERSION" >&2
  exit 1
fi

CORE_VERSION="$(
  sed -nE 's/^public let oneContextVersion = "([^"]+)"/\1/p' \
    "$ROOT/macos/Sources/OneContextCore/Core.swift"
)"

if [[ "$CORE_VERSION" != "$VERSION" ]]; then
  echo "Core.swift version ($CORE_VERSION) does not match VERSION ($VERSION)." >&2
  exit 1
fi

if [[ -f "$ROOT/RELEASE_NOTES.md" ]] && grep -Eq '^# 1Context v[0-9]+\.[0-9]+\.[0-9]+' "$ROOT/RELEASE_NOTES.md"; then
  NOTES_VERSION="$(sed -nE 's/^# 1Context v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' "$ROOT/RELEASE_NOTES.md" | head -1)"
  if [[ "$NOTES_VERSION" != "$VERSION" ]]; then
    echo "RELEASE_NOTES.md version ($NOTES_VERSION) does not match VERSION ($VERSION)." >&2
    exit 1
  fi
fi

echo "1Context version consistency passed ($VERSION)."
