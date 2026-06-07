#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="manual"

usage() {
  cat <<'USAGE'
Usage:
  scripts/test-installed-app-wiki-update.sh [--manual|--automatic]

Environment:
  ONECONTEXT_APP                              Installed app bundle to verify. Defaults to /Applications/1Context Dev.app.
  ONECONTEXT_WIKI_UPDATE_EVIDENCE_DIR        Defaults to dist/installed-app-wiki-update-evidence/<timestamp>.
  ONECONTEXT_WIKI_UPDATE_TIMEOUT_SECONDS     Defaults to 1200.
  ONECONTEXT_WIKI_UPDATE_PUBLISH_WAIT_SECONDS Defaults to 180.
  ONECONTEXT_WIKI_UPDATE_EXECUTE_AGENTS=1    Request agent execution for manual proof. Defaults to 0.

Manual mode calls the installed app daemon's context_engine.update_wiki RPC, then waits
for the app-visible wiki publish queue to settle. Automatic mode launches the
installed app and waits for the daemon's startup automatic context_engine.update_wiki run.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manual)
      MODE="manual"
      ;;
    --automatic)
      MODE="automatic"
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

fail() {
  echo "installed app wiki update proof failed: $*" >&2
  exit 1
}

APP="${ONECONTEXT_APP:-/Applications/1Context Dev.app}"
APP="${APP%/}"
INFO="$APP/Contents/Info.plist"
MAIN_EXE="$APP/Contents/MacOS/1Context"
CLI_EXE="$APP/Contents/MacOS/1context-cli"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_WIKI_UPDATE_EVIDENCE_DIR:-$ROOT/dist/installed-app-wiki-update-evidence/$STAMP}"
TIMEOUT_SECONDS="${ONECONTEXT_WIKI_UPDATE_TIMEOUT_SECONDS:-1200}"
PUBLISH_WAIT_SECONDS="${ONECONTEXT_WIKI_UPDATE_PUBLISH_WAIT_SECONDS:-180}"
EXECUTE_AGENTS="${ONECONTEXT_WIKI_UPDATE_EXECUTE_AGENTS:-0}"
RUN_ID="${ONECONTEXT_WIKI_UPDATE_RUN_ID:-installed-app-wiki-proof-$(date -u +%Y%m%d-%H%M%S)}"

[[ -d "$APP" ]] || fail "app bundle not found: $APP"
[[ -x "$MAIN_EXE" ]] || fail "app executable not found or not executable: $MAIN_EXE"
[[ -x "$CLI_EXE" ]] || fail "bundled CLI not found or not executable: $CLI_EXE"
[[ -f "$INFO" ]] || fail "Info.plist not found: $INFO"

mkdir -p "$EVIDENCE_DIR"

BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$INFO")"
APP_IDENTITY="$(/usr/libexec/PlistBuddy -c 'Print :OneContextAppIdentity' "$INFO" 2>/dev/null || true)"
DISPLAY_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleDisplayName' "$INFO" 2>/dev/null || /usr/libexec/PlistBuddy -c 'Print :CFBundleName' "$INFO")"

