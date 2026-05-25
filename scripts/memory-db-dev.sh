#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MIGRATIONS_DIR="$ROOT/crates/onecontext-memory-db/migrations"
PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

ENGINE="${ONECONTEXT_MEMORY_DB_CONTAINER_ENGINE:-}"
IMAGE="${ONECONTEXT_MEMORY_DB_IMAGE:-timescale/timescaledb-ha:pg17}"
CONTAINER_NAME="${ONECONTEXT_MEMORY_DB_CONTAINER:-onecontext-memory-db-dev}"
VOLUME_NAME="${ONECONTEXT_MEMORY_DB_VOLUME:-onecontext-memory-db-dev-pgdata}"
HOST_PORT="${ONECONTEXT_MEMORY_DB_PORT:-15432}"
POSTGRES_USER="${ONECONTEXT_MEMORY_DB_USER:-onecontext}"
POSTGRES_PASSWORD="${ONECONTEXT_MEMORY_DB_PASSWORD:-onecontext_dev}"
POSTGRES_DB="${ONECONTEXT_MEMORY_DB_NAME:-onecontext_memory}"
CONTAINER_PORT="5432"

usage() {
  cat <<EOF
usage: $0 <start|provision|migrate|verify|status|psql|url|stop|reset>

Dev-only Postgres + Timescale lifecycle for the 1Context memory DB.

Environment overrides:
  ONECONTEXT_MEMORY_DB_CONTAINER_ENGINE  docker|podman
  ONECONTEXT_MEMORY_DB_IMAGE             default: $IMAGE
  ONECONTEXT_MEMORY_DB_CONTAINER         default: $CONTAINER_NAME
  ONECONTEXT_MEMORY_DB_VOLUME            default: $VOLUME_NAME
  ONECONTEXT_MEMORY_DB_PORT              default: $HOST_PORT
  ONECONTEXT_MEMORY_DB_USER              default: $POSTGRES_USER
  ONECONTEXT_MEMORY_DB_PASSWORD          default: $POSTGRES_PASSWORD
  ONECONTEXT_MEMORY_DB_NAME              default: $POSTGRES_DB

Default DATABASE_URL:
  $(database_url)
EOF
}

main() {
  local command="${1:-}"
  case "$command" in
    start)
      require_engine
      start_db
      ;;
    provision)
      require_engine
      start_db
      migrate_db
      verify_db
      print_connection
      ;;
    migrate)
      require_engine
      wait_for_db
      migrate_db
      ;;
    verify)
      require_engine
      wait_for_db
      verify_db
      ;;
    status)
      require_engine
      status_db
      ;;
    psql)
      require_engine
      wait_for_db
      shift
      psql_db "$POSTGRES_DB" "$@"
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

