#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/test-installed-app-managed-postgres.sh [options]

Builds or consumes a macOS app bundle, installs it, and proves the installed
onecontext-memoryd can initialize Perception DB Ultra Max from bundled resources.

Options:
  --build              Build the dev app with Perception DB Ultra Max forced first.
  --channel NAME       Release-train channel for --build (default: dev).
  --app APP            Existing .app to install/test.
  --install-to APP     Installed app path (default: /Applications/<app name>).
  --ingest-sources CSV Run installed onecontext-memoryd daemon --once for sources.
  --require-ingest-data
                       Require each requested ingest source to emit objects.
  --max-events N       Max events per ingest source (default: 1000).
  --max-lines N        Max lines per ingest source (default: 50000).
  --evidence FILE      Write redaction-safe proof JSON.
  -h, --help           Show this help.
USAGE
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD=0
CHANNEL="dev"
APP=""
INSTALL_TO=""
INGEST_SOURCES=""
REQUIRE_INGEST_DATA=0
MAX_EVENTS=1000
MAX_LINES=50000
EVIDENCE_JSON=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --build)
      BUILD=1
      shift
      ;;
    --channel)
      CHANNEL="${2:?missing value for --channel}"
      shift 2
      ;;
    --app)
      APP="${2:?missing value for --app}"
      shift 2
      ;;
    --install-to)
      INSTALL_TO="${2:?missing value for --install-to}"
      shift 2
      ;;
    --ingest-sources)
      INGEST_SOURCES="${2:?missing value for --ingest-sources}"
      shift 2
      ;;
    --require-ingest-data)
      REQUIRE_INGEST_DATA=1
      shift
      ;;
    --max-events)
      MAX_EVENTS="${2:?missing value for --max-events}"
      shift 2
      ;;
    --max-lines)
      MAX_LINES="${2:?missing value for --max-lines}"
      shift 2
      ;;
    --evidence)
      EVIDENCE_JSON="${2:?missing value for --evidence}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$BUILD" == "1" ]]; then
  env ONECONTEXT_INCLUDE_MANAGED_POSTGRES=true "$ROOT/scripts/release-train.sh" build --channel "$CHANNEL"
  if [[ -z "$APP" ]]; then
    if [[ "$CHANNEL" == "dev" ]]; then
      APP="$ROOT/dist/1Context Dev.app"
    else
      APP="$ROOT/dist/1Context.app"
    fi
  fi
fi

if [[ -z "$APP" ]]; then
  APP="$ROOT/dist/1Context Dev.app"
fi
[[ -d "$APP" ]] || { echo "app bundle missing: $APP" >&2; exit 1; }
[[ "$MAX_EVENTS" =~ ^[0-9]+$ ]] || { echo "--max-events must be an integer" >&2; exit 2; }
[[ "$MAX_LINES" =~ ^[0-9]+$ ]] || { echo "--max-lines must be an integer" >&2; exit 2; }

if [[ -z "$INSTALL_TO" ]]; then
  INSTALL_TO="/Applications/$(basename "$APP")"
fi

ditto --norsrc --noqtn "$APP" "$INSTALL_TO"
"$ROOT/scripts/verify-managed-postgres-bundle.sh" --require-sbom "$INSTALL_TO"

MEMORYD="$INSTALL_TO/Contents/MacOS/onecontext-memoryd"
[[ -x "$MEMORYD" ]] || { echo "installed memory daemon missing: $MEMORYD" >&2; exit 1; }

WORKDIR="$(mktemp -d /tmp/onecontext-installed-managed-pg.XXXXXX)"
ENSURE_JSON="$WORKDIR/ensure-storage-ready.json"
HEALTH_JSON="$WORKDIR/storage-health.json"
INGEST_STATUS_JSON=""
INGEST_TIME_TXT=""
PGDATA_DIR=""
SMOKE_PASSED=0

