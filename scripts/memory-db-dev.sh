#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

ENGINE="${ONECONTEXT_MEMORY_DB_CONTAINER_ENGINE:-}"
IMAGE="${ONECONTEXT_MEMORY_DB_IMAGE:-timescale/timescaledb-ha:pg17}"
CONTAINER="${ONECONTEXT_MEMORY_DB_CONTAINER:-onecontext-memory-db-dev}"
VOLUME="${ONECONTEXT_MEMORY_DB_VOLUME:-onecontext-memory-db-dev-pgdata}"
HOST_PORT="${ONECONTEXT_MEMORY_DB_PORT:-15432}"
POSTGRES_USER="${ONECONTEXT_MEMORY_DB_USER:-onecontext}"
PASSWORD="${ONECONTEXT_MEMORY_DB_PASSWORD:-onecontext_dev}"
DB="${ONECONTEXT_MEMORY_DB_NAME:-onecontext_memory}"

usage() {
  cat <<EOF
usage: $0 <start|provision|bootstrap|verify|status|psql|url|stop|reset>

Fresh-schema dev helper for the local 1Context memory DB.

Environment overrides:
  ONECONTEXT_MEMORY_DB_CONTAINER_ENGINE  docker|podman
  ONECONTEXT_MEMORY_DB_IMAGE             default: $IMAGE
  ONECONTEXT_MEMORY_DB_CONTAINER         default: $CONTAINER
  ONECONTEXT_MEMORY_DB_VOLUME            default: $VOLUME
  ONECONTEXT_MEMORY_DB_PORT              default: $HOST_PORT
  ONECONTEXT_MEMORY_DB_USER              default: $POSTGRES_USER
  ONECONTEXT_MEMORY_DB_PASSWORD          default: $PASSWORD
  ONECONTEXT_MEMORY_DB_NAME              default: $DB

Default connection URL:
  $(database_url)
EOF
}

database_url() {
  printf 'postgres://%s:%s@127.0.0.1:%s/%s\n' "$POSTGRES_USER" "$PASSWORD" "$HOST_PORT" "$DB"
}

require_engine() {
  if [[ -n "$ENGINE" ]]; then
    command -v "$ENGINE" >/dev/null || {
      echo "configured container engine not found: $ENGINE" >&2
      exit 1
    }
    return
  fi
  if command -v docker >/dev/null 2>&1; then
    ENGINE="docker"
  elif command -v podman >/dev/null 2>&1; then
    ENGINE="podman"
  else
    echo "missing container engine: install Docker Desktop, docker+colima, or podman" >&2
    exit 1
  fi
}

container_exists() {
  "$ENGINE" inspect "$CONTAINER" >/dev/null 2>&1
}

container_running() {
  container_exists && [[ "$("$ENGINE" inspect -f '{{.State.Running}}' "$CONTAINER")" == "true" ]]
}

start_db() {
  if container_exists; then
    container_running || "$ENGINE" start "$CONTAINER" >/dev/null
  else
    "$ENGINE" run -d \
      --name "$CONTAINER" \
      -p "127.0.0.1:${HOST_PORT}:5432" \
      -v "${VOLUME}:/home/postgres/pgdata/data" \
      -e POSTGRES_USER="$POSTGRES_USER" \
      -e POSTGRES_PASSWORD="$PASSWORD" \
      -e POSTGRES_DB="$DB" \
      "$IMAGE" \
      postgres -c shared_preload_libraries=timescaledb,pg_stat_statements >/dev/null
  fi
  wait_for_db
}

wait_for_db() {
  local i
  for ((i = 1; i <= 60; i++)); do
    if "$ENGINE" exec -e PGPASSWORD="$PASSWORD" "$CONTAINER" \
      pg_isready -U "$POSTGRES_USER" -d "$DB" >/dev/null 2>&1; then
      return
    fi
    sleep 1
  done
  echo "database did not become ready after 60s" >&2
  "$ENGINE" logs --tail 100 "$CONTAINER" >&2 || true
  exit 1
}

bootstrap_schema() {
  (cd "$ROOT" && cargo run -q -p onecontext-memory-db --bin onecontext-memory-db -- \
    bootstrap-schema --database-url "$(database_url)")
}

psql_db() {
  "$ENGINE" exec -i -e PGPASSWORD="$PASSWORD" "$CONTAINER" \
    psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$DB" "$@"
}

status_db() {
  "$ENGINE" ps --filter "name=${CONTAINER}"
  container_exists && echo "ONECONTEXT_MEMORY_DB_URL=$(database_url)"
}

stop_db() {
  container_exists && "$ENGINE" stop "$CONTAINER" >/dev/null
}

reset_db() {
  container_exists && "$ENGINE" rm -f "$CONTAINER" >/dev/null
  "$ENGINE" volume rm "$VOLUME" >/dev/null 2>&1 || true
}

main() {
  local command="${1:-}"
  case "$command" in
    start)
      require_engine
      start_db
      echo "ONECONTEXT_MEMORY_DB_URL=$(database_url)"
      ;;
    provision)
      require_engine
      start_db
      bootstrap_schema
      echo "ONECONTEXT_MEMORY_DB_URL=$(database_url)"
      ;;
    bootstrap|verify)
      require_engine
      wait_for_db
      bootstrap_schema
      ;;
    status)
      require_engine
      status_db
      ;;
    psql)
      require_engine
      wait_for_db
      shift
      psql_db "$@"
      ;;
    url)
      database_url
      ;;
    stop)
      require_engine
      stop_db
      ;;
    reset)
      require_engine
      reset_db
      ;;
    -h|--help|help|"")
      usage
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
