#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
APP="${ONECONTEXT_PACKAGE_APP:-$ROOT/dist/1Context.app}"

if [[ ! -d "$APP" ]]; then
  echo "Packaged app not found: $APP" >&2
  echo "Run ./scripts/package-macos-smoke.sh first." >&2
  exit 1
fi

INFO="$APP/Contents/Info.plist"
DAEMON_PLIST="$APP/Contents/Library/LaunchDaemons/com.haptica.1context.local-web-proxy.plist"
MEMORY_CORE="$APP/Contents/Resources/memory-core"
MEMORY_RUNTIME="$APP/Contents/Resources/memory-runtime"
RUNTIME_DEFAULTS="$APP/Contents/Resources/RuntimeDefaults/1Context"
WIKI_ENGINE="$APP/Contents/Resources/WikiEngine"

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
  "$APP/Contents/Resources/1context-local-web-proxy" \
  "$APP/Contents/Resources/local-web/caddy/caddy"; do
  if [[ ! -x "$executable" ]]; then
    echo "Packaged executable is missing or not executable: $executable" >&2
    exit 1
  fi
done

if [[ -e "$MEMORY_CORE" ]]; then
  echo "Packaged app must not include the memory-core source checkout." >&2
  exit 1
fi
if [[ ! -f "$MEMORY_RUNTIME/manifest.json" || ! -f "$MEMORY_RUNTIME/wiki-site/index.html" ]]; then
  echo "Packaged app must include the allowlisted memory-runtime artifact." >&2
  exit 1
fi
python3 - "$MEMORY_RUNTIME" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
if manifest.get("schema_version") != "1context.memory-runtime.v1":
    raise SystemExit("memory-runtime manifest schema mismatch")
total_bytes = int(manifest.get("total_bytes") or 0)
if total_bytes <= 0 or total_bytes > 262_144:
    raise SystemExit(f"memory-runtime size outside contract: {total_bytes}")
paths = {entry.get("path") for entry in manifest.get("files", [])}
required = {
    "wiki-site/index.html",
    "wiki-site/your-context/index.html",
    "wiki-site/for-you/index.html",
    "wiki-site/__1context/health",
    "wiki-site/api/wiki/search.json",
    "wiki-site/api/wiki/bookmarks.json",
    "wiki-site/api/wiki/state.json",
}
missing = sorted(required - paths)
if missing:
    raise SystemExit("memory-runtime missing files: " + ", ".join(missing))
for path in root.rglob("*"):
    if path.is_file() and path.suffix in {".py", ".pyc", ".sh", ".swift", ".ts", ".tsx", ".js", ".mjs", ".md"}:
        raise SystemExit(f"memory-runtime contains source/script file: {path.relative_to(root)}")
PY
if [[ ! -f "$RUNTIME_DEFAULTS/user-wiki/wiki.toml" ]]; then
  echo "Packaged app must include user-wiki runtime defaults." >&2
  exit 1
fi
if [[ ! -f "$RUNTIME_DEFAULTS/user-wiki/site/.1context/route-manifest.json" ]]; then
  echo "Packaged runtime defaults must include a pre-rendered last-good wiki site." >&2
  exit 1
fi
if [[ ! -f "$WIKI_ENGINE/tools/render-site.mjs" ]]; then
  echo "Packaged app must include the first-class wiki renderer source." >&2
  exit 1
fi
if [[ -e "$WIKI_ENGINE/node_modules" || -e "$WIKI_ENGINE/package-lock.json" ]]; then
  echo "Packaged wiki renderer source must not include runtime package installs." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/runtime-test/*' -print -quit | grep -q .; then
  echo "Packaged app must not include generated runtime-test state." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/generated/*' -print -quit | grep -q .; then
  echo "Packaged app must not include generated wiki source output." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/context-engine/observations/*' -type f -print -quit | grep -q .; then
  echo "Packaged app must not include raw observations." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/context-engine/runs/*' -type f -print -quit | grep -q .; then
  echo "Packaged app must not include run transcripts." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/context-engine/artifacts/wiki/previews/*' -type f -print -quit | grep -q .; then
  echo "Packaged app must not include private preview artifacts." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -type f -print0 \
  | xargs -0 grep -I -n -E '/api/wiki/chat|chat_available|ai-provider|Chat about this page' >/tmp/1context-package-chat-surface.txt; then
  echo "Packaged resources must not include dead chat/provider surfaces. Matches:" >&2
  cat /tmp/1context-package-chat-surface.txt >&2
  exit 1
fi
if find "$APP/Contents/Resources" -type f -print0 \
  | xargs -0 grep -I -n -E '/Users/paulhan|paulhan|/dev/1context|runtime-test|1context-private|(^|[^[:alnum:]_])/goal([^[:alnum:]_]|$)|goal\\.html|goal\\.md' >/tmp/1context-package-local-paths.txt; then
  echo "Packaged resources must not include local developer paths, private fixtures, runtime-test, or development goal routes. Matches:" >&2
  cat /tmp/1context-package-local-paths.txt >&2
  exit 1
fi
if grep -R -a -n -E '/opt/homebrew|/usr/local/Cellar|/Cellar/caddy' "$APP" >/tmp/1context-package-homebrew-paths.txt; then
  echo "Packaged app must not include Homebrew or host Caddy paths. Matches:" >&2
  cat /tmp/1context-package-homebrew-paths.txt >&2
  exit 1
fi
"$ROOT/scripts/audit-macos-app-dependencies.sh" "$APP"

echo "Packaged LaunchAgent smoke passed."
