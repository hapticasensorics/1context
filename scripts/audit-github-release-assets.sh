#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_AUDIT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
TAG="${1:-v$VERSION}"
REPO="${ONECONTEXT_GITHUB_REPO:-hapticasensorics/1context}"
PROBES="${ONECONTEXT_RELEASE_AUDIT_PROBES:-1}"
PROBE_INTERVAL_SECONDS="${ONECONTEXT_RELEASE_AUDIT_INTERVAL_SECONDS:-0}"
LATEST_APPCAST_URL="${ONECONTEXT_LATEST_APPCAST_URL:-https://github.com/$REPO/releases/latest/download/appcast.xml}"
STABLE_DMG_URL="${ONECONTEXT_STABLE_DMG_URL:-https://github.com/$REPO/releases/latest/download/1Context.dmg}"
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
require_tool curl

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

if ! [[ "$PROBES" =~ ^[0-9]+$ ]] || (( PROBES < 1 )); then
  echo "ONECONTEXT_RELEASE_AUDIT_PROBES must be a positive integer." >&2
  exit 1
fi
if ! [[ "$PROBE_INTERVAL_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "ONECONTEXT_RELEASE_AUDIT_INTERVAL_SECONDS must be a non-negative integer." >&2
  exit 1
fi

log "checking latest/download appcast propagation"
latest_ok=0
for ((probe = 1; probe <= PROBES; probe++)); do
  if (( probe > 1 && PROBE_INTERVAL_SECONDS > 0 )); then
    sleep "$PROBE_INTERVAL_SECONDS"
  fi
  if curl --fail --location --silent --show-error "$LATEST_APPCAST_URL" --output "$WORK_DIR/latest-appcast.xml" &&
    "$ROOT/scripts/check-update-policy.sh" --appcast "$WORK_DIR/latest-appcast.xml" &&
    cmp -s "$WORK_DIR/appcast.xml" "$WORK_DIR/latest-appcast.xml"; then
    latest_ok=1
    log "latest/download appcast probe $probe/$PROBES passed"
    break
  fi
  log "latest/download appcast probe $probe/$PROBES has not propagated yet"
done
if [[ "$latest_ok" != "1" ]]; then
  echo "latest/download appcast does not match the $TAG appcast yet: $LATEST_APPCAST_URL" >&2
  exit 1
fi

log "probing appcast enclosure download"
gh release download "$TAG" \
  --repo "$REPO" \
  --pattern "1Context-$VERSION-macos-arm64.dmg.sha256" \
  --pattern "1Context.dmg.sha256" \
  --dir "$WORK_DIR" \
  --clobber >/dev/null

python3 - "$WORK_DIR/appcast.xml" "$VERSION" > "$WORK_DIR/enclosure.txt" <<'PY'
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import urlparse

appcast = Path(sys.argv[1])
version = sys.argv[2]
namespaces = {
    "sparkle": "http://www.andymatuschak.org/xml-namespaces/sparkle",
}
root = ET.parse(appcast).getroot()
item = root.find("channel/item")
if item is None:
    raise SystemExit("appcast missing channel/item")
enclosure = item.find("enclosure")
if enclosure is None:
    raise SystemExit("appcast missing enclosure")
url = enclosure.attrib.get("url", "")
if not url:
    raise SystemExit("appcast enclosure missing url")
name = Path(urlparse(url).path).name
expected_name = f"1Context-{version}-macos-arm64.dmg"
if name != expected_name:
    raise SystemExit(f"appcast enclosure asset {name!r} != expected {expected_name!r}")
length = enclosure.attrib.get("length", "")
print(url)
print(length)
print(name)
PY

ENCLOSURE_URL="$(sed -n '1p' "$WORK_DIR/enclosure.txt")"
ENCLOSURE_LENGTH="$(sed -n '2p' "$WORK_DIR/enclosure.txt")"
ENCLOSURE_NAME="$(sed -n '3p' "$WORK_DIR/enclosure.txt")"
EXPECTED_SHA="$(awk '{ print $1; exit }' "$WORK_DIR/$ENCLOSURE_NAME.sha256")"
STABLE_EXPECTED_SHA="$(awk '{ print $1; exit }' "$WORK_DIR/1Context.dmg.sha256")"

if [[ -z "$EXPECTED_SHA" ]]; then
  echo "Release checksum file for $ENCLOSURE_NAME is empty." >&2
  exit 1
fi
if [[ -z "$STABLE_EXPECTED_SHA" ]]; then
  echo "Release checksum file for 1Context.dmg is empty." >&2
  exit 1
fi
if [[ "$STABLE_EXPECTED_SHA" != "$EXPECTED_SHA" ]]; then
  echo "Stable 1Context.dmg checksum $STABLE_EXPECTED_SHA != versioned $ENCLOSURE_NAME checksum $EXPECTED_SHA." >&2
  exit 1
fi

for ((probe = 1; probe <= PROBES; probe++)); do
  if (( probe > 1 && PROBE_INTERVAL_SECONDS > 0 )); then
    sleep "$PROBE_INTERVAL_SECONDS"
  fi
  output="$WORK_DIR/enclosure-$probe.dmg"
  curl --fail --location --silent --show-error "$ENCLOSURE_URL" --output "$output"
  actual_size="$(wc -c < "$output" | tr -d '[:space:]')"
  if [[ -n "$ENCLOSURE_LENGTH" && "$actual_size" != "$ENCLOSURE_LENGTH" ]]; then
    echo "Downloaded enclosure size $actual_size != appcast length $ENCLOSURE_LENGTH on probe $probe." >&2
    exit 1
  fi
  actual_sha="$(shasum -a 256 "$output" | awk '{ print $1 }')"
  if [[ "$actual_sha" != "$EXPECTED_SHA" ]]; then
    echo "Downloaded enclosure sha256 $actual_sha != expected $EXPECTED_SHA on probe $probe." >&2
    exit 1
  fi
  log "appcast enclosure probe $probe/$PROBES passed"

  stable_output="$WORK_DIR/stable-$probe.dmg"
  curl --fail --location --silent --show-error "$STABLE_DMG_URL" --output "$stable_output"
  stable_size="$(wc -c < "$stable_output" | tr -d '[:space:]')"
  if [[ -n "$ENCLOSURE_LENGTH" && "$stable_size" != "$ENCLOSURE_LENGTH" ]]; then
    echo "Downloaded stable 1Context.dmg size $stable_size != appcast length $ENCLOSURE_LENGTH on probe $probe." >&2
    exit 1
  fi
  stable_sha="$(shasum -a 256 "$stable_output" | awk '{ print $1 }')"
  if [[ "$stable_sha" != "$EXPECTED_SHA" ]]; then
    echo "Downloaded stable 1Context.dmg sha256 $stable_sha != versioned expected $EXPECTED_SHA on probe $probe." >&2
    exit 1
  fi
  log "stable 1Context.dmg probe $probe/$PROBES passed"
done

log "release asset audit passed for $TAG"
