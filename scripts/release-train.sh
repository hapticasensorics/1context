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

Proof dry-run:
  scripts/release-train.sh prove --dry-run
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

quote_command() {
  printf '%q ' "$@"
  printf '\n'
}

developer_id_identity() {
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    security find-identity -v -p codesigning "$CODESIGN_KEYCHAIN"
  else
    security find-identity -v -p codesigning
  fi | awk -F'"' '/Developer ID Application:/ { print $2; exit }'
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
  ARCH="${ONECONTEXT_ARCH:-arm64}"
  DMG="$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg"
  SPARKLE_ACCOUNT="${SPARKLE_KEY_ACCOUNT:-com.haptica.1context.sparkle}"
  GENERATE_KEYS="$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_keys"
  CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-${ONECONTEXT_RELEASE_KEYCHAIN:-}}"
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
  "$ROOT/scripts/check-release-manifest.sh" --appcast "$ROOT/dist/appcast.xml"
  "$ROOT/scripts/release-manifest.py" write-asset-manifest --appcast "$ROOT/dist/appcast.xml" --output "$ASSET_MANIFEST"
}

release_validate() {
  "$ROOT/scripts/check-release-manifest.sh" --require-clean
  ensure_tag_ref
  "$ROOT/scripts/check-version-consistency.sh"
}

