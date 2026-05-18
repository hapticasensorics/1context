#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_UNDER_TEST="${ONECONTEXT_APP_UNDER_TEST:-$ROOT/dist/1Context.app}"
DEFAULTS="$APP_UNDER_TEST/Contents/Resources/RuntimeDefaults/1Context"
FIXTURE="$(mktemp -d /tmp/1ctx-wiki-reinstall-XXXXXX)"
APPLICATIONS="$FIXTURE/Applications"
HOME_DIR="$FIXTURE/home"
USER_ROOT="$HOME_DIR/1Context"
APP_SUPPORT="$HOME_DIR/Library/Application Support/1Context"

cleanup() {
  rm -rf "$FIXTURE"
}
trap cleanup EXIT

if [[ ! -d "$APP_UNDER_TEST" || ! -d "$DEFAULTS" ]]; then
  echo "Missing packaged app defaults. Run ./scripts/package-macos-smoke.sh first." >&2
  exit 1
fi

copy_missing_defaults() {
  while IFS= read -r -d '' directory; do
    mkdir -p "$USER_ROOT/${directory#"$DEFAULTS/"}"
  done < <(find "$DEFAULTS" -type d -print0)

  while IFS= read -r -d '' source; do
    local rel="${source#"$DEFAULTS/"}"
    local dest="$USER_ROOT/$rel"
    mkdir -p "$(dirname "$dest")"
    [[ -f "$dest" ]] || cp -p "$source" "$dest"
  done < <(find "$DEFAULTS" -type f ! -name '.DS_Store' -print0)
}

mkdir -p "$APPLICATIONS" "$APP_SUPPORT/setup"
cp -R "$APP_UNDER_TEST" "$APPLICATIONS/1Context.app"
copy_missing_defaults

WIKI_TOML="$USER_ROOT/user-wiki/wiki.toml"
TOPICS="$USER_ROOT/user-wiki/source/families/reference/topics/source/topics.md"
test -f "$WIKI_TOML"
test -f "$TOPICS"
printf '\n# operator reinstall preservation marker\n' >> "$WIKI_TOML"
printf '\n## Reinstall Preservation\n\nPreserve this user-authored topic note.\n' >> "$TOPICS"
printf 'app machinery stays out of user memory\n' > "$APP_SUPPORT/setup/reinstall-smoke.txt"

rm -rf "$APPLICATIONS/1Context.app"
test -f "$WIKI_TOML"
test -f "$TOPICS"
grep -q "operator reinstall preservation marker" "$WIKI_TOML"
grep -q "Preserve this user-authored topic note" "$TOPICS"

cp -R "$APP_UNDER_TEST" "$APPLICATIONS/1Context.app"
copy_missing_defaults

grep -q "operator reinstall preservation marker" "$WIKI_TOML"
grep -q "Preserve this user-authored topic note" "$TOPICS"
test -f "$APP_SUPPORT/setup/reinstall-smoke.txt"
test ! -d "$USER_ROOT/Library"
test ! -d "$APP_SUPPORT/user-wiki"
test -d "$USER_ROOT/user-wiki"
test -d "$USER_ROOT/context-engine"

echo "wiki reinstall preservation smoke passed."

