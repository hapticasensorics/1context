#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="${ONECONTEXT_GITHUB_REPO:-hapticasensorics/1context}"
WORKFLOW="${ONECONTEXT_RELEASE_PROOF_WORKFLOW:-self-hosted-mac-update-proof.yml}"
APPCAST_URL="${ONECONTEXT_RELEASE_PROOF_APPCAST_URL:-https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml}"
REF="${ONECONTEXT_RELEASE_PROOF_REF:-}"
OLD_VERSION="${ONECONTEXT_RELEASE_PROOF_OLD_VERSION:-}"
NEW_VERSION="${ONECONTEXT_RELEASE_PROOF_NEW_VERSION:-}"
UPDATE_CLASS="${ONECONTEXT_RELEASE_PROOF_UPDATE_CLASS:-}"
PROOF_REASON="${ONECONTEXT_RELEASE_PROOF_REASON:-}"
OLD_TAG="${ONECONTEXT_RELEASE_PROOF_OLD_TAG:-}"
OLD_DMG_URL="${ONECONTEXT_RELEASE_PROOF_OLD_DMG_URL:-}"
UPDATE_TIMEOUT_SECONDS="${ONECONTEXT_RELEASE_PROOF_UPDATE_TIMEOUT_SECONDS:-420}"
STEADY_STATE_SECONDS="${ONECONTEXT_RELEASE_PROOF_STEADY_STATE_SECONDS:-120}"
ARTIFACT_RETENTION_DAYS="${ONECONTEXT_RELEASE_PROOF_ARTIFACT_RETENTION_DAYS:-21}"
MODE="dry-run"
WATCH=0
DOWNLOAD_ARTIFACTS=0

