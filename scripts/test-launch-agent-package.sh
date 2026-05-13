#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
APP="${ONECONTEXT_PACKAGE_APP:-$ROOT/dist/1Context.app}"

if [[ ! -d "$APP" ]]; then
  echo "Packaged app not found: $APP" >&2
  echo "Run ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh first." >&2
  exit 1
fi

INFO="$APP/Contents/Info.plist"
DAEMON_PLIST="$APP/Contents/Library/LaunchDaemons/com.haptica.1context.local-web-proxy.plist"
MEMORY_CORE="$APP/Contents/Resources/memory-core"
WIKI="$MEMORY_CORE/wiki"

plutil -lint "$INFO" >/dev/null
plutil -lint "$DAEMON_PLIST" >/dev/null

test "$(plutil -extract CFBundleShortVersionString raw "$INFO")" = "$VERSION"
test "$(plutil -extract CFBundleIdentifier raw "$INFO")" = "com.haptica.1context"
if plutil -extract SUFeedURL raw "$INFO" >/dev/null 2>&1; then
  test "$(plutil -extract SUVerifyUpdateBeforeExtraction raw "$INFO")" = "true"
fi
test "$(plutil -extract Label raw "$DAEMON_PLIST")" = "com.haptica.1context.local-web-proxy"
test "$(plutil -extract BundleProgram raw "$DAEMON_PLIST")" = "Contents/Resources/1context-local-web-proxy"

for executable in \
  "$APP/Contents/MacOS/1Context" \
  "$APP/Contents/MacOS/1context-cli" \
  "$APP/Contents/MacOS/1contextd" \
  "$APP/Contents/Resources/1context-local-web-proxy"; do
  if [[ ! -x "$executable" ]]; then
    echo "Packaged executable is missing or not executable: $executable" >&2
    exit 1
  fi
done

if find "$WIKI/menu" -type d \( -name '*-goal' -o -name goal -o -name '*.goal' \) -print -quit | grep -q .; then
  echo "Packaged memory core must not include the development /goal wiki family." >&2
  exit 1
fi
if find "$WIKI/menu" -path '*/generated/goal.html' -print -quit | grep -q .; then
  echo "Packaged memory core must not include generated /goal assets." >&2
  exit 1
fi
if find "$WIKI/menu" -path '*/generated/render-manifest.json' -print -quit | grep -q .; then
  echo "Packaged memory core must not include generated render manifests." >&2
  exit 1
fi
if find "$WIKI/menu" -path '*/generated/*.md' -print -quit | grep -q .; then
  echo "Packaged memory core must not include generated markdown source files." >&2
  exit 1
fi
if find "$WIKI" -type f -print0 \
  | xargs -0 grep -I -n -E '/Users/paulhan|paulhan|/dev/1context' >/tmp/1context-package-local-paths.txt; then
  echo "Packaged user wiki must not include local developer paths. Matches:" >&2
  cat /tmp/1context-package-local-paths.txt >&2
  exit 1
fi
if find "$WIKI/generated" -maxdepth 1 -type f -name '*.json' -print0 \
  | xargs -0 grep -I -n -E '(^|[^[:alnum:]_])/goal([^[:alnum:]_]|$)|goal\.html|goal\.md' >/tmp/1context-package-goal-json.txt; then
  echo "Packaged root wiki JSON must not reference the development /goal route or assets. Matches:" >&2
  cat /tmp/1context-package-goal-json.txt >&2
  exit 1
fi
find "$APP" -name publish-manifest.json -print0 | python3 - "$APP" <<'PY'
import json
import os
import sys

app = os.path.realpath(sys.argv[1])
bad = []
raw = sys.stdin.buffer.read()
for path_bytes in raw.split(b"\0"):
  if not path_bytes:
    continue
  path = path_bytes.decode()
  with open(path, "r", encoding="utf-8") as handle:
    payload = json.load(handle)
  files = payload.get("files", [])
  if not isinstance(files, list):
    bad.append(f"{path}: files is not a list")
    continue
  for entry in files:
    if not isinstance(entry, str):
      bad.append(f"{path}: non-string files entry {entry!r}")
      continue
    if entry.startswith("/") or "/Users/" in entry or "/dev/1context" in entry or entry.startswith(app):
      bad.append(f"{path}: absolute/local files entry {entry}")
    if entry == "goal.html" or entry == "goal.md" or entry.endswith("/goal.html") or entry.endswith("/goal.md"):
      bad.append(f"{path}: development goal entry {entry}")
    if entry == "render-manifest.json" or entry.endswith("/render-manifest.json"):
      bad.append(f"{path}: render manifest entry {entry}")
    if entry.endswith(".md"):
      bad.append(f"{path}: generated markdown entry {entry}")
if bad:
  print("\n".join(bad), file=sys.stderr)
  sys.exit(1)
PY

echo "Packaged LaunchAgent smoke passed."