release_package() {
  release_validate
  mkdir -p "$EVIDENCE_DIR"
  "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  export ONECONTEXT_VERSION="$VERSION"
  export ONECONTEXT_SIGNING_MODE="developer-id"
  export CODESIGN_IDENTITY="${CODESIGN_IDENTITY:-$(developer_id_identity)}"
  if [[ -z "$CODESIGN_IDENTITY" ]]; then
    fail "No Developer ID Application signing identity found."
  fi
  export ONECONTEXT_SPARKLE_PUBLIC_ED_KEY="${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-$(sparkle_public_key)}"
  if [[ -z "$ONECONTEXT_SPARKLE_PUBLIC_ED_KEY" ]]; then
    fail "No Sparkle public key found for account '$SPARKLE_ACCOUNT'. Run: CREATE_SPARKLE_KEY=1 scripts/configure-macos-release-secrets.sh"
  fi
  "$ROOT/scripts/build-macos-app.sh"
  "$ROOT/scripts/notarize-macos-artifact.sh" "$ROOT/dist/1Context.app"
  "$ROOT/scripts/create-macos-dmg.sh" "$ROOT/dist/1Context.app" "$DMG" >/dev/null
  codesign_args=(--force --timestamp --sign "$CODESIGN_IDENTITY")
  if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
    codesign_args+=(--keychain "$CODESIGN_KEYCHAIN")
  fi
  codesign "${codesign_args[@]}" "$DMG" >/dev/null
  codesign --verify --strict "$DMG" >/dev/null
  "$ROOT/scripts/notarize-macos-artifact.sh" "$DMG"
  "$ROOT/scripts/validate-macos-dmg.sh" "$DMG"
  "$ROOT/scripts/generate-sparkle-appcast.sh" "$DMG"
  "$ROOT/scripts/release-manifest.py" validate --appcast "$ROOT/dist/sparkle-updates/appcast.xml"
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
  local mode="dispatch"
  local repo="${ONECONTEXT_GITHUB_REPO:-hapticasensorics/1context}"
  local workflow="${ONECONTEXT_RELEASE_PROOF_WORKFLOW:-self-hosted-mac-update-proof.yml}"
  local ref="$TAG"
  local proof_reason="manifest-driven $UPDATE_CLASS Sparkle proof for 1Context $VERSION"
  local update_timeout_seconds="${ONECONTEXT_RELEASE_PROOF_UPDATE_TIMEOUT_SECONDS:-420}"
  local steady_state_seconds="${ONECONTEXT_RELEASE_PROOF_STEADY_STATE_SECONDS:-120}"
  local artifact_retention_days="${ONECONTEXT_RELEASE_PROOF_ARTIFACT_RETENTION_DAYS:-21}"
  local old_tag="${ONECONTEXT_RELEASE_PROOF_OLD_TAG:-}"
  local old_dmg_url="${ONECONTEXT_RELEASE_PROOF_OLD_DMG_URL:-}"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run)
        mode="dry-run"
        shift
        ;;
      --dispatch)
        mode="dispatch"
        shift
        ;;
      --repo)
        repo="${2:?}"
        shift 2
        ;;
      --workflow)
        workflow="${2:?}"
        shift 2
        ;;
      --ref)
        ref="${2:?}"
        shift 2
        ;;
      --proof-reason)
        proof_reason="${2:?}"
        shift 2
        ;;
      --old-tag)
        old_tag="${2:?}"
        shift 2
        ;;
      --old-dmg-url)
        old_dmg_url="${2:?}"
        shift 2
        ;;
      --update-timeout-seconds)
        update_timeout_seconds="${2:?}"
        shift 2
        ;;
      --steady-state-seconds)
        steady_state_seconds="${2:?}"
        shift 2
        ;;
      --artifact-retention-days)
        artifact_retention_days="${2:?}"
        shift 2
        ;;
      *)
        fail "Unknown prove argument: $1"
        ;;
    esac
  done

  case "$ref" in
    main|release/*|rc/*|v*) ;;
    *)
      fail "Ref '$ref' is not allowed for the self-hosted runner. Use main, release/*, rc/*, or a v* tag."
      ;;
  esac

  if [[ "$mode" != "dry-run" ]]; then
    require_tool gh
    release_validate
    ensure_tag_ref
  else
    "$ROOT/scripts/release-manifest.py" validate
  fi

  mkdir -p "$PROOF_RESULTS_DIR"
  "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  local transcript="$EVIDENCE_DIR/release-proof-request.txt"
  cmd=(
    gh workflow run "$workflow"
    --repo "$repo"
    --ref "$ref"
    -f "proof_reason=$proof_reason"
    -f "old_version=$PREVIOUS_VERSION"
    -f "new_version=$VERSION"
    -f "staging_appcast_url=$PUBLIC_APPCAST_URL"
    -f "update_class=$UPDATE_CLASS"
    -f "update_timeout_seconds=$update_timeout_seconds"
    -f "steady_state_seconds=$steady_state_seconds"
    -f "artifact_retention_days=$artifact_retention_days"
  )
  if [[ -n "$old_tag" ]]; then
    cmd+=(-f "old_tag=$old_tag")
  fi
  if [[ -n "$old_dmg_url" ]]; then
    cmd+=(-f "old_dmg_url=$old_dmg_url")
  fi

  cat > "$transcript" <<SUMMARY
release_proof_request:
  mode: $mode
  repo: $repo
  workflow: $workflow
  ref: $ref
  old_version: $PREVIOUS_VERSION
  new_version: $VERSION
  update_class: $UPDATE_CLASS
  appcast_url: $PUBLIC_APPCAST_URL
  proof_reason: $proof_reason
gh_command: $(quote_command "${cmd[@]}")
SUMMARY
  cat "$transcript"

  if [[ "$mode" == "dry-run" ]]; then
    return
  fi

  gh auth status --hostname github.com >/dev/null
  local runs_before_json dispatch_started_at runs_json run_id artifact_dir
  runs_before_json="$(gh run list --repo "$repo" --workflow "$workflow" --event workflow_dispatch --limit 50 --json databaseId)"
  dispatch_started_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  "${cmd[@]}" | tee -a "$transcript"
  sleep 8
  runs_json="$(gh run list --repo "$repo" --workflow "$workflow" --event workflow_dispatch --limit 20 --json databaseId,url,status,conclusion,createdAt,headBranch)"
  run_id="$(
    RUNS_BEFORE_JSON="$runs_before_json" RUNS_JSON="$runs_json" python3 - "$dispatch_started_at" "$ref" <<'PY'
import datetime as dt
import json
import os
import sys

started = dt.datetime.fromisoformat(sys.argv[1].replace("Z", "+00:00"))
ref = sys.argv[2]
before = {str(run.get("databaseId")) for run in json.loads(os.environ["RUNS_BEFORE_JSON"])}
runs = json.loads(os.environ["RUNS_JSON"])
candidates = []
for run in runs:
  created = dt.datetime.fromisoformat(run["createdAt"].replace("Z", "+00:00"))
  head_branch = run.get("headBranch") or ""
  database_id = str(run.get("databaseId") or "")
  if database_id and database_id not in before and created >= started and (not head_branch or head_branch == ref):
    candidates.append(database_id)
if len(candidates) == 1:
  print(candidates[0])
elif len(candidates) > 1:
  print("AMBIGUOUS:" + ",".join(candidates))
PY
  )"
  if [[ -z "$run_id" ]]; then
    fail "Could not find a workflow_dispatch run to watch."
  fi
  if [[ "$run_id" == AMBIGUOUS:* ]]; then
    fail "Multiple new workflow_dispatch runs matched this request (${run_id#AMBIGUOUS:}); refusing to watch the wrong run."
  fi
  echo "watching_run_id=$run_id" | tee -a "$transcript"
  gh run watch "$run_id" --repo "$repo" --exit-status | tee -a "$transcript"
  artifact_dir="$EVIDENCE_DIR/self-hosted-run-$run_id"
  mkdir -p "$artifact_dir"
  gh run download "$run_id" --repo "$repo" --dir "$artifact_dir" | tee -a "$transcript"
  echo "artifact_dir=$artifact_dir" | tee -a "$transcript"
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
