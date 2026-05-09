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
  cat > "$path" <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <item>
      <title>1Context $VERSION</title>
      <sparkle:version>$VERSION</sparkle:version>
$critical
$description
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

MANDATORY_OK="$TMP_DIR/mandatory-ok.xml"
MANDATORY_WITH_NOTES="$TMP_DIR/mandatory-with-notes.xml"
OPTIONAL_OK="$TMP_DIR/optional-ok.xml"
OPTIONAL_WITH_CRITICAL="$TMP_DIR/optional-with-critical.xml"
OPTIONAL_POLICY="$TMP_DIR/optional-policy.toml"

write_appcast "$MANDATORY_OK" "      <sparkle:criticalUpdate/>" ""
write_appcast "$MANDATORY_WITH_NOTES" "      <sparkle:criticalUpdate/>" "      <description>Builder journal notes should not be shown.</description>"
write_appcast "$OPTIONAL_OK" "" ""
write_appcast "$OPTIONAL_WITH_CRITICAL" "      <sparkle:criticalUpdate/>" ""
write_optional_policy "$OPTIONAL_POLICY"

"$ROOT/scripts/update-policy.py" validate --appcast "$MANDATORY_OK"

if "$ROOT/scripts/update-policy.py" validate --appcast "$MANDATORY_WITH_NOTES" >/dev/null 2>&1; then
  echo "Mandatory policy should reject appcast descriptions when release notes are hidden." >&2
  exit 1
fi

"$ROOT/scripts/update-policy.py" validate --policy "$OPTIONAL_POLICY" --appcast "$OPTIONAL_OK"

if "$ROOT/scripts/update-policy.py" validate --policy "$OPTIONAL_POLICY" --appcast "$OPTIONAL_WITH_CRITICAL" >/dev/null 2>&1; then
  echo "Optional policy should reject critical update metadata." >&2
  exit 1
fi

echo "Update policy fixture tests passed."
