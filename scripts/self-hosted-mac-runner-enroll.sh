#!/usr/bin/env bash
set -euo pipefail

REPO_SLUG="${ONECONTEXT_RUNNER_REPO:-hapticasensorics/1context}"
REPO_URL="${ONECONTEXT_RUNNER_REPO_URL:-https://github.com/$REPO_SLUG}"
RUNNER_NAME="${ONECONTEXT_RUNNER_NAME:-$(scutil --get LocalHostName 2>/dev/null || hostname -s)-1context-update}"
RUNNER_LABELS="${ONECONTEXT_RUNNER_LABELS:-onecontext-update-runner}"
RUNNER_DIR="${ONECONTEXT_RUNNER_DIR:-$HOME/actions-runners/1context-update}"
RUNNER_WORK_DIR="${ONECONTEXT_RUNNER_WORK_DIR:-_work}"
RUNNER_ARCH="${ONECONTEXT_RUNNER_ARCH:-$(uname -m)}"

case "$RUNNER_ARCH" in
  arm64) RUNNER_ASSET_ARCH="arm64" ;;
  x86_64) RUNNER_ASSET_ARCH="x64" ;;
  *)
    echo "Unsupported macOS runner architecture: $RUNNER_ARCH" >&2
    exit 1
    ;;
esac

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

require_tool curl
require_tool python3
require_tool tar
if [[ -z "${ACTIONS_RUNNER_INPUT_TOKEN:-}" ]]; then
  require_tool gh
fi

mkdir -p "$RUNNER_DIR"
cd "$RUNNER_DIR"

latest_json="$(curl --fail --location --show-error --silent https://api.github.com/repos/actions/runner/releases/latest)"
asset_url="$(
  RUNNER_LATEST_JSON="$latest_json" python3 - "$RUNNER_ASSET_ARCH" <<'PY'
import json
import os
import sys

arch = sys.argv[1]
payload = json.loads(os.environ["RUNNER_LATEST_JSON"])
needle = f"osx-{arch}-"
for asset in payload.get("assets", []):
    name = asset.get("name", "")
    if name.startswith("actions-runner-") and needle in name and name.endswith(".tar.gz"):
        print(asset["browser_download_url"])
        break
else:
    raise SystemExit(f"Could not find actions runner asset for osx-{arch}.")
PY
)"
asset_name="$(basename "$asset_url")"

if [[ ! -f "$asset_name" ]]; then
  echo "Downloading $asset_name"
  curl --fail --location --show-error --silent "$asset_url" --output "$asset_name"
fi

if [[ ! -x ./config.sh ]]; then
  tar xzf "$asset_name"
fi

if [[ -f .runner ]]; then
  echo "Runner is already configured in $RUNNER_DIR."
  echo "Run ./run.sh from a logged-in GUI session, or remove/reconfigure it intentionally."
  exit 0
fi

token="${ACTIONS_RUNNER_INPUT_TOKEN:-}"
if [[ -z "$token" ]]; then
  token="$(gh api -X POST "repos/$REPO_SLUG/actions/runners/registration-token" --jq .token)"
fi
if [[ -z "$token" ]]; then
  echo "Could not obtain a GitHub Actions runner registration token." >&2
  echo "Set ACTIONS_RUNNER_INPUT_TOKEN from GitHub Settings > Actions > Runners and rerun." >&2
  exit 1
fi

./config.sh \
  --url "$REPO_URL" \
  --token "$token" \
  --name "$RUNNER_NAME" \
  --labels "$RUNNER_LABELS" \
  --work "$RUNNER_WORK_DIR" \
  --replace \
  --unattended

cat <<EOF
Configured runner:
  directory: $RUNNER_DIR
  name: $RUNNER_NAME
  labels: self-hosted, macOS, <architecture>, $RUNNER_LABELS

Run it from the logged-in GUI session:
  cd "$RUNNER_DIR"
  ./run.sh

For first proof runs, prefer the foreground runner over svc.sh so AppleScript,
Hammerspoon capture, Sparkle prompts, and menu-bar automation share the desktop session.
EOF

if [[ "${ONECONTEXT_RUNNER_START_NOW:-0}" == "1" ]]; then
  exec ./run.sh
fi
