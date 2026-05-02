#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SPARKLE_ACCOUNT="${SPARKLE_KEY_ACCOUNT:-com.haptica.1context.sparkle}"
GENERATE_KEYS="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_keys"

developer_id_identity() {
  security find-identity -v -p codesigning \
    | awk -F'"' '/Developer ID Application:/ { print $2; exit }'
}

sparkle_public_key() {
  if [[ ! -x "$GENERATE_KEYS" ]]; then
    swift build --package-path "$ROOT/macos" -c release >/dev/null
  fi
  "$GENERATE_KEYS" --account "$SPARKLE_ACCOUNT" -p \
    | awk -F'[<>]' '
      /<string>/ { print $3; found = 1; exit }
      /^[A-Za-z0-9+\/]+=*$/ { print; found = 1; exit }
      END { if (!found) exit 1 }
    '
}

if [[ -z "${ONECONTEXT_SPARKLE_FEED_URL:-}" ]]; then
  echo "Set ONECONTEXT_SPARKLE_FEED_URL to the production appcast URL." >&2
  exit 1
fi

export CODESIGN_IDENTITY="${CODESIGN_IDENTITY:-$(developer_id_identity)}"
if [[ -z "$CODESIGN_IDENTITY" ]]; then
  echo "No Developer ID Application signing identity found." >&2
  exit 1
fi

export ONECONTEXT_SPARKLE_PUBLIC_ED_KEY="${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-$(sparkle_public_key)}"
if [[ -z "$ONECONTEXT_SPARKLE_PUBLIC_ED_KEY" ]]; then
  echo "No Sparkle public key found for account '$SPARKLE_ACCOUNT'." >&2
  echo "Run: CREATE_SPARKLE_KEY=1 scripts/configure-macos-release-secrets.sh" >&2
  exit 1
fi

export NOTARIZE="${NOTARIZE:-1}"
export GENERATE_SPARKLE_APPCAST="${GENERATE_SPARKLE_APPCAST:-1}"

"$ROOT/scripts/package-macos-release.sh"
