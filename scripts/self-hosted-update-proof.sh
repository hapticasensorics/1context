#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${ONECONTEXT_INSTALLED_APP:-/Applications/1Context.app}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
REPO="${ONECONTEXT_GITHUB_REPO:-hapticasensorics/1context}"
OLD_VERSION="${ONECONTEXT_OLD_VERSION:-${ONECONTEXT_EXPECTED_OLD_VERSION:-}}"
NEW_VERSION="${ONECONTEXT_NEW_VERSION:-${ONECONTEXT_EXPECTED_NEW_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}}"
OLD_TAG="${ONECONTEXT_OLD_TAG:-}"
OLD_DMG_URL="${ONECONTEXT_OLD_DMG_URL:-}"
OLD_DMG_PATH="${ONECONTEXT_OLD_DMG_PATH:-}"
OLD_DMG_ASSET="${ONECONTEXT_OLD_DMG_ASSET:-}"
APPCAST_URL="${ONECONTEXT_STAGING_APPCAST_URL:-${ONECONTEXT_REMOTE_APPCAST_URL:-}}"
PUBLIC_APPCAST_URL="${ONECONTEXT_PUBLIC_APPCAST_URL:-https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml}"
PROOF_REASON="${ONECONTEXT_UPDATE_PROOF_REASON:-}"
UPDATE_CLASS="${ONECONTEXT_EXPECTED_UPDATE_CLASS:-mandatory}"
TIMEOUT_SECONDS="${ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS:-420}"
POLL_SECONDS="${ONECONTEXT_UPDATE_PROOF_POLL_SECONDS:-5}"
STEADY_STATE_SECONDS="${ONECONTEXT_STEADY_STATE_SECONDS:-120}"
STEADY_STATE_INTERVAL_SECONDS="${ONECONTEXT_STEADY_STATE_INTERVAL_SECONDS:-5}"
RUNNER_SETUP_PREFLIGHT="${ONECONTEXT_RUNNER_SETUP_PREFLIGHT:-1}"
ALLOW_NON_PUBLIC_FINAL_FEED="${ONECONTEXT_UPDATE_RUNNER_ALLOW_NON_PUBLIC_FINAL_FEED:-0}"
RESTORE_PUBLIC_FINAL_FEED="${ONECONTEXT_UPDATE_RUNNER_RESTORE_PUBLIC_FINAL_FEED:-1}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_SELF_HOSTED_UPDATE_EVIDENCE_DIR:-$ROOT/dist/self-hosted-update-proof/$STAMP}"
DOWNLOAD_DIR="$EVIDENCE_DIR/downloads"
MOUNT_POINT=""
DOWNLOADED_OLD_DMG=""
DOWNLOADED_RELEASE_DMG=""

fail() {
  echo "self-hosted update proof failed: $*" >&2
  mkdir -p "$EVIDENCE_DIR"
  printf '%s\n' "$*" > "$EVIDENCE_DIR/failure.txt"
  exit 1
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "Missing required tool: $1"
  fi
}

cleanup() {
  if [[ -n "$MOUNT_POINT" ]]; then
    hdiutil detach "$MOUNT_POINT" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ "${ONECONTEXT_UPDATE_RUNNER_I_UNDERSTAND_DESTRUCTIVE:-}" != "1" ]]; then
  fail "Refusing to mutate $APP without ONECONTEXT_UPDATE_RUNNER_I_UNDERSTAND_DESTRUCTIVE=1."
fi
if [[ -z "$OLD_VERSION" ]]; then
  fail "Set ONECONTEXT_OLD_VERSION to the version N that should be installed before the update."
fi
if [[ -z "$NEW_VERSION" ]]; then
  fail "Set ONECONTEXT_NEW_VERSION or keep VERSION populated for the expected N+1 update."
fi
if [[ "$OLD_VERSION" == "$NEW_VERSION" ]]; then
  fail "Old and new versions are both $OLD_VERSION; update proof requires a real version hop."
fi
if [[ -z "$APPCAST_URL" ]]; then
  fail "Set ONECONTEXT_STAGING_APPCAST_URL to the staged N+1 appcast."