case "$APP_IDENTITY" in
  official|release|prod|production|public)
    USER_CONTENT_DIR="$HOME/1Context"
    APP_SUPPORT_DIR="$HOME/Library/Application Support/1Context"
    ;;
  dev|development|debug|local)
    USER_CONTENT_DIR="$HOME/1Context-Dev"
    APP_SUPPORT_DIR="$HOME/Library/Application Support/1Context Dev"
    ;;
  dev-permission:*)
    suffix="${APP_IDENTITY#dev-permission:}"
    USER_CONTENT_DIR="$HOME/1Context-Dev-$suffix"
    APP_SUPPORT_DIR="$HOME/Library/Application Support/1Context Dev - $suffix"
    ;;
  *)
    case "$BUNDLE_ID" in
      com.haptica.1context)
        USER_CONTENT_DIR="$HOME/1Context"
        APP_SUPPORT_DIR="$HOME/Library/Application Support/1Context"
        ;;
      com.haptica.1context.dev)
        USER_CONTENT_DIR="$HOME/1Context-Dev"
        APP_SUPPORT_DIR="$HOME/Library/Application Support/1Context Dev"
        ;;
      com.haptica.1context.dev.permission.*)
        suffix="${BUNDLE_ID#com.haptica.1context.dev.permission.}"
        USER_CONTENT_DIR="$HOME/1Context-Dev-$suffix"
        APP_SUPPORT_DIR="$HOME/Library/Application Support/1Context Dev - $suffix"
        ;;
      *)
        fail "cannot map bundle identity to runtime paths: $BUNDLE_ID / ${APP_IDENTITY:-unset}"
        ;;
    esac
    ;;
esac

SOCKET="$APP_SUPPORT_DIR/run/1context.sock"
WIKI_SITE="$APP_SUPPORT_DIR/wiki-site/current"

printf '%s\n' "$BUNDLE_ID" >"$EVIDENCE_DIR/bundle-identifier.txt"
printf '%s\n' "${APP_IDENTITY:-}" >"$EVIDENCE_DIR/app-identity.txt"
python3 - "$EVIDENCE_DIR/paths.json" "$APP" "$DISPLAY_NAME" "$USER_CONTENT_DIR" "$APP_SUPPORT_DIR" "$SOCKET" "$WIKI_SITE" <<'PY'
import json
import sys
from pathlib import Path

out, app, display_name, user_content, app_support, socket_path, wiki_site = sys.argv[1:]
Path(out).write_text(json.dumps({
    "app": app,
    "display_name": display_name,
    "user_content": user_content,
    "app_support": app_support,
    "socket": socket_path,
    "wiki_site": wiki_site,
}, indent=2, sort_keys=True) + "\n")
PY

rpc() {
  local method="$1"
  local params_file="$2"
  local output_file="$3"
  local timeout_seconds="$4"
  python3 - "$SOCKET" "$method" "$params_file" "$timeout_seconds" >"$output_file" <<'PY'
import json
import socket
import sys
from pathlib import Path

socket_path, method, params_file, timeout_raw = sys.argv[1:]
timeout = max(1.0, float(timeout_raw))
params = {}
if params_file != "-":
    params = json.loads(Path(params_file).read_text())
request = json.dumps({
    "jsonrpc": "2.0",
    "id": 1,
    "method": method,
    "params": params,
}, separators=(",", ":")).encode() + b"\n"

client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
client.settimeout(timeout)
try:
    client.connect(socket_path)
    client.sendall(request)
    chunks = []
    while True:
        chunk = client.recv(65536)
        if not chunk:
            break
        chunks.append(chunk)
        if b"\n" in chunk:
            break
finally:
    client.close()

line = b"".join(chunks).split(b"\n", 1)[0]
payload = json.loads(line.decode())
if payload.get("error"):
    raise SystemExit(payload["error"].get("message") or json.dumps(payload["error"]))
print(json.dumps(payload.get("result", {}), indent=2, sort_keys=True))
PY
}

wait_for_health() {
  local deadline=$((SECONDS + 60))
  while (( SECONDS < deadline )); do
    if [[ -S "$SOCKET" ]] && rpc health - "$EVIDENCE_DIR/health.json" 5 2>"$EVIDENCE_DIR/health.err"; then
      return 0
    fi
    sleep 1
  done
  [[ -s "$EVIDENCE_DIR/health.err" ]] && cat "$EVIDENCE_DIR/health.err" >&2
  fail "runtime did not become healthy at $SOCKET"
}

