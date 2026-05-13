#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/dist/1Context.app}"

if [[ "${ONECONTEXT_PRODUCT_HTTPS_SMOKE_INTERACTIVE:-0}" != "1" ]]; then
  cat >&2 <<'EOF'
Product HTTPS smoke is interactive because it validates real macOS setup:
  - user login-keychain certificate trust
  - ServiceManagement background helper approval
  - portless HTTPS at https://localhost

Complete Settings > Setup in the app once before running this smoke.

Re-run intentionally with:
  ONECONTEXT_PRODUCT_HTTPS_SMOKE_INTERACTIVE=1 ./scripts/test-release-app-product-https.sh
EOF
  exit 77
fi

if [[ ! -d "$APP" ]]; then
  echo "1Context.app not found: $APP" >&2
  echo "Build one first with: ./scripts/package-macos-smoke.sh" >&2
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

wait_url_contains() {
  local url="$1"
  local expected="$2"
  local family="${3:-}"
  for _ in {1..80}; do
    if assert_url_contains "$url" "$expected" "$family"; then
      return 0
    fi
    sleep 0.25
  done
  assert_url_contains "$url" "$expected" "$family"
}

export no_proxy="wiki.1context.localhost,localhost,127.0.0.1,::1"
export NO_PROXY="$no_proxy"

DIAGNOSE_OUTPUT="$(mktemp /tmp/1context-product-https-diagnose.XXXXXX)"
trap 'rm -f "$DIAGNOSE_OUTPUT"' EXIT
"$CLI" diagnose > "$DIAGNOSE_OUTPUT"
if ! grep -q "Setup Ready: yes" "$DIAGNOSE_OUTPUT"; then
  cat "$DIAGNOSE_OUTPUT" >&2
  echo "Local Wiki Access is not ready. Open 1Context and choose Settings > Setup..., then rerun." >&2
  exit 1
fi

open "$APP"

wait_url_contains "https://localhost/your-context" "Your Context"
wait_url_contains "https://localhost/for-you" "How This Page Works"
wait_url_contains "https://localhost/api/wiki/health" "1context-wiki-api"
wait_url_contains "https://localhost/__1context/health" "ok"
wait_url_contains "https://wiki.1context.localhost/your-context" "Your Context"
wait_url_contains "https://wiki.1context.localhost/__1context/health" "ok" "-4"
wait_url_contains "https://wiki.1context.localhost/__1context/health" "ok" "-6"
unknown_api_status="$(curl --silent --output /dev/null --write-out "%{http_code}" --noproxy '*' "https://localhost/api/wiki/does-not-exist")"
if [[ "$unknown_api_status" != "404" ]]; then
  echo "Unknown wiki API route returned $unknown_api_status instead of 404." >&2
  exit 1
fi

"$CLI" diagnose | grep -q "Local Wiki Access: Granted"
"$CLI" diagnose | grep -q "URL: https://localhost/your-context"
"$CLI" diagnose | grep -q "Branded Host Probe URL: https://wiki.1context.localhost/__1context/health"

echo "Product HTTPS smoke passed."
