#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${ONECONTEXT_INSTALLED_APP:-/Applications/1Context.app}"
APP_SUPPORT="${ONECONTEXT_APP_SUPPORT_DIR:-$HOME/Library/Application Support/1Context}"
INSTALLED_CORE="${ONECONTEXT_INSTALLED_MEMORY_CORE_DIR:-$APP_SUPPORT/memory-core/core}"
CURRENT_SITE="${ONECONTEXT_WIKI_CURRENT_SITE:-$APP_SUPPORT/wiki-site/current}"
MARKER="${ONECONTEXT_GOAL_PUBLISH_MARKER:-scripts/publish-goal-to-installed-wiki.sh}"

log() {
  printf '[publish-goal] %s\n' "$*"
}

copy_if_present() {
  local source="$1"
  local destination="$2"
  if [[ -f "$source" ]]; then
    mkdir -p "$(dirname "$destination")"
    install -m 0644 "$source" "$destination"
  fi
}

require_marker() {
  local file="$1"
  if ! grep -Fq "$MARKER" "$file"; then
    echo "Expected marker '$MARKER' was not found in $file" >&2
    exit 1
  fi
}

log "rendering repo /goal"
(
  cd "$ROOT/memory-core"
  uv run 1context wiki render goal --json >/dev/null
)

SOURCE_DIR="$ROOT/memory-core/wiki/menu/20-project/20-goal/source"
GENERATED_DIR="$ROOT/memory-core/wiki/menu/20-project/20-goal/generated"
SITE_JSON_DIR="$ROOT/memory-core/wiki/generated"

require_marker "$GENERATED_DIR/goal.html"

if [[ ! -d "$INSTALLED_CORE" ]]; then
  echo "Installed memory core not found: $INSTALLED_CORE" >&2
  exit 1
fi

log "copying /goal into installed app-support memory core"
mkdir -p "$INSTALLED_CORE/wiki/menu/20-project/20-goal/source"
mkdir -p "$INSTALLED_CORE/wiki/menu/20-project/20-goal/generated"
install -m 0644 "$SOURCE_DIR/goal.md" "$INSTALLED_CORE/wiki/menu/20-project/20-goal/source/goal.md"
rsync -a --delete "$GENERATED_DIR/" "$INSTALLED_CORE/wiki/menu/20-project/20-goal/generated/"
mkdir -p "$INSTALLED_CORE/wiki/generated"
copy_if_present "$SITE_JSON_DIR/site-manifest.json" "$INSTALLED_CORE/wiki/generated/site-manifest.json"
copy_if_present "$SITE_JSON_DIR/content-index.json" "$INSTALLED_CORE/wiki/generated/content-index.json"
copy_if_present "$SITE_JSON_DIR/wiki-stats.json" "$INSTALLED_CORE/wiki/generated/wiki-stats.json"

log "copying /goal into currently served wiki site"
mkdir -p "$CURRENT_SITE"
copy_if_present "$GENERATED_DIR/goal.html" "$CURRENT_SITE/goal.html"
copy_if_present "$GENERATED_DIR/goal.md" "$CURRENT_SITE/goal.md"
copy_if_present "$GENERATED_DIR/goal.talk.html" "$CURRENT_SITE/goal.talk.html"
copy_if_present "$GENERATED_DIR/goal.talk.md" "$CURRENT_SITE/goal.talk.md"
copy_if_present "$SITE_JSON_DIR/site-manifest.json" "$CURRENT_SITE/site-manifest.json"
copy_if_present "$SITE_JSON_DIR/content-index.json" "$CURRENT_SITE/content-index.json"
copy_if_present "$SITE_JSON_DIR/wiki-stats.json" "$CURRENT_SITE/wiki-stats.json"
require_marker "$CURRENT_SITE/goal.html"

CLI="$APP/Contents/MacOS/1context-cli"
if [[ "${ONECONTEXT_SKIP_REFRESH:-0}" != "1" && -x "$CLI" ]]; then
  log "asking installed app to refresh from the patched app-support core"
  "$CLI" wiki refresh >/dev/null || true
  deadline=$(( "$(date +%s)" + 30 ))
  while true; do
    if [[ -f "$CURRENT_SITE/goal.html" ]] && grep -Fq "$MARKER" "$CURRENT_SITE/goal.html"; then
      break
    fi
    if (( "$(date +%s)" >= deadline )); then
      echo "Timed out waiting for refreshed current site to keep /goal marker." >&2
      exit 1
    fi
    sleep 1
  done
fi

if [[ "${ONECONTEXT_SKIP_LIVE_CURL:-0}" != "1" ]]; then
  LIVE_BODY="$(mktemp /tmp/1context-goal-live-XXXXXX.html)"
  if curl --silent --show-error --noproxy '*' --max-time 5 https://wiki.1context.localhost/goal > "$LIVE_BODY" &&
    grep -Fq "$MARKER" "$LIVE_BODY"; then
    log "live wiki contains current /goal"
  else
    echo "Live wiki did not return current /goal marker." >&2
    exit 1
  fi
  rm -f "$LIVE_BODY"
fi

log "published durable /goal into $CURRENT_SITE and $INSTALLED_CORE"