wait_for_publish() {
  python3 - "$SOCKET" "$PUBLISH_WAIT_SECONDS" "$EVIDENCE_DIR/after-status.json" <<'PY'
import json
import socket
import sys
import time
from pathlib import Path

socket_path, wait_raw, output = sys.argv[1:]
deadline = time.monotonic() + max(1, int(wait_raw))
last = None

def call_status():
    request = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "wiki.status", "params": {}}, separators=(",", ":")).encode() + b"\n"
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(10)
    try:
        client.connect(socket_path)
        client.sendall(request)
        chunks = []
        while True:
            chunk = client.recv(65536)
            if not chunk:
                break
            chunks.append(chunk)
            if b"\n" in chunk:
                break
    finally:
        client.close()
    payload = json.loads(b"".join(chunks).split(b"\n", 1)[0].decode())
    if payload.get("error"):
        raise RuntimeError(payload["error"].get("message") or payload["error"])
    return payload.get("result", {})

while time.monotonic() < deadline:
    last = call_status()
    publish = last.get("publish_status", {})
    last_publish = publish.get("last", {})
    if (
        not publish.get("running")
        and not publish.get("scheduled")
        and not publish.get("pending")
        and last_publish.get("status") in {"published", "skipped"}
    ):
        Path(output).write_text(json.dumps(last, indent=2, sort_keys=True) + "\n")
        print(f"publish_status={last_publish.get('status')}")
        raise SystemExit(0)
    time.sleep(2)

Path(output).write_text(json.dumps(last or {}, indent=2, sort_keys=True) + "\n")
raise SystemExit("wiki publish queue did not settle before timeout")
PY
}

LAUNCH_EPOCH="$(python3 - <<'PY'
import time
print(time.time())
PY
)"
/usr/bin/open -na "$APP" >"$EVIDENCE_DIR/open.out" 2>"$EVIDENCE_DIR/open.err"
wait_for_health
rpc wiki.status - "$EVIDENCE_DIR/before-status.json" 10

if [[ "$MODE" == "manual" ]]; then
  python3 - "$EVIDENCE_DIR/update-params.json" "$RUN_ID" "$TIMEOUT_SECONDS" "$EXECUTE_AGENTS" <<'PY'
import json
import sys
from pathlib import Path

out, run_id, timeout_raw, execute_agents_raw = sys.argv[1:]
params = {
    "run_id": run_id,
    "trigger": "installed-app-proof.manual",
    "execute_agents": execute_agents_raw == "1",
    "source_window_days": 3,
    "max_concurrent": 5,
    "timeout_seconds": max(1, int(timeout_raw)),
}
Path(out).write_text(json.dumps(params, indent=2, sort_keys=True) + "\n")
PY
  rpc context_engine.update_wiki "$EVIDENCE_DIR/update-params.json" "$EVIDENCE_DIR/update-response.json" "$((TIMEOUT_SECONDS + 30))"
else
  python3 - "$SOCKET" "$EVIDENCE_DIR/before-status.json" "$EVIDENCE_DIR/automatic-status.json" "$TIMEOUT_SECONDS" "$LAUNCH_EPOCH" <<'PY'
import json
import socket
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

socket_path, before_path, output_path, timeout_raw, launch_epoch_raw = sys.argv[1:]
before = json.loads(Path(before_path).read_text())
before_memory_update = before.get("memory_update", {})
before_count = int(before_memory_update.get("completed_count") or 0)
before_running = bool(before_memory_update.get("running"))
deadline = time.monotonic() + max(1, int(timeout_raw))
launch_epoch = float(launch_epoch_raw)
last = before

def call_status():
    request = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "wiki.status", "params": {}}, separators=(",", ":")).encode() + b"\n"
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(10)
    try:
        client.connect(socket_path)
        client.sendall(request)
        chunks = []
        while True:
            chunk = client.recv(65536)
            if not chunk:
                break
            chunks.append(chunk)
            if b"\n" in chunk:
                break
    finally:
        client.close()
    payload = json.loads(b"".join(chunks).split(b"\n", 1)[0].decode())
    if payload.get("error"):
        raise RuntimeError(payload["error"].get("message") or payload["error"])
    return payload.get("result", {})

