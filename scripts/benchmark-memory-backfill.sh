#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

RUN_ID="${ONECONTEXT_MEMORY_BACKFILL_BENCH_ID:-$(date +%Y%m%d-%H%M%S)}"
SOURCES="${ONECONTEXT_MEMORY_BACKFILL_SOURCES:-codex,claude,imessage}"
MAX_EVENTS="${ONECONTEXT_MEMORY_BACKFILL_MAX_EVENTS:-1000}"
MAX_LINES="${ONECONTEXT_MEMORY_BACKFILL_MAX_LINES:-50000}"
DB_PORT="${ONECONTEXT_MEMORY_BACKFILL_DB_PORT:-15433}"
DB_CONTAINER="${ONECONTEXT_MEMORY_BACKFILL_DB_CONTAINER:-onecontext-memory-db-bench}"
DB_VOLUME="${ONECONTEXT_MEMORY_BACKFILL_DB_VOLUME:-onecontext-memory-db-bench-pgdata}"
KEEP_DB="${ONECONTEXT_MEMORY_BACKFILL_KEEP_DB:-0}"
RESULTS_ROOT="${ONECONTEXT_MEMORY_BACKFILL_RESULTS_ROOT:-$ROOT/test-results/memory-db-benchmarks}"
OUT_DIR="${ONECONTEXT_MEMORY_BACKFILL_OUT_DIR:-$RESULTS_ROOT/$RUN_ID}"

failures=0
round_names=()

db_env=(
  ONECONTEXT_MEMORY_DB_PORT="$DB_PORT"
  ONECONTEXT_MEMORY_DB_CONTAINER="$DB_CONTAINER"
  ONECONTEXT_MEMORY_DB_VOLUME="$DB_VOLUME"
)

run_db() {
  env "${db_env[@]}" "$ROOT/scripts/memory-db-dev.sh" "$@"
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
}

