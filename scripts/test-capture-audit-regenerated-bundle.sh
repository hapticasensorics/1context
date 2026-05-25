#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_AUDIT_ROOT="/Users/paulhan/Library/Application Support/1Context Dev/capture-audits/capture-audit-20260525T093158Z-60s"

usage() {
  cat <<'USAGE'
Usage:
  scripts/test-capture-audit-regenerated-bundle.sh

Environment:
  ONECONTEXT_CAPTURE_AUDIT_ROOT       Saved capture audit root. Defaults to the
                                      2026-05-25 60s audit when present.
  ONECONTEXT_REGEN_EVIDENCE_DIR       Evidence output directory. Defaults to
                                      dist/capture-audit-regeneration/<timestamp>.
  ONECONTEXT_CAPTURE_BUNDLER          Optional path to onecontext-capture-bundler.

Regenerates a READY capture bundle from a saved audit raw/window spool copy and
saved capability JSON files. The command does not query live macOS capture
permissions. It fails when the regenerated bundle is not attention-filter ready.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

fail() {
  echo "capture audit regenerated-bundle regression failed: $*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "required tool not found: $1"
}

line_count() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    printf '0'
    return
  fi
  awk 'NF { count += 1 } END { print count + 0 }' "$file"
}

record_failure() {
  local label="$1"
  local detail="${2:-}"
  failures=$((failures + 1))
  printf 'FAIL %s\n' "$label" | tee -a "$CHECK_LOG" >&2
  if [[ -n "$detail" ]]; then
    printf '  %s\n' "$detail" | tee -a "$CHECK_LOG" >&2
  fi
}

record_pass() {
  local label="$1"
  printf 'PASS %s\n' "$label" | tee -a "$CHECK_LOG"
}

check_command() {
  local label="$1"
  shift
  if "$@"; then
    record_pass "$label"
  else
    record_failure "$label"
  fi
}

jq_check() {
  local label="$1"
  local file="$2"
  shift 2
  if jq -e "$@" "$file" >/dev/null; then
    record_pass "$label"
  else
    record_failure "$label" "Payload: $file"
  fi
}

run_bundler() {
  if [[ -n "${ONECONTEXT_CAPTURE_BUNDLER:-}" ]]; then
    "$ONECONTEXT_CAPTURE_BUNDLER" "$@"
  else
    cargo run --quiet -p onecontext-capture-bundler -- "$@"
  fi
}

collect_jsonl_records() {
  local source_root="$1"
  local output="$2"
  : >"$output"
  for spool_dir in events windows displays; do
    local dir="$source_root/$spool_dir"
    [[ -d "$dir" ]] || continue
    while IFS= read -r -d '' file; do
      jq -c . "$file" >>"$output"
    done < <(find "$dir" -type f -name '*.jsonl' -print0 | sort -z)
  done
}

filter_records_to_window() {
  local input="$1"
  local output="$2"
  local start="$3"
  local end="$4"
  jq -c --arg start "$start" --arg end "$end" '
    def normalized_rfc3339:
      if type == "string" then
        sub("([+-][0-9][0-9]:[0-9][0-9])$"; "Z")
        | if test("\\.[0-9]+Z$") then . else sub("Z$"; ".000Z") end
      else
        null
      end;
    def primary_time:
      .eventTimeStart // .event_time_start // .recordedAt // .recorded_at;
    ($start | normalized_rfc3339) as $start_time
    | ($end | normalized_rfc3339) as $end_time
    | (primary_time | normalized_rfc3339) as $event_time
    | select($event_time != null and $event_time >= $start_time and $event_time <= $end_time)
  ' "$input" >"$output"
}

write_event_type_counts() {
  local records="$1"
  local output="$2"
  jq -s '
    def event_type: .eventType // .event_type // .type // "";
    [ .[] | {event_type: event_type} ]
    | sort_by(.event_type)
    | group_by(.event_type)
    | map({event_type: .[0].event_type, count: length})
  ' "$records" >"$output"
}

