#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMMAND="${1:-}"
if [[ $# -gt 0 ]]; then
  shift
fi
CHANNEL_ARG=""
POSITIONAL_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel)
      CHANNEL_ARG="${2:?}"
      shift 2
      ;;
    *)
      POSITIONAL_ARGS+=("$1")
      shift
      ;;
  esac
done
if ((${#POSITIONAL_ARGS[@]})); then
  set -- "${POSITIONAL_ARGS[@]}"
else
  set --
fi

usage() {
  cat <<'USAGE'
Usage: scripts/release-train.sh <validate|build|publish|prove|audit|bless> [--channel <name>]

Runs the manifest-driven release train. release/release.toml is the only release
source of truth; VERSION, update policy, appcast, workflows, and proof evidence
must all agree with it.

Build channels:
  scripts/release-train.sh build --channel dev
  scripts/release-train.sh build --channel prototype
  scripts/release-train.sh build --channel private
  scripts/release-train.sh build --channel official

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
  if [[ -n "$CHANNEL_ARG" ]]; then
    eval "$("$ROOT/scripts/release-manifest.py" export-env --channel "$CHANNEL_ARG")"
  else
    eval "$("$ROOT/scripts/release-manifest.py" export-env)"
  fi
  VERSION="$ONECONTEXT_RELEASE_VERSION"
  PREVIOUS_VERSION="$ONECONTEXT_RELEASE_PREVIOUS_VERSION"
  TAG="$ONECONTEXT_RELEASE_TAG"
  UPDATE_CLASS="$ONECONTEXT_RELEASE_UPDATE_CLASS"
  PUBLIC_APPCAST_URL="$ONECONTEXT_RELEASE_PUBLIC_APPCAST_URL"
  STABLE_DMG_NAME="$ONECONTEXT_RELEASE_STABLE_DMG_NAME"
  CHANNEL="$ONECONTEXT_RELEASE_CHANNEL"
  CHANNEL_REQUIRES_CLEAN_TREE="$ONECONTEXT_RELEASE_CHANNEL_REQUIRES_CLEAN_TREE"
  CHANNEL_REQUIRES_TAG="$ONECONTEXT_RELEASE_CHANNEL_REQUIRES_TAG"
  CHANNEL_SIGNING_MODE="$ONECONTEXT_RELEASE_CHANNEL_SIGNING_MODE"
  CHANNEL_NOTARIZE="$ONECONTEXT_RELEASE_CHANNEL_NOTARIZE"
  CHANNEL_APPCAST="$ONECONTEXT_RELEASE_CHANNEL_APPCAST"
  CHANNEL_ARTIFACT_REPO="$ONECONTEXT_RELEASE_CHANNEL_ARTIFACT_REPO"
  CHANNEL_PUBLIC_ASSET_MUTATION="$ONECONTEXT_RELEASE_CHANNEL_PUBLIC_ASSET_MUTATION"
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
    "timing_summary": "timing-summary.json",
    "proof_results": "proof-results/*.json",
  },
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

write_timing_summary() {
  local timing_dir="$EVIDENCE_DIR/timings"
  local output="$EVIDENCE_DIR/timing-summary.json"
  [[ -d "$timing_dir" ]] || return 0
  python3 - "$timing_dir" "$output" "$VERSION" "$CHANNEL" <<'PY'
import datetime as dt
import json
import sys
from pathlib import Path

timing_dir = Path(sys.argv[1])
output = Path(sys.argv[2])
version = sys.argv[3]
channel = sys.argv[4]

def load_json(path: Path) -> dict:
  try:
    data = json.loads(path.read_text(encoding="utf-8"))
  except json.JSONDecodeError as exc:
    raise SystemExit(f"invalid timing JSON {path}: {exc}") from exc
  data["_path"] = str(path.relative_to(timing_dir.parent))
  return data

stage_records = [
  load_json(path)
  for path in sorted(timing_dir.glob("*.json"))
  if path.name != "summary.json"
]
steps_dir = timing_dir / "steps"
step_records = [
  load_json(path)
  for path in sorted(steps_dir.glob("*.json"))
] if steps_dir.is_dir() else []

def public_record(record: dict) -> dict:
  return {
    key: record[key]
    for key in (
      "_path",
      "stage",
      "step",
      "channel",
      "status",
      "elapsed_seconds",
      "budget_seconds",
      "budget_advisory",
      "budget_exceeded",
      "started_at",
      "ended_at",
    )
    if key in record
  }

all_records = stage_records + step_records
budget_exceeded = [
  public_record(record)
  for record in all_records
  if bool(record.get("budget_exceeded"))
]
failed = [
  public_record(record)
  for record in all_records
  if str(record.get("status", "")).lower() not in {"passed", "dry-run"}
]
slowest_steps = sorted(
  (public_record(record) for record in step_records),
  key=lambda record: int(record.get("elapsed_seconds", 0)),
  reverse=True,
)[:8]

output.write_text(json.dumps({
  "schema_version": "1context.release-timing-summary.v1",
  "version": version,
  "channel": channel,
  "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
  "stage_count": len(stage_records),
  "step_count": len(step_records),
  "stage_elapsed_seconds": sum(int(record.get("elapsed_seconds", 0)) for record in stage_records),
  "step_elapsed_seconds": sum(int(record.get("elapsed_seconds", 0)) for record in step_records),
  "budget_exceeded_count": len(budget_exceeded),
  "failed_count": len(failed),
  "budget_exceeded": budget_exceeded,
  "failed": failed,
  "stages": [public_record(record) for record in stage_records],
  "slowest_steps": slowest_steps,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

write_stage_timing() {
  local stage="$1"
  local status="$2"
  local started_epoch="$3"
  local ended_epoch elapsed_seconds budget_key budget_seconds advisory output
  ended_epoch="$(date +%s)"
  elapsed_seconds=$((ended_epoch - started_epoch))
  budget_key="$(printf 'ONECONTEXT_RELEASE_%s_SECONDS' "$(printf '%s' "$stage" | tr '[:lower:]' '[:upper:]')")"
  budget_seconds="${!budget_key:-0}"
  advisory="${ONECONTEXT_RELEASE_BUDGET_ADVISORY:-1}"
  output="$EVIDENCE_DIR/timings/$stage-$CHANNEL.json"
  mkdir -p "$(dirname "$output")"
  python3 - "$output" \
    "${ONECONTEXT_RELEASE_STAGE_TIMING_SCHEMA:-1context.release-stage-timing.v1}" \
    "$stage" "$CHANNEL" "$status" "$started_epoch" "$ended_epoch" \
    "$elapsed_seconds" "$budget_seconds" "$advisory" <<'PY'
import datetime as dt
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
started = int(sys.argv[6])
ended = int(sys.argv[7])
elapsed = int(sys.argv[8])
budget = int(sys.argv[9])
advisory = sys.argv[10] == "1"
output.write_text(json.dumps({
  "schema_version": sys.argv[2],
  "stage": sys.argv[3],
  "channel": sys.argv[4],
  "status": sys.argv[5],
  "started_epoch": started,
  "ended_epoch": ended,
  "started_at": dt.datetime.fromtimestamp(started, dt.timezone.utc).isoformat(),
  "ended_at": dt.datetime.fromtimestamp(ended, dt.timezone.utc).isoformat(),
  "elapsed_seconds": elapsed,
  "budget_seconds": budget,
  "budget_advisory": advisory,
  "budget_exceeded": budget > 0 and elapsed > budget,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  write_timing_summary
  if [[ "$advisory" != "1" && "$budget_seconds" =~ ^[0-9]+$ && "$budget_seconds" != "0" && "$elapsed_seconds" -gt "$budget_seconds" ]]; then
    fail "$stage exceeded ${budget_seconds}s budget for $CHANNEL channel (${elapsed_seconds}s)."
  fi
}

write_step_timing() {
  local stage="$1"
  local step="$2"
  local status="$3"
  local started_epoch="$4"
  local ended_epoch elapsed_seconds slug output
  ended_epoch="$(date +%s)"
  elapsed_seconds=$((ended_epoch - started_epoch))
  slug="$(printf '%s' "$step" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9]+/-/g; s/^-//; s/-$//')"
  output="$EVIDENCE_DIR/timings/steps/$stage-$CHANNEL-$slug.json"
  mkdir -p "$(dirname "$output")"
  python3 - "$output" \
    "${ONECONTEXT_RELEASE_STAGE_TIMING_SCHEMA:-1context.release-stage-timing.v1}" \
    "$stage" "$step" "$CHANNEL" "$status" "$started_epoch" "$ended_epoch" \
    "$elapsed_seconds" <<'PY'
import datetime as dt
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
started = int(sys.argv[7])
ended = int(sys.argv[8])
elapsed = int(sys.argv[9])
output.write_text(json.dumps({
  "schema_version": sys.argv[2],
  "stage": sys.argv[3],
  "step": sys.argv[4],
  "channel": sys.argv[5],
  "status": sys.argv[6],
  "started_epoch": started,
  "ended_epoch": ended,
  "started_at": dt.datetime.fromtimestamp(started, dt.timezone.utc).isoformat(),
  "ended_at": dt.datetime.fromtimestamp(ended, dt.timezone.utc).isoformat(),
  "elapsed_seconds": elapsed,
  "budget_seconds": 0,
  "budget_advisory": True,
  "budget_exceeded": False,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  write_timing_summary
}

time_release_step() {
  local stage="$1"
  local step="$2"
  shift 2
  local started_epoch exit_code
  started_epoch="$(date +%s)"
  set +e
  (
    set -euo pipefail
    "$@"
  )
  exit_code=$?
  set -e
  if [[ "$exit_code" -eq 0 ]]; then
    write_step_timing "$stage" "$step" "passed" "$started_epoch"
    return 0
  else
    write_step_timing "$stage" "$step" "failed" "$started_epoch"
    return "$exit_code"
  fi
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

write_sparkle_fixture_proof_results() {
  local test_log="$EVIDENCE_DIR/sparkle-fixture-tests.log"
  mkdir -p "$EVIDENCE_DIR"
  time_release_step "prove" "run_sparkle_fixture_tests" sh -c 'swift test --package-path "$1" --filter OneContextSparkleUpdateTests > "$2" 2>&1' sh "$ROOT/macos" "$test_log"
  time_release_step "prove" "write_sparkle_fixture_results" "$ROOT/scripts/release-manifest.py" write-fixture-proof-results --output-dir "$PROOF_RESULTS_DIR"
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
    "asset-manifest.json",
}
missing = sorted(required - assets)
if missing:
    raise SystemExit(f"release is missing assets: {', '.join(missing)}")
print(release.get("url", ""))
PY

  release_audit_log "downloading appcast.xml"
  gh release download "$tag" --repo "$repo" --pattern appcast.xml --dir "$work_dir" --clobber >/dev/null
  "$ROOT/scripts/release-manifest.py" validate --appcast "$work_dir/appcast.xml"
  gh release download "$tag" --repo "$repo" --pattern asset-manifest.json --dir "$work_dir" --clobber >/dev/null
  mkdir -p "$(dirname "$ASSET_MANIFEST")"
  cp "$work_dir/asset-manifest.json" "$ASSET_MANIFEST"

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
  "$ROOT/scripts/release-manifest.py" validate --appcast "$ROOT/dist/appcast.xml" --channel official
  "$ROOT/scripts/release-manifest.py" write-asset-manifest --appcast "$ROOT/dist/appcast.xml" --output "$ASSET_MANIFEST"
}

release_validate() {
  local args=(validate)
  if [[ "${CHANNEL_REQUIRES_CLEAN_TREE:-0}" == "1" ]]; then
    args+=(--require-clean)
  fi
  "$ROOT/scripts/release-manifest.py" "${args[@]}" --channel "$CHANNEL"
  if [[ "${CHANNEL_REQUIRES_TAG:-0}" == "1" ]]; then
    ensure_tag_ref
  fi
  "$ROOT/scripts/check-version-consistency.sh"
}

release_build() {
  local dry_run="0"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run)
        dry_run="1"
        shift
        ;;
      *)
        fail "Unknown build argument: $1"
        ;;
    esac
  done

  local stage_started
  stage_started="$(date +%s)"
  time_release_step "build" "validate_preflight" release_validate
  mkdir -p "$EVIDENCE_DIR"
  time_release_step "build" "write_runner_attestation" "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  if [[ "$dry_run" == "1" ]]; then
    write_release_evidence "build-$CHANNEL-dry-run"
    write_stage_timing "build" "dry-run" "$stage_started"
    return
  fi

  export ONECONTEXT_VERSION="$VERSION"
  export ONECONTEXT_SIGNING_MODE="$CHANNEL_SIGNING_MODE"
  if [[ "$CHANNEL_SIGNING_MODE" == "developer-id" ]]; then
    export CODESIGN_IDENTITY="${CODESIGN_IDENTITY:-$(developer_id_identity)}"
    if [[ -z "$CODESIGN_IDENTITY" ]]; then
      fail "No Developer ID Application signing identity found."
    fi
  fi
  if [[ "$CHANNEL_APPCAST" == "none" ]]; then
    unset ONECONTEXT_SPARKLE_PUBLIC_ED_KEY
  else
    export ONECONTEXT_SPARKLE_PUBLIC_ED_KEY="${ONECONTEXT_SPARKLE_PUBLIC_ED_KEY:-$(sparkle_public_key)}"
    if [[ -z "$ONECONTEXT_SPARKLE_PUBLIC_ED_KEY" ]]; then
      fail "No Sparkle public key found for account '$SPARKLE_ACCOUNT'. Run: CREATE_SPARKLE_KEY=1 scripts/configure-macos-release-secrets.sh"
    fi
  fi
  time_release_step "build" "build_app_bundle" "$ROOT/scripts/build-macos-app.sh"
  if [[ "$CHANNEL" == "dev" ]]; then
    write_release_evidence "build-$CHANNEL"
    time_release_step "build" "redact_evidence" "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
    time_release_step "build" "audit_evidence_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
    write_stage_timing "build" "passed" "$stage_started"
    return
  fi

  if [[ "$CHANNEL_NOTARIZE" == "1" ]]; then
    time_release_step "build" "notarize_app_bundle" "$ROOT/scripts/notarize-macos-artifact.sh" "$ROOT/dist/1Context.app"
  fi
  time_release_step "build" "create_dmg" "$ROOT/scripts/create-macos-dmg.sh" "$ROOT/dist/1Context.app" "$DMG" >/dev/null
  if [[ "$CHANNEL_SIGNING_MODE" == "developer-id" ]]; then
    codesign_args=(--force --timestamp --sign "$CODESIGN_IDENTITY")
    if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
      codesign_args+=(--keychain "$CODESIGN_KEYCHAIN")
    fi
    time_release_step "build" "sign_dmg" codesign "${codesign_args[@]}" "$DMG" >/dev/null
    time_release_step "build" "verify_signed_dmg" codesign --verify --strict "$DMG" >/dev/null
  fi
  if [[ "$CHANNEL_NOTARIZE" == "1" ]]; then
    time_release_step "build" "notarize_dmg" "$ROOT/scripts/notarize-macos-artifact.sh" "$DMG"
    time_release_step "build" "validate_dmg" "$ROOT/scripts/validate-macos-dmg.sh" "$DMG"
  else
    time_release_step "build" "validate_dmg" env ALLOW_UNNOTARIZED=1 "$ROOT/scripts/validate-macos-dmg.sh" "$DMG"
  fi
  if [[ "$CHANNEL_APPCAST" != "none" ]]; then
    time_release_step "build" "generate_appcast" "$ROOT/scripts/generate-sparkle-appcast.sh" "$DMG"
    time_release_step "build" "validate_appcast" "$ROOT/scripts/release-manifest.py" validate --appcast "$ROOT/dist/sparkle-updates/appcast.xml" --channel "$CHANNEL"
    if [[ "$CHANNEL" == "official" ]]; then
      time_release_step "build" "collect_release_assets" collect_release_assets
    else
      mkdir -p "$ROOT/dist/$CHANNEL"
      cp "$ROOT/dist/sparkle-updates/appcast.xml" "$ROOT/dist/$CHANNEL/appcast.xml"
    fi
  fi
  write_release_evidence "build-$CHANNEL"
  time_release_step "build" "redact_evidence" "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  time_release_step "build" "audit_evidence_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  write_stage_timing "build" "passed" "$stage_started"
}

release_publish() {
  local stage_started
  stage_started="$(date +%s)"
  if [[ "$CHANNEL" == "private" ]]; then
    release_publish_private "$stage_started"
    return
  fi
  if [[ "$CHANNEL" != "official" ]]; then
    fail "publish --channel $CHANNEL is not wired yet; only official public publishing is active."
  fi
  require_tool gh
  time_release_step "publish" "validate_preflight" release_validate
  time_release_step "publish" "ensure_tag_ref" ensure_tag_ref
  mkdir -p "$EVIDENCE_DIR"
  if [[ ! -f "$ASSET_MANIFEST" ]]; then
    time_release_step "publish" "write_asset_manifest" "$ROOT/scripts/release-manifest.py" write-asset-manifest --appcast "$ROOT/dist/appcast.xml" --output "$ASSET_MANIFEST"
  fi
  time_release_step "publish" "write_runner_attestation" "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  write_release_evidence "publish-preflight"
  time_release_step "publish" "redact_preflight_evidence" "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  time_release_step "publish" "audit_preflight_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  if ! gh release view "$TAG" >/dev/null 2>&1; then
    time_release_step "publish" "create_github_release" gh release create "$TAG" --title "1Context $TAG" --notes-file "$ROOT/RELEASE_NOTES.md"
  fi
  time_release_step "publish" "upload_official_assets" gh release upload "$TAG" \
    "$ROOT/dist/1Context-$VERSION-macos-arm64.dmg" \
    "$ROOT/dist/1Context-$VERSION-macos-arm64.dmg.sha256" \
    "$ROOT/dist/$STABLE_DMG_NAME" \
    "$ROOT/dist/$STABLE_DMG_NAME.sha256" \
    "$ROOT/dist/appcast.xml" \
    "$ASSET_MANIFEST" \
    --clobber
  time_release_step "publish" "audit_public_release_assets" audit_public_release_assets "$TAG"
  write_release_evidence "publish"
  time_release_step "publish" "redact_evidence" "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  time_release_step "publish" "audit_evidence_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  write_stage_timing "publish" "passed" "$stage_started"
}

release_publish_private() {
  local stage_started="$1"
  local repo="$CHANNEL_ARTIFACT_REPO"
  local private_dir="$ROOT/dist/private"
  local private_appcast="$private_dir/appcast.xml"
  local private_asset_manifest="$EVIDENCE_DIR/private-asset-manifest.json"
  local versioned_dmg="$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg"
  local versioned_sha="$private_dir/1Context-$VERSION-macos-$ARCH.dmg.sha256"
  require_tool gh
  time_release_step "publish" "validate_preflight" release_validate
  mkdir -p "$EVIDENCE_DIR" "$private_dir"
  [[ -f "$versioned_dmg" ]] || fail "Missing private release DMG: $versioned_dmg"
  [[ -f "$private_appcast" ]] || fail "Missing private appcast. Run: scripts/release-train.sh build --channel private"
  time_release_step "publish" "validate_private_appcast" "$ROOT/scripts/release-manifest.py" validate --channel private --appcast "$private_appcast"
  time_release_step "publish" "write_private_checksum" sh -c 'shasum -a 256 "$1" > "$2"' sh "$versioned_dmg" "$versioned_sha"
  time_release_step "publish" "write_runner_attestation" "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
  python3 - "$private_asset_manifest" "$VERSION" "$TAG" "$versioned_dmg" "$versioned_sha" "$private_appcast" "$repo" <<'PY'
import datetime as dt
import hashlib
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
version = sys.argv[2]
tag = sys.argv[3]
repo = sys.argv[7]

def sha256(path: Path) -> str:
  h = hashlib.sha256()
  with path.open("rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
      h.update(chunk)
  return h.hexdigest()

assets = []
for raw in sys.argv[4:7]:
  path = Path(raw)
  assets.append({
    "name": path.name,
    "path": str(path),
    "size": path.stat().st_size,
    "sha256": sha256(path),
  })
output.write_text(json.dumps({
  "schema_version": "1context.private-asset-manifest.v1",
  "version": version,
  "tag": tag,
  "repo": repo,
  "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
  "assets": assets,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

  if ! gh release view "$TAG" --repo "$repo" >/dev/null 2>&1; then
    time_release_step "publish" "create_private_release" gh release create "$TAG" --repo "$repo" --target main --title "1Context $TAG private" \
      --notes "Private 1Context release-factory update assets for $TAG."
  fi
  time_release_step "publish" "upload_private_assets" gh release upload "$TAG" --repo "$repo" \
    "$versioned_dmg" \
    "$versioned_sha" \
    "$private_appcast#appcast.xml" \
    "$private_asset_manifest" \
    --clobber

  local work_dir
  work_dir="$(mktemp -d /tmp/1context-private-release-audit-XXXXXX)"
  time_release_step "publish" "download_private_appcast" gh release download "$TAG" --repo "$repo" --pattern appcast.xml --dir "$work_dir" --clobber >/dev/null
  time_release_step "publish" "download_private_dmg" gh release download "$TAG" --repo "$repo" --pattern "1Context-$VERSION-macos-$ARCH.dmg" --dir "$work_dir" --clobber >/dev/null
  time_release_step "publish" "download_private_checksum" gh release download "$TAG" --repo "$repo" --pattern "1Context-$VERSION-macos-$ARCH.dmg.sha256" --dir "$work_dir" --clobber >/dev/null
  time_release_step "publish" "audit_private_appcast" "$ROOT/scripts/release-manifest.py" validate --channel private --appcast "$work_dir/appcast.xml"
  time_release_step "publish" "audit_private_checksum" sh -c 'cd "$1" && shasum -a 256 --check "$2" >/dev/null' sh "$work_dir" "1Context-$VERSION-macos-$ARCH.dmg.sha256"
  write_release_evidence "publish-private"
  time_release_step "publish" "redact_evidence" "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  time_release_step "publish" "audit_evidence_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  write_stage_timing "publish" "passed" "$stage_started"
  rm -rf "$work_dir"
}

release_prove() {
  local stage_started
  stage_started="$(date +%s)"
  local mode="dispatch"
  local repo="${ONECONTEXT_GITHUB_REPO:-hapticasensorics/1context}"
  local default_workflow="self-hosted-mac-update-proof.yml"
  local ref="$TAG"
  local proof_reason="manifest-driven $UPDATE_CLASS Sparkle proof for 1Context $VERSION"
  if [[ "$CHANNEL" == "private" ]]; then
    default_workflow="self-hosted-mac-private-update-proof.yml"
    ref="main"
  fi
  local workflow="${ONECONTEXT_RELEASE_PROOF_WORKFLOW:-$default_workflow}"

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
    "$ROOT/scripts/release-manifest.py" validate --require-clean --channel "$CHANNEL"
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
    export ONECONTEXT_STAGING_APPCAST_URL="$ONECONTEXT_SPARKLE_FEED_URL"
    export ONECONTEXT_EXPECTED_UPDATE_CLASS="$UPDATE_CLASS"
    export ONECONTEXT_REMOTE_UPDATE_MANIFEST_CHANNEL="$CHANNEL"
    if [[ "$CHANNEL" == "private" ]]; then
      export ONECONTEXT_RUN_UNINSTALL_REINSTALL_PROOF=0
      export ONECONTEXT_GITHUB_REPO="$CHANNEL_ARTIFACT_REPO"
      export ONECONTEXT_REMOTE_APPCAST_GITHUB_REPO="$ONECONTEXT_GITHUB_REPO"
      export ONECONTEXT_UPDATE_RUNNER_ALLOW_NON_PUBLIC_FINAL_FEED=1
      export ONECONTEXT_UPDATE_RUNNER_RESTORE_PUBLIC_FINAL_FEED=0
    else
      export ONECONTEXT_RUN_UNINSTALL_REINSTALL_PROOF=1
      export ONECONTEXT_UPDATE_RUNNER_ALLOW_DELETE_DATA=1
    fi
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
    time_release_step "prove" "validate_preflight" release_validate
    if [[ "${CHANNEL_REQUIRES_TAG:-0}" == "1" ]]; then
      time_release_step "prove" "ensure_tag_ref" ensure_tag_ref
    fi
  else
    time_release_step "prove" "validate_preflight" "$ROOT/scripts/release-manifest.py" validate --channel "$CHANNEL"
  fi

  mkdir -p "$PROOF_RESULTS_DIR"
  time_release_step "prove" "write_runner_attestation" "$ROOT/scripts/write-runner-attestation.sh" "$RUNNER_ATTESTATION"
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
  channel: $CHANNEL
  channel_appcast: $CHANNEL_APPCAST
  channel_appcast_url: $ONECONTEXT_SPARKLE_FEED_URL
  proof_reason: $proof_reason
gh_command: $(quote_command "${cmd[@]}")
SUMMARY
  cat "$transcript"

  if [[ "$mode" == "dry-run" ]]; then
    write_stage_timing "prove" "dry-run" "$stage_started"
    return
  fi

  time_release_step "prove" "gh_auth_status" gh auth status --hostname github.com >/dev/null
  local runs_before_json dispatch_started_at runs_json run_id artifact_dir
  runs_before_json="$(gh run list --repo "$repo" --workflow "$workflow" --event workflow_dispatch --limit 50 --json databaseId)"
  dispatch_started_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  time_release_step "prove" "dispatch_workflow" sh -c 'transcript="$1"; shift; "$@" | tee -a "$transcript"' sh "$transcript" "${cmd[@]}"
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
  time_release_step "prove" "watch_workflow" sh -c 'gh run watch "$1" --repo "$2" --exit-status | tee -a "$3"' sh "$run_id" "$repo" "$transcript"
  artifact_dir="$EVIDENCE_DIR/self-hosted-run-$run_id"
  mkdir -p "$artifact_dir"
  time_release_step "prove" "download_proof_artifacts" sh -c 'gh run download "$1" --repo "$2" --dir "$3" | tee -a "$4"' sh "$run_id" "$repo" "$artifact_dir" "$transcript"
  echo "artifact_dir=$artifact_dir" | tee -a "$transcript"
  time_release_step "prove" "collect_proof_results" collect_downloaded_proof_results "$artifact_dir"
  write_sparkle_fixture_proof_results
  write_release_evidence "prove"
  time_release_step "prove" "redact_evidence" "$ROOT/scripts/redact-evidence.sh" "$EVIDENCE_DIR"
  time_release_step "prove" "audit_evidence_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  write_stage_timing "prove" "passed" "$stage_started"
}

release_audit() {
  local stage_started
  stage_started="$(date +%s)"
  if [[ "$CHANNEL" != "official" ]]; then
    fail "audit --channel $CHANNEL is not wired yet; only official public audit is active."
  fi
  time_release_step "audit" "validate_preflight" release_validate
  time_release_step "audit" "audit_public_release_assets" audit_public_release_assets "$TAG"
  if [[ -d "$EVIDENCE_DIR" ]]; then
    time_release_step "audit" "audit_evidence_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  fi
  write_stage_timing "audit" "passed" "$stage_started"
}

release_bless() {
  local stage_started
  stage_started="$(date +%s)"
  if [[ "$CHANNEL" != "official" ]]; then
    fail "bless --channel $CHANNEL is only valid for the official channel."
  fi
  time_release_step "bless" "validate_preflight" release_validate
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
  time_release_step "bless" "audit_evidence_redaction" "$ROOT/scripts/audit-evidence-redaction.sh" "$EVIDENCE_DIR"
  write_release_evidence "bless"
  write_stage_timing "bless" "passed" "$stage_started"
  echo "release blessed: $TAG"
}

case "$COMMAND" in
  validate)
    load_manifest_env
    stage_started="$(date +%s)"
    release_validate "$@"
    write_stage_timing "validate" "passed" "$stage_started"
    ;;
  build)
    load_manifest_env
    release_build "$@"
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
