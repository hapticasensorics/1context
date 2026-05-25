#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RESULTS_ROOT="${ONECONTEXT_MEMORY_BACKFILL_RESULTS_ROOT:-$ROOT/test-results/memory-db-benchmarks}"
CURRENT_ID="${1:-}"

if ! command -v jq >/dev/null 2>&1; then
  echo "missing required tool: jq" >&2
  exit 1
fi

if [[ ! -d "$RESULTS_ROOT" ]]; then
  echo "no benchmark results directory: $RESULTS_ROOT" >&2
  exit 1
fi

summaries=()
while IFS= read -r summary; do
  summaries+=("$summary")
done < <(find "$RESULTS_ROOT" -mindepth 2 -maxdepth 2 -name summary.json -print | sort)
if [[ "${#summaries[@]}" -eq 0 ]]; then
  echo "no memory DB benchmark summaries found under $RESULTS_ROOT" >&2
  exit 1
fi

jq -r -s --arg current_id "$CURRENT_ID" '
  def run_status:
    .status // (if ([.rounds[]?.status == "ok"] | all) then "ok" else "failed" end);
  def r($name): .rounds[$name] // {};
  def metric($name):
    (r($name).objects_emitted // 0 | tostring) + "e/" +
    (r($name).db_write.objects_written // 0 | tostring) + "w/" +
    (r($name).db_write.objects_deduplicated // 0 | tostring) + "d/" +
    (r($name).elapsed_ms // 0 | tostring) + "ms";
  def db_count($key):
    ([.database_counts[]? | select(.key == $key) | .count][0] // 0 | tostring);
  def line:
    .benchmark_id +
    " status=" + run_status +
    " cold=" + metric("cold_insert") +
    " fresh_dedupe=" + metric("fresh_cursor_dedupe") +
    " steady=" + metric("steady_no_new") +
    " db_objects=" + db_count("objects_total") +
    " true_no_new=" + (.steady_no_new_truth.true_no_new // false | tostring);

  sort_by(.created_at // .benchmark_id) as $runs
  | (if $current_id == "" then $runs[-1] else ($runs | map(select(.benchmark_id == $current_id))[0]) end) as $current
  | if $current == null then
      error("benchmark summary not found for run id " + $current_id)
    else
      ($runs | map(select(.benchmark_id != $current.benchmark_id and (run_status == "ok"))) | .[-1]) as $previous
      | "current          " + ($current | line),
        (if $previous then
          "previous_success " + ($previous | line)
        else
          "previous_success none"
        end),
        (if $previous then
          "delta            cold_written=" +
          (((($current | r("cold_insert").db_write.objects_written // 0) - ($previous | r("cold_insert").db_write.objects_written // 0))) | tostring) +
          " fresh_deduped=" +
          (((($current | r("fresh_cursor_dedupe").db_write.objects_deduplicated // 0) - ($previous | r("fresh_cursor_dedupe").db_write.objects_deduplicated // 0))) | tostring) +
          " steady_emitted=" +
          (((($current | r("steady_no_new").objects_emitted // 0) - ($previous | r("steady_no_new").objects_emitted // 0))) | tostring)
        else empty end),
        "",
        "recent_successes",
        ($runs | map(select(run_status == "ok")) | .[-5:][]? | "  " + line)
    end
' "${summaries[@]}"