write_expected_lane_counts() {
  local records="$1"
  local output="$2"
  jq -s '
    def event_type: .eventType // .event_type // .type // "";
    {
      "capture.windows": ([.[] | select(event_type == "capture.window_snapshot")] | length),
      "capture.displays": ([.[] | select(event_type == "capture.display_snapshot")] | length),
      "capture.events": length,
      "capture.ax": ([.[] | select(event_type | startswith("capture.ax"))] | length),
      "capture.ux": ([.[] | select(event_type | startswith("capture.ux"))] | length),
      "capture.active_window_frames": ([.[] | select(event_type == "capture.active_window_frame_metadata")] | length),
      "capture.browser": ([.[] | select(event_type | startswith("capture.browser"))] | length),
      "capture.terminal": ([.[] | select(event_type | startswith("capture.terminal"))] | length),
      "capture.editor": ([.[] | select(event_type | startswith("capture.editor"))] | length)
    }
  ' "$records" >"$output"
}

write_source_counts() {
  local sources="$1"
  local output="$2"
  jq '
    [.sources[] | {key: .lane_id, value: .record_count}]
    | from_entries
  ' "$sources" >"$output"
}

write_capability_timestamp_report() {
  local file="$1"
  local label="$2"
  local start="$3"
  local end="$4"
  local output="$5"
  jq \
    --arg file "$label" \
    --arg start "$start" \
    --arg end "$end" '
      def epoch($value):
        if ($value | type) == "string" then
          try ($value | sub("\\.[0-9]+Z$"; "Z") | sub("\\.[0-9]+([+-][0-9]{2}:[0-9]{2})$"; "\\1") | fromdateiso8601) catch null
        else
          null
        end;
      def timestamp_leaf($path):
        ($path[-1] | tostring) as $key
        | ($key | test("time|Time|timestamp|Timestamp|generated|generatedAt|generated_at|recordedAt|capturedAt|updatedAt|created_at|ready_at"));

      (epoch($start)) as $start_epoch
      | (epoch($end)) as $end_epoch
      | [
          paths(scalars) as $p
          | select(timestamp_leaf($p))
          | {path: ($p | map(tostring) | join(".")), value: getpath($p)}
          | .epoch = epoch(.value)
          | select(.epoch != null)
          | .inside_window = (.epoch >= $start_epoch and .epoch <= $end_epoch)
        ] as $timestamps
      | {
          file: $file,
          timestamp_count: ($timestamps | length),
          inside_count: ([ $timestamps[] | select(.inside_window) ] | length),
          outside_count: ([ $timestamps[] | select(.inside_window | not) ] | length),
          timestamps: $timestamps
        }
    ' "$file" >"$output"
}

require_tool jq
require_tool awk
require_tool sort
require_tool diff
require_tool comm
require_tool ditto
require_tool find
require_tool grep
if [[ -z "${ONECONTEXT_CAPTURE_BUNDLER:-}" ]]; then
  require_tool cargo
fi

AUDIT_ROOT="${ONECONTEXT_CAPTURE_AUDIT_ROOT:-$DEFAULT_AUDIT_ROOT}"
AUDIT_ROOT="${AUDIT_ROOT%/}"
RUN_JSON="$AUDIT_ROOT/run.json"
RAW_ROOT="$AUDIT_ROOT/raw/window"

[[ -d "$AUDIT_ROOT" ]] || fail "audit root not found: $AUDIT_ROOT"
[[ -f "$RUN_JSON" ]] || fail "run.json not found: $RUN_JSON"
[[ -d "$RAW_ROOT" ]] || fail "raw/window spool copy not found: $RAW_ROOT"

RUN_ID="$(jq -r '.run_id // "capture-audit-regeneration"' "$RUN_JSON")"
TIME_START="$(jq -r '.time_start' "$RUN_JSON")"
TIME_END="$(jq -r '.time_end' "$RUN_JSON")"
[[ "$TIME_START" != "null" && -n "$TIME_START" ]] || fail "run.json lacks time_start"
[[ "$TIME_END" != "null" && -n "$TIME_END" ]] || fail "run.json lacks time_end"

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="${ONECONTEXT_REGEN_EVIDENCE_DIR:-$ROOT/dist/capture-audit-regeneration/$STAMP}"
WORK_CAPTURE_ROOT="$EVIDENCE_DIR/capture-root"
CHECK_LOG="$EVIDENCE_DIR/checks.log"
BUNDLER_HELP="$EVIDENCE_DIR/bundler-help.txt"
EXPORT_RESPONSE="$EVIDENCE_DIR/export-response.json"
EXPORT_STDERR="$EVIDENCE_DIR/export.stderr"
VALIDATE_RESPONSE="$EVIDENCE_DIR/validate-response.json"
VALIDATE_STDERR="$EVIDENCE_DIR/validate.stderr"

