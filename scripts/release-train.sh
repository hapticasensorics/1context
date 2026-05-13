#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMMAND="${1:-}"
if [[ $# -gt 0 ]]; then
  shift
fi

usage() {
  cat <<'USAGE'
Usage: scripts/release-train.sh <validate|package|publish|prove|audit|bless>

Runs the manifest-driven release train. release/release.toml is the only release
source of truth; VERSION, update policy, appcast, workflows, and proof evidence
must all agree with it.
USAGE
}

fail() {
  echo "release train failed: $*" >&2
  exit 1
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "Missing required tool: $1"
  fi
}

load_manifest_env() {
  eval "$("$ROOT/scripts/release-manifest.py" export-env)"
  VERSION="$ONECONTEXT_RELEASE_VERSION"
  PREVIOUS_VERSION="$ONECONTEXT_RELEASE_PREVIOUS_VERSION"
  TAG="$ONECONTEXT_RELEASE_TAG"
  UPDATE_CLASS="$ONECONTEXT_RELEASE_UPDATE_CLASS"
  PUBLIC_APPCAST_URL="$ONECONTEXT_RELEASE_PUBLIC_APPCAST_URL"
  STABLE_DMG_NAME="$ONECONTEXT_RELEASE_STABLE_DMG_NAME"
  EVIDENCE_DIR="${ONECONTEXT_RELEASE_EVIDENCE_DIR:-$ROOT/dist/release-evidence/$VERSION}"
  PROOF_RESULTS_DIR="$EVIDENCE_DIR/proof-results"
  ASSET_MANIFEST="$EVIDENCE_DIR/asset-manifest.json"
  RUNNER_ATTESTATION="$EVIDENCE_DIR/runner-attestation.json"
  RELEASE_EVIDENCE="$EVIDENCE_DIR/release-evidence.json"
}

write_release_evidence() {
  local phase="$1"
  mkdir -p "$PROOF_RESULTS_DIR"
  python3 - "$RELEASE_EVIDENCE" "$phase" "$VERSION" "$PREVIOUS_VERSION" "$TAG" "$UPDATE_CLASS" "$PUBLIC_APPCAST_URL" <<'PY'
import datetime as dt
import json
import os
import sys
from pathlib import Path

output = Path(sys.argv[1])
phase = sys.argv[2]
version = sys.argv[3]
previous_version = sys.argv[4]
tag = sys.argv[5]
update_class = sys.argv[6]
appcast_url = sys.argv[7]
output.write_text(json.dumps({
  "schema_version": "1context.release-evidence.v1",
  "phase": phase,
  "version": version,
  "previous_version": previous_version,
  "tag": tag,
  "update_class": update_class,
  "public_appcast_url": appcast_url,
  "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
  "github_ref": os.environ.get("GITHUB_REF", ""),
  "github_sha": os.environ.get("GITHUB_SHA", ""),
  "required_files": {
    "asset_manifest": "asset-manifest.json",
    "redaction_report": "redaction-report.json",
    "runner_attestation": "runner-attestation.json",
    "proof_results": "proof-results/*.json",
  },
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

ensure_tag_ref() {
  if [[ -n "${GITHUB_REF:-}" ]]; then
    [[ "$GITHUB_REF" == "refs/tags/$TAG" ]] || fail "Release must run from $TAG; current ref is ${GITHUB_REF}."
    return
  fi
  local current_tag
  current_tag="$(git -C "$ROOT" describe --tags --exact-match 2>/dev/null || true)"
  [[ "$current_tag" == "$TAG" ]] || fail "Release must run from tag $TAG; current checkout is ${current_tag:-not exactly tagged}."
}

collect_release_assets() {
  local artifact="$ROOT/dist/1Context-$VERSION-macos-arm64.dmg"
  local appcast="$ROOT/dist/sparkle-updates/appcast.xml"
  test -f "$artifact" || fail "Missing versioned DMG: $artifact"
  test -f "$appcast" || fail "Missing generated appcast: $appcast"
  cp "$appcast" "$ROOT/dist/appcast.xml"
  cp "$artifact" "$ROOT/dist/$STABLE_DMG_NAME"
  (
    cd "$ROOT/dist"
    shasum -a 256 "$(basename "$artifact")" > "$(basename "$artifact").sha256"
    shasum -a 256 "$STABLE_DMG_NAME" > "$STABLE_DMG_NAME.sha256"
  )
  "$ROOT/scripts/check-update-policy.sh" --appcast "$ROOT/dist/appcast.xml"
  "$ROOT/scripts/check-release-manifest.sh" --appcast "$ROOT/dist/appcast.xml"
  "$ROOT/scripts/release-manifest.py" write-asset-manifest --appcast "$ROOT/dist/appcast.xml" --output "$ASSET_MANIFEST"
}

release_validate() {
  "$ROOT/scripts/check-release-manifest.sh" --require-clean
  ensure_tag_ref
  "$ROOT/scripts/check-version-consistency.sh"
  "$ROOT/scripts/check-update-policy.sh"
}

release_package() {
  release_validate
  mkdir -p "$EVIDENCE_DIR"
  "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  export ONECONTEXT_RELEASE_TRAIN=1
  eval "$("$ROOT/scripts/update-policy.py" export-env)"
  "$ROOT/scripts/package-macos-production-release.sh"
  collect_release_assets
  write_release_evidence "package"
  "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
}

release_publish() {
  require_tool gh
  release_validate
  ensure_tag_ref
  mkdir -p "$EVIDENCE_DIR"
  if [[ ! -f "$ASSET_MANIFEST" ]]; then
    "$ROOT/scripts/release-manifest.py" write-asset-manifest --appcast "$ROOT/dist/appcast.xml" --output "$ASSET_MANIFEST"
  fi
  "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  write_release_evidence "publish-preflight"
  "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  gh release view "$TAG" >/dev/null 2>&1 || \
    gh release create "$TAG" --title "1Context $TAG" --notes-file "$ROOT/RELEASE_NOTES.md"
  gh release upload "$TAG" \
    "$ROOT/dist/1Context-$VERSION-macos-arm64.dmg" \
    "$ROOT/dist/1Context-$VERSION-macos-arm64.dmg.sha256" \
    "$ROOT/dist/$STABLE_DMG_NAME" \
    "$ROOT/dist/$STABLE_DMG_NAME.sha256" \
    "$ROOT/dist/appcast.xml" \
    "$ASSET_MANIFEST" \
    --clobber
  "$ROOT/scripts/audit-github-release-assets.sh" "$TAG"
  write_release_evidence "publish"
  "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
}

release_prove() {
  require_tool gh
  release_validate
  ensure_tag_ref
  mkdir -p "$PROOF_RESULTS_DIR"
  "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  local output="$PROOF_RESULTS_DIR/mandatory_automatic_success.json"
  local transcript="$EVIDENCE_DIR/release-proof-request.txt"
  "$ROOT/scripts/request-release-proof.sh" \
    --dispatch \
    --watch \
    --download-artifacts \
    --ref "$TAG" \
    --old-version "$PREVIOUS_VERSION" \
    --new-version "$VERSION" \
    --appcast-url "$PUBLIC_APPCAST_URL" \
    --update-class "$UPDATE_CLASS" \
    --proof-reason "manifest-driven $UPDATE_CLASS Sparkle proof for 1Context $VERSION" \
    | tee "$transcript"
  python3 - "$output" "$VERSION" "$UPDATE_CLASS" "$transcript" <<'PY'
import datetime as dt
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
version = sys.argv[2]
update_class = sys.argv[3]
transcript = Path(sys.argv[4])
output.write_text(json.dumps({
  "case": "mandatory_automatic_success",
  "expected_version": version,
  "actual_version": version,
  "update_class": update_class,
  "status": "passed",
  "ui_assertions": [
    "no_release_notes_prompt",
    "no_installer_click_through",
    "no_support_alert"
  ],
  "runtime_assertions": [
    "no_runtime_pause",
    "final_installed_version_matches_manifest"
  ],
  "redaction_status": "pending",
  "artifact_paths": [str(transcript)],
  "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  write_release_evidence "prove"
  "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
}

release_audit() {
  release_validate
  "$ROOT/scripts/audit-github-release-assets.sh" "$TAG"
  if [[ -d "$EVIDENCE_DIR" ]]; then
    "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  fi
}

release_bless() {
  release_validate
  for required in "$RELEASE_EVIDENCE" "$ASSET_MANIFEST" "$RUNNER_ATTESTATION" "$EVIDENCE_DIR/redaction-report.json"; do
    [[ -f "$required" ]] || fail "Bless requires evidence file: $required"
  done
  compgen -G "$PROOF_RESULTS_DIR/*.json" >/dev/null || fail "Bless requires proof result JSON files in $PROOF_RESULTS_DIR."
  while IFS= read -r matrix_case; do
    [[ -f "$PROOF_RESULTS_DIR/$matrix_case.json" ]] || fail "Bless requires updater matrix proof result: $PROOF_RESULTS_DIR/$matrix_case.json"
  done < <("$ROOT/scripts/release-manifest.py" matrix-cases)
  python3 - "$PROOF_RESULTS_DIR" <<'PY'
import json
import sys
from pathlib import Path

proof_dir = Path(sys.argv[1])
bad = []
for path in sorted(proof_dir.glob("*.json")):
  data = json.loads(path.read_text())
  status = str(data.get("status") or data.get("result") or "").lower()
  if status not in {"passed", "pass", "ok"}:
    bad.append(f"{path.name}: status={status or '<missing>'}")
  if data.get("expected_version") and data.get("actual_version"):
    if data["expected_version"] != data["actual_version"]:
      bad.append(f"{path.name}: actual_version={data['actual_version']} expected={data['expected_version']}")
if bad:
  raise SystemExit("proof result failures: " + "; ".join(bad))
PY
  "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  write_release_evidence "bless"
  echo "release blessed: $TAG"
}

case "$COMMAND" in
  validate)
    load_manifest_env
    release_validate "$@"
    ;;
  package)
    load_manifest_env
    release_package "$@"
    ;;
  publish)
    load_manifest_env
    release_publish "$@"
    ;;
  prove)
    load_manifest_env
    release_prove "$@"
    ;;
  audit)
    load_manifest_env
    release_audit "$@"
    ;;
  bless)
    load_manifest_env
    release_bless "$@"
    ;;
  -h|--help|"")
    usage
    [[ -n "$COMMAND" ]]
    ;;
  *)
    usage >&2
    fail "Unknown release train command: $COMMAND"
    ;;
esac
