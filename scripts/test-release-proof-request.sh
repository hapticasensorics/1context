#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d /tmp/1ctx-release-proof-request-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT
EXPECTED_NEW_VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
EXPECTED_OLD_VERSION="$(awk -F'"' '/^minimum_autoupdate_version[[:space:]]*=/ { print $2; exit }' "$ROOT/release/update-policy.toml")"

assert_contains() {
  local needle="$1"
  local path="$2"
  if ! grep -Fq -- "$needle" "$path"; then
    echo "Expected $path to contain: $needle" >&2
    cat "$path" >&2
    exit 1
  fi
}

bash -n "$ROOT/scripts/request-release-proof.sh"

"$ROOT/scripts/request-release-proof.sh" --dry-run > "$TMP_DIR/dry-run.out"
assert_contains "mode: dry-run" "$TMP_DIR/dry-run.out"
assert_contains "old_version: $EXPECTED_OLD_VERSION" "$TMP_DIR/dry-run.out"
assert_contains "new_version: $EXPECTED_NEW_VERSION" "$TMP_DIR/dry-run.out"
assert_contains "update_class: mandatory" "$TMP_DIR/dry-run.out"
assert_contains "staging_appcast_url=https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml" "$TMP_DIR/dry-run.out"

mkdir -p "$TMP_DIR/bin"
cat > "$TMP_DIR/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%q ' "$@" >> "$GH_ARGS_LOG"
printf '\n' >> "$GH_ARGS_LOG"
if [[ "${1:-}" == "auth" && "${2:-}" == "status" ]]; then
  exit 0
fi
if [[ "${1:-}" == "workflow" && "${2:-}" == "run" ]]; then
  exit 0
fi
if [[ "${1:-}" == "run" && "${2:-}" == "list" ]]; then
  count_file="${GH_RUN_LIST_COUNT_FILE:-}"
  count=0
  if [[ -n "$count_file" && -f "$count_file" ]]; then
    count="$(cat "$count_file")"
  fi
  count=$((count + 1))
  if [[ -n "$count_file" ]]; then
    printf '%s\n' "$count" > "$count_file"
  fi
  if [[ "${GH_AMBIGUOUS_RUNS:-0}" == "1" && "$count" -ge 2 ]]; then
    printf '[{"databaseId":777,"createdAt":"2999-01-01T00:00:00Z","headBranch":"main","status":"queued","conclusion":"","url":"https://example.test/777"},{"databaseId":778,"createdAt":"2999-01-01T00:00:01Z","headBranch":"main","status":"queued","conclusion":"","url":"https://example.test/778"},{"databaseId":111,"createdAt":"2000-01-01T00:00:00Z","headBranch":"main","status":"completed","conclusion":"success","url":"https://example.test/111"}]\n'
  elif [[ "$count" -ge 2 ]]; then
    printf '[{"databaseId":777,"createdAt":"2999-01-01T00:00:00Z","headBranch":"main","status":"queued","conclusion":"","url":"https://example.test/777"},{"databaseId":111,"createdAt":"2000-01-01T00:00:00Z","headBranch":"main","status":"completed","conclusion":"success","url":"https://example.test/111"}]\n'
  else
    printf '[{"databaseId":111,"createdAt":"2000-01-01T00:00:00Z","headBranch":"main","status":"completed","conclusion":"success","url":"https://example.test/111"}]\n'
  fi
  exit 0
fi
if [[ "${1:-}" == "run" && "${2:-}" == "watch" ]]; then
  test "${3:-}" = "777"
  exit 0
fi
if [[ "${1:-}" == "run" && "${2:-}" == "download" ]]; then
  test "${3:-}" = "777"
  dir=""
  while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--dir" ]]; then
      dir="${2:-}"
      break
    fi
    shift
  done
  test -n "$dir"
  mkdir -p "$dir"
  printf 'fixture\n' > "$dir/proof.txt"
  exit 0