cleanup() {
  local rc=$?
  if [[ -n "$PGDATA_DIR" && -d "$PGDATA_DIR" ]]; then
    "$INSTALL_TO/Contents/Resources/managed-postgres/macos-arm64/bin/pg_ctl" \
      -D "$PGDATA_DIR" stop -m fast >/dev/null 2>&1 || true
  fi
  if [[ "$SMOKE_PASSED" == "1" ]]; then
    rm -rf "$WORKDIR"
  else
    echo "installed Perception DB Ultra Max smoke evidence preserved: $WORKDIR" >&2
  fi
  return "$rc"
}
trap cleanup EXIT

RUN_ENV=(
  env -i
  "HOME=$HOME"
  "TMPDIR=${TMPDIR:-/tmp}"
  "PATH=/usr/bin:/bin:/usr/sbin:/sbin"
  "ONECONTEXT_STORAGE_BACKEND=managed_postgres"
  "ONECONTEXT_APP_SUPPORT_DIR=$WORKDIR/app-support"
)

run_memoryd_protocol() {
  local method="$1"
  local request_json="$2"
  local output_json="$3"
  local attempt rc
  for attempt in 1 2 3; do
    set +e
    printf '%s\n' "$request_json" | "${RUN_ENV[@]}" "$MEMORYD" protocol "$method" --request-json - >"$output_json"
    rc=$?
    set -e
    if [[ "$rc" == "0" ]]; then
      return 0
    fi
    echo "installed Perception DB Ultra Max smoke: $method attempt $attempt failed with exit $rc" >&2
    sleep "$attempt"
  done
  return "$rc"
}

ensure_state() {
  /usr/bin/python3 - "$ENSURE_JSON" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], "r", encoding="utf-8") as handle:
        payload = json.load(handle)
except Exception:
    raise SystemExit(2)

result = payload.get("result", payload)
if result.get("ready") and result.get("storage_ready"):
    raise SystemExit(0)
if result.get("safe_to_retry"):
    raise SystemExit(2)
raise SystemExit(1)
PY
}

run_ensure_storage_ready() {
  local attempt state
  for attempt in 1 2 3 4; do
    if ! run_memoryd_protocol \
      memory.ensureStorageReady \
      '{"reason":"installed-app-managed-postgres-smoke","repair":true}' \
      "$ENSURE_JSON"; then
      sleep "$attempt"
      continue
    fi

    set +e
    ensure_state
    state=$?
    set -e
    case "$state" in
      0)
        return 0
        ;;
      2)
        echo "installed Perception DB Ultra Max smoke: ensure returned retryable non-ready state on attempt $attempt" >&2
        sleep "$attempt"
        ;;
      *)
        return "$state"
        ;;
    esac
  done
  return 1
}

run_ensure_storage_ready

PGDATA_DIR="$(/usr/bin/python3 - "$ENSURE_JSON" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)
result = payload.get("result", payload)
print(result.get("pgdata_dir", ""))
PY
)"

run_memoryd_protocol \
  memory.storageHealth \
  '{"reason":"installed-app-managed-postgres-smoke"}' \
  "$HEALTH_JSON"

/usr/bin/python3 - "$ENSURE_JSON" "$HEALTH_JSON" <<'PY'
import json
import sys

def unwrap(path):
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
    return payload.get("result", payload)

ensure = unwrap(sys.argv[1])
health = unwrap(sys.argv[2])
for label, payload in [("ensure", ensure), ("health", health)]:
    assert payload.get("backend") == "managed_postgres", f"{label}: wrong backend {payload.get('backend')!r}"
    assert payload.get("ready") is True, f"{label}: not ready: {payload}"
    assert payload.get("storage_ready") is True, f"{label}: storage not ready: {payload}"
    assert payload.get("schema_state") == "valid", f"{label}: schema not valid: {payload.get('schema_state')!r}"
    assert payload.get("socket_dir"), f"{label}: missing socket_dir"
    assert payload.get("pgdata_dir"), f"{label}: missing pgdata_dir"
    assert payload.get("bundle_prefix"), f"{label}: missing bundle_prefix"
    assert payload.get("bundle_postgres_version"), f"{label}: missing bundle_postgres_version"
    assert payload.get("bundle_timescale_version"), f"{label}: missing bundle_timescale_version"