mkdir -p "$EVIDENCE_DIR"
: >"$CHECK_LOG"
rm -rf "$WORK_CAPTURE_ROOT"
mkdir -p "$WORK_CAPTURE_ROOT/events" "$WORK_CAPTURE_ROOT/windows" "$WORK_CAPTURE_ROOT/displays"

for spool_dir in events windows displays; do
  if [[ -d "$RAW_ROOT/$spool_dir" ]]; then
    ditto "$RAW_ROOT/$spool_dir" "$WORK_CAPTURE_ROOT/$spool_dir"
  fi
done

FRAMES_2FPS_DIR="$WORK_CAPTURE_ROOT/media/frames-2fps"
mkdir -p "$FRAMES_2FPS_DIR"
if [[ -d "$RAW_ROOT/media/frames-2fps" ]] && find "$RAW_ROOT/media/frames-2fps" -type f | grep -q .; then
  ditto "$RAW_ROOT/media/frames-2fps" "$FRAMES_2FPS_DIR"
else
  # Older saved capture audits predate decoded screenshot artifacts. Seed tiny
  # deterministic frame files so this harness keeps validating bundle export
  # behavior against the current V0 contract without depending on live decoding.
  printf 'fixture frame 1\n' >"$FRAMES_2FPS_DIR/frame-000001.jpg"
  printf 'fixture frame 2\n' >"$FRAMES_2FPS_DIR/frame-000002.jpg"
fi

set +e
run_bundler --help >"$BUNDLER_HELP" 2>&1
help_status=$?
set -e
if [[ "$help_status" -ne 0 ]]; then
  cat "$BUNDLER_HELP" >&2 || true
  fail "could not inspect bundler help; evidence: $EVIDENCE_DIR"
fi

bundler_supports() {
  grep -q -- "$1" "$BUNDLER_HELP"
}

export_args=(
  export
  --capture-root "$WORK_CAPTURE_ROOT"
  --start "$TIME_START"
  --end "$TIME_END"
)

bundler_supports "--debug-pin" && export_args+=(--debug-pin)
[[ -f "$AUDIT_ROOT/outputs/status-after.json" ]] && bundler_supports "--status-json" && export_args+=(--status-json "$AUDIT_ROOT/outputs/status-after.json")
[[ -f "$AUDIT_ROOT/outputs/ux-status.json" ]] && bundler_supports "--ux-status-json" && export_args+=(--ux-status-json "$AUDIT_ROOT/outputs/ux-status.json")
[[ -f "$AUDIT_ROOT/outputs/sampler-status.json" ]] && bundler_supports "--sampler-json" && export_args+=(--sampler-json "$AUDIT_ROOT/outputs/sampler-status.json")
[[ -f "$AUDIT_ROOT/outputs/browser-proof.json" ]] && bundler_supports "--browser-proof-json" && export_args+=(--browser-proof-json "$AUDIT_ROOT/outputs/browser-proof.json")
bundler_supports "--frames-2fps-dir" && export_args+=(--frames-2fps-dir "$FRAMES_2FPS_DIR")

set +e
run_bundler "${export_args[@]}" >"$EXPORT_RESPONSE" 2>"$EXPORT_STDERR"
export_status=$?
set -e

if [[ "$export_status" -ne 0 ]]; then
  cat "$EXPORT_RESPONSE" >&2 || true
  cat "$EXPORT_STDERR" >&2 || true
  fail "bundle export failed; evidence: $EVIDENCE_DIR"
fi

BUNDLE_PATH="$(jq -r '.bundle.bundle_path // empty' "$EXPORT_RESPONSE")"
[[ -n "$BUNDLE_PATH" && -d "$BUNDLE_PATH" ]] || fail "export did not produce a bundle_path; evidence: $EXPORT_RESPONSE"