fi
if [[ "$UPDATE_CLASS" != "mandatory" && "$UPDATE_CLASS" != "optional" ]]; then
  fail "ONECONTEXT_EXPECTED_UPDATE_CLASS must be mandatory or optional."
fi

require_tool curl
require_tool hdiutil
require_tool osascript
require_tool plutil
require_tool python3

mkdir -p "$DOWNLOAD_DIR"
exec > >(tee -a "$EVIDENCE_DIR/self-hosted-update-proof.log") 2>&1

log() {
  printf '[self-hosted-update-proof] %s\n' "$*"
}

installed_plist_version() {
  plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist" 2>/dev/null || true
}

installed_cli_version() {
  if [[ -x "$APP/Contents/MacOS/1context-cli" ]]; then
    "$APP/Contents/MacOS/1context-cli" --version 2>/dev/null || true
  fi
}

installed_feed_url() {
  plutil -extract SUFeedURL raw "$APP/Contents/Info.plist" 2>/dev/null || true
}

installed_cli() {
  printf '%s\n' "$APP/Contents/MacOS/1context-cli"
}

write_versions() {
  local output="$1"
  {
    echo "plist=$(installed_plist_version)"
    echo "cli=$(installed_cli_version)"
    echo "feed=$(installed_feed_url)"
  } > "$output"
}

capture_process_state() {
  local name="$1"
  ps -axo pid,ppid,user,stat,command |
    awk '/1Context|1context|Sparkle/ && !/self-hosted-update-proof/ { print }' \
      > "$EVIDENCE_DIR/processes-$name.txt" || true
  launchctl print "gui/$(id -u)/com.haptica.1context" > "$EVIDENCE_DIR/launchctl-runtime-$name.txt" 2>&1 || true
  launchctl print "gui/$(id -u)/com.haptica.1context.menu" > "$EVIDENCE_DIR/launchctl-menu-$name.txt" 2>&1 || true
}

stop_1context() {
  log "stopping existing 1Context processes"
  if [[ -x "$APP/Contents/MacOS/1context-cli" ]]; then
    "$APP/Contents/MacOS/1context-cli" quit >/dev/null 2>&1 || true
  fi
  launchctl bootout "gui/$(id -u)/com.haptica.1context" >/dev/null 2>&1 || true
  launchctl bootout "gui/$(id -u)/com.haptica.1context.menu" >/dev/null 2>&1 || true
  pkill -f "$APP/Contents/MacOS/1Context" >/dev/null 2>&1 || true
  pkill -f "$APP/Contents/MacOS/1contextd" >/dev/null 2>&1 || true
  sleep 2
}

clear_disposable_update_state() {
  if [[ "${ONECONTEXT_UPDATE_RUNNER_CLEAR_SPARKLE_STATE:-1}" != "1" ]]; then
    return
  fi
  log "clearing disposable Sparkle/WebKit update caches"
  rm -rf \
    "$HOME/Library/Caches/com.haptica.1context/org.sparkle-project.Sparkle" \
    "$HOME/Library/Caches/com.haptica.1context.menu/org.sparkle-project.Sparkle" \
    "$HOME/Library/HTTPStorages/com.haptica.1context" \
    "$HOME/Library/HTTPStorages/com.haptica.1context.binarycookies" \
    "$HOME/Library/HTTPStorages/com.haptica.1context.menu" \
    "$HOME/Library/HTTPStorages/com.haptica.1context.menu.binarycookies" \
    "$HOME/Library/Saved Application State/com.haptica.1context.savedState" \
    "$HOME/Library/Saved Application State/com.haptica.1context.menu.savedState" \
    "$HOME/Library/WebKit/com.haptica.1context" \
    "$HOME/Library/WebKit/com.haptica.1context.menu"
  defaults delete com.haptica.1context >/dev/null 2>&1 || true
  defaults delete com.haptica.1context.menu >/dev/null 2>&1 || true
}

