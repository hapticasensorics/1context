#!/usr/bin/env bash
set -euo pipefail

KEYCHAIN="${ONECONTEXT_RELEASE_KEYCHAIN:-${CODESIGN_KEYCHAIN:-}}"
PASSWORD="${ONECONTEXT_RELEASE_KEYCHAIN_PASSWORD:-}"

if [[ -z "$KEYCHAIN" ]]; then
  echo "No explicit release keychain configured; using the current user keychain search list."
  exit 0
fi

if [[ ! -f "$KEYCHAIN" ]]; then
  echo "Release keychain does not exist: $KEYCHAIN" >&2
  exit 1
fi

if [[ -z "$PASSWORD" ]]; then
  echo "ONECONTEXT_RELEASE_KEYCHAIN_PASSWORD is required for explicit release keychain '$KEYCHAIN'." >&2
  exit 1
fi

security unlock-keychain -p "$PASSWORD" "$KEYCHAIN"
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  -k "$PASSWORD" \
  "$KEYCHAIN" >/dev/null

new_keychains=("$KEYCHAIN")
while IFS= read -r existing; do
  if [[ -n "$existing" && "$existing" != "$KEYCHAIN" ]]; then
    new_keychains+=("$existing")
  fi
done < <(security list-keychains -d user | sed 's/^[[:space:]]*"\(.*\)"[[:space:]]*$/\1/')
security list-keychains -d user -s "${new_keychains[@]}"

if [[ -n "${GITHUB_ENV:-}" ]]; then
  {
    echo "CODESIGN_KEYCHAIN=$KEYCHAIN"
    echo "NOTARYTOOL_KEYCHAIN=$KEYCHAIN"
  } >> "$GITHUB_ENV"
fi

echo "Prepared release keychain: $KEYCHAIN"