fi
echo "unexpected gh invocation: $*" >&2
exit 2
SH
chmod +x "$TMP_DIR/bin/gh"

GH_ARGS_LOG="$TMP_DIR/gh-args.log" \
PATH="$TMP_DIR/bin:$PATH" \
  "$ROOT/scripts/request-release-proof.sh" \
    --dispatch \
    --ref main \
    --proof-reason "fixture proof" \
    --appcast-url https://updates.example.test/appcast.xml \
    > "$TMP_DIR/dispatch.out"

assert_contains "mode: dispatch" "$TMP_DIR/dispatch.out"
assert_contains "dispatched=1" "$TMP_DIR/dispatch.out"
assert_contains "workflow run self-hosted-mac-update-proof.yml" "$TMP_DIR/gh-args.log"
assert_contains "proof_reason=fixture\\ proof" "$TMP_DIR/gh-args.log"
assert_contains "old_version=$EXPECTED_OLD_VERSION" "$TMP_DIR/gh-args.log"
assert_contains "new_version=$EXPECTED_NEW_VERSION" "$TMP_DIR/gh-args.log"
assert_contains "staging_appcast_url=https://updates.example.test/appcast.xml" "$TMP_DIR/gh-args.log"
assert_contains "update_class=mandatory" "$TMP_DIR/gh-args.log"

GH_ARGS_LOG="$TMP_DIR/gh-watch-args.log" \
GH_RUN_LIST_COUNT_FILE="$TMP_DIR/gh-watch-list-count" \
PATH="$TMP_DIR/bin:$PATH" \
  "$ROOT/scripts/request-release-proof.sh" \
    --dispatch \
    --watch \
    --download-artifacts \
    --ref main \
    --proof-reason "fixture proof" \
    --appcast-url https://updates.example.test/appcast.xml \
    > "$TMP_DIR/watch.out"

assert_contains "watching_run_id=777" "$TMP_DIR/watch.out"
assert_contains "artifact_dir=$ROOT/dist/self-hosted-run-777" "$TMP_DIR/watch.out"
assert_contains "run watch 777" "$TMP_DIR/gh-watch-args.log"
assert_contains "run download 777" "$TMP_DIR/gh-watch-args.log"
test -f "$ROOT/dist/self-hosted-run-777/proof.txt"
rm -rf "$ROOT/dist/self-hosted-run-777"

if GH_ARGS_LOG="$TMP_DIR/gh-ambiguous-args.log" \
  GH_RUN_LIST_COUNT_FILE="$TMP_DIR/gh-ambiguous-list-count" \
  GH_AMBIGUOUS_RUNS=1 \
  PATH="$TMP_DIR/bin:$PATH" \
    "$ROOT/scripts/request-release-proof.sh" \
      --dispatch \
      --watch \
      --ref main \
      --proof-reason "fixture proof" \
      --appcast-url https://updates.example.test/appcast.xml \
      > "$TMP_DIR/ambiguous.out" 2>&1; then
  echo "release proof request should fail closed when multiple new runs match" >&2
  exit 1
fi
assert_contains "Multiple new workflow_dispatch runs matched this request" "$TMP_DIR/ambiguous.out"

if "$ROOT/scripts/request-release-proof.sh" --dispatch --download-artifacts --ref main > "$TMP_DIR/download-without-watch.out" 2>&1; then
  echo "release proof request should require --watch before artifact download" >&2
  exit 1
fi
assert_contains "--download-artifacts requires --watch" "$TMP_DIR/download-without-watch.out"

if "$ROOT/scripts/request-release-proof.sh" --dry-run --ref feature/nope > "$TMP_DIR/bad-ref.out" 2>&1; then
  echo "release proof request should reject untrusted refs" >&2
  exit 1
fi
assert_contains "not allowed for the self-hosted runner" "$TMP_DIR/bad-ref.out"

echo "1Context release proof request checks passed."
