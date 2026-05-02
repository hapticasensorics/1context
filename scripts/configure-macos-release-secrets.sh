#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="${NOTARYTOOL_PROFILE:-1context-notary}"
SPARKLE_ACCOUNT="${SPARKLE_KEY_ACCOUNT:-com.haptica.1context.sparkle}"
GENERATE_KEYS="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_keys"

first_developer_id_identity() {
  security find-identity -v -p codesigning \
    | awk -F'"' '/Developer ID Application:/ { print $2; exit }'
}

default_app_store_key_path() {
  find "$HOME/Downloads" -maxdepth 1 -type f -name 'AuthKey_*.p8' | sort | head -n 1
}

app_store_key_id_from_path() {
  local key_path="$1"
  local name
  name="$(basename "$key_path")"
  name="${name#AuthKey_}"
  name="${name%.p8}"
  printf '%s' "$name"
}

ensure_sparkle_tools() {
  if [[ ! -x "$GENERATE_KEYS" ]]; then
    swift build --package-path "$ROOT/macos" -c release >/dev/null
  fi
  if [[ ! -x "$GENERATE_KEYS" ]]; then
    echo "Sparkle generate_keys tool was not found at $GENERATE_KEYS." >&2
    exit 1
  fi
}

print_codesign_identity() {
  local identity
  identity="${CODESIGN_IDENTITY:-$(first_developer_id_identity)}"
  if [[ -z "$identity" ]]; then
    echo "No Developer ID Application signing identity found in this keychain." >&2
    echo "Import the Developer ID Application certificate/private key, then rerun this script." >&2
    return 1
  fi
  echo "CODESIGN_IDENTITY=$identity"
}

configure_notary_profile() {
  local key_path="${APP_STORE_CONNECT_KEY_PATH:-$(default_app_store_key_path)}"
  local key_id="${APP_STORE_CONNECT_KEY_ID:-}"
  local issuer_id="${APP_STORE_CONNECT_ISSUER_ID:-}"

  if [[ -z "$key_path" || ! -f "$key_path" ]]; then
    echo "No App Store Connect API key found. Set APP_STORE_CONNECT_KEY_PATH or place AuthKey_<KEYID>.p8 in Downloads." >&2
    return 1
  fi
  if [[ -z "$key_id" ]]; then
    key_id="$(app_store_key_id_from_path "$key_path")"
  fi
  if [[ -z "$issuer_id" ]]; then
    echo "APP_STORE_CONNECT_ISSUER_ID is required to create the notarytool profile." >&2
    echo "Detected key: $key_path"
    echo "Detected key id: $key_id"
    return 1
  fi

  xcrun notarytool store-credentials "$PROFILE" \
    --key "$key_path" \
    --key-id "$key_id" \
    --issuer "$issuer_id"
}

print_sparkle_public_key() {
  ensure_sparkle_tools
  if "$GENERATE_KEYS" --account "$SPARKLE_ACCOUNT" -p; then
    return 0
  fi
  echo "No Sparkle EdDSA key exists for account '$SPARKLE_ACCOUNT'." >&2
  echo "Run with CREATE_SPARKLE_KEY=1 to create one in your login keychain." >&2
  return 1
}

create_sparkle_key() {
  ensure_sparkle_tools
  "$GENERATE_KEYS" --account "$SPARKLE_ACCOUNT"
}

cat <<EOF
1Context macOS release secret setup

This script does not print private keys. It only reports the signing identity,
stores notarization credentials when requested, and prints Sparkle's public key
for Info.plist/release configuration.
EOF

print_codesign_identity || true

if [[ "${CREATE_NOTARY_PROFILE:-0}" == "1" ]]; then
  configure_notary_profile
else
  echo "Notary profile: set CREATE_NOTARY_PROFILE=1 and APP_STORE_CONNECT_ISSUER_ID to store '$PROFILE'."
fi

if [[ "${CREATE_SPARKLE_KEY:-0}" == "1" ]]; then
  create_sparkle_key
else
  print_sparkle_public_key || true
fi