set +e
run_bundler validate --bundle "$BUNDLE_PATH" --strict >"$VALIDATE_RESPONSE" 2>"$VALIDATE_STDERR"
validate_status=$?
set -e

RAW_RECORDS="$EVIDENCE_DIR/raw-records.jsonl"
BUNDLE_RECORDS="$EVIDENCE_DIR/bundle-capture-records.jsonl"
RAW_CANONICAL="$EVIDENCE_DIR/raw-membership.canonical.jsonl"
BUNDLE_CANONICAL="$EVIDENCE_DIR/bundle-membership.canonical.jsonl"
RAW_EVENT_COUNTS="$EVIDENCE_DIR/raw-event-type-counts.json"
BUNDLE_EVENT_COUNTS="$EVIDENCE_DIR/bundle-event-type-counts.json"
RAW_LANE_COUNTS="$EVIDENCE_DIR/raw-expected-lane-counts.json"
SOURCE_LANE_COUNTS="$EVIDENCE_DIR/bundle-source-lane-counts.json"
MEMBERSHIP_DIFF="$EVIDENCE_DIR/raw-vs-bundle-membership.diff"
EVENT_COUNTS_DIFF="$EVIDENCE_DIR/raw-vs-bundle-event-counts.diff"
LANE_COUNTS_DIFF="$EVIDENCE_DIR/raw-vs-source-lane-counts.diff"
MISSING_RAW_RECORDS="$EVIDENCE_DIR/raw-records-missing-from-bundle.jsonl"
EXTRA_BUNDLE_RECORDS="$EVIDENCE_DIR/bundle-records-not-in-raw.jsonl"
UNMARKED_EXTRA_BUNDLE_RECORDS="$EVIDENCE_DIR/bundle-extra-records-unmarked.jsonl"
DEGRADED_WITHOUT_GAP="$EVIDENCE_DIR/degraded-sources-without-known-gaps.txt"
CAP_TS_DIR="$EVIDENCE_DIR/capability-timestamps"
CAP_TS_REPORT="$EVIDENCE_DIR/capability-timestamps.json"

RAW_ALL_RECORDS="$EVIDENCE_DIR/raw-records.all.jsonl"
collect_jsonl_records "$RAW_ROOT" "$RAW_ALL_RECORDS"
filter_records_to_window "$RAW_ALL_RECORDS" "$RAW_RECORDS" "$TIME_START" "$TIME_END"
cp "$BUNDLE_PATH/events/capture.events.jsonl" "$BUNDLE_RECORDS"
jq -S -c . "$RAW_RECORDS" | sort >"$RAW_CANONICAL"
jq -S -c . "$BUNDLE_RECORDS" | sort >"$BUNDLE_CANONICAL"
write_event_type_counts "$RAW_RECORDS" "$RAW_EVENT_COUNTS"
write_event_type_counts "$BUNDLE_RECORDS" "$BUNDLE_EVENT_COUNTS"
write_expected_lane_counts "$RAW_RECORDS" "$RAW_LANE_COUNTS"
write_source_counts "$BUNDLE_PATH/sources.json" "$SOURCE_LANE_COUNTS"

failures=0

jq -S . "$RAW_EVENT_COUNTS" >"$RAW_EVENT_COUNTS.sorted"
jq -S . "$BUNDLE_EVENT_COUNTS" >"$BUNDLE_EVENT_COUNTS.sorted"
diff -u "$RAW_EVENT_COUNTS.sorted" "$BUNDLE_EVENT_COUNTS.sorted" >"$EVENT_COUNTS_DIFF" || true