require_engine() {
  if [[ -n "$ENGINE" ]]; then
    if ! command -v "$ENGINE" >/dev/null 2>&1; then
      echo "configured container engine not found: $ENGINE" >&2
      exit 1
    fi
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

database_url() {
  printf 'postgres://%s:%s@127.0.0.1:%s/%s\n' \
    "$POSTGRES_USER" "$POSTGRES_PASSWORD" "$HOST_PORT" "$POSTGRES_DB"
}

start_db() {
  if "$ENGINE" inspect "$CONTAINER_NAME" >/dev/null 2>&1; then
    if [[ "$("$ENGINE" inspect -f '{{.State.Running}}' "$CONTAINER_NAME")" != "true" ]]; then
      "$ENGINE" start "$CONTAINER_NAME" >/dev/null
    fi
  else
    "$ENGINE" run -d \
      --name "$CONTAINER_NAME" \
      -p "127.0.0.1:${HOST_PORT}:${CONTAINER_PORT}" \
      -v "${VOLUME_NAME}:/home/postgres/pgdata/data" \
      -e POSTGRES_USER="$POSTGRES_USER" \
      -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
      -e POSTGRES_DB="$POSTGRES_DB" \
      "$IMAGE" \
      postgres -c shared_preload_libraries=timescaledb,pg_stat_statements >/dev/null
  fi

  wait_for_db
  print_connection
}

wait_for_db() {
  local attempts=60
  local i
  for ((i = 1; i <= attempts; i++)); do
    if "$ENGINE" exec -e PGPASSWORD="$POSTGRES_PASSWORD" "$CONTAINER_NAME" \
      pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
      return
    fi
    sleep 1
  done

  echo "database did not become ready after ${attempts}s" >&2
  "$ENGINE" logs --tail 100 "$CONTAINER_NAME" >&2 || true
  exit 1
}

migrate_db() {
  local attempts=20
  local i
  for ((i = 1; i <= attempts; i++)); do
    if (cd "$ROOT" && cargo run -q -p onecontext-memory-db --bin onecontext-memory-db -- migrate --database-url "$(database_url)"); then
      return
    fi
    sleep 1
  done

  echo "migration runner did not connect successfully after ${attempts}s" >&2
  "$ENGINE" logs --tail 100 "$CONTAINER_NAME" >&2 || true
  exit 1
}

verify_db() {
  local result
  result="$(psql_db "$POSTGRES_DB" -Atc "
SELECT
  extname
FROM pg_extension
WHERE extname IN ('timescaledb', 'vector', 'pg_trgm', 'btree_gist', 'pgcrypto', 'pg_stat_statements')
ORDER BY extname;
")"
  echo "$result"

  psql_db "$POSTGRES_DB" -Atc "
SELECT
  format('perception.%s rows=%s hypertable=%s',
    relname,
    row_count,
    EXISTS (
      SELECT 1
      FROM timescaledb_information.hypertables h
      WHERE h.hypertable_schema = 'perception'
        AND h.hypertable_name = relname
    )
  )
FROM (
  SELECT 'blobs' AS relname, count(*) AS row_count FROM perception.blobs
  UNION ALL SELECT 'lanes', count(*) FROM perception.lanes
  UNION ALL SELECT 'object_edges', count(*) FROM perception.object_edges
  UNION ALL SELECT 'objects', count(*) FROM perception.objects
  UNION ALL SELECT 'series', count(*) FROM perception.series
  UNION ALL SELECT 'source_cursors', count(*) FROM perception.source_cursors
  UNION ALL SELECT 'source_records', count(*) FROM perception.source_records
  UNION ALL SELECT 'sources', count(*) FROM perception.sources
  UNION ALL SELECT 'timeline_projection_items', count(*) FROM perception.timeline_projection_items
  UNION ALL SELECT 'timeline_projections', count(*) FROM perception.timeline_projections
) tables
ORDER BY relname;
"

  psql_db "$POSTGRES_DB" -Atc "
SELECT format(
  'perception.object_density_1m continuous_aggregate=%s refresh_policy=%s',
  EXISTS (
    SELECT 1
    FROM timescaledb_information.continuous_aggregates aggregate_metadata
    WHERE aggregate_metadata.view_schema = 'perception'
      AND aggregate_metadata.view_name = 'object_density_1m'
  ),
  EXISTS (
    SELECT 1
    FROM timescaledb_information.jobs job_metadata
    WHERE job_metadata.proc_name = 'policy_refresh_continuous_aggregate'
      AND job_metadata.hypertable_schema = 'perception'
      AND job_metadata.hypertable_name = 'object_density_1m'
  )
);
"

  psql_db "$POSTGRES_DB" -Atc "
WITH embedding_constraints AS (
  SELECT
    count(*) FILTER (
      WHERE table_constraint.contype = 'f'
        AND attribute.attname = 'object_id'
    ) AS object_fk_count,
    count(*) FILTER (
      WHERE table_constraint.contype = 'f'
        AND attribute.attname = 'user_id'
    ) AS user_fk_count
  FROM pg_constraint table_constraint
  JOIN pg_class table_class ON table_class.oid = table_constraint.conrelid
  JOIN pg_namespace table_namespace ON table_namespace.oid = table_class.relnamespace
  JOIN unnest(table_constraint.conkey) AS key(attnum) ON TRUE
  JOIN pg_attribute attribute
    ON attribute.attrelid = table_class.oid
   AND attribute.attnum = key.attnum
  WHERE table_namespace.nspname = 'search'
    AND table_class.relname = 'object_embeddings'
),
embedding_indexes AS (
  SELECT count(*) AS unique_object_model_indexes
  FROM pg_index index_metadata
  JOIN pg_class table_class ON table_class.oid = index_metadata.indrelid
  JOIN pg_namespace table_namespace ON table_namespace.oid = table_class.relnamespace
  WHERE table_namespace.nspname = 'search'
    AND table_class.relname = 'object_embeddings'
    AND index_metadata.indisunique
    AND (
      SELECT array_agg(attribute.attname::TEXT ORDER BY key.ordinality)
      FROM unnest(index_metadata.indkey) WITH ORDINALITY AS key(attnum, ordinality)
      JOIN pg_attribute attribute
        ON attribute.attrelid = table_class.oid
       AND attribute.attnum = key.attnum
      WHERE key.attnum > 0
    ) = ARRAY['object_id', 'embedding_model']::TEXT[]
)
SELECT format(
  'search.object_embeddings rows=%s object_fk=%s user_fk=%s unique_object_model_indexes=%s hnsw=%s',
  (SELECT count(*) FROM search.object_embeddings),
  (SELECT object_fk_count FROM embedding_constraints),
  (SELECT user_fk_count FROM embedding_constraints),
  (SELECT unique_object_model_indexes FROM embedding_indexes),
  to_regclass('search.object_embeddings_hnsw_idx') IS NOT NULL
);
"

  psql_db "$POSTGRES_DB" -Atc "
SELECT format('migrations=%s latest=%s', count(*), max(version))
FROM app.schema_migrations;
"
}

status_db() {
  "$ENGINE" ps --filter "name=${CONTAINER_NAME}"
  if "$ENGINE" inspect "$CONTAINER_NAME" >/dev/null 2>&1; then
    print_connection
  fi
}

stop_db() {
  if "$ENGINE" inspect "$CONTAINER_NAME" >/dev/null 2>&1; then
    "$ENGINE" stop "$CONTAINER_NAME" >/dev/null
  fi
}

reset_db() {
  if "$ENGINE" inspect "$CONTAINER_NAME" >/dev/null 2>&1; then
    "$ENGINE" rm -f "$CONTAINER_NAME" >/dev/null
  fi
  "$ENGINE" volume rm "$VOLUME_NAME" >/dev/null 2>&1 || true
}

print_connection() {
  echo "DATABASE_URL=$(database_url)"
}

psql_db() {
  local db="$1"
  shift
  "$ENGINE" exec -i -e PGPASSWORD="$POSTGRES_PASSWORD" "$CONTAINER_NAME" \
    psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$db" "$@"
}

main "$@"
