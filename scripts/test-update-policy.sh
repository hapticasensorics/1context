#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
TMP_DIR="$(mktemp -d /tmp/1ctx-update-policy-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

write_appcast() {
  local path="$1"
  local critical="$2"
  local description="$3"
  local minimum_autoupdate="${4:-}"
  local enclosure_url="https://github.com/hapticasensorics/1context/releases/download/v$VERSION/1Context-$VERSION-macos-arm64.dmg"
  local enclosure_length="12345"
  local ed_signature="fixture-signature"
  local minimum_autoupdate_xml=""
  local length_attr=""
  local signature_attr=""
  local enclosure_xml=""
  if [[ $# -ge 5 ]]; then
    enclosure_url="$5"
  fi
  if [[ $# -ge 6 ]]; then
    enclosure_length="$6"
  fi
  if [[ $# -ge 7 ]]; then
    ed_signature="$7"
  fi
  if [[ -n "$minimum_autoupdate" ]]; then
    minimum_autoupdate_xml="      <sparkle:minimumAutoupdateVersion>$minimum_autoupdate</sparkle:minimumAutoupdateVersion>"
  fi
  if [[ -n "$enclosure_length" ]]; then
    length_attr=" length=\"$enclosure_length\""
  fi
  if [[ -n "$ed_signature" ]]; then
    signature_attr=" sparkle:edSignature=\"$ed_signature\""
  fi
  if [[ "$enclosure_url" != "__none__" ]]; then
    enclosure_xml="      <enclosure url=\"$enclosure_url\"$length_attr type=\"application/octet-stream\"$signature_attr/>"
  fi
  cat > "$path" <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <item>
      <title>1Context $VERSION</title>
      <sparkle:version>$VERSION</sparkle:version>
$minimum_autoupdate_xml
$critical
$description
$enclosure_xml
    </item>
  </channel>
</rss>
XML
}

write_optional_policy() {
  local path="$1"
  cat > "$path" <<TOML
schema_version = "1context.update-policy.v1"
version = "$VERSION"
update_class = "optional"
approved_by = "paul"
reason = "test_optional_policy"
reason_detail = "Fixture policy used by scripts/test-update-policy.sh."
minimum_autoupdate_version = ""
minimum_update_version = ""
critical_update_version = ""

[ui]
show_release_notes_in_update_window = false

[ui.optional_prompt]
title = "Update 1Context?"
body = "A 1Context update is ready."

[ui.failure_message]
title = "Update failed."
body = "Please contact support at paul@haptica.ai."

[ui.post_install_message]
enabled = false
title = "1Context Improved!"
body = ""
TOML
}

write_mandatory_policy() {
  local path="$1"
  cat > "$path" <<TOML
schema_version = "1context.update-policy.v1"
version = "$VERSION"
update_class = "mandatory"
approved_by = "paul"
reason = "test_mandatory_policy"
reason_detail = "Fixture policy used by scripts/test-update-policy.sh."
minimum_autoupdate_version = "0.1.59"
minimum_update_version = ""
critical_update_version = "$VERSION"

[ui]
show_release_notes_in_update_window = false

[ui.optional_prompt]
title = "Update 1Context?"
body = "A 1Context update is ready."

[ui.failure_message]
title = "Update failed."
body = "Please contact support at paul@haptica.ai."

[ui.post_install_message]
enabled = false
title = "1Context Improved!"
body = ""
TOML
}

MANDATORY_OK="$TMP_DIR/mandatory-ok.xml"
MANDATORY_WITH_NOTES="$TMP_DIR/mandatory-with-notes.xml"
MANDATORY_WRONG_CRITICAL="$TMP_DIR/mandatory-wrong-critical.xml"
MANDATORY_FOREIGN_URL="$TMP_DIR/mandatory-foreign-url.xml"
MANDATORY_MISSING_SIGNATURE="$TMP_DIR/mandatory-missing-signature.xml"
MANDATORY_MISSING_ENCLOSURE="$TMP_DIR/mandatory-missing-enclosure.xml"
MANDATORY_MISSING_LENGTH="$TMP_DIR/mandatory-missing-length.xml"
MANDATORY_STALE_MINIMUM="$TMP_DIR/mandatory-stale-minimum.xml"
MANDATORY_POLICY="$TMP_DIR/mandatory-policy.toml"
OPTIONAL_OK="$TMP_DIR/optional-ok.xml"
OPTIONAL_WITH_CRITICAL="$TMP_DIR/optional-with-critical.xml"
OPTIONAL_POLICY="$TMP_DIR/optional-policy.toml"

write_appcast "$MANDATORY_OK" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "0.1.59"
write_appcast "$MANDATORY_WITH_NOTES" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "      <description>Builder journal notes should not be shown.</description>" "0.1.59"
write_appcast "$MANDATORY_WRONG_CRITICAL" "      <sparkle:criticalUpdate sparkle:version=\"0.1.99\"/>" "" "0.1.59"
write_appcast "$MANDATORY_FOREIGN_URL" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "0.1.59" "https://example.test/1Context-$VERSION-macos-arm64.dmg"
write_appcast "$MANDATORY_MISSING_SIGNATURE" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "0.1.59" "https://github.com/hapticasensorics/1context/releases/download/v$VERSION/1Context-$VERSION-macos-arm64.dmg" "12345" ""
write_appcast "$MANDATORY_MISSING_ENCLOSURE" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "0.1.59" "__none__"
write_appcast "$MANDATORY_MISSING_LENGTH" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "0.1.59" "https://github.com/hapticasensorics/1context/releases/download/v$VERSION/1Context-$VERSION-macos-arm64.dmg" "" "fixture-signature"
write_appcast "$MANDATORY_STALE_MINIMUM" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "0.1.58"
write_appcast "$OPTIONAL_OK" "" ""
write_appcast "$OPTIONAL_WITH_CRITICAL" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" ""
write_mandatory_policy "$MANDATORY_POLICY"
write_optional_policy "$OPTIONAL_POLICY"

"$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_OK"

if "$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_WITH_NOTES" >/dev/null 2>&1; then
  echo "Mandatory policy should reject appcast descriptions when release notes are hidden." >&2
  exit 1
fi
if "$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_WRONG_CRITICAL" >/dev/null 2>&1; then
  echo "Mandatory policy should reject wrong critical update versions." >&2
  exit 1
fi
if "$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_FOREIGN_URL" >/dev/null 2>&1; then
  echo "Mandatory policy should reject foreign enclosure URLs." >&2
  exit 1
fi
if "$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_MISSING_SIGNATURE" >/dev/null 2>&1; then
  echo "Mandatory policy should reject missing EdDSA signatures." >&2
  exit 1
fi
if "$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_MISSING_ENCLOSURE" >/dev/null 2>&1; then
  echo "Mandatory policy should reject missing enclosures." >&2
  exit 1
fi
if "$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_MISSING_LENGTH" >/dev/null 2>&1; then
  echo "Mandatory policy should reject missing enclosure lengths." >&2
  exit 1
fi
if "$ROOT/scripts/update-policy.py" validate --policy "$MANDATORY_POLICY" --appcast "$MANDATORY_STALE_MINIMUM" >/dev/null 2>&1; then
  echo "Mandatory policy should reject stale minimum autoupdate versions." >&2
  exit 1
fi

"$ROOT/scripts/update-policy.py" validate --policy "$OPTIONAL_POLICY" --appcast "$OPTIONAL_OK"

if "$ROOT/scripts/update-policy.py" validate --policy "$OPTIONAL_POLICY" --appcast "$OPTIONAL_WITH_CRITICAL" >/dev/null 2>&1; then
  echo "Optional policy should reject critical update metadata." >&2
  exit 1
fi

echo "Update policy fixture tests passed."