download_old_dmg() {
  local output="$DOWNLOAD_DIR/1Context-$OLD_VERSION-macos-$ARCH.dmg"
  if [[ -n "$OLD_DMG_PATH" ]]; then
    [[ -f "$OLD_DMG_PATH" ]] || fail "ONECONTEXT_OLD_DMG_PATH does not exist: $OLD_DMG_PATH"
    cp "$OLD_DMG_PATH" "$output"
  elif [[ -n "$OLD_DMG_URL" ]]; then
    log "downloading old DMG from explicit URL"
    curl --fail --location --show-error --silent "$OLD_DMG_URL" --output "$output"
  else
    local tag="${OLD_TAG:-v$OLD_VERSION}"
    download_release_dmg "$OLD_VERSION" "$tag" "${OLD_DMG_ASSET:-1Context-$OLD_VERSION-macos-$ARCH.dmg}" "$output"
  fi
  shasum -a 256 "$output" | tee "$EVIDENCE_DIR/old-dmg.sha256"
  DOWNLOADED_OLD_DMG="$output"
}

download_release_dmg() {
  local version="$1"
  local tag="$2"
  local asset="$3"
  local output="$4"
  require_tool gh
  log "downloading $REPO@$tag asset $asset"
  gh release download "$tag" --repo "$REPO" --pattern "$asset" --dir "$DOWNLOAD_DIR" --clobber >/dev/null
  if [[ ! -f "$DOWNLOAD_DIR/$asset" ]]; then
    fail "Downloaded release asset was not found: $asset"
  fi
  if [[ "$DOWNLOAD_DIR/$asset" != "$output" ]]; then
    mv "$DOWNLOAD_DIR/$asset" "$output"
  fi
  shasum -a 256 "$output" | tee "$EVIDENCE_DIR/release-$version-dmg.sha256"
  DOWNLOADED_RELEASE_DMG="$output"
}

mount_dmg() {
  local dmg="$1"
  local plist="$EVIDENCE_DIR/hdiutil-attach.plist"
  hdiutil attach -nobrowse -readonly -plist "$dmg" > "$plist"
  MOUNT_POINT="$(
    python3 - "$plist" <<'PY'
import plistlib
import sys
from pathlib import Path

payload = plistlib.loads(Path(sys.argv[1]).read_bytes())
for entity in payload.get("system-entities", []):
    mount = entity.get("mount-point")
    if mount:
        print(mount)
        break
else:
    raise SystemExit("No mount-point in hdiutil attach plist.")
PY
  )"
  [[ -d "$MOUNT_POINT" ]] || fail "Mounted DMG path is not a directory: $MOUNT_POINT"
  log "mounted old DMG at $MOUNT_POINT"
}

install_app_from_dmg() {
  local dmg="$1"
  local expected_version="$2"
  local evidence_name="$3"
  local expected_feed="${4:-}"
  mount_dmg "$dmg"
  local source="$MOUNT_POINT/1Context.app"
  [[ -d "$source" ]] || fail "DMG does not contain 1Context.app at $source"

  log "installing $expected_version into $APP"
  rm -rf "$APP"
  mkdir -p "$(dirname "$APP")"
  COPYFILE_DISABLE=1 ditto --norsrc --noextattr --noqtn --noacl "$source" "$APP"
  xattr -dr com.apple.quarantine "$APP" >/dev/null 2>&1 || true
  hdiutil detach "$MOUNT_POINT" >/dev/null
  MOUNT_POINT=""

  codesign --verify --deep --strict "$APP" > "$EVIDENCE_DIR/codesign-$evidence_name-app.txt" 2>&1
  spctl --assess --type execute --verbose=4 "$APP" > "$EVIDENCE_DIR/spctl-$evidence_name-app.txt" 2>&1
  write_versions "$EVIDENCE_DIR/version-after-$evidence_name-install.txt"
  [[ "$(installed_plist_version)" == "$expected_version" ]] || fail "Installed plist version is not $expected_version."
  [[ "$(installed_cli_version)" == "$expected_version" ]] || fail "Installed CLI version is not $expected_version."
  if [[ -n "$expected_feed" && "$(installed_feed_url)" != "$expected_feed" ]]; then
    fail "Installed $evidence_name app SUFeedURL does not match the expected proof feed. Expected: $expected_feed. Actual: $(installed_feed_url)."
  fi
}

install_old_app() {
  install_app_from_dmg "$1" "$OLD_VERSION" "old" "$APPCAST_URL"
}

