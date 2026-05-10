#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_AUDIT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
TAG="${1:-v$VERSION}"
REPO="${ONECONTEXT_GITHUB_REPO:-hapticasensorics/1context}"
WORK_DIR="$(mktemp -d /tmp/1context-release-audit-XXXXXX)"
trap 'rm -rf "$WORK_DIR"' EXIT

log() {
  printf '[release-audit] %s\n' "$*"
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

require_tool gh
require_tool python3

log "reading GitHub release $REPO@$TAG"
gh release view "$TAG" --repo "$REPO" --json tagName,isDraft,isPrerelease,assets,url > "$WORK_DIR/release.json"

python3 - "$WORK_DIR/release.json" "$TAG" "$VERSION" <<'PY'
import json
import sys
from pathlib import Path

release = json.loads(Path(sys.argv[1]).read_text())
tag = sys.argv[2]
version = sys.argv[3]
if release.get("tagName") != tag:
    raise SystemExit(f"release tag {release.get('tagName')!r} != expected {tag!r}")
if release.get("isDraft"):
    raise SystemExit("release is still draft")
if release.get("isPrerelease"):
    raise SystemExit("release is marked prerelease")
assets = {asset["name"] for asset in release.get("assets", [])}
required = {
    f"1Context-{version}-macos-arm64.dmg",
    f"1Context-{version}-macos-arm64.dmg.sha256",
    "1Context.dmg",
    "1Context.dmg.sha256",
    "appcast.xml",
}
missing = sorted(required - assets)
if missing:
    raise SystemExit(f"release is missing assets: {', '.join(missing)}")
print(release.get("url", ""))
PY

log "downloading appcast.xml"
gh release download "$TAG" --repo "$REPO" --pattern appcast.xml --dir "$WORK_DIR" --clobber >/dev/null
"$ROOT/scripts/check-update-policy.sh" --appcast "$WORK_DIR/appcast.xml"

log "release asset audit passed for $TAG"
