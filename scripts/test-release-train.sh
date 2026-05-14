#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d /tmp/1ctx-release-train-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
eval "$("$ROOT/scripts/release-manifest.py" export-env)"
PREVIOUS_VERSION="$ONECONTEXT_RELEASE_PREVIOUS_VERSION"

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

write_optional_manifest() {
  local path="$1"
  cp "$ROOT/release/release.toml" "$path"
  python3 - "$path" "$VERSION" "$PREVIOUS_VERSION" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
version = sys.argv[2]
previous = sys.argv[3]
text = path.read_text(encoding="utf-8")
text = text.replace('update_class = "mandatory"', 'update_class = "optional"', 1)
text = text.replace(f'minimum_autoupdate_version = "{previous}"', 'minimum_autoupdate_version = ""', 1)
text = text.replace(f'critical_update_version = "{version}"', 'critical_update_version = ""', 1)
path.write_text(text, encoding="utf-8")
PY
}

bash -n \
  "$ROOT/scripts/release-train.sh" \
  "$ROOT/scripts/redact-evidence.sh" \
  "$ROOT/scripts/audit-evidence-redaction.sh" \
  "$ROOT/scripts/lib-gui-evidence.sh" \
  "$ROOT/scripts/prove-remote-sparkle-update.sh" \
  "$ROOT/scripts/release/internal/self-hosted-update-proof.sh" \
  "$ROOT/scripts/write-runner-attestation.sh" \
  "$ROOT/scripts/package-macos-smoke.sh"
python3 -m py_compile "$ROOT/scripts/release-manifest.py"

"$ROOT/scripts/release-manifest.py" validate
ONECONTEXT_RELEASE_MANIFEST_FORCE_SIMPLE_TOML=1 "$ROOT/scripts/release-manifest.py" validate
test "$("$ROOT/scripts/release-manifest.py" matrix-cases | wc -l | tr -d '[:space:]')" = "13"
"$ROOT/scripts/release-manifest.py" matrix-cases | grep -q "^login_restart_recovery$"
"$ROOT/scripts/release-manifest.py" export-env --channel dev | grep -q "ONECONTEXT_RELEASE_CHANNEL=dev"
"$ROOT/scripts/release-manifest.py" export-env --channel private | grep -q "ONECONTEXT_RELEASE_CHANNEL_APPCAST=private"
test -f "$ROOT/release/tools/caddy/darwin-arm64/caddy-v2.11.2-darwin-arm64.tar.gz"
test -f "$ROOT/release/tools/caddy/darwin-arm64/caddy-v2.11.2-darwin-arm64.tar.gz.sha256"
(
  cd "$ROOT/release/tools/caddy/darwin-arm64"
  shasum -a 256 -c caddy-v2.11.2-darwin-arm64.tar.gz.sha256
) >/dev/null

MANDATORY_OK="$TMP_DIR/mandatory-ok.xml"
MANDATORY_WITH_NOTES="$TMP_DIR/mandatory-with-notes.xml"
MANDATORY_WRONG_CRITICAL="$TMP_DIR/mandatory-wrong-critical.xml"
MANDATORY_FOREIGN_URL="$TMP_DIR/mandatory-foreign-url.xml"
MANDATORY_MISSING_SIGNATURE="$TMP_DIR/mandatory-missing-signature.xml"
MANDATORY_MISSING_ENCLOSURE="$TMP_DIR/mandatory-missing-enclosure.xml"
MANDATORY_MISSING_LENGTH="$TMP_DIR/mandatory-missing-length.xml"
MANDATORY_STALE_MINIMUM="$TMP_DIR/mandatory-stale-minimum.xml"
OPTIONAL_OK="$TMP_DIR/optional-ok.xml"
OPTIONAL_WITH_CRITICAL="$TMP_DIR/optional-with-critical.xml"
OPTIONAL_MANIFEST="$TMP_DIR/optional-release.toml"