ensure_final_app_uses_public_feed() {
  local final_feed
  final_feed="$(installed_feed_url)"
  {
    echo "final_feed=$final_feed"
    echo "public_appcast_url=$PUBLIC_APPCAST_URL"
    echo "allow_non_public_final_feed=$ALLOW_NON_PUBLIC_FINAL_FEED"
    echo "restore_public_final_feed=$RESTORE_PUBLIC_FINAL_FEED"
  } > "$EVIDENCE_DIR/final-feed-policy.txt"

  if [[ "$final_feed" == "$PUBLIC_APPCAST_URL" ]]; then
    echo "final_feed_action=already_public" >> "$EVIDENCE_DIR/final-feed-policy.txt"
    return
  fi
  if [[ "$ALLOW_NON_PUBLIC_FINAL_FEED" == "1" ]]; then
    echo "final_feed_action=allowed_non_public" >> "$EVIDENCE_DIR/final-feed-policy.txt"
    log "leaving non-public final feed because ONECONTEXT_UPDATE_RUNNER_ALLOW_NON_PUBLIC_FINAL_FEED=1"
    return
  fi
  if [[ "$RESTORE_PUBLIC_FINAL_FEED" != "1" ]]; then
    fail "Final app feed is not the public feed ($final_feed), and public-feed restoration is disabled. Set ONECONTEXT_UPDATE_RUNNER_ALLOW_NON_PUBLIC_FINAL_FEED=1 only for a deliberate staging-only runner."
  fi

  log "final app feed is not public; restoring public $NEW_VERSION release before leaving runner"
  echo "final_feed_action=restored_public_release" >> "$EVIDENCE_DIR/final-feed-policy.txt"
  local restore_tag="${ONECONTEXT_PUBLIC_RESTORE_TAG:-v$NEW_VERSION}"
  local restore_asset="${ONECONTEXT_PUBLIC_RESTORE_DMG_ASSET:-1Context-$NEW_VERSION-macos-$ARCH.dmg}"
  local restore_dmg="$DOWNLOAD_DIR/public-$restore_asset"
  download_release_dmg "$NEW_VERSION" "$restore_tag" "$restore_asset" "$restore_dmg"
  stop_1context
  install_app_from_dmg "$DOWNLOADED_RELEASE_DMG" "$NEW_VERSION" "public-restore" "$PUBLIC_APPCAST_URL"
  rm -f "$DOWNLOADED_RELEASE_DMG"
  open "$APP" || true
  sleep 5
  if [[ -x "$APP/Contents/MacOS/1context-cli" ]]; then
    "$APP/Contents/MacOS/1context-cli" status --debug > "$EVIDENCE_DIR/status-after-public-restore.txt" 2>&1 || true
  fi
}

collect_host_snapshot() {
  {
    echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "repo=$REPO"
    echo "github_ref=${GITHUB_REF:-}"
    echo "github_sha=${GITHUB_SHA:-}"
    echo "runner_name=${RUNNER_NAME:-}"
    echo "runner_os=${RUNNER_OS:-}"
    echo "app=$APP"
    echo "old_version=$OLD_VERSION"
    echo "new_version=$NEW_VERSION"
    echo "update_class=$UPDATE_CLASS"
    echo "proof_reason=$PROOF_REASON"
    echo "appcast_url=$APPCAST_URL"
    echo "public_appcast_url=$PUBLIC_APPCAST_URL"
    echo "old_tag=${OLD_TAG:-v$OLD_VERSION}"
    echo "old_dmg_url=$OLD_DMG_URL"
    echo "timeout_seconds=$TIMEOUT_SECONDS"
    echo "poll_seconds=$POLL_SECONDS"
    echo "steady_state_seconds=$STEADY_STATE_SECONDS"
    echo "steady_state_interval_seconds=$STEADY_STATE_INTERVAL_SECONDS"
    echo "runner_setup_preflight=$RUNNER_SETUP_PREFLIGHT"
    echo "allow_non_public_final_feed=$ALLOW_NON_PUBLIC_FINAL_FEED"
    echo "restore_public_final_feed=$RESTORE_PUBLIC_FINAL_FEED"
    echo "evidence_dir=$EVIDENCE_DIR"
    echo
    sw_vers || true
    echo
    uname -a
    echo
    whoami
    id
  } > "$EVIDENCE_DIR/environment.txt"
}