comm -23 "$RAW_CANONICAL" "$BUNDLE_CANONICAL" >"$MISSING_RAW_RECORDS"
comm -13 "$RAW_CANONICAL" "$BUNDLE_CANONICAL" >"$EXTRA_BUNDLE_RECORDS"
jq -c '
  select(
    (((.sourceRecordID // .source_record_id // "") | test("^(inferred|derived):")) or
     ((.eventType // .event_type // "") | endswith(".inferred")) or
     (.payload.inferred == true) or
     (.payload.derived == true))
    | not
  )
' "$EXTRA_BUNDLE_RECORDS" >"$UNMARKED_EXTRA_BUNDLE_RECORDS"
if [[ ! -s "$MISSING_RAW_RECORDS" && ! -s "$UNMARKED_EXTRA_BUNDLE_RECORDS" ]]; then
  record_pass "raw spool records are preserved in bundle capture membership"
else
  {
    echo "missing_raw_records=$MISSING_RAW_RECORDS"
    echo "extra_bundle_records=$EXTRA_BUNDLE_RECORDS"
    echo "unmarked_extra_bundle_records=$UNMARKED_EXTRA_BUNDLE_RECORDS"
  } >"$MEMBERSHIP_DIFF"
  record_failure "raw spool records are preserved in bundle capture membership" "Summary: $MEMBERSHIP_DIFF"
fi

if jq -n -e --slurpfile raw "$RAW_EVENT_COUNTS" --slurpfile bundle "$BUNDLE_EVENT_COUNTS" '
  ($bundle[0] | map({key: .event_type, value: .count}) | from_entries) as $bundle_counts
  | all($raw[0][]; ($bundle_counts[.event_type] // 0) >= .count)
' >/dev/null; then
  record_pass "bundle event-type counts include every raw event type"
else
  record_failure "bundle event-type counts include every raw event type" "Diff: $EVENT_COUNTS_DIFF"
fi

if jq -n -e --slurpfile expected "$RAW_LANE_COUNTS" --slurpfile actual "$SOURCE_LANE_COUNTS" '
  ($expected[0]) as $expected_counts
  | ($actual[0]) as $actual_counts
  | all($expected_counts | keys[]; ($actual_counts[.] // -1) >= $expected_counts[.])
' >/dev/null; then
  record_pass "bundle source lane counts cover raw spool lane counts"
else
  jq -s '.[0] as $expected | .[1] as $actual | {expected: $expected, actual: $actual}' \
    "$RAW_LANE_COUNTS" "$SOURCE_LANE_COUNTS" >"$LANE_COUNTS_DIFF"
  record_failure "bundle source lane counts cover raw spool lane counts" "Summary: $LANE_COUNTS_DIFF"
fi

if jq -e -s '
  .[0].lane_count == ([.[1].sources[] | select(.lane_id | startswith("capture."))] | length)
' "$BUNDLE_PATH/manifest.json" "$BUNDLE_PATH/sources.json" >/dev/null; then
  record_pass "manifest lane_count matches capture source inventory"
else
  record_failure "manifest lane_count matches capture source inventory" "Manifest: $BUNDLE_PATH/manifest.json"
fi

jq -r '.sources[] | select(.status != "present") | .source_id' "$BUNDLE_PATH/sources.json" \
  | sort -u >"$EVIDENCE_DIR/degraded-source-ids.txt"
jq -r 'select(.source_id != null) | .source_id' "$BUNDLE_PATH/quality/known_gaps.jsonl" \
  | sort -u >"$EVIDENCE_DIR/known-gap-source-ids.txt"
comm -23 "$EVIDENCE_DIR/degraded-source-ids.txt" "$EVIDENCE_DIR/known-gap-source-ids.txt" \
  >"$DEGRADED_WITHOUT_GAP"
if [[ ! -s "$DEGRADED_WITHOUT_GAP" ]]; then
  record_pass "every degraded source is covered by known_gaps"
else
  record_failure "every degraded source is covered by known_gaps" "Missing gaps: $DEGRADED_WITHOUT_GAP"
fi

mkdir -p "$CAP_TS_DIR"
: >"$EVIDENCE_DIR/capability-timestamp-report-files.txt"
while IFS= read -r -d '' cap_file; do
  cap_name="$(basename "$cap_file")"
  report="$CAP_TS_DIR/$cap_name.report.json"
  write_capability_timestamp_report "$cap_file" "$cap_name" "$TIME_START" "$TIME_END" "$report"
  printf '%s\n' "$report" >>"$EVIDENCE_DIR/capability-timestamp-report-files.txt"
  if jq -e --arg end "$TIME_END" '
    def epoch($value):
      if ($value | type) == "string" then
        try ($value | sub("\\.[0-9]+Z$"; "Z") | sub("\\.[0-9]+([+-][0-9]{2}:[0-9]{2})$"; "\\1") | fromdateiso8601) catch null
      else
        null
      end;
    (epoch($end)) as $end_epoch
    | .timestamp_count == 0
      or (.inside_count > 0 and .outside_count == 0)
      or (.inside_count == 0 and all(.timestamps[]; .epoch >= $end_epoch and .epoch <= ($end_epoch + 120)))
  ' "$report" >/dev/null; then
    record_pass "capability timestamp in or adjacent to capture window: $cap_name"
  else
    record_failure "capability timestamp in or adjacent to capture window: $cap_name" "Report: $report"
  fi
done < <(find "$BUNDLE_PATH/capabilities" -type f -name '*.json' -print0 | sort -z)
jq -s . "$CAP_TS_DIR"/*.report.json >"$CAP_TS_REPORT"

frame_metadata_count="$(line_count "$BUNDLE_PATH/events/sck-frame-metadata.events.jsonl")"
media_index_count="$(line_count "$BUNDLE_PATH/media/media.index.jsonl")"
jq -c . "$BUNDLE_PATH/media/media.index.jsonl" >"$EVIDENCE_DIR/media-index.validated.jsonl"
if [[ "$frame_metadata_count" -eq 0 || "$media_index_count" -gt 0 ]]; then
  record_pass "media.index is ready when frame metadata is present"
else
  record_failure "media.index is ready when frame metadata is present" \
    "frame_metadata_count=$frame_metadata_count media_index_count=$media_index_count"
fi

jq_check "time_alignment is not scaffold" "$BUNDLE_PATH/time_alignment.json" '
  (.status // "") != "scaffold"
'

if [[ "$validate_status" -eq 0 ]] && jq -e '.status == "ok" and .bundle.ok == true' "$VALIDATE_RESPONSE" >/dev/null; then
  record_pass "strict bundle validation passes"
else
  record_failure "strict bundle validation passes" "Validation: $VALIDATE_RESPONSE"
fi

jq -n \
  --arg run_id "$RUN_ID" \
  --arg audit_root "$AUDIT_ROOT" \
  --arg raw_root "$RAW_ROOT" \
  --arg capture_root "$WORK_CAPTURE_ROOT" \
  --arg bundle_path "$BUNDLE_PATH" \
  --arg time_start "$TIME_START" \
  --arg time_end "$TIME_END" \
  --arg evidence_dir "$EVIDENCE_DIR" \
  --argjson failures "$failures" \
  --slurpfile raw_event_counts "$RAW_EVENT_COUNTS" \
  --slurpfile bundle_event_counts "$BUNDLE_EVENT_COUNTS" \
  --slurpfile raw_lane_counts "$RAW_LANE_COUNTS" \
  --slurpfile source_lane_counts "$SOURCE_LANE_COUNTS" \
  --slurpfile capability_timestamps "$CAP_TS_REPORT" \
  '{
    schema_version: 1,
    run_id: $run_id,
    status: (if $failures == 0 then "passed" else "failed" end),
    failure_count: $failures,
    audit_root: $audit_root,
    raw_root: $raw_root,
    regenerated_capture_root: $capture_root,
    regenerated_bundle_path: $bundle_path,
    time_window: {start: $time_start, end: $time_end},
    evidence_dir: $evidence_dir,
    raw_event_type_counts: $raw_event_counts[0],
    bundle_event_type_counts: $bundle_event_counts[0],
    raw_expected_lane_counts: $raw_lane_counts[0],
    bundle_source_lane_counts: $source_lane_counts[0],
    capability_timestamps: $capability_timestamps[0]
  }' >"$EVIDENCE_DIR/regression-summary.json"

if [[ "$failures" -ne 0 ]]; then
  echo "" >&2
  echo "Regenerated bundle regression failed with $failures failing check(s)." >&2
  echo "Evidence: $EVIDENCE_DIR" >&2
  exit 1
fi

echo "Regenerated bundle regression passed."
echo "Evidence: $EVIDENCE_DIR"