write_appcast "$MANDATORY_OK" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "$PREVIOUS_VERSION"
write_appcast "$MANDATORY_WITH_NOTES" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "      <description>Builder journal notes should not be shown.</description>" "$PREVIOUS_VERSION"
write_appcast "$MANDATORY_WRONG_CRITICAL" "      <sparkle:criticalUpdate sparkle:version=\"0.1.99\"/>" "" "$PREVIOUS_VERSION"
write_appcast "$MANDATORY_FOREIGN_URL" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "$PREVIOUS_VERSION" "https://example.test/1Context-$VERSION-macos-arm64.dmg"
write_appcast "$MANDATORY_MISSING_SIGNATURE" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "$PREVIOUS_VERSION" "https://github.com/hapticasensorics/1context/releases/download/v$VERSION/1Context-$VERSION-macos-arm64.dmg" "12345" ""
write_appcast "$MANDATORY_MISSING_ENCLOSURE" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "$PREVIOUS_VERSION" "__none__"
write_appcast "$MANDATORY_MISSING_LENGTH" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "$PREVIOUS_VERSION" "https://github.com/hapticasensorics/1context/releases/download/v$VERSION/1Context-$VERSION-macos-arm64.dmg" "" "fixture-signature"
write_appcast "$MANDATORY_STALE_MINIMUM" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" "" "0.1.58"
write_appcast "$OPTIONAL_OK" "" ""
write_appcast "$OPTIONAL_WITH_CRITICAL" "      <sparkle:criticalUpdate sparkle:version=\"$VERSION\"/>" ""
write_optional_manifest "$OPTIONAL_MANIFEST"

"$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_OK"

if "$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_WITH_NOTES" >/dev/null 2>&1; then
  echo "Mandatory release policy should reject appcast descriptions when release notes are hidden." >&2
  exit 1
fi
if "$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_WRONG_CRITICAL" >/dev/null 2>&1; then
  echo "Mandatory release policy should reject wrong critical update versions." >&2
  exit 1
fi
if "$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_FOREIGN_URL" >/dev/null 2>&1; then
  echo "Mandatory release policy should reject foreign enclosure URLs." >&2
  exit 1
fi
if "$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_MISSING_SIGNATURE" >/dev/null 2>&1; then
  echo "Mandatory release policy should reject missing EdDSA signatures." >&2
  exit 1
fi
if "$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_MISSING_ENCLOSURE" >/dev/null 2>&1; then
  echo "Mandatory release policy should reject missing enclosures." >&2
  exit 1
fi
if "$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_MISSING_LENGTH" >/dev/null 2>&1; then
  echo "Mandatory release policy should reject missing enclosure lengths." >&2
  exit 1
fi
if "$ROOT/scripts/release-manifest.py" validate --appcast "$MANDATORY_STALE_MINIMUM" >/dev/null 2>&1; then
  echo "Mandatory release policy should reject stale minimum autoupdate versions." >&2
  exit 1
fi

"$ROOT/scripts/release-manifest.py" validate --manifest "$OPTIONAL_MANIFEST" --appcast "$OPTIONAL_OK"

if "$ROOT/scripts/release-manifest.py" validate --manifest "$OPTIONAL_MANIFEST" --appcast "$OPTIONAL_WITH_CRITICAL" >/dev/null 2>&1; then
  echo "Optional release policy should reject critical update metadata." >&2
  exit 1
fi

test ! -e "$ROOT/scripts/package-macos-production-release.sh"
test ! -e "$ROOT/scripts/package-macos-release.sh"
test ! -e "$ROOT/scripts/check-release-manifest.sh"
test ! -e "$ROOT/scripts/audit-github-release-assets.sh"
test ! -e "$ROOT/scripts/self-hosted-update-proof.sh"
test -x "$ROOT/scripts/release/internal/self-hosted-update-proof.sh"
grep -q "./scripts/release-train.sh prove --runner-execute" "$ROOT/.github/workflows/self-hosted-mac-update-proof.yml"
grep -q "proof_reason:" "$ROOT/.github/workflows/self-hosted-mac-update-proof.yml"
grep -q "./scripts/release-train.sh prove --channel private --runner-execute" "$ROOT/.github/workflows/self-hosted-mac-private-update-proof.yml"
grep -q "proof_reason:" "$ROOT/.github/workflows/self-hosted-mac-private-update-proof.yml"
if rg -n '^\s+(old_version|new_version|staging_appcast_url|update_class|old_tag|old_dmg_url|update_timeout_seconds|steady_state_seconds|artifact_retention_days):' \
  "$ROOT/.github/workflows/self-hosted-mac-update-proof.yml" \
  "$ROOT/.github/workflows/self-hosted-mac-private-update-proof.yml" > "$TMP_DIR/proof-workflow-release-inputs.out"
