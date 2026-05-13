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

release_audit_log() {
  printf '[release-audit] %s\n' "$*"
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

collect_downloaded_proof_results() {
  local artifact_dir="$1"
  local list_file="$EVIDENCE_DIR/downloaded-proof-results.txt"
  mkdir -p "$PROOF_RESULTS_DIR"
  find "$artifact_dir" -path "*/proof-results/*.json" -type f | sort > "$list_file"
  if [[ ! -s "$list_file" ]]; then
    fail "Downloaded self-hosted proof artifacts did not contain proof-results/*.json under $artifact_dir."
  fi
  while IFS= read -r proof_json; do
    cp "$proof_json" "$PROOF_RESULTS_DIR/$(basename "$proof_json")"
  done < "$list_file"
}

audit_public_release_assets() (
  set -euo pipefail

  local tag="$1"
  local repo="${ONECONTEXT_GITHUB_REPO:-hapticasensorics/1context}"
  local probes="${ONECONTEXT_RELEASE_AUDIT_PROBES:-1}"
  local probe_interval_seconds="${ONECONTEXT_RELEASE_AUDIT_INTERVAL_SECONDS:-0}"
  local latest_appcast_url="${ONECONTEXT_LATEST_APPCAST_URL:-https://github.com/$repo/releases/latest/download/appcast.xml}"
  local stable_dmg_url="${ONECONTEXT_STABLE_DMG_URL:-https://github.com/$repo/releases/latest/download/1Context.dmg}"
  local work_dir
  work_dir="$(mktemp -d /tmp/1context-release-audit-XXXXXX)"
  trap 'rm -rf "$work_dir"' EXIT

  require_tool gh
  require_tool python3
  require_tool curl

  release_audit_log "reading GitHub release $repo@$tag"
  gh release view "$tag" --repo "$repo" --json tagName,isDraft,isPrerelease,assets,url > "$work_dir/release.json"

  python3 - "$work_dir/release.json" "$tag" "$VERSION" <<'PY'
import json
import sys
from pathlib import Path

release = json.loads(Path(sys.argv[1]).read_text())
tag = sys.argv[2]
version = sys.argv[3]
if release.get("tagName") != tag:
    raise SystemExit(f"release tag {release.get('tagName')!r} != expected {tag!r}")
if release.get("isDraft"):
    raise SystemExit("release is still draft")
if release.get("isPrerelease"):
    raise SystemExit("release is marked prerelease")
assets = {asset["name"] for asset in release.get("assets", [])}
required = {
    f"1Context-{version}-macos-arm64.dmg",
    f"1Context-{version}-macos-arm64.dmg.sha256",
    "1Context.dmg",
    "1Context.dmg.sha256",
    "appcast.xml",
}
missing = sorted(required - assets)
if missing:
    raise SystemExit(f"release is missing assets: {', '.join(missing)}")
print(release.get("url", ""))
PY

  release_audit_log "downloading appcast.xml"
  gh release download "$tag" --repo "$repo" --pattern appcast.xml --dir "$work_dir" --clobber >/dev/null
  "$ROOT/scripts/release-manifest.py" validate --appcast "$work_dir/appcast.xml"

  if ! [[ "$probes" =~ ^[0-9]+$ ]] || (( probes < 1 )); then
    fail "ONECONTEXT_RELEASE_AUDIT_PROBES must be a positive integer."
  fi
  if ! [[ "$probe_interval_seconds" =~ ^[0-9]+$ ]]; then
    fail "ONECONTEXT_RELEASE_AUDIT_INTERVAL_SECONDS must be a non-negative integer."
  fi

  release_audit_log "checking latest/download appcast propagation"
  local latest_ok=0
  local probe
  for ((probe = 1; probe <= probes; probe++)); do
    if (( probe > 1 && probe_interval_seconds > 0 )); then
      sleep "$probe_interval_seconds"
    fi
    if curl --fail --location --silent --show-error "$latest_appcast_url" --output "$work_dir/latest-appcast.xml" &&
      "$ROOT/scripts/release-manifest.py" validate --appcast "$work_dir/latest-appcast.xml" &&
      cmp -s "$work_dir/appcast.xml" "$work_dir/latest-appcast.xml"; then
      latest_ok=1
      release_audit_log "latest/download appcast probe $probe/$probes passed"
      break
    fi
    release_audit_log "latest/download appcast probe $probe/$probes has not propagated yet"
  done
  if [[ "$latest_ok" != "1" ]]; then
    fail "latest/download appcast does not match the $tag appcast yet: $latest_appcast_url"
  fi

  release_audit_log "probing appcast enclosure download"
  gh release download "$tag" \
    --repo "$repo" \
    --pattern "1Context-$VERSION-macos-arm64.dmg.sha256" \
    --pattern "1Context.dmg.sha256" \
    --dir "$work_dir" \
    --clobber >/dev/null

  python3 - "$work_dir/appcast.xml" "$VERSION" > "$work_dir/enclosure.txt" <<'PY'
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import urlparse

appcast = Path(sys.argv[1])
version = sys.argv[2]
root = ET.parse(appcast).getroot()
item = root.find("channel/item")
if item is None:
    raise SystemExit("appcast missing channel/item")
enclosure = item.find("enclosure")
if enclosure is None:
    raise SystemExit("appcast missing enclosure")
url = enclosure.attrib.get("url", "")
if not url:
    raise SystemExit("appcast enclosure missing url")
name = Path(urlparse(url).path).name
expected_name = f"1Context-{version}-macos-arm64.dmg"
if name != expected_name:
    raise SystemExit(f"appcast enclosure asset {name!r} != expected {expected_name!r}")
length = enclosure.attrib.get("length", "")
print(url)
print(length)
print(name)
PY

  local enclosure_url enclosure_length enclosure_name expected_sha stable_expected_sha
  enclosure_url="$(sed -n '1p' "$work_dir/enclosure.txt")"
  enclosure_length="$(sed -n '2p' "$work_dir/enclosure.txt")"
  enclosure_name="$(sed -n '3p' "$work_dir/enclosure.txt")"
  expected_sha="$(awk '{ print $1; exit }' "$work_dir/$enclosure_name.sha256")"
  stable_expected_sha="$(awk '{ print $1; exit }' "$work_dir/1Context.dmg.sha256")"

  if [[ -z "$expected_sha" ]]; then
    fail "Release checksum file for $enclosure_name is empty."
  fi
  if [[ -z "$stable_expected_sha" ]]; then
    fail "Release checksum file for 1Context.dmg is empty."
  fi
  if [[ "$stable_expected_sha" != "$expected_sha" ]]; then
    fail "Stable 1Context.dmg checksum $stable_expected_sha != versioned $enclosure_name checksum $expected_sha."
  fi

  for ((probe = 1; probe <= probes; probe++)); do
    if (( probe > 1 && probe_interval_seconds > 0 )); then
      sleep "$probe_interval_seconds"
    fi
    local output actual_size actual_sha stable_output stable_size stable_sha
    output="$work_dir/enclosure-$probe.dmg"
    curl --fail --location --silent --show-error "$enclosure_url" --output "$output"
    actual_size="$(wc -c < "$output" | tr -d '[:space:]')"
    if [[ -n "$enclosure_length" && "$actual_size" != "$enclosure_length" ]]; then
      fail "Downloaded enclosure size $actual_size != appcast length $enclosure_length on probe $probe."
    fi
    actual_sha="$(shasum -a 256 "$output" | awk '{ print $1 }')"
    if [[ "$actual_sha" != "$expected_sha" ]]; then
      fail "Downloaded enclosure sha256 $actual_sha != expected $expected_sha on probe $probe."
    fi
    release_audit_log "appcast enclosure probe $probe/$probes passed"

    stable_output="$work_dir/stable-$probe.dmg"
    curl --fail --location --silent --show-error "$stable_dmg_url" --output "$stable_output"
    stable_size="$(wc -c < "$stable_output" | tr -d '[:space:]')"
    if [[ -n "$enclosure_length" && "$stable_size" != "$enclosure_length" ]]; then
      fail "Downloaded stable 1Context.dmg size $stable_size != appcast length $enclosure_length on probe $probe."
    fi
    stable_sha="$(shasum -a 256 "$stable_output" | awk '{ print $1 }')"
    if [[ "$stable_sha" != "$expected_sha" ]]; then
      fail "Downloaded stable 1Context.dmg sha256 $stable_sha != versioned expected $expected_sha on probe $probe."
    fi
    release_audit_log "stable 1Context.dmg probe $probe/$probes passed"
  done

  release_audit_log "release asset audit passed for $tag"
)

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
  audit_public_release_assets "$TAG"
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
      --runner-execute)
        mode="runner-execute"
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
      *)
        fail "Unknown prove argument: $1"
        ;;
    esac
  done

  if [[ "$mode" == "runner-execute" ]]; then
    "$ROOT/scripts/check-release-manifest.sh" --require-clean
    local forbidden_runner_env
    for forbidden_runner_env in \
      ONECONTEXT_OLD_VERSION \
      ONECONTEXT_NEW_VERSION \
      ONECONTEXT_OLD_TAG \
      ONECONTEXT_OLD_DMG_URL \
      ONECONTEXT_STAGING_APPCAST_URL \
      ONECONTEXT_EXPECTED_UPDATE_CLASS \
      ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS \
      ONECONTEXT_STEADY_STATE_SECONDS
    do
      if [[ -n "${!forbidden_runner_env:-}" ]]; then
        fail "Runner release facts must come from release/release.toml; unset $forbidden_runner_env."
      fi
    done
    export ONECONTEXT_OLD_VERSION="$PREVIOUS_VERSION"
    export ONECONTEXT_NEW_VERSION="$VERSION"
    export ONECONTEXT_STAGING_APPCAST_URL="$PUBLIC_APPCAST_URL"
    export ONECONTEXT_EXPECTED_UPDATE_CLASS="$UPDATE_CLASS"
    exec "$ROOT/scripts/release/internal/self-hosted-update-proof.sh"
  fi

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
  )

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
  collect_downloaded_proof_results "$artifact_dir"
  write_release_evidence "prove"
  "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
}

release_audit() {
  release_validate
  audit_public_release_assets "$TAG"
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
