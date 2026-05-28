#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$ROOT/macos"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
STATE_DIR="$(mktemp -d /tmp/1ctx-test-XXXXXX)"

cleanup() {
  rm -rf "$STATE_DIR"
}

swift build --package-path "$MACOS_DIR"
BIN_DIR="$(swift build --package-path "$MACOS_DIR" --show-bin-path)"
trap cleanup EXIT

"$BIN_DIR/1context" | grep -q "1Context $VERSION"
test "$("$BIN_DIR/1context" --version)" = "$VERSION"
"$BIN_DIR/1context" --help > "$STATE_DIR/help.out"
grep -q "1context diagnose" "$STATE_DIR/help.out"
grep -q "1context uninstall" "$STATE_DIR/help.out"
grep -q "1context wiki local-url" "$STATE_DIR/help.out"
for old_help in \
  "1context start" \
  "1context stop" \
  "1context quit" \
  "1context restart" \
  "1context status" \
  "1context logs" \
  "1context update" \
  "1context setup local-web" \
  "wiki <local-url|refresh>"
do
  if grep -q "$old_help" "$STATE_DIR/help.out"; then
    echo "old CLI help surface is still present: $old_help" >&2
    exit 1
  fi
done
if "$BIN_DIR/1context" wiki status >"$STATE_DIR/wiki-old-status.out" 2>&1; then
  echo "old wiki status command should fail" >&2
  exit 1
fi
grep -q "Unknown wiki subcommand: status" "$STATE_DIR/wiki-old-status.out"
"$BIN_DIR/1context" diagnose | grep -q "1Context Diagnose"
"$BIN_DIR/1context" diagnose | grep -q "~/"
"$BIN_DIR/1context" diagnose | grep -q "Local Web"
"$BIN_DIR/1context" diagnose | grep -q "Bundled Caddy Path"
"$BIN_DIR/1context" diagnose | grep -q "Current Has Health"
if "$BIN_DIR/1context" diagnose | grep -q "Desired State"; then
  echo "diagnose must not expose the deleted runtime desired-state pause path" >&2
  exit 1
fi
if rg -n 'desiredStatePath|desired-state|startStopItem|toggleRuntime|requestStop|shouldAutoStartRuntime|RuntimeIntent|startRemembering' \
  "$ROOT/macos/Sources" "$ROOT/macos/Tests" > "$STATE_DIR/desired-state-leftovers.out"
then
  cat "$STATE_DIR/desired-state-leftovers.out" >&2
  echo "persistent runtime pause/desired-state code should be deleted" >&2
  exit 1
fi
if "$BIN_DIR/1context" diagnose --no-redact >"$STATE_DIR/no-redact.out" 2>&1; then
  echo "diagnose --no-redact should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown argument: --no-redact" "$STATE_DIR/no-redact.out"
if "$BIN_DIR/1context" debug >"$STATE_DIR/debug.out" 2>&1; then
  echo "debug should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown command: debug" "$STATE_DIR/debug.out"
if "$BIN_DIR/1context" permissions >"$STATE_DIR/permissions.out" 2>&1; then
  echo "permissions should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown command: permissions" "$STATE_DIR/permissions.out"
for old_command in start stop quit restart status logs update setup; do
  if "$BIN_DIR/1context" "$old_command" >"$STATE_DIR/old-$old_command.out" 2>&1; then
    echo "$old_command should not be public CLI surface" >&2
    exit 1
  fi
  grep -q "Unknown command: $old_command" "$STATE_DIR/old-$old_command.out"
done
if "$BIN_DIR/1context" status --debug >"$STATE_DIR/status-debug.out" 2>&1; then
  echo "status --debug should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown command: status" "$STATE_DIR/status-debug.out"
if "$BIN_DIR/1context" wiki refresh >"$STATE_DIR/wiki-refresh.out" 2>&1; then
  echo "wiki refresh should not be public CLI surface" >&2
  exit 1
fi
grep -q "Unknown wiki subcommand: refresh" "$STATE_DIR/wiki-refresh.out"
"$BIN_DIR/1context" diagnose | grep -q "Local Web"
"$BIN_DIR/1context" diagnose | grep -q "URL Mode: local-https-portless"
"$BIN_DIR/1context" diagnose | grep -q "Privileged Bind Required: yes"

echo "1Context smoke tests passed."
