#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ARTIFACT="${1:-$ROOT/dist/1Context.app}"
PROFILE="${NOTARYTOOL_PROFILE:-1context-notary}"
NOTARYTOOL_KEYCHAIN="${NOTARYTOOL_KEYCHAIN:-${ONECONTEXT_RELEASE_KEYCHAIN:-}}"
TMPDIR="$(mktemp -d /tmp/1ctx-notary-XXXXXX)"

cleanup() {
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

if [[ ! -e "$ARTIFACT" ]]; then
  echo "Artifact not found: $ARTIFACT" >&2
  exit 1
fi

if ! command -v xcrun >/dev/null 2>&1; then
  echo "xcrun is required for notarization. Install Xcode command line tools first." >&2
  exit 1
fi

for tool in notarytool stapler; do
  if ! xcrun --find "$tool" >/dev/null 2>&1; then
    echo "xcrun $tool is required for notarization. Install a current Xcode first." >&2
    exit 1
  fi
done

if ! command -v codesign >/dev/null 2>&1 || ! command -v spctl >/dev/null 2>&1; then
  echo "codesign and spctl are required to verify notarized artifacts." >&2
  exit 1
fi

APPLE_ID_CREDENTIAL_COUNT=0
for value in "${APPLE_ID:-}" "${APPLE_TEAM_ID:-}" "${APPLE_APP_SPECIFIC_PASSWORD:-}"; do
  if [[ -n "$value" ]]; then
    APPLE_ID_CREDENTIAL_COUNT=$((APPLE_ID_CREDENTIAL_COUNT + 1))
  fi
done

if [[ "$APPLE_ID_CREDENTIAL_COUNT" -ne 0 && "$APPLE_ID_CREDENTIAL_COUNT" -ne 3 ]]; then
  echo "Set APPLE_ID, APPLE_TEAM_ID, and APPLE_APP_SPECIFIC_PASSWORD together, or set none and use NOTARYTOOL_PROFILE." >&2
  exit 1
fi

submit_to_notary() {
  local upload="$1"
  if [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]]; then
    if ! xcrun notarytool submit "$upload" \
      --apple-id "$APPLE_ID" \
      --team-id "$APPLE_TEAM_ID" \
      --password "$APPLE_APP_SPECIFIC_PASSWORD" \
      --wait; then
      echo "Notarization failed for $upload using Apple ID credentials." >&2
      exit 1
    fi
  else
    notary_args=(submit "$upload" --keychain-profile "$PROFILE" --wait)
    if [[ -n "$NOTARYTOOL_KEYCHAIN" ]]; then
      notary_args+=(--keychain "$NOTARYTOOL_KEYCHAIN")
    fi
    if ! xcrun notarytool "${notary_args[@]}"; then
      echo "Notarization failed for $upload using keychain profile '$PROFILE'." >&2
      if [[ -n "$NOTARYTOOL_KEYCHAIN" ]]; then
        echo "Checked keychain: $NOTARYTOOL_KEYCHAIN" >&2
      fi
      echo "Create it with: xcrun notarytool store-credentials $PROFILE" >&2
      exit 1
    fi
  fi
}

if [[ -d "$ARTIFACT" && "$ARTIFACT" == *.app ]]; then
  codesign --verify --deep --strict "$ARTIFACT" >/dev/null
  ZIP_PATH="$TMPDIR/$(basename "$ARTIFACT" .app)-notary.zip"
  COPYFILE_DISABLE=1 ditto \
    --norsrc \
    --noextattr \
    --noqtn \
    --noacl \
    -c \
    -k \
    --keepParent \
    "$ARTIFACT" \
    "$ZIP_PATH"
  submit_to_notary "$ZIP_PATH"
  xcrun stapler staple "$ARTIFACT"
  xcrun stapler validate "$ARTIFACT"
  spctl --assess --type execute --verbose "$ARTIFACT"
elif [[ -f "$ARTIFACT" && "$ARTIFACT" == *.dmg ]]; then
  if ! codesign --verify --strict "$ARTIFACT" >/dev/null 2>&1; then
    echo "DMG must be signed before notarization: $ARTIFACT" >&2
    echo "Use scripts/release-train.sh build --channel official so the DMG is signed before submission." >&2
    exit 1
  fi
  submit_to_notary "$ARTIFACT"
  xcrun stapler staple "$ARTIFACT"
  xcrun stapler validate "$ARTIFACT"
  spctl --assess --type open --context context:primary-signature --verbose "$ARTIFACT"
else
  echo "Unsupported notarization artifact. Expected .app bundle or .dmg: $ARTIFACT" >&2
  exit 1
fi

echo "$ARTIFACT"
