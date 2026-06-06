#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/smoke-managed-postgres-bundle.sh [--run] [--allow-host-fingerprints] <bundle-prefix-or-app>

Validates a staged managed Postgres bundle and optionally proves it can
initialize a temporary cluster, start on a Unix socket, and create the required
extensions.
USAGE
}

RUN=0
ALLOW_HOST_FINGERPRINTS=0
ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --run)
      RUN=1
      shift
      ;;
    --allow-host-fingerprints)
      ALLOW_HOST_FINGERPRINTS=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      ARGS+=( "$1" )
      shift
      ;;
  esac
done

if [[ ${#ARGS[@]} -ne 1 ]]; then
  usage >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${ARGS[0]}"
VERIFY_ARGS=()
if [[ "$ALLOW_HOST_FINGERPRINTS" == "1" ]]; then
  VERIFY_ARGS+=( --allow-host-fingerprints )
fi
"$ROOT/scripts/verify-managed-postgres-bundle.sh" "${VERIFY_ARGS[@]}" "$TARGET"

if [[ "$TARGET" == *.app ]]; then
  PREFIX="$TARGET/Contents/Resources/managed-postgres/macos-arm64"
else
  PREFIX="$TARGET"
fi
PREFIX="$(cd "$PREFIX" && pwd)"

cat <<PLAN
managed Postgres smoke plan:
  prefix: $PREFIX
  mode:   $([[ $RUN -eq 1 ]] && echo run || echo dry-run)
  db:     onecontext
  user:   onecontext
PLAN

if [[ $RUN -ne 1 ]]; then
  exit 0
fi

WORKDIR="$(mktemp -d /tmp/onecontext-managed-postgres-smoke.XXXXXX)"
PGDATA="$WORKDIR/pgdata"
SOCKET_DIR="$WORKDIR/run"
LOGFILE="$WORKDIR/postgres.log"
SMOKE_PASSED=0
PORT="$(/usr/bin/python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"

cleanup() {
  local rc=$?
  if [[ -x "$PREFIX/bin/pg_ctl" && -d "$PGDATA" ]]; then
    "$PREFIX/bin/pg_ctl" -D "$PGDATA" stop -m fast >/dev/null 2>&1 || true
  fi
  if [[ "$SMOKE_PASSED" == "1" ]]; then
    rm -rf "$WORKDIR"
  else
    echo "managed Postgres smoke workdir preserved: $WORKDIR" >&2
    if [[ -f "$LOGFILE" ]]; then
      echo "managed Postgres smoke postgres.log tail:" >&2
      tail -n 120 "$LOGFILE" >&2
    fi
  fi
  return "$rc"
}
trap cleanup EXIT

mkdir -p "$SOCKET_DIR"
"$PREFIX/bin/initdb" -D "$PGDATA" -U postgres -A trust >/dev/null

cat >> "$PGDATA/postgresql.conf" <<EOF
listen_addresses = ''
unix_socket_directories = '$SOCKET_DIR'
port = $PORT
shared_preload_libraries = 'timescaledb,pg_stat_statements'
dynamic_library_path = '\$libdir:$PREFIX/lib/postgresql'
EOF
cat > "$PGDATA/pg_hba.conf" <<'EOF'
local all postgres trust
local all onecontext scram-sha-256
local all all reject
host all all 127.0.0.1/32 reject
host all all ::1/128 reject
EOF

"$PREFIX/bin/pg_ctl" -w -D "$PGDATA" -l "$LOGFILE" start >/dev/null
"$PREFIX/bin/pg_isready" -h "$SOCKET_DIR" -p "$PORT" -d postgres -U postgres >/dev/null

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "managed Postgres smoke failed: a TCP listener was created on port $PORT" >&2
  exit 1
fi

"$PREFIX/bin/psql" -v ON_ERROR_STOP=1 -h "$SOCKET_DIR" -p "$PORT" -U postgres -d postgres <<'SQL' >/dev/null
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'onecontext') THEN
    CREATE ROLE onecontext NOSUPERUSER LOGIN PASSWORD 'onecontext_dev';
  ELSE
    ALTER ROLE onecontext WITH NOSUPERUSER LOGIN PASSWORD 'onecontext_dev';
  END IF;
END
$$;
SELECT 'CREATE DATABASE onecontext OWNER onecontext'
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = 'onecontext')
\gexec
SQL

"$PREFIX/bin/psql" -v ON_ERROR_STOP=1 -h "$SOCKET_DIR" -p "$PORT" -U postgres -d onecontext <<'SQL' >/dev/null
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
SQL

APP_ROLE_PROOF="$(PGPASSWORD=onecontext_dev "$PREFIX/bin/psql" -At -h "$SOCKET_DIR" -p "$PORT" -U onecontext -d onecontext -c "SELECT current_user || ',' || (inet_server_addr() IS NULL)::text || ',' || (NOT rolsuper)::text FROM pg_roles WHERE rolname = current_user")"
if [[ "$APP_ROLE_PROOF" != "onecontext,true,true" ]]; then
  echo "managed Postgres smoke failed: app role/socket proof was $APP_ROLE_PROOF" >&2
  exit 1
fi

TIMESCALE_VERSION="$(PGPASSWORD=onecontext_dev "$PREFIX/bin/psql" -At -h "$SOCKET_DIR" -p "$PORT" -U onecontext -d onecontext -c "SELECT extversion FROM pg_extension WHERE extname='timescaledb'")"
VECTOR_VERSION="$(PGPASSWORD=onecontext_dev "$PREFIX/bin/psql" -At -h "$SOCKET_DIR" -p "$PORT" -U onecontext -d onecontext -c "SELECT extversion FROM pg_extension WHERE extname='vector'")"

echo "managed Postgres smoke passed"
echo "  socket_dir: $SOCKET_DIR"
echo "  port:       $PORT"
echo "  app_role:   non-superuser socket"
echo "  timescale:  ${TIMESCALE_VERSION:-missing}"
echo "  vector:     ${VECTOR_VERSION:-missing}"
SMOKE_PASSED=1