def started_after_launch(memory_update):
    raw = memory_update.get("last_started_at")
    if not raw:
        return False
    try:
        started = datetime.fromisoformat(str(raw).replace("Z", "+00:00"))
    except ValueError:
        return False
    return started.astimezone(timezone.utc).timestamp() >= launch_epoch - 1.0

while time.monotonic() < deadline:
    last = call_status()
    memory_update = last.get("memory_update", {})
    trigger = str(memory_update.get("last_trigger") or "")
    completed_count = int(memory_update.get("completed_count") or 0)
    if (
        trigger.startswith("context_engine.update_wiki.automatic.")
        and not memory_update.get("running")
        and (
            completed_count > before_count
            or (before_running and completed_count >= before_count)
            or started_after_launch(memory_update)
        )
        and memory_update.get("state") == "completed"
    ):
        Path(output_path).write_text(json.dumps(last, indent=2, sort_keys=True) + "\n")
        print(f"automatic_trigger={trigger}")
        raise SystemExit(0)
    time.sleep(2)

Path(output_path).write_text(json.dumps(last, indent=2, sort_keys=True) + "\n")
raise SystemExit("automatic context_engine.update_wiki did not complete before timeout")
PY
fi

wait_for_publish >"$EVIDENCE_DIR/publish-wait.out"

python3 - "$MODE" "$EVIDENCE_DIR" "$WIKI_SITE" <<'PY'
import json
import sys
from pathlib import Path

mode, evidence_dir_raw, wiki_site_raw = sys.argv[1:]
evidence_dir = Path(evidence_dir_raw)
wiki_site = Path(wiki_site_raw)
errors = []

if mode == "manual":
    response = json.loads((evidence_dir / "update-response.json").read_text())
    if response.get("status") != "accepted":
        errors.append(f"context_engine.update_wiki RPC status was {response.get('status')!r}")
    if response.get("context_engine_status") != "ok":
        errors.append(f"context_engine_status was {response.get('context_engine_status')!r}: {response.get('context_engine_error')}")
    result = response.get("context_engine") or {}
    if result.get("status") not in {"planned", "executed", "completed"}:
        errors.append(f"context-engine result status was {result.get('status')!r}")
    marker = result.get("wiki_refresh_marker") or {}
    if marker.get("status") != "written":
        errors.append(f"wiki_refresh_marker status was {marker.get('status')!r}: {marker.get('error')}")
    if not result.get("mail_receipt_path"):
        errors.append("context-engine mail receipt path was missing")
else:
    status = json.loads((evidence_dir / "automatic-status.json").read_text())
    memory_update = status.get("memory_update") or {}
    if not str(memory_update.get("last_trigger") or "").startswith("context_engine.update_wiki.automatic."):
        errors.append(f"last automatic trigger was {memory_update.get('last_trigger')!r}")
    if memory_update.get("state") != "completed":
        errors.append(f"automatic memory update state was {memory_update.get('state')!r}")

after = json.loads((evidence_dir / "after-status.json").read_text())
publish = (after.get("publish_status") or {}).get("last") or {}
if publish.get("status") not in {"published", "skipped"}:
    errors.append(f"publish status was {publish.get('status')!r}")
if not wiki_site.is_dir():
    errors.append(f"app-visible wiki site mirror is missing: {wiki_site}")
for name in ["for-you.html", "your-context.html", "projects.html", "topics.html"]:
    path = wiki_site / name
    if not path.is_file() or path.stat().st_size == 0:
        errors.append(f"missing or empty app-visible wiki file: {path}")

if errors:
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print(f"mode={mode}")
print(f"publish_status={publish.get('status')}")
print(f"evidence={evidence_dir}")
PY

echo "Installed app wiki update proof passed."
echo "Evidence: $EVIDENCE_DIR"
