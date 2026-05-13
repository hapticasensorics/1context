#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="${NOTARYTOOL_PROFILE:-1context-notary}"
SPARKLE_ACCOUNT="${SPARKLE_KEY_ACCOUNT:-com.haptica.1context.sparkle}"
GENERATE_KEYS="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_keys"
CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-${ONECONTEXT_RELEASE_KEYCHAIN:-}}"
NOTARYTOOL_KEYCHAIN="${NOTARYTOOL_KEYCHAIN:-${ONECONTEXT_RELEASE_KEYCHAIN:-}}"

security_find_identity() {
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    security find-identity -v -p codesigning "$CODESIGN_KEYCHAIN"
  else
    security find-identity -v -p codesigning
  fi
}

require_developer_id() {
  local identity="${CODESIGN_IDENTITY:-}"
  if [[ -z "$identity" ]]; then
    identity="$(security_find_identity | awk -F'"' '/Developer ID Application:/ { print $2; exit }')"
  fi
  if [[ -z "$identity" ]]; then
    echo "No Developer ID Application signing identity found." >&2
    if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
      echo "Checked keychain: $CODESIGN_KEYCHAIN" >&2
    fi
    return 1
  fi
  if ! security_find_identity | grep -F "$identity" >/dev/null; then
    echo "Configured CODESIGN_IDENTITY is not available: $identity" >&2
    return 1
  fi
  echo "Developer ID identity: $identity"
}

ensure_sparkle_tools() {
  if [[ ! -x "$GENERATE_KEYS" ]]; then
    swift build --package-path "$ROOT/macos" -c release >/dev/null
  fi
  if [[ ! -x "$GENERATE_KEYS" ]]; then
    echo "Sparkle generate_keys tool was not found at $GENERATE_KEYS." >&2
    return 1
  fi
}

require_sparkle_signing_key() {
  ensure_sparkle_tools
  if [[ -n "${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-}" ]]; then
    echo "Sparkle public key: provided by ONECONTEXT_SPARKLE_PUBLIC_ED_KEY"
  elif "$GENERATE_KEYS" --account "$SPARKLE_ACCOUNT" -p >/dev/null; then
    echo "Sparkle public key: keychain account '$SPARKLE_ACCOUNT'"
  else
    echo "No Sparkle public EdDSA key found." >&2
    echo "Set ONECONTEXT_SPARKLE_PUBLIC_ED_KEY or import the keychain account '$SPARKLE_ACCOUNT'." >&2
    return 1
  fi

  if [[ -n "${SPARKLE_PRIVATE_ED_KEY:-}" ]]; then
    echo "Sparkle private signing key: provided by SPARKLE_PRIVATE_ED_KEY"
  elif [[ -n "${SPARKLE_ED_KEY_FILE:-}" && -f "${SPARKLE_ED_KEY_FILE:-}" ]]; then
    echo "Sparkle private signing key: provided by SPARKLE_ED_KEY_FILE"
  elif "$GENERATE_KEYS" --account "$SPARKLE_ACCOUNT" -p >/dev/null; then
    echo "Sparkle private signing key: keychain account '$SPARKLE_ACCOUNT'"
  else
    echo "No Sparkle private EdDSA signing key found." >&2
    echo "Set SPARKLE_PRIVATE_ED_KEY, SPARKLE_ED_KEY_FILE, or import the Sparkle keychain account." >&2
    return 1
  fi
}

require_notary_profile() {
  local args=(history --keychain-profile "$PROFILE" --output-format json)
  if [[ -n "$NOTARYTOOL_KEYCHAIN" ]]; then
    args+=(--keychain "$NOTARYTOOL_KEYCHAIN")
  fi
  if ! xcrun notarytool "${args[@]}" >/dev/null; then
    echo "Notary profile is not available: $PROFILE" >&2
    if [[ -n "$NOTARYTOOL_KEYCHAIN" ]]; then
      echo "Checked keychain: $NOTARYTOOL_KEYCHAIN" >&2
    fi
    return 1
  fi
  echo "Notary profile: $PROFILE"
}

require_developer_id
require_sparkle_signing_key
require_notary_profile
