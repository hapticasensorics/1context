#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/dist/1Context.app}"

if [[ "${ONECONTEXT_PRODUCT_HTTPS_SMOKE_INTERACTIVE:-0}" != "1" ]]; then
  cat >&2 <<'EOF'
Product HTTPS smoke is interactive because it validates real macOS setup:
  - user login-keychain certificate trust
  - ServiceManagement background helper approval
  - portless HTTPS at https://wiki.1context.localhost

Re-run intentionally with:
  ONECONTEXT_PRODUCT_HTTPS_SMOKE_INTERACTIVE=1 ./scripts/test-release-app-product-https.sh
EOF
  exit 77
fi

if [[ ! -d "$APP" ]]; then
  echo "1Context.app not found: $APP" >&2
  echo "Build one first with: ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh" >&2
  exit 1
fi

CLI="$APP/Contents/MacOS/1context-cli"
if [[ ! -x "$CLI" ]]; then
  echo "Packaged CLI is missing or not executable: $CLI" >&2
  exit 1
fi

MENU_APP="$APP/Contents/MacOS/1Context"
if [[ ! -x "$MENU_APP" ]]; then
  echo "Packaged menu app is missing or not executable: $MENU_APP" >&2
  exit 1
fi

assert_url_contains() {
  local url="$1"
  local expected="$2"
  local family="${3:-}"
  local output
  output="$(curl --fail --silent --show-error --noproxy '*' ${family:+"$family"} "$url")"
  grep -q "$expected" <<<"$output"
}

export ONECONTEXT_NO_UPDATE_CHECK=1
export ONECONTEXT_SKIP_APP_INSTALL_PROMPT=1
export no_proxy="wiki.1context.localhost,localhost,127.0.0.1,::1"
export NO_PROXY="$no_proxy"

open -na "$APP"

"$CLI" setup local-web install
"$CLI" start >/dev/null

for _ in {1..80}; do
  if assert_url_contains "https://wiki.1context.localhost/your-context" "Your Context"; then
    break
  fi
  sleep 0.25
done

assert_url_contains "https://wiki.1context.localhost/your-context" "Your Context"
assert_url_contains "https://wiki.1context.localhost/for-you" "How This Page Works"
assert_url_contains "https://wiki.1context.localhost/api/wiki/health" "1context-wiki-api"
assert_url_contains "https://wiki.1context.localhost/__1context/health" "ok" "-4"
assert_url_contains "https://wiki.1context.localhost/__1context/health" "ok" "-6"
unknown_api_status="$(curl --silent --output /dev/null --write-out "%{http_code}" --noproxy '*' "https://wiki.1context.localhost/api/wiki/does-not-exist")"
if [[ "$unknown_api_status" != "404" ]]; then
  echo "Unknown wiki API route returned $unknown_api_status instead of 404." >&2
  exit 1
fi

"$CLI" permissions | grep -q "Local Wiki Access: Granted"
"$CLI" status --debug | grep -q "URL: https://wiki.1context.localhost/your-context"

echo "Product HTTPS smoke passed."