then
  cat "$TMP_DIR/proof-workflow-release-inputs.out" >&2
  echo "self-hosted proof workflow must expose proof_reason only; release facts come from release/release.toml." >&2
  exit 1
fi
if grep -q "run: ./scripts/self-hosted-update-proof.sh" "$ROOT/.github/workflows/self-hosted-mac-update-proof.yml"; then
  echo "self-hosted workflow must enter proof execution through release-train.sh." >&2
  exit 1
fi
if grep -q "run: ./scripts/self-hosted-update-proof.sh" "$ROOT/.github/workflows/self-hosted-mac-private-update-proof.yml"; then
  echo "self-hosted private workflow must enter proof execution through release-train.sh." >&2
  exit 1
fi
grep -q "./scripts/release-train.sh build --channel official" "$ROOT/.github/workflows/release.yml"
grep -q "ONECONTEXT_REMOTE_APPCAST_GITHUB_REPO" "$ROOT/scripts/release-train.sh"
grep -q "ONECONTEXT_REMOTE_APPCAST_GITHUB_REPO" "$ROOT/scripts/prove-remote-sparkle-update.sh"
if rg -n --glob '!test-release-train.sh' 'release-train\.sh package|ONECONTEXT_RUNTIME_ROOT|dev-runtime-env|with-dev-runtime|release/update-policy' \
  "$ROOT/.github" "$ROOT/scripts" "$ROOT/docs/README.md" "$ROOT/docs/development.md" "$ROOT/docs/macos-release-runbook.md" "$ROOT/docs/ci/self-hosted-mac-runner.md" "$ROOT/release" \
  > "$TMP_DIR/no-shim-scan.out"
then
  cat "$TMP_DIR/no-shim-scan.out" >&2
  echo "active release surfaces must not mention deleted shims, old package commands, or old update-policy files." >&2
  exit 1
fi
if rg -n '\bbrew (install|--prefix)\b|command -v caddy' \
  "$ROOT/.github/workflows/ci.yml" \
  "$ROOT/.github/workflows/release.yml" \
  "$ROOT/.github/workflows/self-hosted-mac-update-proof.yml" \
  "$ROOT/.github/workflows/self-hosted-mac-private-update-proof.yml" \
  "$ROOT/scripts/build-macos-app.sh" > "$TMP_DIR/no-brew-release-build.out"
then
  cat "$TMP_DIR/no-brew-release-build.out" >&2
  echo "release app/DMG builds must not depend on Homebrew installs, Homebrew prefixes, or host caddy discovery." >&2
  exit 1
fi
if rg -n 'ONECONTEXT_(SPARKLE_FEED_URL|UPDATE_OPTIONAL_PROMPT_TITLE|UPDATE_OPTIONAL_PROMPT_BODY|UPDATE_FAILURE_TITLE|UPDATE_FAILURE_BODY|UPDATE_POST_INSTALL_MESSAGE_ENABLED|UPDATE_POST_INSTALL_TITLE|UPDATE_POST_INSTALL_BODY|SPARKLE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW):-' \
  "$ROOT/scripts/build-macos-app.sh" > "$TMP_DIR/build-update-env-overrides.out"
then
  cat "$TMP_DIR/build-update-env-overrides.out" >&2
  echo "build-macos-app.sh must read updater UI policy from release/release.toml, not caller env defaults." >&2
  exit 1
fi
if rg -n '\b1context-cli (start|stop|quit|restart|status|logs|update|setup)\b|"\$CLI" (start|stop|quit|restart|status|logs|update|setup)\b|\$CLI (start|stop|quit|restart|status|logs|update|setup)\b' \
  "$ROOT/scripts/release" \
  "$ROOT/scripts/prove-remote-sparkle-update.sh" \
  "$ROOT/scripts/verify-macos-steady-state.sh" > "$TMP_DIR/deleted-cli-script-uses.out"
then
  cat "$TMP_DIR/deleted-cli-script-uses.out" >&2
  echo "release proof scripts must not depend on deleted public CLI control-plane commands." >&2
  exit 1
fi

ONECONTEXT_RELEASE_EVIDENCE_DIR="$TMP_DIR/build-evidence" \
  "$ROOT/scripts/release-train.sh" build --channel dev --dry-run \
  > "$TMP_DIR/build-dev-dry-run.out"