usage() {
  cat <<'USAGE'
Usage: scripts/request-release-proof.sh [--dry-run|--dispatch] [options]

Resolves the routine self-hosted Mac update proof inputs from release/release.toml,
then prints or dispatches the protected GitHub
workflow.

Defaults:
  --dry-run
  --repo hapticasensorics/1context
  --workflow self-hosted-mac-update-proof.yml
  --appcast-url https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml
  --new-version release/release.toml version
  --update-class release/release.toml update_class
  --old-version release/release.toml previous_version

Options:
  --dispatch                         Actually call gh workflow run.
  --watch                            After dispatch, watch the newest workflow_dispatch run.
  --download-artifacts               With --watch, download artifacts to dist/self-hosted-run-<id>.
  --repo OWNER/REPO                  GitHub repo.
  --workflow FILE                    Workflow file name.
  --ref REF                          Trusted ref: main, release/*, rc/*, or v* tag.
  --old-version VERSION              Installed version N.
  --new-version VERSION              Expected version N+1.
  --old-tag TAG                      Release tag for N, when not v<old-version>.
  --old-dmg-url URL                  Direct version-N DMG URL for staging proofs.
  --appcast-url URL                  Appcast URL the version-N app must use.
  --update-class mandatory|optional  Expected Sparkle update class.
  --proof-reason TEXT                Approval/audit reason.
  --update-timeout-seconds N         Sparkle update timeout.
  --steady-state-seconds N           Post-update steady-state duration.
  --artifact-retention-days N        GitHub artifact retention days.
USAGE
}

fail() {
  echo "release proof request failed: $*" >&2
  exit 1
}

quote_command() {
  printf '%q ' "$@"
  printf '\n'
}

resolve_ref() {
  if [[ -n "$REF" ]]; then
    return
  fi

  local tag branch
  tag="$(git -C "$ROOT" describe --tags --exact-match 2>/dev/null || true)"
  branch="$(git -C "$ROOT" branch --show-current 2>/dev/null || true)"

  if [[ "$tag" == v* ]]; then
    REF="$tag"
  elif [[ "$branch" == "main" || "$branch" == release/* || "$branch" == rc/* ]]; then
    REF="$branch"
  else
    REF="main"
  fi
}

validate_ref() {
  case "$REF" in
    main|release/*|rc/*|v*) ;;
    *)
      fail "Ref '$REF' is not allowed for the self-hosted runner. Use main, release/*, rc/*, or a v* tag."
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      MODE="dry-run"
      shift
      ;;
    --dispatch)
      MODE="dispatch"
      shift
      ;;
    --watch)
      WATCH=1
      shift
      ;;
    --download-artifacts)
      DOWNLOAD_ARTIFACTS=1
      shift
      ;;
    --repo)
      REPO="${2:?}"
      shift 2
      ;;
    --workflow)
      WORKFLOW="${2:?}"
      shift 2
      ;;
    --ref)
      REF="${2:?}"
      shift 2
      ;;
    --old-version)
      OLD_VERSION="${2:?}"
      shift 2
      ;;
    --new-version)
      NEW_VERSION="${2:?}"
      shift 2
      ;;
    --old-tag)
      OLD_TAG="${2:?}"
      shift 2
      ;;
    --old-dmg-url)
      OLD_DMG_URL="${2:?}"
      shift 2
      ;;
    --appcast-url|--staging-appcast-url)
      APPCAST_URL="${2:?}"
      shift 2
      ;;
    --update-class)
      UPDATE_CLASS="${2:?}"
      shift 2
      ;;
    --proof-reason)
      PROOF_REASON="${2:?}"
      shift 2
      ;;
    --update-timeout-seconds)
      UPDATE_TIMEOUT_SECONDS="${2:?}"
      shift 2
      ;;
    --steady-state-seconds)
      STEADY_STATE_SECONDS="${2:?}"
      shift 2
      ;;
    --artifact-retention-days)
      ARTIFACT_RETENTION_DAYS="${2:?}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "Unknown argument: $1"
      ;;
  esac
done

eval "$("$ROOT/scripts/release-manifest.py" export-env)"

NEW_VERSION="${NEW_VERSION:-$ONECONTEXT_RELEASE_VERSION}"
UPDATE_CLASS="${UPDATE_CLASS:-$ONECONTEXT_RELEASE_UPDATE_CLASS}"
OLD_VERSION="${OLD_VERSION:-$ONECONTEXT_RELEASE_PREVIOUS_VERSION}"
APPCAST_URL="${APPCAST_URL:-$ONECONTEXT_RELEASE_PUBLIC_APPCAST_URL}"
if [[ -z "$OLD_VERSION" ]]; then
  fail "Could not infer old version. Pass --old-version for optional releases or policies without minimum_autoupdate_version."
fi
if [[ -z "$NEW_VERSION" ]]; then
  fail "Could not resolve new version."
fi
if [[ "$OLD_VERSION" == "$NEW_VERSION" ]]; then
  fail "Old and new versions are both $OLD_VERSION; release proof requires a real update hop."
fi
if [[ "$UPDATE_CLASS" != "mandatory" && "$UPDATE_CLASS" != "optional" ]]; then
  fail "Update class must be mandatory or optional."
fi
if [[ "$DOWNLOAD_ARTIFACTS" == "1" && "$WATCH" != "1" ]]; then
  fail "--download-artifacts requires --watch so artifacts come from the watched run."
fi
if [[ -z "$APPCAST_URL" ]]; then
  fail "Appcast URL is required."
fi
if [[ -z "$PROOF_REASON" ]]; then
  PROOF_REASON="routine $UPDATE_CLASS Sparkle release proof for 1Context $NEW_VERSION"
fi

resolve_ref
validate_ref

cmd=(
  gh workflow run "$WORKFLOW"
  --repo "$REPO"
  --ref "$REF"
  -f "proof_reason=$PROOF_REASON"
  -f "old_version=$OLD_VERSION"
  -f "new_version=$NEW_VERSION"
  -f "staging_appcast_url=$APPCAST_URL"
  -f "update_class=$UPDATE_CLASS"
  -f "update_timeout_seconds=$UPDATE_TIMEOUT_SECONDS"
  -f "steady_state_seconds=$STEADY_STATE_SECONDS"
  -f "artifact_retention_days=$ARTIFACT_RETENTION_DAYS"
)
if [[ -n "$OLD_TAG" ]]; then
  cmd+=(-f "old_tag=$OLD_TAG")
fi
if [[ -n "$OLD_DMG_URL" ]]; then
  cmd+=(-f "old_dmg_url=$OLD_DMG_URL")
fi

cat <<SUMMARY
release_proof_request:
  mode: $MODE
  repo: $REPO
  workflow: $WORKFLOW
  ref: $REF
  old_version: $OLD_VERSION
  new_version: $NEW_VERSION
  update_class: $UPDATE_CLASS
  appcast_url: $APPCAST_URL
  proof_reason: $PROOF_REASON
  watch: $WATCH
  download_artifacts: $DOWNLOAD_ARTIFACTS
gh_command: $(quote_command "${cmd[@]}")
SUMMARY

if [[ "$MODE" == "dry-run" ]]; then
  exit 0
fi

command -v gh >/dev/null 2>&1 || fail "gh is required for --dispatch."
gh auth status --hostname github.com >/dev/null
runs_before_json="[]"
if [[ "$WATCH" == "1" ]]; then
  runs_before_json="$(gh run list --repo "$REPO" --workflow "$WORKFLOW" --event workflow_dispatch --limit 50 --json databaseId)"
fi
dispatch_started_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
"${cmd[@]}"

if [[ "$WATCH" != "1" ]]; then
  echo "dispatched=1"
  echo "watch_command=$(quote_command gh run list --repo "$REPO" --workflow "$WORKFLOW" --event workflow_dispatch --limit 5)"
  exit 0
fi

sleep 8
runs_json="$(gh run list --repo "$REPO" --workflow "$WORKFLOW" --event workflow_dispatch --limit 20 --json databaseId,url,status,conclusion,createdAt,headBranch)"
run_id="$(
  RUNS_BEFORE_JSON="$runs_before_json" RUNS_JSON="$runs_json" python3 - "$dispatch_started_at" "$REF" <<'PY'
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
echo "watching_run_id=$run_id"
gh run watch "$run_id" --repo "$REPO" --exit-status

if [[ "$DOWNLOAD_ARTIFACTS" == "1" ]]; then
  artifact_dir="$ROOT/dist/self-hosted-run-$run_id"
  mkdir -p "$artifact_dir"
  gh run download "$run_id" --repo "$REPO" --dir "$artifact_dir"
  echo "artifact_dir=$artifact_dir"
fi