run_round() {
  local name="$1"
  local round_root="$2"
  local status_file="$round_root/run/memoryd-status.json"
  local summary_file="$OUT_DIR/${name}-summary.json"
  local raw_status_file="$OUT_DIR/${name}-memoryd-status.json"
  local round_status_file="$OUT_DIR/${name}-round-status.json"
  local stdout_file="$OUT_DIR/${name}-stdout.log"
  local stderr_file="$OUT_DIR/${name}-stderr.log"
  local command_status=0
  local round_failed=0

  echo "== $name =="
  echo "round_root=$round_root"
  round_names+=("$name")
  mkdir -p "$round_root/run" "$round_root/context-engine"

  set +e
  /usr/bin/time -p env DATABASE_URL="$DATABASE_URL" \
    cargo run -q -p onecontext-memory-db --bin onecontext-memoryd -- daemon \
      --home "$HOME" \
      --context-engine-root "$round_root/context-engine" \
      --run-dir "$round_root/run" \
      --sources "$SOURCES" \
      --max-events "$MAX_EVENTS" \
      --max-lines "$MAX_LINES" \
      --no-audit-spool \
      --once >"$stdout_file" 2>"$stderr_file"
  command_status=$?
  set -e

  if [[ -f "$status_file" ]]; then
    cp "$status_file" "$raw_status_file"
    jq --arg name "$name" --arg round_root "$round_root" '{
      round: $name,
      round_root: $round_root,
      status,
      elapsed_ms,
      cursor_file,
      cursor_saved,
      objects_emitted,
      db_write: {
        status: .db_write.status,
        attempted: .db_write.attempted,
        write_mode: .db_write.write_mode,
        writer: .db_write.writer,
        elapsed_ms: .db_write.elapsed_ms,
        objects_seen: (.db_write.objects_seen // .db_write.records_seen // 0),
        objects_attempted: (.db_write.objects_attempted // .db_write.records_seen // 0),
        objects_written: (.db_write.objects_written // .db_write.records_written // 0),
        objects_deduplicated: (.db_write.objects_deduplicated // .db_write.records_deduplicated // 0),
        objects_failed: (.db_write.objects_failed // .db_write.records_failed // 0)
      },
      source_counts: [
        .sources[]? | {
          source,
          status,
          object_count: (.object_count // .record_count // .objects_emitted // 0),
          elapsed_ms,
          reached_event_limit: (.report.reached_event_limit // false),
          reached_line_limit: (.report.reached_line_limit // false),
          partial_line_deferred: (.report.partial_line_deferred // false),
          files_seen: (.report.files_seen // 0),
          files_with_new_bytes: (.report.files_with_new_bytes // 0),
          lines_scanned: (.report.lines_scanned // 0),
          bytes_read: (.report.bytes_read // 0),
          sqlite_rows_scanned: (.report.sqlite_rows_scanned // 0),
          object_kinds: (.object_kinds // .kinds // {})
        }
      ],
      cursor_complete: ([.sources[]? | ((.report.reached_event_limit // false) or (.report.reached_line_limit // false) or (.report.partial_line_deferred // false))] | any | not)
    }' "$status_file" | tee "$summary_file"
  else
    jq -n \
      --arg name "$name" \
      --arg round_root "$round_root" \
      --arg stderr_file "$(basename "$stderr_file")" \
      '{
        round: $name,
        round_root: $round_root,
        status: "error",
        error: "memoryd did not produce a status file",
        stderr_path: $stderr_file,
        elapsed_ms: null,
        cursor_saved: false,
        objects_emitted: 0,
        db_write: {
          status: "not_attempted",
          attempted: false,
          objects_seen: 0,
          objects_attempted: 0,
          objects_written: 0,
          objects_deduplicated: 0,
          objects_failed: 0
        },
        source_counts: [],
        cursor_complete: false
      }' | tee "$summary_file"
  fi

  if [[ "$command_status" -ne 0 ]]; then
    round_failed=1
  fi
  if ! jq -e '
    .status == "ok"
    and ((.db_write.status // "ok") == "ok")
    and ((.db_write.objects_failed // 0) == 0)
    and ([.source_counts[]? | .status == "ok"] | all)
  ' "$summary_file" >/dev/null; then
    round_failed=1
  fi

  jq -n \
    --arg round "$name" \
    --arg summary_path "$(basename "$summary_file")" \
    --arg raw_status_path "$(basename "$raw_status_file")" \
    --arg stdout_path "$(basename "$stdout_file")" \
    --arg stderr_path "$(basename "$stderr_file")" \
    --argjson command_status "$command_status" \
    --argjson failed "$round_failed" \
    --slurpfile summary "$summary_file" \
    '{
      round: $round,
      status: (if $failed == 0 then "ok" else "failed" end),
      command_exit_status: $command_status,
      summary_path: $summary_path,
      raw_status_path: $raw_status_path,
      stdout_path: $stdout_path,
      stderr_path: $stderr_path,
      memoryd_status: ($summary[0].status // "missing"),
      db_write_status: ($summary[0].db_write.status // "missing"),
      source_statuses: ($summary[0].source_counts | map({source, status})),
      cursor_complete: ($summary[0].cursor_complete // false)
    }' | tee "$round_status_file"

  if [[ "$round_failed" -ne 0 ]]; then
    echo "benchmark round $name failed; see $round_status_file" >&2
    return 1
  fi

  return 0
}

cleanup() {
  if [[ "$KEEP_DB" != "1" ]]; then
    run_db reset >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

cd "$ROOT"
mkdir -p "$OUT_DIR"
require_tool jq

echo "benchmark_id=$RUN_ID"
echo "sources=$SOURCES"
echo "max_events=$MAX_EVENTS"
echo "max_lines=$MAX_LINES"
echo "db_port=$DB_PORT"
echo "out_dir=$OUT_DIR"

run_db reset >/dev/null 2>&1 || true
run_db provision
DATABASE_URL="$(run_db url)"
export DATABASE_URL
echo "database_url=$DATABASE_URL"

cold_root="$(mktemp -d "/tmp/onecontext-memory-backfill-cold_insert.XXXXXX")"
dedupe_root="$(mktemp -d "/tmp/onecontext-memory-backfill-fresh_cursor_dedupe.XXXXXX")"

if ! run_round cold_insert "$cold_root"; then
  failures=1
fi
if ! run_round fresh_cursor_dedupe "$dedupe_root"; then
  failures=1
fi
if ! run_round steady_no_new "$cold_root"; then
  failures=1
fi

echo "== database_counts =="
run_db psql -Atc "
SELECT 'sources_total', count(*) FROM perception.sources;
SELECT 'series_total', count(*) FROM perception.series;
SELECT 'source_records_total', count(*) FROM perception.source_records;
SELECT 'objects_total', count(*) FROM perception.objects;
SELECT 'object_edges_total', count(*) FROM perception.object_edges;
SELECT COALESCE(s.source_key, sr.source_id::text) AS source_key, count(*)
FROM perception.source_records sr
LEFT JOIN perception.sources s
  ON s.source_id = sr.source_id
GROUP BY 1
ORDER BY 1;
SELECT 'kind:' || kind, count(*) FROM perception.objects GROUP BY 1 ORDER BY 1;
SELECT 'series_kind:' || series_kind, count(*) FROM perception.series GROUP BY 1 ORDER BY 1;
" | tee "$OUT_DIR/database-counts.txt"

jq -Rn '
  [inputs | select(length > 0) | split("|") | {key: .[0], count: (.[1] | tonumber)}]
' "$OUT_DIR/database-counts.txt" | tee "$OUT_DIR/database-counts.json"

round_json_args=()
for round_name in "${round_names[@]}"; do
  round_json_args+=(--slurpfile "$round_name" "$OUT_DIR/${round_name}-summary.json")
done

jq -n \
  --arg benchmark_id "$RUN_ID" \
  --arg created_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg sources "$SOURCES" \
  --argjson max_events "$MAX_EVENTS" \
  --argjson max_lines "$MAX_LINES" \
  --argjson failures "$failures" \
  "${round_json_args[@]}" \
  --slurpfile counts "$OUT_DIR/database-counts.json" \
  '{
    benchmark_id: $benchmark_id,
    created_at: $created_at,
    status: (if $failures == 0 then "ok" else "failed" end),
    sources: ($sources | split(",")),
    max_events: $max_events,
    max_lines: $max_lines,
    rounds: {
      cold_insert: $cold_insert[0],
      fresh_cursor_dedupe: $fresh_cursor_dedupe[0],
      steady_no_new: $steady_no_new[0]
    },
    steady_no_new_truth: {
      true_no_new: (($cold_insert[0].cursor_complete // false) and (($steady_no_new[0].objects_emitted // 0) == 0)),
      prior_cursor_complete: ($cold_insert[0].cursor_complete // false),
      steady_objects_emitted: ($steady_no_new[0].objects_emitted // 0),
      note: (if (($cold_insert[0].cursor_complete // false) and (($steady_no_new[0].objects_emitted // 0) == 0)) then
        "steady_no_new reused the cold_insert cursor root and observed no emitted objects"
      elif ($cold_insert[0].cursor_complete // false | not) then
        "cold_insert hit an ingest cap or deferred data, so steady_no_new is cursor-continuation rather than true no-new polling"
      else
        "steady_no_new reused the cold_insert cursor root but observed emitted objects; source files likely changed during the run"
      end)
    },
    database_counts_path: "database-counts.txt",
    database_counts_json_path: "database-counts.json",
    database_counts: $counts[0],
    trend_path: "trend.txt"
  }' | tee "$OUT_DIR/summary.json"

jq -r '
  def r($name): .rounds[$name] // {};
  def metric($name):
    (r($name).objects_emitted // 0 | tostring) + "e/" +
    (r($name).db_write.objects_written // 0 | tostring) + "w/" +
    (r($name).db_write.objects_deduplicated // 0 | tostring) + "d/" +
    (r($name).elapsed_ms // 0 | tostring) + "ms";
  def count($key):
    ([.database_counts[]? | select(.key == $key) | .count][0] // 0 | tostring);
  .benchmark_id +
  " status=" + .status +
  " cold=" + metric("cold_insert") +
  " fresh_dedupe=" + metric("fresh_cursor_dedupe") +
  " steady=" + metric("steady_no_new") +
  " db_objects=" + count("objects_total") +
  " db_source_records=" + count("source_records_total") +
  " true_no_new=" + (.steady_no_new_truth.true_no_new | tostring)
' "$OUT_DIR/summary.json" | tee "$OUT_DIR/trend.txt"

if [[ -x "$ROOT/scripts/summarize-memory-benchmarks.sh" ]]; then
  "$ROOT/scripts/summarize-memory-benchmarks.sh" "$RUN_ID" | tee "$OUT_DIR/latest-vs-previous.txt" || true
fi

if [[ "$failures" -ne 0 ]]; then
  echo "benchmark failed; see $OUT_DIR/summary.json" >&2
  exit 1
fi