test -f "$TMP_DIR/build-evidence/timings/build-dev.json"
grep -q '"channel": "dev"' "$TMP_DIR/build-evidence/timings/build-dev.json"
if "$ROOT/scripts/release-train.sh" package > "$TMP_DIR/package-command.out" 2>&1; then
  echo "release-train package must not remain as a compatibility shim." >&2
  exit 1
fi
grep -q "Unknown release train command: package" "$TMP_DIR/package-command.out"

ONECONTEXT_RELEASE_EVIDENCE_DIR="$TMP_DIR/proof-evidence" \
  "$ROOT/scripts/release-train.sh" prove --dry-run --ref main --proof-reason "fixture proof" \
  > "$TMP_DIR/prove-dry-run.out"
grep -q "mode: dry-run" "$TMP_DIR/prove-dry-run.out"
grep -q "old_version: $PREVIOUS_VERSION" "$TMP_DIR/prove-dry-run.out"
grep -q "new_version: $VERSION" "$TMP_DIR/prove-dry-run.out"
grep -q "workflow run self-hosted-mac-update-proof.yml" "$TMP_DIR/prove-dry-run.out"
grep -q "proof_reason=fixture\\\\ proof" "$TMP_DIR/prove-dry-run.out"
if grep -Eq -- '-f (old_version|new_version|staging_appcast_url|update_class|old_tag|old_dmg_url|update_timeout_seconds|steady_state_seconds|artifact_retention_days)=' "$TMP_DIR/prove-dry-run.out"; then
  cat "$TMP_DIR/prove-dry-run.out" >&2
  echo "release-train prove dispatch must pass only proof_reason to the workflow." >&2
  exit 1
fi

ONECONTEXT_RELEASE_EVIDENCE_DIR="$TMP_DIR/private-proof-evidence" \
  "$ROOT/scripts/release-train.sh" prove --channel private --dry-run --proof-reason "fixture private proof" \
  > "$TMP_DIR/private-prove-dry-run.out"
grep -q "workflow run self-hosted-mac-private-update-proof.yml" "$TMP_DIR/private-prove-dry-run.out"
grep -q "ref: main" "$TMP_DIR/private-prove-dry-run.out"
grep -q "channel: private" "$TMP_DIR/private-prove-dry-run.out"
grep -q "1context-private-release/releases/latest/download/appcast.xml" "$TMP_DIR/private-prove-dry-run.out"
if grep -Eq -- '-f (old_version|new_version|staging_appcast_url|update_class|old_tag|old_dmg_url|update_timeout_seconds|steady_state_seconds|artifact_retention_days|channel)=' "$TMP_DIR/private-prove-dry-run.out"; then
  cat "$TMP_DIR/private-prove-dry-run.out" >&2
  echo "private release-train prove dispatch must pass only proof_reason to the workflow." >&2
  exit 1
fi

if ONECONTEXT_RELEASE_EVIDENCE_DIR="$TMP_DIR/proof-evidence-bad-ref" \
  "$ROOT/scripts/release-train.sh" prove --dry-run --ref feature/nope > "$TMP_DIR/prove-bad-ref.out" 2>&1; then
  echo "release train proof dry-run should reject untrusted refs" >&2
  exit 1
fi
grep -q "not allowed for the self-hosted runner" "$TMP_DIR/prove-bad-ref.out"

bad_manifest="$TMP_DIR/release-bad-version.toml"
cp "$ROOT/release/release.toml" "$bad_manifest"
python3 - "$bad_manifest" "$VERSION" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
version = sys.argv[2]
text = path.read_text(encoding="utf-8")
text = text.replace(f'version = "{version}"', 'version = "9.9.9"', 1)
path.write_text(text, encoding="utf-8")
PY
if "$ROOT/scripts/release-manifest.py" validate --manifest "$bad_manifest" > "$TMP_DIR/bad-version.out" 2>&1; then
  echo "release manifest validation should fail when manifest version drifts from VERSION." >&2
  exit 1
fi
grep -Eq "VERSION|tag" "$TMP_DIR/bad-version.out"

asset_dist="$TMP_DIR/asset-dist"
mkdir -p "$asset_dist"
printf 'versioned dmg\n' > "$asset_dist/1Context-$VERSION-macos-arm64.dmg"
printf 'versioned sha\n' > "$asset_dist/1Context-$VERSION-macos-arm64.dmg.sha256"
printf 'stable dmg\n' > "$asset_dist/1Context.dmg"
printf 'stable sha\n' > "$asset_dist/1Context.dmg.sha256"
printf '<rss />\n' > "$asset_dist/appcast.xml"
"$ROOT/scripts/release-manifest.py" write-asset-manifest \
  --dist-dir "$asset_dist" \
  --output "$TMP_DIR/asset-manifest-ok.json" > "$TMP_DIR/asset-manifest-ok.out"
