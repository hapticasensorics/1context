#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/dist/1Context.app}"
CLI="$APP/Contents/MacOS/1context-cli"
MENU="$APP/Contents/MacOS/1Context"

if [[ ! -d "$APP" || ! -x "$CLI" || ! -x "$MENU" ]]; then
  echo "Built app not found: $APP" >&2
  echo "Build one first with: ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh" >&2
  exit 1
fi

if [[ "${ONECONTEXT_SETUP_HARNESS_RESET:-0}" == "1" ]]; then
  echo "Resetting local-web setup before launching harness..."
  "$CLI" setup local-web uninstall >/dev/null 2>&1 || true
fi

pkill -x 1Context >/dev/null 2>&1 || true
launchctl unsetenv ONECONTEXT_SHOW_SETUP_ON_LAUNCH >/dev/null 2>&1 || true

echo "Launching 1Context setup window..."
ONECONTEXT_SHOW_SETUP_ON_LAUNCH=1 \
ONECONTEXT_NO_UPDATE_CHECK=1 \
ONECONTEXT_SKIP_APP_INSTALL_PROMPT=1 \
ONECONTEXT_MENU_PERF_LOG=1 \
"$MENU" >/tmp/1context-setup-harness-menu.log 2>&1 &
MENU_PID=$!

cleanup() {
  if [[ "${ONECONTEXT_SETUP_HARNESS_KEEP_OPEN:-1}" != "1" ]]; then
    kill "$MENU_PID" >/dev/null 2>&1 || true
    wait "$MENU_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "Menu PID: $MENU_PID"
echo "Menu log: /tmp/1context-setup-harness-menu.log"
echo
echo "Watch the setup window. The harness will keep probing until Local Wiki Access is granted."
echo "Set ONECONTEXT_SETUP_HARNESS_RESET=1 to force a fresh permissions flow."
echo

for attempt in {1..240}; do
  permissions="$("$CLI" permissions 2>&1 || true)"
  local_line="$(grep -m1 "Local Wiki Access:" <<<"$permissions" || true)"
  setup_line="$(grep -m1 "Setup Ready:" <<<"$permissions" || true)"
  api_line="$(curl --silent --show-error --noproxy '*' --max-time 1 https://wiki.1context.localhost/api/wiki/health 2>&1 || true)"
  wiki_code="$(curl --silent --output /dev/null --write-out "%{http_code}" --noproxy '*' --max-time 1 https://wiki.1context.localhost/your-context 2>/dev/null || true)"

  printf "[%03d] %s | %s | wiki=%s\n" "$attempt" "${local_line:-Local Wiki Access: unknown}" "${setup_line:-Setup Ready: unknown}" "${wiki_code:-000}"

  if ! kill -0 "$MENU_PID" >/dev/null 2>&1; then
    echo "Setup harness failed because 1Context exited before the wiki became available." >&2
    echo "Menu log:" >&2
    cat /tmp/1context-setup-harness-menu.log >&2 || true
    echo "Last permissions output:" >&2
    echo "$permissions" >&2
    exit 1
  fi

  if grep -q "Local Wiki Access: Granted" <<<"$permissions" && grep -q "1context-wiki-api" <<<"$api_line" && [[ "$wiki_code" == "200" ]]; then
    echo
    echo "Setup harness passed. Local wiki is available at https://wiki.1context.localhost/your-context"
    exit 0
  fi

  sleep 1
done

echo "Setup harness timed out." >&2
echo "Last permissions output:" >&2
echo "$permissions" >&2
exit 1