extensions = {item.get("name"): item for item in health.get("required_extensions", [])}
for name in ["timescaledb", "btree_gist", "pgcrypto", "pg_trgm", "vector", "pg_stat_statements"]:
    assert extensions.get(name, {}).get("installed") is True, f"extension missing: {name}"
for name in ["timescaledb", "pg_stat_statements"]:
    assert extensions.get(name, {}).get("preload_active") is True, f"preload inactive: {name}"
PY

if [[ -n "$INGEST_SOURCES" ]]; then
  INGEST_CONTEXT_ENGINE="$WORKDIR/context-engine"
  INGEST_RUN_DIR="$WORKDIR/ingest-run"
  INGEST_STDOUT="$WORKDIR/ingest-stdout.json"
  INGEST_TIME_TXT="$WORKDIR/ingest-time.txt"
  INGEST_STATUS_JSON="$INGEST_RUN_DIR/memoryd-status.json"
  mkdir -p "$INGEST_CONTEXT_ENGINE" "$INGEST_RUN_DIR"
  /usr/bin/time -p "${RUN_ENV[@]}" "$MEMORYD" daemon \
    --context-engine-root "$INGEST_CONTEXT_ENGINE" \
    --run-dir "$INGEST_RUN_DIR" \
    --sources "$INGEST_SOURCES" \
    --max-events "$MAX_EVENTS" \
    --max-lines "$MAX_LINES" \
    --once \
    >"$INGEST_STDOUT" \
    2>"$INGEST_TIME_TXT"

  /usr/bin/python3 - "$INGEST_STATUS_JSON" "$INGEST_SOURCES" "$REQUIRE_INGEST_DATA" <<'PY'
import json
import sys

status_path, sources_csv, require_data_raw = sys.argv[1:]
require_data = require_data_raw == "1"
requested = [item.strip() for item in sources_csv.split(",") if item.strip()]
with open(status_path, "r", encoding="utf-8") as handle:
    status = json.load(handle)
assert status.get("status") == "ok", f"ingest status is not ok: {status.get('status')!r}"
seen = {item.get("source"): item for item in status.get("sources", [])}
for source in requested:
    assert source in seen, f"ingest source missing from status: {source}"
    assert seen[source].get("status") == "ok", f"ingest source {source} failed: {seen[source]}"
    if require_data:
        assert int(seen[source].get("objects_emitted") or 0) > 0, f"ingest source {source} emitted no objects"
assert int(status.get("objects_emitted") or 0) >= 0
if status.get("db_write"):
    assert status["db_write"].get("status") == "ok", f"db_write failed: {status['db_write']}"
PY
fi

if lsof -nP -iTCP:15432 -sTCP:LISTEN >/dev/null 2>&1; then
  echo "installed Perception DB Ultra Max smoke failed: TCP listener exists on 15432" >&2
  lsof -nP -iTCP:15432 -sTCP:LISTEN >&2 || true
  exit 1
fi

if [[ -n "$EVIDENCE_JSON" ]]; then
  mkdir -p "$(dirname "$EVIDENCE_JSON")"
  /usr/bin/python3 - "$ENSURE_JSON" "$HEALTH_JSON" "${INGEST_STATUS_JSON:-}" "${INGEST_TIME_TXT:-}" "$EVIDENCE_JSON" "$INSTALL_TO" "$INGEST_SOURCES" "$MAX_EVENTS" "$MAX_LINES" <<'PY'
import json
import pathlib
import sys

ensure_path, health_path, ingest_path, ingest_time_path, output_path, app_path, ingest_sources, max_events, max_lines = sys.argv[1:]

def unwrap(path):
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
    return payload.get("result", payload)

def read_ingest(path):
    if not path:
        return None
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)

def parse_time(path):
    if not path:
        return {}
    values = {}
    for line in pathlib.Path(path).read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0] in {"real", "user", "sys"}:
            try:
                values[parts[0]] = float(parts[1])
            except ValueError:
                pass
    return values