if grep -Eq '"/|/Users/' "$TMP_DIR/asset-manifest-ok.json"; then
  echo "asset manifest should store only relative artifact paths." >&2
  exit 1
fi
grep -q "\"path\": \"dist/1Context-$VERSION-macos-arm64.dmg\"" "$TMP_DIR/asset-manifest-ok.json"

if "$ROOT/scripts/release-manifest.py" write-asset-manifest \
  --dist-dir "$TMP_DIR/missing-dist" \
  --output "$TMP_DIR/asset-manifest.json" > "$TMP_DIR/missing-assets.out" 2>&1; then
  echo "asset manifest generation should fail when release artifacts are missing." >&2
  exit 1
fi
grep -q "Missing release assets" "$TMP_DIR/missing-assets.out"

dirty_repo="$TMP_DIR/dirty-repo"
mkdir "$dirty_repo"
git -C "$dirty_repo" init -q
printf 'clean\n' > "$dirty_repo/tracked.txt"
git -C "$dirty_repo" add tracked.txt
git -C "$dirty_repo" -c user.name=Test -c user.email=test@example.com commit -qm init
printf 'dirty\n' > "$dirty_repo/untracked.txt"
if "$ROOT/scripts/release-manifest.py" check-clean-tree --root "$dirty_repo" > "$TMP_DIR/dirty-tree.out" 2>&1; then
  echo "clean-tree gate should fail on untracked release files." >&2
  exit 1
fi
grep -q "Release tree is dirty" "$TMP_DIR/dirty-tree.out"

helper_repo="$TMP_DIR/helper-repo"
mkdir -p "$helper_repo/scripts"
git -C "$helper_repo" init -q
{
  printf '%s\n' '#!/usr/bin/env bash'
  # shellcheck disable=SC2016
  printf '%s\n' 'ROOT="$(cd "$(dirname "$0")/.." && pwd)"'
  # shellcheck disable=SC2016
  printf '%s%s\n' 'source "$ROOT/scripts/' 'helper.sh"'
} > "$helper_repo/scripts/main.sh"
chmod +x "$helper_repo/scripts/main.sh"
git -C "$helper_repo" add scripts/main.sh
git -C "$helper_repo" -c user.name=Test -c user.email=test@example.com commit -qm init
printf '# helper\n' > "$helper_repo/scripts/helper.sh"
if "$ROOT/scripts/release-manifest.py" check-sourced-helpers --root "$helper_repo" > "$TMP_DIR/helper.out" 2>&1; then
  echo "sourced-helper gate should fail when a sourced helper is untracked." >&2
  exit 1
fi
grep -q "not tracked by Git" "$TMP_DIR/helper.out"

evidence_dir="$TMP_DIR/evidence"
mkdir -p "$evidence_dir/proof-results"
cat > "$evidence_dir/raw.txt" <<'TXT'
/Users/paulhan/dev/1context-public-launch
GITHUB_TOKEN=redacted-by-test
Library/Application Support/1Context
TXT
cat > "$evidence_dir/proof-results/case.json" <<'JSON'
{
  "case": "already_current_manual_check",
  "expected_version": "__VERSION__",
  "actual_version": "__VERSION__",
  "status": "passed",
  "redaction_status": "pending"
}
JSON
python3 - "$evidence_dir/proof-results/case.json" "$VERSION" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
version = sys.argv[2]
path.write_text(path.read_text(encoding="utf-8").replace("__VERSION__", version), encoding="utf-8")
PY
"$ROOT/scripts/redact-evidence.sh" "$evidence_dir"
"$ROOT/scripts/audit-evidence-redaction.sh" "$evidence_dir" > "$TMP_DIR/redaction-audit.out"
if grep -q "/Users/paulhan" "$evidence_dir/raw.txt"; then
  echo "redaction script left a home path in evidence." >&2
  exit 1
fi
grep -q '"status": "passed"' "$evidence_dir/redaction-report.json"
grep -q '"redaction_status": "pending"' "$evidence_dir/proof-results/case.json"

echo "1Context release train checks passed."