preflight_runner_setup() {
  if [[ "$RUNNER_SETUP_PREFLIGHT" != "1" ]]; then
    log "skipping runner setup preflight"
    return
  fi

  local cli
  cli="$(installed_cli)"
  log "checking runner Local Wiki setup readiness"
  if [[ ! -x "$cli" ]]; then
    fail "Runner setup preflight requires an existing installed app at $APP. Install 1Context once as the runner user and complete Settings > Setup before update proof, or set ONECONTEXT_RUNNER_SETUP_PREFLIGHT=0 for an intentional first-run setup experiment."
  fi

  local status_file="$EVIDENCE_DIR/setup-preflight.txt"
  if ! "$cli" setup local-web status > "$status_file" 2>&1; then
    fail "Runner setup preflight command failed. See $status_file."
  fi
  if ! grep -q "Setup Ready: yes" "$status_file"; then
    fail "Runner setup preflight failed: Local Wiki setup is not ready for the runner user. Open 1Context as that user, choose Settings > Setup, grant Local Wiki Access, approve the 1Context background item in System Settings, then rerun."
  fi
}

collect_final_logs() {
  mkdir -p "$EVIDENCE_DIR/logs"
  if [[ -d "$HOME/Library/Logs/1Context" ]]; then
    find "$HOME/Library/Logs/1Context" -maxdepth 1 -type f -print0 |
      while IFS= read -r -d '' file; do
        tail -n 300 "$file" > "$EVIDENCE_DIR/logs/$(basename "$file").tail" || true
      done
  fi
  write_versions "$EVIDENCE_DIR/version-final.txt"
  capture_process_state "final"
}

collect_host_snapshot
write_versions "$EVIDENCE_DIR/version-before-runner-reset.txt"
preflight_runner_setup
capture_process_state "before"
download_old_dmg
old_dmg="$DOWNLOADED_OLD_DMG"
stop_1context
clear_disposable_update_state
install_old_app "$old_dmg"
rm -f "$old_dmg"

log "running staged Sparkle update proof"
ONECONTEXT_INSTALLED_APP="$APP" \
ONECONTEXT_REMOTE_APPCAST_URL="$APPCAST_URL" \
ONECONTEXT_EXPECTED_OLD_VERSION="$OLD_VERSION" \
ONECONTEXT_EXPECTED_NEW_VERSION="$NEW_VERSION" \
ONECONTEXT_EXPECTED_UPDATE_CLASS="$UPDATE_CLASS" \
ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS="$TIMEOUT_SECONDS" \
ONECONTEXT_UPDATE_PROOF_POLL_SECONDS="$POLL_SECONDS" \
ONECONTEXT_REMOTE_UPDATE_VALIDATE_REPO_POLICY="${ONECONTEXT_REMOTE_UPDATE_VALIDATE_REPO_POLICY:-0}" \
ONECONTEXT_REMOTE_UPDATE_EVIDENCE_DIR="$EVIDENCE_DIR/update-proof" \
  "$ROOT/scripts/prove-remote-sparkle-update.sh"

log "running post-update steady-state proof"
ONECONTEXT_APP="$APP" \
ONECONTEXT_STEADY_STATE_SECONDS="$STEADY_STATE_SECONDS" \
ONECONTEXT_STEADY_STATE_INTERVAL_SECONDS="$STEADY_STATE_INTERVAL_SECONDS" \
ONECONTEXT_STEADY_STATE_EVIDENCE_DIR="$EVIDENCE_DIR/steady-state" \
  "$ROOT/scripts/verify-macos-steady-state.sh"

ensure_final_app_uses_public_feed
collect_final_logs

cat > "$EVIDENCE_DIR/result.txt" <<RESULT
result=passed
old_version=$OLD_VERSION
new_version=$NEW_VERSION
update_class=$UPDATE_CLASS
proof_reason=$PROOF_REASON
appcast_url=$APPCAST_URL
public_appcast_url=$PUBLIC_APPCAST_URL
final_feed=$(installed_feed_url)
evidence_dir=$EVIDENCE_DIR
RESULT

log "passed; evidence at $EVIDENCE_DIR"