health = unwrap(health_path)
ensure = unwrap(ensure_path)
ingest = read_ingest(ingest_path)
extensions = []
for item in health.get("required_extensions", []):
    extensions.append({
        "name": item.get("name"),
        "installed": bool(item.get("installed")),
        "version": item.get("version"),
        "preload_required": bool(item.get("preload_required")),
        "preload_active": item.get("preload_active"),
    })

payload = {
    "schema_version": "1context.perception-db-ultra-max.installed-smoke.v1",
    "status": "passed",
    "app": pathlib.Path(app_path).name,
    "storage": {
        "backend": health.get("backend"),
        "status": health.get("status"),
        "ready": bool(health.get("ready")),
        "storage_ready": bool(health.get("storage_ready")),
        "schema_state": health.get("schema_state"),
        "schema_version": health.get("schema_version"),
        "expected_schema_version": health.get("expected_schema_version"),
        "bundle_postgres_version": health.get("bundle_postgres_version"),
        "bundle_timescale_version": health.get("bundle_timescale_version"),
        "bundle_build_id": health.get("bundle_build_id"),
        "extensions": extensions,
    },
    "ensure": {
        "status": ensure.get("status"),
        "ready": bool(ensure.get("ready")),
        "storage_ready": bool(ensure.get("storage_ready")),
    },
    "network": {
        "tcp_15432_listener": False,
        "product_path": "unix_socket",
    },
    "dependency": {
        "runtime": "bundled_managed_postgres",
        "sbom_verified": True,
        "host_runtime_dependency": False,
    },
}
if ingest is not None:
    payload["ingest"] = {
        "enabled": True,
        "requested_sources": [item.strip() for item in ingest_sources.split(",") if item.strip()],
        "max_events": int(max_events),
        "max_lines": int(max_lines),
        "status": ingest.get("status"),
        "elapsed_ms": ingest.get("elapsed_ms"),
        "daemon_elapsed_ms": ingest.get("daemon_elapsed_ms"),
        "objects_emitted": ingest.get("objects_emitted"),
        "db_write": {
            "status": (ingest.get("db_write") or {}).get("status"),
            "elapsed_ms": (ingest.get("db_write") or {}).get("elapsed_ms"),
            "objects_attempted": (ingest.get("db_write") or {}).get("objects_attempted"),
            "objects_written": (ingest.get("db_write") or {}).get("objects_written"),
            "objects_deduplicated": (ingest.get("db_write") or {}).get("objects_deduplicated"),
        },
        "sources": [
            {
                "source": item.get("source"),
                "status": item.get("status"),
                "elapsed_ms": item.get("elapsed_ms"),
                "objects_emitted": item.get("objects_emitted"),
                "connector_key": (item.get("report") or {}).get("connector_key"),
                "files_seen": (item.get("report") or {}).get("files_seen"),
                "files_with_new_bytes": (item.get("report") or {}).get("files_with_new_bytes"),
                "lines_scanned": (item.get("report") or {}).get("lines_scanned"),
                "reached_event_limit": (item.get("report") or {}).get("reached_event_limit"),
                "reached_line_limit": (item.get("report") or {}).get("reached_line_limit"),
            }
            for item in ingest.get("sources", [])
        ],
        "time": parse_time(ingest_time_path),
    }
else:
    payload["ingest"] = {"enabled": False}

with open(output_path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY
fi

echo "installed Perception DB Ultra Max smoke passed"
echo "  app:        $INSTALL_TO"
echo "  evidence:   $WORKDIR"
echo "  tcp_15432:  no listener"
echo "  dependency: bundled managed-postgres verified with SBOM"
if [[ -n "$INGEST_SOURCES" ]]; then
  echo "  ingest:     $INGEST_SOURCES"
fi
if [[ -n "$EVIDENCE_JSON" ]]; then
  echo "  proof:      $EVIDENCE_JSON"
fi
SMOKE_PASSED=1
