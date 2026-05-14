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
if find "$APP/Contents/Resources" -path '*/generated/*' -print -quit | grep -q .; then
  echo "Packaged app must not include generated wiki source output." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -type f -print0 \
  | xargs -0 grep -I -n -E '/api/wiki/chat|chat_available|ai-provider|Chat about this page' >/tmp/1context-package-chat-surface.txt; then
  echo "Packaged resources must not include dead chat/provider surfaces. Matches:" >&2
  cat /tmp/1context-package-chat-surface.txt >&2
  exit 1
fi
if find "$APP/Contents/Resources" -type f -print0 \
  | xargs -0 grep -I -n -E '/Users/paulhan|paulhan|/dev/1context|(^|[^[:alnum:]_])/goal([^[:alnum:]_]|$)|goal\\.html|goal\\.md' >/tmp/1context-package-local-paths.txt; then
  echo "Packaged resources must not include local developer paths or development goal routes. Matches:" >&2
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
